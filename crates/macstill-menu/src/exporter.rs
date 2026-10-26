//! One publisher per window; dropping it closes its bus connection and menu.
use crate::{Endpoint, Entry, dbus};
use gio::glib::{
    self,
    variant::{ObjectPath, ToVariant},
};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

const XML: &str = r#"<node><interface name="com.canonical.dbusmenu">
<method name="GetLayout"><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="as" direction="in"/><arg type="u" direction="out"/><arg type="(ia{sv}av)" direction="out"/></method>
<method name="GetGroupProperties"><arg type="ai" direction="in"/><arg type="as" direction="in"/><arg type="a(ia{sv})" direction="out"/></method>
<method name="GetProperty"><arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
<method name="Event"><arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="in"/><arg type="u" direction="in"/></method>
<method name="EventGroup"><arg type="a(isvu)" direction="in"/><arg type="ai" direction="out"/></method>
<method name="AboutToShow"><arg type="i" direction="in"/><arg type="b" direction="out"/></method>
<method name="AboutToShowGroup"><arg type="ai" direction="in"/><arg type="ai" direction="out"/><arg type="ai" direction="out"/></method>
<property name="Version" type="u" access="read"/><property name="TextDirection" type="s" access="read"/><property name="Status" type="s" access="read"/><property name="IconThemePath" type="as" access="read"/>
<signal name="LayoutUpdated"><arg type="u"/><arg type="i"/></signal>
<signal name="ItemsPropertiesUpdated"><arg type="a(ia{sv})"/><arg type="a(ias)"/></signal>
<signal name="ItemActivationRequested"><arg type="i"/><arg type="u"/></signal>
</interface></node>"#;
struct Model {
    revision: u32,
    root: Entry,
}
pub struct Publisher {
    model: Arc<Mutex<Model>>,
    stop: mpsc::Sender<()>,
    events: mpsc::Receiver<i32>,
    pub endpoint: Endpoint,
}
impl Publisher {
    /// X11 callers pass the actual XID. Wayland callers bind endpoint using wayland::Binding.
    pub fn new(root: Entry, xid: Option<u32>) -> anyhow::Result<Self> {
        root.validate()?;
        let model = Arc::new(Mutex::new(Model { revision: 1, root }));
        let shared = model.clone();
        let (stop, stopped) = mpsc::channel();
        let (commands, events) = mpsc::sync_channel(128);
        let (ready, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("menu-export".into())
            .spawn(move || {
                let context = glib::MainContext::new();
                let result = context.with_thread_default(|| -> anyhow::Result<()> {
                    let c = dbus::connect()?;
                    let path = "/org/macstill/Menu";
                    let info = gio::DBusNodeInfo::for_xml(XML)?
                        .lookup_interface(dbus::INTERFACE)
                        .unwrap();
                    let current = shared.clone();
                    let _registration = c
                        .register_object(path, &info)
                        .method_call(move |_, _, _, _, method, args, inv| {
                            let m = current.lock().unwrap();
                            let error = |inv: gio::DBusMethodInvocation| {
                                inv.return_dbus_error(
                                    "com.canonical.dbusmenu.Error.InvalidMenuItem",
                                    "Unknown menu item or property",
                                )
                            };
                            match method {
                                "GetLayout" => {
                                    let (id, depth, names) =
                                        args.get::<(i32, i32, Vec<String>)>().unwrap();
                                    if let Some(e) = m.root.find(id) {
                                        inv.return_value(Some(&glib::Variant::tuple_from_iter([
                                            m.revision.to_variant(),
                                            dbus::serialize(e, depth, &names),
                                        ])));
                                    } else {
                                        error(inv);
                                    }
                                }
                                "GetGroupProperties" => {
                                    let (ids, names) =
                                        args.get::<(Vec<i32>, Vec<String>)>().unwrap();
                                    let ids = if ids.is_empty() {
                                        all_ids(&m.root)
                                    } else {
                                        ids
                                    };
                                    let values: Vec<_> = ids
                                        .into_iter()
                                        .filter_map(|id| {
                                            m.root
                                                .find(id)
                                                .map(|e| (id, dbus::properties(e, &names)))
                                        })
                                        .collect();
                                    inv.return_value(Some(&(values,).to_variant()));
                                }
                                "GetProperty" => {
                                    let (id, name) = args.get::<(i32, String)>().unwrap();
                                    if let Some(v) = m
                                        .root
                                        .find(id)
                                        .and_then(|e| dbus::properties(e, &[]).remove(&name))
                                    {
                                        inv.return_value(Some(&(v,).to_variant()));
                                    } else {
                                        error(inv);
                                    }
                                }
                                "Event" => {
                                    let (id, event, _, _) =
                                        args.get::<(i32, String, glib::Variant, u32)>().unwrap();
                                    if m.root.find(id).is_none() {
                                        error(inv);
                                        return;
                                    }
                                    if event == "clicked" && m.root.actionable(id) {
                                        let _ = commands.try_send(id);
                                    }
                                    inv.return_value(None);
                                }
                                "EventGroup" => {
                                    let (events,) = args
                                        .get::<(Vec<(i32, String, glib::Variant, u32)>,)>()
                                        .unwrap();
                                    let mut errors = Vec::new();
                                    for (id, event, _, _) in events {
                                        if m.root.find(id).is_none() {
                                            errors.push(id);
                                        } else if event == "clicked" && m.root.actionable(id) {
                                            let _ = commands.try_send(id);
                                        }
                                    }
                                    inv.return_value(Some(&(errors,).to_variant()));
                                }
                                "AboutToShow" => {
                                    let (id,) = args.get::<(i32,)>().unwrap();
                                    if m.root.find(id).is_some() {
                                        inv.return_value(Some(&(false,).to_variant()));
                                    } else {
                                        error(inv);
                                    }
                                }
                                "AboutToShowGroup" => {
                                    let (ids,) = args.get::<(Vec<i32>,)>().unwrap();
                                    let missing: Vec<_> = ids
                                        .into_iter()
                                        .filter(|id| m.root.find(*id).is_none())
                                        .collect();
                                    inv.return_value(Some(
                                        &(Vec::<i32>::new(), missing).to_variant(),
                                    ));
                                }
                                _ => inv.return_dbus_error(
                                    "org.freedesktop.DBus.Error.UnknownMethod",
                                    "Unknown method",
                                ),
                            }
                        })
                        .property(|_, _, _, _, name| match name {
                            "Version" => 3u32.to_variant(),
                            "TextDirection" => "ltr".to_variant(),
                            "Status" => "normal".to_variant(),
                            _ => Vec::<String>::new().to_variant(),
                        })
                        .build()?;
                    let endpoint = Endpoint {
                        owner: c.unique_name().unwrap().to_string(),
                        path: path.into(),
                    };
                    let _ = ready.send(Ok(endpoint));
                    let mut revision = 1;
                    let mut registrar = None;
                    let mut ticks = 0u32;
                    loop {
                        match stopped.try_recv() {
                            Ok(()) | Err(mpsc::TryRecvError::Disconnected) => break,
                            _ => {}
                        }
                        while context.pending() {
                            context.iteration(false);
                        }
                        let next = shared.lock().unwrap().revision;
                        if next != revision {
                            revision = next;
                            c.emit_signal(
                                None,
                                path,
                                dbus::INTERFACE,
                                "LayoutUpdated",
                                Some(&(revision, 0i32).to_variant()),
                            )?;
                        }
                        if let Some(xid) = xid.filter(|_| ticks.is_multiple_of(50)) {
                            let owner = dbus::owner(&c, dbus::REGISTRAR);
                            if owner != registrar {
                                if let Some(owner) = owner.as_ref() {
                                    if dbus::call(
                                        &c,
                                        owner,
                                        dbus::REGISTRAR_PATH,
                                        dbus::REGISTRAR,
                                        "RegisterWindow",
                                        Some(
                                            &(xid, ObjectPath::try_from(path).unwrap())
                                                .to_variant(),
                                        ),
                                    )
                                    .is_ok()
                                    {
                                        registrar = Some(owner.clone());
                                    }
                                } else {
                                    registrar = None;
                                }
                            }
                        }
                        ticks = ticks.wrapping_add(1);
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    let _ = c.close_sync(gio::Cancellable::NONE);
                    Ok(())
                });
                if let Err(e) = result.unwrap_or_else(|e| Err(e.into())) {
                    let _ = ready.send(Err(e.to_string()));
                }
            })?;
        let endpoint = result
            .recv_timeout(Duration::from_secs(3))?
            .map_err(anyhow::Error::msg)?;
        Ok(Self {
            model,
            stop,
            events,
            endpoint,
        })
    }
    pub fn update(&self, root: Entry) -> anyhow::Result<()> {
        root.validate()?;
        let mut model = self.model.lock().unwrap();
        if model.root != root {
            model.root = root;
            model.revision = model.revision.wrapping_add(1);
        }
        Ok(())
    }
    pub fn try_event(&self) -> Option<i32> {
        self.events.try_recv().ok()
    }
}
impl Drop for Publisher {
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}
fn all_ids(e: &Entry) -> Vec<i32> {
    std::iter::once(e.id)
        .chain(e.children.iter().flat_map(all_ids))
        .collect()
}
