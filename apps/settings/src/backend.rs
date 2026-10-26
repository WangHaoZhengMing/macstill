//! Settings owns persistence and catalog refresh, independently of the shell process.
use crate::{control::Server, request::Request};
use macstill_apps::DesktopApp;
use macstill_config::Preferences;
use notify::Watcher;
use std::{
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
    pub preferences: Preferences,
    pub apps: Vec<DesktopApp>,
    pub error: Option<String>,
    pub open_request: (u64, Request),
}
pub enum Action {
    Save(Preferences),
    Launch(String),
    Refresh,
}
pub struct Backend {
    state: Arc<Mutex<Snapshot>>,
    tx: SyncSender<Action>,
    stop: Arc<AtomicBool>,
}
impl Backend {
    #[cfg(test)]
    pub fn fixture() -> (Self, mpsc::Receiver<Action>) {
        let (tx, rx) = mpsc::sync_channel(16);
        (
            Self {
                state: Arc::new(Mutex::new(Snapshot {
                    revision: 0,
                    preferences: Preferences::default(),
                    apps: vec![],
                    error: None,
                    open_request: (0, Request::Show(crate::request::Page::General)),
                })),
                tx,
                stop: Arc::new(AtomicBool::new(false)),
            },
            rx,
        )
    }
    pub fn new(request: Request, server: Option<Server>) -> Self {
        let (preferences, error) = match macstill_config::load() {
            Ok(p) => (p, None),
            Err(e) => (Preferences::default(), Some(e.to_string())),
        };
        let state = Arc::new(Mutex::new(Snapshot {
            revision: 0,
            preferences,
            apps: Vec::new(),
            error,
            open_request: (0, request),
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::sync_channel(16);
        let shared = state.clone();
        let stopped = stop.clone();
        thread::spawn(move || {
            let dirty = Arc::new(AtomicBool::new(true));
            let changed = dirty.clone();
            let mut watcher =
                notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                    if event.is_ok() {
                        changed.store(true, Ordering::Relaxed);
                    }
                })
                .ok();
            if let Some(w) = watcher.as_mut() {
                for root in macstill_apps::roots() {
                    if root.is_dir() {
                        let _ = w.watch(&root, notify::RecursiveMode::Recursive);
                    }
                }
            }
            let mut catalog_at = Instant::now() - Duration::from_secs(60);
            let mut config_at = Instant::now() - Duration::from_secs(1);
            while !stopped.load(Ordering::Relaxed) {
                if let Some(args) = server.as_ref().and_then(Server::receive) {
                    let mut s = shared.lock().unwrap();
                    s.open_request.0 += 1;
                    s.open_request.1 = args;
                    s.revision += 1;
                }
                if config_at.elapsed() >= Duration::from_secs(1) {
                    let result = macstill_config::load();
                    let mut s = shared.lock().unwrap();
                    match result {
                        Ok(p) if p != s.preferences => {
                            s.preferences = p;
                            s.error = None;
                            s.revision += 1;
                        }
                        Err(e) => {
                            let error = Some(e.to_string());
                            if s.error != error {
                                s.error = error;
                                s.revision += 1;
                            }
                        }
                        _ => {}
                    }
                    config_at = Instant::now();
                }
                if dirty.swap(false, Ordering::Relaxed)
                    || catalog_at.elapsed() >= Duration::from_secs(60)
                {
                    let apps = macstill_apps::scan();
                    let mut s = shared.lock().unwrap();
                    s.apps = apps;
                    s.revision += 1;
                    catalog_at = Instant::now();
                }
                let result = match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(Action::Save(p)) => {
                        let result =
                            macstill_config::load().and_then(|_| macstill_config::save(&p));
                        if result.is_ok() {
                            shared.lock().unwrap().preferences = p;
                        }
                        result
                    }
                    Ok(Action::Refresh) => {
                        catalog_at = Instant::now() - Duration::from_secs(60);
                        continue;
                    }
                    Ok(Action::Launch(id)) => {
                        let app = shared
                            .lock()
                            .unwrap()
                            .apps
                            .iter()
                            .find(|a| a.id == id)
                            .cloned();
                        app.as_ref()
                            .ok_or_else(|| anyhow::anyhow!("Application is no longer installed"))
                            .and_then(|app| {
                                let window =
                                    macstill_hyprland::clients().ok().and_then(|clients| {
                                        clients.into_iter().find(|c| app.matches_class(&c.class))
                                    });
                                if let Some(window) = window {
                                    macstill_hyprland::focus(&window.address)
                                } else {
                                    macstill_apps::launch(app)
                                }
                            })
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                };
                let mut s = shared.lock().unwrap();
                s.error = result.err().map(|e| format!("{e:#}"));
                s.revision += 1;
            }
        });
        Self { state, tx, stop }
    }
    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().unwrap().clone()
    }
    pub fn send(&self, action: Action) -> anyhow::Result<()> {
        self.tx
            .try_send(action)
            .map_err(|_| anyhow::anyhow!("Settings is busy; please try again"))
    }
}
impl Drop for Backend {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
