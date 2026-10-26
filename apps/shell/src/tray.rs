//! StatusNotifier host. All D-Bus work stays off the rendering thread.
use gio::glib::{self, variant::ToVariant};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const PATH: &str = "/StatusNotifierWatcher";
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub key: String,
    pub title: String,
    pub icon: Option<Vec<u8>>,
    pub menu: bool,
    pub exported_menu: bool,
}
pub use macstill_menu::Entry;
#[derive(Clone, Default)]
pub struct State {
    pub items: Vec<Item>,
    pub entries: Vec<Entry>,
    pub selected: Option<String>,
}
pub enum Command {
    Activate(String),
    Secondary(String),
    Menu(String),
    Event(String, i32),
    Expand(String, i32),
    Scroll(String, i32, String),
    Close,
    Stop,
}
pub struct Tray {
    state: Arc<Mutex<State>>,
    tx: mpsc::Sender<Command>,
}
impl Tray {
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(State::default()));
        let (tx, rx) = mpsc::channel();
        let shared = state.clone();
        std::thread::spawn(move || {
            if let Err(e) = run(shared, rx) {
                eprintln!("System tray: {e}");
            }
        });
        Self { state, tx }
    }
    pub fn snapshot(&self) -> State {
        self.state.lock().unwrap().clone()
    }
    pub fn send(&self, c: Command) {
        let _ = self.tx.send(c);
    }
}
impl Drop for Tray {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Stop);
    }
}
impl Default for Tray {
    fn default() -> Self {
        Self::new()
    }
}
fn call(
    c: &gio::DBusConnection,
    d: &str,
    p: &str,
    i: &str,
    m: &str,
    v: Option<&glib::Variant>,
) -> Result<glib::Variant, glib::Error> {
    macstill_menu::dbus::call(c, d, p, i, m, v)
}
fn endpoint(key: &str) -> (&str, &str) {
    key.split_once('/')
        .map(|(s, _)| (s.trim_end_matches('@'), &key[s.len()..]))
        .unwrap_or((key, "/StatusNotifierItem"))
}
fn properties(c: &gio::DBusConnection, key: &str) -> Option<HashMap<String, glib::Variant>> {
    let (d, p) = endpoint(key);
    call(
        c,
        d,
        p,
        "org.freedesktop.DBus.Properties",
        "GetAll",
        Some(&("org.kde.StatusNotifierItem",).to_variant()),
    )
    .ok()?
    .child_value(0)
    .get()
}
fn pixmap(v: &glib::Variant) -> Option<Vec<u8>> {
    let images = v.get::<Vec<(i32, i32, Vec<u8>)>>()?;
    let (w, h, b) = images
        .into_iter()
        .filter(|(w, h, b)| {
            *w > 0
                && *h > 0
                && *w <= 512
                && *h <= 512
                && b.len() == (*w as usize) * (*h as usize) * 4
        })
        .min_by_key(|(w, _, _)| (w - 24).abs())?;
    let rgba: Vec<u8> = b
        .chunks_exact(4)
        .flat_map(|p| [p[1], p[2], p[3], p[0]])
        .collect();
    let im = image::RgbaImage::from_raw(w as u32, h as u32, rgba)?;
    let mut out = std::io::Cursor::new(Vec::new());
    im.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}
fn icon(props: &HashMap<String, glib::Variant>, attention: bool) -> Option<Vec<u8>> {
    let prefix = if attention { "AttentionIcon" } else { "Icon" };
    if let Some(v) = props.get(&format!("{prefix}Pixmap")) {
        if let Some(p) = pixmap(v) {
            return Some(p);
        }
    }
    let name = props.get(&format!("{prefix}Name"))?.str()?;
    if std::path::Path::new(name).is_absolute() {
        return std::fs::read(name).ok();
    }
    let theme = props
        .get("IconThemePath")
        .and_then(|v| v.str())
        .unwrap_or("");
    let mut roots = vec![
        std::path::PathBuf::from(theme),
        glib::home_dir().join(".local/share/icons"),
        std::path::PathBuf::from("/usr/share/icons"),
        std::path::PathBuf::from("/usr/share/pixmaps"),
    ];
    fn find(root: &std::path::Path, name: &str, depth: u8) -> Option<Vec<u8>> {
        if depth == 0 {
            return None;
        }
        let entries = std::fs::read_dir(root).ok()?;
        for e in entries.flatten() {
            let p = e.path();
            if p.is_file() && p.file_stem().and_then(|s| s.to_str()) == Some(name) {
                if let Ok(b) = std::fs::read(&p) {
                    return Some(b);
                }
            }
            if p.is_dir() {
                if let Some(b) = find(&p, name, depth - 1) {
                    return Some(b);
                }
            }
        }
        None
    }
    roots.drain(..).find_map(|r| find(&r, name, 6))
}
fn layout(v: glib::Variant, depth: u8) -> Option<Entry> {
    macstill_menu::dbus::layout(v, depth)
}
fn run(state: Arc<Mutex<State>>, rx: mpsc::Receiver<Command>) -> Result<(), glib::Error> {
    let context = glib::MainContext::new();
    let _guard = context.acquire().unwrap();
    context
        .with_thread_default(|| run_connected(&context, state, rx))
        .unwrap()
}
fn run_connected(
    context: &glib::MainContext,
    state: Arc<Mutex<State>>,
    rx: mpsc::Receiver<Command>,
) -> Result<(), glib::Error> {
    let address =
        gio::dbus_address_get_for_bus_sync(gio::BusType::Session, gio::Cancellable::NONE)?;
    let c = gio::DBusConnection::for_address_sync(
        &address,
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        gio::Cancellable::NONE,
    )?;
    let registrations = Arc::new(Mutex::new(Vec::<String>::new()));
    let items = registrations.clone();
    let info=gio::DBusNodeInfo::for_xml(r#"<node><interface name="org.kde.StatusNotifierWatcher"><method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method><method name="RegisterStatusNotifierHost"><arg type="s" direction="in"/></method><property name="RegisteredStatusNotifierItems" type="as" access="read"/><property name="IsStatusNotifierHostRegistered" type="b" access="read"/><property name="ProtocolVersion" type="i" access="read"/><signal name="StatusNotifierItemRegistered"><arg type="s"/></signal><signal name="StatusNotifierItemUnregistered"><arg type="s"/></signal><signal name="StatusNotifierHostRegistered"/></interface></node>"#)?.lookup_interface(WATCHER).unwrap();
    let list = registrations.clone();
    let _object = c
        .register_object(PATH, &info)
        .method_call(move |c, sender, _, _, method, args, inv| {
            if method == "RegisterStatusNotifierItem" {
                let service = args.child_value(0).str().unwrap_or("").to_string();
                let key = if service.starts_with('/') {
                    format!("{}{service}", sender.unwrap_or(""))
                } else {
                    format!("{service}/StatusNotifierItem")
                };
                let mut l = list.lock().unwrap();
                if !l.contains(&key) {
                    l.push(key.clone());
                    let _ = c.emit_signal(
                        None,
                        PATH,
                        WATCHER,
                        "StatusNotifierItemRegistered",
                        Some(&(key,).to_variant()),
                    );
                }
            }
            inv.return_value(None);
        })
        .property(move |_, _, _, _, name| match name {
            "RegisteredStatusNotifierItems" => items.lock().unwrap().clone().to_variant(),
            "IsStatusNotifierHostRegistered" => true.to_variant(),
            _ => 0i32.to_variant(),
        })
        .build()?;
    let result = call(
        &c,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
        Some(&(WATCHER, 4u32).to_variant()),
    )?;
    let mut own = result.child_value(0).get::<u32>() == Some(1);
    let host = format!("org.kde.StatusNotifierHost.macstill{}", std::process::id());
    let _ = call(
        &c,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
        Some(&(host.as_str(), 4u32).to_variant()),
    );
    let mut tick = 0u32;
    let mut menus = HashMap::<String, String>::new();
    let mut icons = HashMap::<String, (String, Option<Vec<u8>>)>::new();
    loop {
        while context.pending() {
            context.iteration(false);
        }
        while let Ok(command) = rx.try_recv() {
            if matches!(command, Command::Stop) {
                let _ = c.close_sync(gio::Cancellable::NONE);
                return Ok(());
            }
            let (key, method) = match &command {
                Command::Activate(k) => (k, "Activate"),
                Command::Secondary(k) => (k, "SecondaryActivate"),
                Command::Menu(k) => (k, "ContextMenu"),
                Command::Scroll(k, _, _) => (k, "Scroll"),
                Command::Event(k, _) | Command::Expand(k, _) => (k, ""),
                Command::Stop => unreachable!(),
                Command::Close => {
                    let mut s = state.lock().unwrap();
                    s.selected = None;
                    s.entries.clear();
                    continue;
                }
            };
            let (d, p) = endpoint(key);
            if let Command::Expand(_, id) = &command {
                if let Some(path) = menus.get(key) {
                    let _ = call(
                        &c,
                        d,
                        path,
                        "com.canonical.dbusmenu",
                        "AboutToShow",
                        Some(&(*id,).to_variant()),
                    );
                }
                continue;
            }
            if let Command::Event(_, id) = command {
                if let Some(path) = menus.get(key) {
                    let _ = call(
                        &c,
                        d,
                        path,
                        "com.canonical.dbusmenu",
                        "Event",
                        Some(&(id, "clicked", 0i32.to_variant(), 0u32).to_variant()),
                    );
                }
                continue;
            }
            if matches!(command, Command::Menu(_)) {
                if let Some(path) = menus.get(key) {
                    let _ = call(
                        &c,
                        d,
                        path,
                        "com.canonical.dbusmenu",
                        "AboutToShow",
                        Some(&(0i32,).to_variant()),
                    );
                    if let Ok(v) = call(
                        &c,
                        d,
                        path,
                        "com.canonical.dbusmenu",
                        "GetLayout",
                        Some(&(0i32, -1i32, Vec::<String>::new()).to_variant()),
                    ) {
                        if let Some(root) = layout(v.child_value(1), 0) {
                            let mut s = state.lock().unwrap();
                            s.selected = Some(key.clone());
                            s.entries = root.children;
                            continue;
                        }
                    }
                }
            }
            let args = if let Command::Scroll(_, delta, orientation) = &command {
                (*delta, orientation.as_str()).to_variant()
            } else {
                (0i32, 30i32).to_variant()
            };
            let _ = call(&c, d, p, "org.kde.StatusNotifierItem", method, Some(&args));
            if matches!(command, Command::Menu(_)) {
                let mut s = state.lock().unwrap();
                s.selected = Some(key.clone());
                s.entries = vec![Entry {
                    id: -1,
                    label: "Menu is provided by the application".into(),
                    enabled: false,
                    separator: false,
                    checked: None,
                    children: vec![],
                    ..Entry::default()
                }];
            }
        }
        if tick.is_multiple_of(20) {
            if !own {
                own = call(
                    &c,
                    "org.freedesktop.DBus",
                    "/org/freedesktop/DBus",
                    "org.freedesktop.DBus",
                    "RequestName",
                    Some(&(WATCHER, 4u32).to_variant()),
                )
                .ok()
                .and_then(|v| v.child_value(0).get::<u32>())
                .is_some_and(|v| v == 1 || v == 4);
                let _ = call(
                    &c,
                    WATCHER,
                    PATH,
                    WATCHER,
                    "RegisterStatusNotifierHost",
                    Some(&(host.as_str(),).to_variant()),
                );
            }
            let keys = if own {
                registrations.lock().unwrap().clone()
            } else {
                call(
                    &c,
                    WATCHER,
                    PATH,
                    "org.freedesktop.DBus.Properties",
                    "Get",
                    Some(&(WATCHER, "RegisteredStatusNotifierItems").to_variant()),
                )
                .ok()
                .and_then(|v| v.child_value(0).as_variant())
                .and_then(|v| v.get::<Vec<String>>())
                .unwrap_or_default()
            };
            let mut next = Vec::new();
            for key in keys {
                if let Some(p) = properties(&c, &key) {
                    let status = p.get("Status").and_then(|v| v.str()).unwrap_or("Active");
                    if status == "Passive" {
                        continue;
                    }
                    if let Some(path) = p.get("Menu").and_then(|v| v.str()) {
                        menus.insert(key.clone(), path.to_string());
                    }
                    let signature = format!(
                        "{:?}{:?}{:?}{:?}{:?}{:?}",
                        p.get("IconPixmap"),
                        p.get("IconName"),
                        p.get("IconThemePath"),
                        p.get("AttentionIconPixmap"),
                        p.get("AttentionIconName"),
                        status
                    );
                    let cached = icons
                        .entry(key.clone())
                        .or_insert_with(|| (String::new(), None));
                    if cached.0 != signature {
                        *cached = (
                            signature,
                            icon(&p, status == "NeedsAttention").or_else(|| icon(&p, false)),
                        );
                    }
                    next.push(Item {
                        title: p
                            .get("Title")
                            .and_then(|v| v.str())
                            .unwrap_or("Application")
                            .to_string(),
                        icon: cached.1.clone(),
                        menu: p.get("ItemIsMenu").and_then(|v| v.get()).unwrap_or(false),
                        exported_menu: menus.get(&key).is_some_and(|p| p != "/"),
                        key,
                    });
                } else if own {
                    let (destination, _) = endpoint(&key);
                    let alive = call(
                        &c,
                        "org.freedesktop.DBus",
                        "/org/freedesktop/DBus",
                        "org.freedesktop.DBus",
                        "NameHasOwner",
                        Some(&(destination,).to_variant()),
                    )
                    .ok()
                    .and_then(|v| v.child_value(0).get::<bool>())
                    .unwrap_or(true);
                    if alive {
                        if let Some(old) = state
                            .lock()
                            .unwrap()
                            .items
                            .iter()
                            .find(|i| i.key == key)
                            .cloned()
                        {
                            next.push(old);
                        }
                        continue;
                    }
                    registrations.lock().unwrap().retain(|k| k != &key);
                    let _ = c.emit_signal(
                        None,
                        PATH,
                        WATCHER,
                        "StatusNotifierItemUnregistered",
                        Some(&(key,).to_variant()),
                    );
                }
            }
            icons.retain(|key, _| next.iter().any(|i| &i.key == key));
            menus.retain(|key, _| next.iter().any(|i| &i.key == key));
            let selected = state.lock().unwrap().selected.clone();
            if let Some(key) = selected.as_ref() {
                if let Some(path) = menus.get(key) {
                    let (d, _) = endpoint(key);
                    if let Ok(v) = call(
                        &c,
                        d,
                        path,
                        "com.canonical.dbusmenu",
                        "GetLayout",
                        Some(&(0i32, -1i32, Vec::<String>::new()).to_variant()),
                    ) {
                        if let Some(root) = layout(v.child_value(1), 0) {
                            let mut s = state.lock().unwrap();
                            if s.selected.as_ref() == Some(key) {
                                s.entries = root.children;
                            }
                        }
                    }
                }
            }
            let mut s = state.lock().unwrap();
            s.items = next;
            if s.selected
                .as_ref()
                .is_some_and(|k| !s.items.iter().any(|i| &i.key == k))
            {
                s.selected = None;
                s.entries.clear();
            }
        }
        tick = tick.wrapping_add(1);
        std::thread::sleep(Duration::from_millis(50));
    }
}
#[cfg(test)]
impl Tray {
    pub(crate) fn fixture() -> Self {
        let (tx, _) = mpsc::channel();
        Self {
            state: Arc::new(Mutex::new(State::default())),
            tx,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints() {
        assert_eq!(
            endpoint(":1.2/org/example/Tray"),
            (":1.2", "/org/example/Tray")
        );
    }
    #[test]
    fn rejects_invalid_pixmaps() {
        assert!(pixmap(&vec![(50000i32, 50000i32, vec![0u8])].to_variant()).is_none());
    }
    #[test]
    fn parses_nested_menu() {
        let props = HashMap::from([("label", "_Open".to_variant())]);
        let v = (1i32, props, Vec::<glib::Variant>::new()).to_variant();
        assert_eq!(layout(v, 0).unwrap().label, "Open");
    }
}
