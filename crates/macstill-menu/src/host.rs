//! Shell-side menu service. Window identity comes exclusively from the compositor.
use crate::{Endpoint, Entry, dbus};
use gio::glib::{
    self,
    variant::{ObjectPath, ToVariant},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default)]
pub struct Window {
    pub address: String,
    pub xid: Option<u32>,
    pub endpoint: Option<Endpoint>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub window: String,
    pub endpoint: Endpoint,
    pub generation: u64,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub session: Option<Session>,
    pub root: Option<Entry>,
    pub revision: u32,
    pub error: Option<String>,
}
enum Command {
    Show(Session, i32),
    Hide(Session, i32),
    Click(Session, u32, i32),
    Stop,
}
pub struct Host {
    state: Arc<Mutex<Snapshot>>,
    target: Arc<Mutex<Option<String>>>,
    commands: mpsc::SyncSender<Command>,
}
impl Host {
    pub fn new(mut windows: impl FnMut() -> anyhow::Result<Vec<Window>> + Send + 'static) -> Self {
        let state = Arc::new(Mutex::new(Snapshot::default()));
        let target = Arc::new(Mutex::new(None));
        let (commands, rx) = mpsc::sync_channel(64);
        let shared = state.clone();
        let active = target.clone();
        std::thread::Builder::new()
            .name("menu-host".into())
            .spawn(move || {
                let context = glib::MainContext::new();
                let _ = context.with_thread_default(|| {
                    loop {
                        match run(&context, &shared, &active, &rx, &mut windows) {
                            Ok(()) => break,
                            Err(e) => {
                                *shared.lock().unwrap() = Snapshot {
                                    error: Some(e.to_string()),
                                    ..Default::default()
                                };
                                match rx.recv_timeout(Duration::from_secs(1)) {
                                    Ok(Command::Stop)
                                    | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                                    _ => {}
                                }
                            }
                        }
                    }
                });
            })
            .expect("start menu host");
        Self {
            state,
            target,
            commands,
        }
    }
    /// For previews and tests: never advertises a global-menu service.
    pub fn disabled() -> Self {
        let (commands, _) = mpsc::sync_channel(1);
        Self {
            state: Default::default(),
            target: Default::default(),
            commands,
        }
    }
    pub fn select(&self, window: Option<String>) {
        *self.target.lock().unwrap() = window;
    }
    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().unwrap().clone()
    }
    pub fn show(&self, session: Session, id: i32) {
        let _ = self.commands.try_send(Command::Show(session, id));
    }
    pub fn hide(&self, session: Session, id: i32) {
        let _ = self.commands.try_send(Command::Hide(session, id));
    }
    pub fn click(&self, session: Session, revision: u32, id: i32) {
        let _ = self
            .commands
            .try_send(Command::Click(session, revision, id));
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.commands.try_send(Command::Stop);
    }
}
const XML: &str = r#"<node><interface name="com.canonical.AppMenu.Registrar">
<method name="RegisterWindow"><arg type="u" direction="in"/><arg type="o" direction="in"/></method>
<method name="UnregisterWindow"><arg type="u" direction="in"/></method>
<method name="GetMenuForWindow"><arg type="u" direction="in"/><arg type="s" direction="out"/><arg type="o" direction="out"/></method>
<method name="GetMenus"><arg type="a(uso)" direction="out"/></method>
<signal name="WindowRegistered"><arg type="u"/><arg type="s"/><arg type="o"/></signal>
<signal name="WindowUnregistered"><arg type="u"/></signal>
</interface></node>"#;
fn run(
    context: &glib::MainContext,
    shared: &Arc<Mutex<Snapshot>>,
    target: &Arc<Mutex<Option<String>>>,
    rx: &mpsc::Receiver<Command>,
    windows: &mut impl FnMut() -> anyhow::Result<Vec<Window>>,
) -> anyhow::Result<()> {
    windows()?; // Never advertise menu support without a working window association bridge.
    let c = dbus::connect()?;
    let registrations = Arc::new(Mutex::new(HashMap::<u32, Endpoint>::new()));
    let info = gio::DBusNodeInfo::for_xml(XML)?
        .lookup_interface(dbus::REGISTRAR)
        .unwrap();
    let entries = registrations.clone();
    let _object = c
        .register_object(dbus::REGISTRAR_PATH, &info)
        .method_call(move |c, sender, _, _, method, args, inv| {
            let mut entries = entries.lock().unwrap();
            match method {
                "RegisterWindow" => {
                    let (xid, path) = args.get::<(u32, ObjectPath)>().unwrap();
                    let owner = sender.unwrap_or("").to_string();
                    if xid == 0 || entries.get(&xid).is_some_and(|e| e.owner != owner) {
                        inv.return_dbus_error(
                            "org.freedesktop.DBus.Error.AccessDenied",
                            "Window is already registered or invalid",
                        );
                        return;
                    }
                    entries.insert(
                        xid,
                        Endpoint {
                            owner: owner.clone(),
                            path: path.to_string(),
                        },
                    );
                    let _ = c.emit_signal(
                        None,
                        dbus::REGISTRAR_PATH,
                        dbus::REGISTRAR,
                        "WindowRegistered",
                        Some(&(xid, owner, path).to_variant()),
                    );
                    inv.return_value(None);
                }
                "UnregisterWindow" => {
                    let (xid,) = args.get::<(u32,)>().unwrap();
                    if entries
                        .get(&xid)
                        .is_some_and(|e| Some(e.owner.as_str()) == sender)
                    {
                        entries.remove(&xid);
                        let _ = c.emit_signal(
                            None,
                            dbus::REGISTRAR_PATH,
                            dbus::REGISTRAR,
                            "WindowUnregistered",
                            Some(&(xid,).to_variant()),
                        );
                    }
                    inv.return_value(None);
                }
                "GetMenuForWindow" => {
                    let (xid,) = args.get::<(u32,)>().unwrap();
                    if let Some(e) = entries.get(&xid) {
                        inv.return_value(Some(
                            &(
                                e.owner.as_str(),
                                ObjectPath::try_from(e.path.as_str()).unwrap(),
                            )
                                .to_variant(),
                        ));
                    } else {
                        inv.return_dbus_error(
                            "com.canonical.AppMenu.Registrar.Error.WindowNotFound",
                            "Window has no registered menu",
                        );
                    }
                }
                "GetMenus" => {
                    let list: Vec<_> = entries
                        .iter()
                        .map(|(xid, e)| {
                            (
                                *xid,
                                e.owner.as_str(),
                                ObjectPath::try_from(e.path.as_str()).unwrap(),
                            )
                        })
                        .collect();
                    inv.return_value(Some(&(list,).to_variant()));
                }
                _ => inv.return_dbus_error(
                    "org.freedesktop.DBus.Error.UnknownMethod",
                    "Unknown method",
                ),
            }
        })
        .build()?;
    let dirty = Arc::new(Mutex::new(true));
    let changed = dirty.clone();
    let _updates = c.subscribe_to_signal(
        None,
        Some(dbus::INTERFACE),
        None,
        None,
        None,
        gio::DBusSignalFlags::NONE,
        move |_| *changed.lock().unwrap() = true,
    );
    let entries = registrations.clone();
    let changed = dirty.clone();
    let _owners = c.subscribe_to_signal(
        Some("org.freedesktop.DBus"),
        Some("org.freedesktop.DBus"),
        Some("NameOwnerChanged"),
        None,
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let c = signal.connection;
            let args = signal.parameters;
            if let Some((name, _, new)) = args.get::<(String, String, String)>() {
                if new.is_empty() {
                    let mut entries = entries.lock().unwrap();
                    let removed: Vec<_> = entries
                        .iter()
                        .filter(|(_, e)| e.owner == name)
                        .map(|(id, _)| *id)
                        .collect();
                    for id in removed {
                        entries.remove(&id);
                        let _ = c.emit_signal(
                            None,
                            dbus::REGISTRAR_PATH,
                            dbus::REGISTRAR,
                            "WindowUnregistered",
                            Some(&(id,).to_variant()),
                        );
                    }
                }
                *changed.lock().unwrap() = true;
            }
        },
    );
    // Do not replace another desktop's registrar. Its GetMenuForWindow is used below.
    let name = dbus::call(
        &c,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
        Some(&(dbus::REGISTRAR, 4u32).to_variant()),
    )?;
    let owns = matches!(name.child_value(0).get::<u32>(), Some(1 | 4));
    let mut generation = 0u64;
    let mut remote_revision = None;
    let mut opened: Vec<(Session, i32)> = Vec::new();
    let mut last_requested = None;
    let mut known_xids: HashMap<u32, String> = HashMap::new();
    let mut refresh = Instant::now() - Duration::from_secs(2);
    loop {
        while context.pending() {
            context.iteration(false);
        }
        anyhow::ensure!(!c.is_closed(), "Menu session bus disconnected");
        let requested = target.lock().unwrap().clone();
        let current = shared.lock().unwrap().clone();
        let dirty_now = std::mem::take(&mut *dirty.lock().unwrap());
        if dirty_now
            || refresh.elapsed() >= Duration::from_millis(300)
            || requested != last_requested
        {
            refresh = Instant::now();
            last_requested = requested.clone();
            let records = windows().unwrap_or_default();
            let next_xids: HashMap<_, _> = records
                .iter()
                .filter_map(|w| Some((w.xid?, w.address.clone())))
                .collect();
            for (xid, address) in &known_xids {
                if next_xids.get(xid) != Some(address) {
                    registrations.lock().unwrap().remove(xid);
                    let _ = c.emit_signal(
                        None,
                        dbus::REGISTRAR_PATH,
                        dbus::REGISTRAR,
                        "WindowUnregistered",
                        Some(&(*xid,).to_variant()),
                    );
                }
            }
            known_xids = next_xids;
            let endpoint = requested
                .as_ref()
                .and_then(|address| records.iter().find(|w| &w.address == address))
                .and_then(|w| {
                    if let Some(endpoint) = &w.endpoint {
                        return Some(endpoint.clone());
                    }
                    let xid = w.xid?;
                    if owns {
                        registrations.lock().unwrap().get(&xid).cloned()
                    } else {
                        let v = dbus::call(
                            &c,
                            dbus::REGISTRAR,
                            dbus::REGISTRAR_PATH,
                            dbus::REGISTRAR,
                            "GetMenuForWindow",
                            Some(&(xid,).to_variant()),
                        )
                        .ok()?;
                        let (owner, path) = v.get::<(String, ObjectPath)>()?;
                        Some(Endpoint {
                            owner,
                            path: path.to_string(),
                        })
                    }
                })
                .and_then(|e| {
                    Some(Endpoint {
                        owner: dbus::owner(&c, &e.owner)?,
                        path: e.path,
                    })
                });
            let next = endpoint.and_then(|endpoint| {
                let window = requested.clone()?;
                let session = if let Some(s) = current
                    .session
                    .as_ref()
                    .filter(|s| s.window == window && s.endpoint == endpoint)
                {
                    s.clone()
                } else {
                    generation += 1;
                    Session {
                        window,
                        endpoint,
                        generation,
                    }
                };
                Some(session)
            });
            let mut snapshot = Snapshot {
                session: next.clone(),
                ..Default::default()
            };
            let mut next_remote_revision = None;
            if let Some(s) = next {
                match dbus::read(&c, &s.endpoint) {
                    Ok((revision, root)) => {
                        next_remote_revision = Some(revision);
                        snapshot.root = Some(root);
                    }
                    Err(e) => {
                        snapshot.error = Some(e.to_string());
                    }
                }
            }
            // Some exporters return a constant protocol revision. Use our own display
            // revision as well as comparing the freshly read tree before activation.
            snapshot.revision = current.revision;
            if current.session != snapshot.session
                || current.root != snapshot.root
                || remote_revision != next_remote_revision
            {
                snapshot.revision = snapshot.revision.wrapping_add(1);
            }
            remote_revision = next_remote_revision;
            opened.retain(|(s, id)| {
                if snapshot.session.as_ref() == Some(s) {
                    true
                } else {
                    dbus::event(&c, &s.endpoint, *id, "closed");
                    false
                }
            });
            *shared.lock().unwrap() = snapshot;
        }
        loop {
            let command = match rx.try_recv() {
                Ok(c) => c,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    let _ = c.close_sync(gio::Cancellable::NONE);
                    return Ok(());
                }
            };
            let current = shared.lock().unwrap().clone();
            match command {
                Command::Stop => {
                    let _ = c.close_sync(gio::Cancellable::NONE);
                    return Ok(());
                }
                Command::Show(s, id) if current.session.as_ref() == Some(&s) => {
                    dbus::show(&c, &s.endpoint, id);
                    dbus::event(&c, &s.endpoint, id, "opened");
                    if !opened.contains(&(s.clone(), id)) {
                        opened.push((s, id));
                    }
                    *dirty.lock().unwrap() = true;
                }
                Command::Hide(s, id) => {
                    if opened.contains(&(s.clone(), id)) {
                        dbus::event(&c, &s.endpoint, id, "closed");
                        opened.retain(|item| item != &(s.clone(), id));
                    }
                }
                Command::Click(s, revision, id) if current.session.as_ref() == Some(&s) => {
                    // Re-resolve the compositor binding and menu before acting; queued UI callbacks
                    // must never target a replaced window, owner, revision, or now-disabled item.
                    let selected = target.lock().unwrap().clone();
                    let live = selected.as_ref() == Some(&s.window)
                        && windows().unwrap_or_default().iter().any(|w| {
                            w.address == s.window
                                && (w.endpoint.as_ref().is_some_and(|e| {
                                    e.path == s.endpoint.path
                                        && dbus::owner(&c, &e.owner).as_ref()
                                            == Some(&s.endpoint.owner)
                                }) || w.xid.is_some_and(|xid| {
                                    if owns {
                                        registrations.lock().unwrap().get(&xid) == Some(&s.endpoint)
                                    } else {
                                        dbus::call(
                                            &c,
                                            dbus::REGISTRAR,
                                            dbus::REGISTRAR_PATH,
                                            dbus::REGISTRAR,
                                            "GetMenuForWindow",
                                            Some(&(xid,).to_variant()),
                                        )
                                        .ok()
                                        .and_then(|v| v.get::<(String, ObjectPath)>())
                                        .is_some_and(
                                            |(name, path)| {
                                                path.as_str() == s.endpoint.path
                                                    && dbus::owner(&c, &name).as_ref()
                                                        == Some(&s.endpoint.owner)
                                            },
                                        )
                                    }
                                }))
                        });
                    if live
                        && let Ok((now, root)) = dbus::read(&c, &s.endpoint)
                        && revision == current.revision
                        && Some(now) == remote_revision
                        && current.root.as_ref() == Some(&root)
                        && root.actionable(id)
                    {
                        dbus::click(&c, &s.endpoint, id);
                    }
                    *dirty.lock().unwrap() = true;
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
