use crate::{
    catalog::{self, DesktopApp},
    config::{self, Preferences},
    hyprland::{self, Client},
};
use notify::Watcher;
use std::{
    io::Read,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Snapshot {
    pub revision: u64,
    pub launch_sequence: u64,
    pub preferences: Preferences,
    pub apps: Vec<DesktopApp>,
    pub clients: Vec<Client>,
    pub active_title: String,
    pub error: Option<String>,
    pub battery: Option<u8>,
    pub battery_charging: bool,
    pub stage: hyprland::stage::Snapshot,
}
pub enum Action {
    Save(Preferences),
    Launch(String),
    LaunchNew(String),
    Quit(String),
    CloseWindow(String),
    WindowMenu { address: String, class: String, action: hyprland::window_menu::Action },
    Focus(String),
    OpenFolder(Folder),
    Refresh,
    Stage(hyprland::stage::Command),
}
#[derive(Clone, Copy)]
pub enum Folder {
    Downloads,
    Trash,
}
/// UI never owns the worker: cancellation and a bounded command queue keep its lifetime explicit.
pub struct Backend {
    state: Arc<Mutex<Snapshot>>,
    tx: SyncSender<Action>,
    stop: Arc<AtomicBool>,
    pub open_requests: async_channel::Receiver<String>,
}
impl Backend {
    pub fn new(serve_control: bool) -> Self {
        let _span = crate::diagnostics::span("backend.new");
        let (preferences, error) = match {
            let _span = crate::diagnostics::span("config.load");
            config::load()
        } {
            Ok(p) => (p, None),
            Err(e) => (Preferences::default(), Some(e.to_string())),
        };
        let state = Arc::new(Mutex::new(Snapshot {
            revision: 0,
            launch_sequence: 0,
            preferences,
            apps: vec![],
            clients: vec![],
            active_title: String::new(),
            error,
            battery: None,
            battery_charging: false,
            stage: Default::default(),
        }));
        let (tx, rx) = mpsc::sync_channel(16);
        let stop = Arc::new(AtomicBool::new(false));
        let (open_tx, open_requests) = async_channel::unbounded();
        let control_tx = tx.clone();
        let shared = state.clone();
        let stopped = stop.clone();
        thread::spawn(move || {
            let startup = crate::diagnostics::span("backend.worker.initialize");
            if serve_control {
                let _ = hyprland::mission::cache::shared();
            }
            let dirty = Arc::new(AtomicBool::new(true));
            let config_dirty = Arc::new(AtomicBool::new(false));
            let d = dirty.clone();
            let cd = config_dirty.clone();
            let dir = config::config_dir();
            let watched_config = dir.clone();
            let mut watcher =
                notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                    if let Ok(event) = event {
                        if event.paths.iter().any(|p| p.starts_with(&watched_config)) {
                            cd.store(true, Ordering::Relaxed);
                        } else {
                            d.store(true, Ordering::Relaxed);
                        }
                    }
                })
                .ok();
            let _ = std::fs::create_dir_all(&dir);
            if let Some(w) = watcher.as_mut() {
                let _ = w.watch(&dir, notify::RecursiveMode::NonRecursive);
                for root in catalog::roots() {
                    if root.exists() {
                        let _ = w.watch(&root, notify::RecursiveMode::Recursive);
                    }
                }
            }
            let mut events = hyprland::socket(".socket2.sock")
                .ok()
                .and_then(|p| std::os::unix::net::UnixStream::connect(p).ok());
            if let Some(socket) = events.as_mut() {
                let _ = socket.set_nonblocking(true);
            }
            let control = if serve_control {
                match crate::control::Server::bind() {
                    Ok(server) => Some(server),
                    Err(e) => {
                        let mut s = shared.lock().unwrap();
                        s.error = Some(e.to_string());
                        s.revision += 1;
                        None
                    }
                }
            } else {
                None
            };
            let mut last_scan = Instant::now() - Duration::from_secs(60);
            let mut stage = if control.is_some() {
                let _ = hyprland::stage::connect_native();
                if let Some(path) = crate::control::path() {
                    let _ = hyprland::request(&format!("macstill-mission {}", serde_json::json!({
                        "control": path, "gesture_file": hyprland::socket("macstill-mission-gesture.json").ok()
                    })));
                }
                hyprland::stage::FileJournal::for_session()
                    .map(|journal| {
                        let mut manager = hyprland::stage::MultiManager::new(journal);
                        let _ = manager.recover();
                        manager
                    })
                    .ok()
            } else {
                None
            };
            if let Some(manager) = &stage {
                let mut s = shared.lock().unwrap();
                s.stage = manager.snapshot.clone();
                s.revision += 1;
            }
            if let Some(server) = control {
                let stopped = stopped.clone();
                thread::spawn(move || {
                    // A dedicated blocking accept wakes immediately, independently of
                    // catalog scans, Stage Manager work and the backend's timer.
                    if server.listener.set_nonblocking(false).is_err() {
                        return;
                    }
                    while !stopped.load(Ordering::Relaxed) {
                        let Ok((mut stream, _)) = server.listener.accept() else {
                            break;
                        };
                        if stopped.load(Ordering::Relaxed) {
                            break;
                        }
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
                        let mut bytes = [0; 64];
                        if let Ok(n) = stream.read(&mut bytes) {
                            let command = String::from_utf8_lossy(&bytes[..n]);
                            if matches!(
                                command.as_ref(),
                                "launchpad"
                                    | "mission-toggle"
                                    | "mission-open"
                                    | "mission-close"
                                    | "mission-gesture"
                                    | "mission-prepare"
                                    | "mission-unprepare"
                            ) {
                                if open_tx.send_blocking(command.into_owned()).is_err() {
                                    break;
                                }
                            } else if let Some(command) = stage_command(&command) {
                                let _ = control_tx.try_send(Action::Stage(command));
                            }
                        }
                    }
                });
            }
            drop(startup);
            crate::diagnostics::event("backend.worker.ready");
            let mut last_stage = Instant::now() - Duration::from_secs(1);
            let mut last_status = Instant::now() - Duration::from_secs(60);
            let mut windows_dirty = true;
            while !stopped.load(Ordering::Relaxed) {
                let action = rx.recv_timeout(Duration::from_millis(200));
                match action {
                    Ok(Action::Stage(command)) => {
                        if let Some(manager) = &mut stage {
                            let _ = manager.handle(command);
                            let mut s = shared.lock().unwrap();
                            s.stage = manager.snapshot.clone();
                            s.revision += 1;
                        }
                    }
                    Ok(Action::Save(p)) => {
                        let error = config::load()
                            .and_then(|_| config::save(&p))
                            .err()
                            .map(|e| e.to_string());
                        let mut s = shared.lock().unwrap();
                        if error.is_none() {
                            s.preferences = p
                        }
                        s.error = error;
                        s.revision += 1;
                    }
                    Ok(Action::Launch(id)) => {
                        let (app, window) = {
                            let s = shared.lock().unwrap();
                            let app = s.apps.iter().find(|a| a.id == id).cloned();
                            let window =
                                app.as_ref()
                                    .and_then(|a| {
                                        s.clients.iter().filter(|c| matches_app(a, c)).min_by_key(
                                            |c| {
                                                c.focus_history_id
                                                    .filter(|id| *id >= 0)
                                                    .unwrap_or(i64::MAX)
                                            },
                                        )
                                    })
                                    .map(|c| c.address.clone());
                            (app, window)
                        };
                        let result = if let Some(address) = window {
                            hyprland::focus(&address)
                        } else if let Some(app) = app {
                            catalog::launch(&app)
                        } else {
                            Err(anyhow::anyhow!("This application is no longer installed"))
                        };
                        let mut s = shared.lock().unwrap();
                        s.error = result.err().map(|e| format!("{e:#}"));
                        s.launch_sequence += 1;
                        s.revision += 1;
                    }
                    Ok(Action::LaunchNew(id)) => {
                        let app = shared
                            .lock()
                            .unwrap()
                            .apps
                            .iter()
                            .find(|a| a.id == id)
                            .cloned();
                        let result = app
                            .as_ref()
                            .ok_or_else(|| anyhow::anyhow!("Application is no longer installed"))
                            .and_then(catalog::launch);
                        let mut s = shared.lock().unwrap();
                        s.error = result.err().map(|e| format!("{e:#}"));
                        s.launch_sequence += 1;
                        s.revision += 1;
                    }
                    Ok(Action::Quit(id)) => {
                        let app = shared
                            .lock()
                            .unwrap()
                            .apps
                            .iter()
                            .find(|a| a.id == id)
                            .cloned();
                        let result = app
                            .as_ref()
                            .ok_or_else(|| anyhow::anyhow!("Application is no longer installed"))
                            .and_then(|app| {
                                let clients = hyprland::clients()?;
                                close_app_windows(app, &clients, hyprland::close)
                            });
                        let mut s = shared.lock().unwrap();
                        s.error = result.err().map(|e| format!("{e:#}"));
                        s.revision += 1;
                    }
                    Ok(Action::WindowMenu { address, class, action }) => {
                        let result = hyprland::window_menu::perform(&address, &class, action);
                        let mut s = shared.lock().unwrap();
                        s.error = result.err().map(|e| format!("{e:#}"));
                        s.revision += 1;
                        windows_dirty = true;
                    }
                    Ok(Action::CloseWindow(address)) => {
                        let result = hyprland::close(&address);
                        let mut s = shared.lock().unwrap();
                        s.error = result.err().map(|e| format!("{e:#}"));
                        s.revision += 1;
                    }
                    Ok(Action::Focus(address)) => {
                        let result = hyprland::focus(&address);
                        let mut s = shared.lock().unwrap();
                        s.error = result.err().map(|e| format!("{e:#}"));
                        s.revision += 1;
                    }
                    Ok(Action::OpenFolder(folder)) => {
                        let argument = match folder {
                            Folder::Trash => "--trash".into(),
                            Folder::Downloads => glib_downloads().into_os_string(),
                        };
                        let result = catalog::open_finder(&[argument]);
                        let mut s = shared.lock().unwrap();
                        s.error = result.err().map(|e| e.to_string());
                        s.revision += 1;
                    }
                    Ok(Action::Refresh) => dirty.store(true, Ordering::Relaxed),
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    _ => {}
                }
                // Watcher events coalesce; fallback rescans also detect newly-created export directories.
                if dirty.swap(false, Ordering::Relaxed)
                    || last_scan.elapsed() > Duration::from_secs(60)
                {
                    let apps = {
                        let _span = crate::diagnostics::span("catalog.scan");
                        catalog::scan()
                    };
                    let mut s = shared.lock().unwrap();
                    s.apps = apps;
                    s.revision += 1;
                    last_scan = Instant::now();
                }
                if config_dirty.swap(false, Ordering::Relaxed) {
                    let mut s = shared.lock().unwrap();
                    match config::load() {
                        Ok(p) if p != s.preferences => {
                            s.preferences = p;
                            s.revision += 1;
                        }
                        Err(e) => {
                            s.error = Some(e.to_string());
                            s.revision += 1;
                        }
                        _ => {}
                    }
                }
                if let Some(socket) = events.as_mut() {
                    let mut buf = [0; 4096];
                    loop {
                        match socket.read(&mut buf) {
                            Ok(0) => {
                                events = None;
                                break;
                            }
                            Ok(_) => windows_dirty = true,
                            Err(_) => break,
                        }
                    }
                }
                if let Some(manager) = &mut stage
                    && manager.snapshot.enabled
                    && (last_stage.elapsed() >= Duration::from_millis(400)
                        || (windows_dirty && last_stage.elapsed() >= Duration::from_millis(80)))
                {
                    if let Err(error) = manager.tick() {
                        manager.snapshot.error = Some(format!("{error:#}"));
                    }
                    let mut s = shared.lock().unwrap();
                    if s.stage != manager.snapshot {
                        s.stage = manager.snapshot.clone();
                        s.revision += 1;
                    }
                    last_stage = Instant::now();
                }
                if windows_dirty {
                    if let Ok(clients) = hyprland::clients() {
                        let active = hyprland::request("j/activewindow")
                            .ok()
                            .and_then(|v| serde_json::from_str::<serde_json::Value>(&v).ok())
                            .and_then(|v| v["class"].as_str().map(str::to_owned))
                            .unwrap_or_default();
                        let mut s = shared.lock().unwrap();
                        s.active_title = if active == "org.macstill.Desktop" {
                            "Macstill".into()
                        } else {
                            clients
                                .iter()
                                .find(|c| c.class == active)
                                .and_then(|c| s.apps.iter().find(|a| matches_app(a, c)))
                                .map(|a| a.name.clone())
                                .unwrap_or(active)
                        };
                        s.clients = clients;
                        s.revision += 1;
                    }
                    windows_dirty = false;
                }
                if last_status.elapsed() > Duration::from_secs(5) {
                    if let Ok(power) = macstill_system::SystemApi::new().power().snapshot() {
                        let battery = power
                            .battery
                            .as_ref()
                            .map(|battery| battery.percentage.round().clamp(0.0, 100.0) as u8);
                        // UPower device state 1 means actively charging, not just plugged in.
                        let charging = power
                            .battery
                            .as_ref()
                            .is_some_and(|battery| battery.state == 1);
                        let mut s = shared.lock().unwrap();
                        if s.battery != battery || s.battery_charging != charging {
                            s.battery = battery;
                            s.battery_charging = charging;
                            s.revision += 1;
                        }
                    }
                    last_status = Instant::now();
                }
            }
            if let Some(manager) = &mut stage
                && let Err(error) = manager.handle(hyprland::stage::Command::Disable)
            {
                eprintln!("Stage Manager recovery record retained: {error:#}");
            }
        });
        Self {
            state,
            tx,
            stop,
            open_requests,
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().unwrap().clone()
    }
    pub fn send(&self, action: Action) -> anyhow::Result<()> {
        let output = match &action {
            Action::Stage(hyprland::stage::Command::OnOutput(name, _)) => Some(name.clone()),
            _ => None,
        };
        let stage = matches!(action, Action::Stage(_));
        let mut state = stage.then(|| self.state.lock().unwrap());
        if let Some(s) = &state {
            let target = output
                .as_ref()
                .and_then(|name| {
                    s.stage
                        .outputs
                        .iter()
                        .find(|o| o.output.as_ref() == Some(name))
                })
                .unwrap_or(&s.stage);
            anyhow::ensure!(
                target.available && !target.busy,
                "Stage Manager is unavailable or busy"
            );
        }
        self.tx
            .try_send(action)
            .map_err(|_| anyhow::anyhow!("Desktop is busy; please try again"))?;
        if let Some(s) = &mut state {
            if let Some(target) = output.as_ref().and_then(|name| {
                s.stage
                    .outputs
                    .iter_mut()
                    .find(|o| o.output.as_ref() == Some(name))
            }) {
                target.busy = true;
            } else {
                s.stage.busy = true;
            }
            s.revision += 1;
        }
        Ok(())
    }
}
fn stage_command(command: &str) -> Option<hyprland::stage::Command> {
    use hyprland::stage::Command;
    Some(match command {
        "stage-toggle" => Command::Animate(Box::new(Command::Toggle)),
        "stage-on" => Command::Animate(Box::new(Command::Enable)),
        "stage-off" => Command::Animate(Box::new(Command::Disable)),
        "stage-next" => Command::Animate(Box::new(Command::Next)),
        "stage-ungroup" => Command::Ungroup,
        "stage-desktop" => Command::Animate(Box::new(Command::ShowDesktop)),
        "stage-hide-recent" => Command::SetRecentHidden(true),
        "stage-show-recent" => Command::SetRecentHidden(false),
        _ => return None,
    })
}
impl Drop for Backend {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Wake blocking accept so the server releases its session socket.
        if let Some(path) = crate::control::path() {
            let _ = std::os::unix::net::UnixStream::connect(path);
        }
    }
}
pub fn matches_app(app: &DesktopApp, client: &Client) -> bool {
    app.matches_class(&client.class) || app.matches_class(&client.initial_class)
}

fn glib_downloads() -> std::path::PathBuf {
    // GLib reads user-dirs.dirs, including localized/custom XDG download folders.
    gio::glib::user_special_dir(gio::glib::UserDirectory::Downloads)
        .unwrap_or_else(|| config::home().join("Downloads"))
}

fn close_app_windows(
    app: &DesktopApp,
    clients: &[Client],
    mut close: impl FnMut(&str) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let mut first_error = None;
    for client in clients.iter().filter(|c| matches_app(app, c)) {
        if let Err(error) = close(&client.address) {
            if first_error.is_none() {
                first_error = Some(error);
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}

#[cfg(test)]
impl Backend {
    pub(crate) fn fixture_stage(
        stage: hyprland::stage::Snapshot,
    ) -> (Self, mpsc::Receiver<Action>) {
        let mut backend = Self::fixture();
        let (tx, rx) = mpsc::sync_channel(16);
        backend.tx = tx;
        backend.state.lock().unwrap().stage = stage;
        (backend, rx)
    }
    pub(crate) fn fixture() -> Self {
        let (tx, _rx) = mpsc::sync_channel(16);
        Self {
            state: Arc::new(Mutex::new(Snapshot {
                revision: 0,
                launch_sequence: 0,
                preferences: Preferences::default(),
                apps: vec![],
                clients: vec![],
                active_title: String::new(),
                error: None,
                battery: None,
                battery_charging: false,
                stage: Default::default(),
            })),
            tx,
            stop: Arc::new(AtomicBool::new(false)),
            open_requests: async_channel::unbounded().1,
        }
    }
}

#[cfg(test)]
mod quit_tests {
    use super::*;
    fn app() -> DesktopApp {
        DesktopApp {
            id: "org.example.Editor.desktop".into(),
            name: "Editor".into(),
            desktop_file: "editor.desktop".into(),
            icon: None,
            class_aliases: Vec::new(),
            startup_class: Some("Editor".into()),
        }
    }
    fn clients() -> Vec<Client> {
        vec![
            Client {
                focus_history_id: None,
                auxiliary: false,
                initial_class: String::new(),
                address: "0x1".into(),
                class: "Editor".into(),
                title: "Document".into(),
            },
            Client {
                focus_history_id: None,
                auxiliary: false,
                initial_class: String::new(),
                address: "0x2".into(),
                class: "Other".into(),
                title: "Other app".into(),
            },
            Client {
                focus_history_id: None,
                auxiliary: false,
                initial_class: String::new(),
                address: "0x3".into(),
                class: "org.example.Editor".into(),
                title: "Second document".into(),
            },
        ]
    }
    #[test]
    fn quit_requests_all_matching_windows_and_leaves_other_apps_alone() {
        let mut closed = Vec::new();
        close_app_windows(&app(), &clients(), |address| {
            closed.push(address.to_string());
            Ok(())
        })
        .unwrap();
        assert_eq!(closed, ["0x1", "0x3"]);
    }
    #[test]
    fn a_failed_close_is_reported_without_skipping_the_remaining_windows() {
        let mut closed = Vec::new();
        let result = close_app_windows(&app(), &clients(), |address| {
            closed.push(address.to_string());
            if address == "0x1" {
                anyhow::bail!("window disappeared");
            }
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(closed, ["0x1", "0x3"]);
    }
}
