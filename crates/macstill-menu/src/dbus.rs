use crate::{Endpoint, Entry};
use gio::glib::{self, variant::ToVariant};
use std::collections::HashMap;
pub const INTERFACE: &str = "com.canonical.dbusmenu";
pub const REGISTRAR: &str = "com.canonical.AppMenu.Registrar";
pub const REGISTRAR_PATH: &str = "/com/canonical/AppMenu/Registrar";
pub type Properties = HashMap<String, glib::Variant>;

pub fn connect() -> Result<gio::DBusConnection, glib::Error> {
    let address =
        gio::dbus_address_get_for_bus_sync(gio::BusType::Session, gio::Cancellable::NONE)?;
    gio::DBusConnection::for_address_sync(
        &address,
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        gio::Cancellable::NONE,
    )
}
pub fn call(
    c: &gio::DBusConnection,
    owner: &str,
    path: &str,
    interface: &str,
    method: &str,
    args: Option<&glib::Variant>,
) -> Result<glib::Variant, glib::Error> {
    if !gio::dbus_is_name(owner) || !glib::Variant::is_object_path(path) {
        return Err(glib::Error::new(
            gio::IOErrorEnum::InvalidArgument,
            "Invalid menu endpoint",
        ));
    }
    c.call_sync(
        Some(owner),
        path,
        interface,
        method,
        args,
        None,
        gio::DBusCallFlags::NO_AUTO_START,
        500,
        gio::Cancellable::NONE,
    )
}
pub fn owner(c: &gio::DBusConnection, name: &str) -> Option<String> {
    call(
        c,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetNameOwner",
        Some(&(name,).to_variant()),
    )
    .ok()?
    .child_value(0)
    .get()
}
pub fn layout(v: glib::Variant, depth: u8) -> Option<Entry> {
    let mut budget = 4096;
    parse(v, depth, &mut budget)
}
fn parse(v: glib::Variant, depth: u8, budget: &mut usize) -> Option<Entry> {
    if depth > 12 || *budget == 0 {
        return None;
    }
    *budget -= 1;
    let (id, p, children) = v.get::<(i32, Properties, Vec<glib::Variant>)>()?;
    if p.get("visible").and_then(|v| v.get::<bool>()) == Some(false) {
        return None;
    }
    let children: Vec<_> = children
        .into_iter()
        .filter_map(|v| parse(v, depth + 1, budget))
        .collect();
    let toggle = p.get("toggle-type").and_then(|v| v.str());
    Some(Entry {
        id,
        label: p
            .get("label")
            .and_then(|v| v.str())
            .unwrap_or("")
            .replace("__", "\u{1}")
            .replace('_', "")
            .replace('\u{1}', "_"),
        enabled: p.get("enabled").and_then(|v| v.get()).unwrap_or(true),
        separator: p.get("type").and_then(|v| v.str()) == Some("separator"),
        checked: toggle
            .filter(|s| !s.is_empty())
            .map(|_| p.get("toggle-state").and_then(|v| v.get::<i32>()) == Some(1)),
        radio: toggle == Some("radio"),
        submenu: !children.is_empty()
            || p.get("children-display").and_then(|v| v.str()) == Some("submenu"),
        shortcut: p.get("shortcut").and_then(|v| v.get()).unwrap_or_default(),
        children,
    })
}
pub fn read(c: &gio::DBusConnection, endpoint: &Endpoint) -> anyhow::Result<(u32, Entry)> {
    let v = call(
        c,
        &endpoint.owner,
        &endpoint.path,
        INTERFACE,
        "GetLayout",
        Some(&(0i32, -1i32, Vec::<String>::new()).to_variant()),
    )?;
    anyhow::ensure!(
        v.type_().as_str() == "(u(ia{sv}av))",
        "Invalid menu layout signature"
    );
    let root = layout(v.child_value(1), 0).ok_or_else(|| anyhow::anyhow!("Invalid menu layout"))?;
    root.validate()?;
    Ok((v.child_value(0).get().unwrap(), root))
}
pub fn show(c: &gio::DBusConnection, endpoint: &Endpoint, id: i32) {
    let _ = call(
        c,
        &endpoint.owner,
        &endpoint.path,
        INTERFACE,
        "AboutToShow",
        Some(&(id,).to_variant()),
    );
}
pub fn event(c: &gio::DBusConnection, endpoint: &Endpoint, id: i32, event: &str) {
    let _ = call(
        c,
        &endpoint.owner,
        &endpoint.path,
        INTERFACE,
        "Event",
        Some(&(id, event, 0i32.to_variant(), 0u32).to_variant()),
    );
}
pub fn click(c: &gio::DBusConnection, endpoint: &Endpoint, id: i32) {
    event(c, endpoint, id, "clicked");
}
pub fn properties(e: &Entry, names: &[String]) -> Properties {
    let mut p = Properties::from([
        ("label".into(), e.label.replace('_', "__").to_variant()),
        ("enabled".into(), e.enabled.to_variant()),
        ("visible".into(), true.to_variant()),
        (
            "type".into(),
            (if e.separator { "separator" } else { "standard" }).to_variant(),
        ),
    ]);
    if e.submenu {
        p.insert("children-display".into(), "submenu".to_variant());
    }
    if let Some(checked) = e.checked {
        p.insert(
            "toggle-type".into(),
            (if e.radio { "radio" } else { "checkmark" }).to_variant(),
        );
        p.insert("toggle-state".into(), i32::from(checked).to_variant());
    }
    if !e.shortcut.is_empty() {
        p.insert("shortcut".into(), e.shortcut.to_variant());
    }
    if !names.is_empty() {
        p.retain(|name, _| names.contains(name));
    }
    p
}
pub fn serialize(e: &Entry, depth: i32, names: &[String]) -> glib::Variant {
    let children: Vec<_> = if depth == 0 {
        vec![]
    } else {
        e.children
            .iter()
            .map(|child| serialize(child, if depth < 0 { -1 } else { depth - 1 }, names))
            .collect()
    };
    (e.id, properties(e, names), children).to_variant()
}
