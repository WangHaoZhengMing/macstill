//! Bounded, typed GDBus calls; no shell commands or root execution.
use crate::error::{Result, SystemError};
use gio::glib::{
    Variant,
    variant::{FromVariant, ToVariant},
};
use std::collections::HashMap;
pub type Properties = HashMap<String, Variant>;
pub type Interfaces = HashMap<String, Properties>;
pub type Objects = HashMap<gio::glib::variant::ObjectPath, Interfaces>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bus {
    System,
    Session,
}
/// Public to allow isolated fake services/transports in consumers' tests.
#[allow(clippy::too_many_arguments)] // Mirrors the D-Bus message envelope.
pub trait Transport {
    fn call(
        &self,
        bus: Bus,
        service: &str,
        path: &str,
        interface: &str,
        method: &str,
        args: Variant,
        interactive: bool,
    ) -> Result<Variant>;
}
#[derive(Default)]
pub struct GioTransport {
    system: std::cell::RefCell<Option<gio::DBusConnection>>,
    session: std::cell::RefCell<Option<gio::DBusConnection>>,
}
impl GioTransport {
    /// Connect to explicitly supplied buses, including isolated peer test servers.
    pub fn from_connections(
        system: Option<gio::DBusConnection>,
        session: Option<gio::DBusConnection>,
    ) -> Self {
        Self {
            system: std::cell::RefCell::new(system),
            session: std::cell::RefCell::new(session),
        }
    }
}
impl Transport for GioTransport {
    fn call(
        &self,
        bus: Bus,
        service: &str,
        path: &str,
        interface: &str,
        method: &str,
        args: Variant,
        interactive: bool,
    ) -> Result<Variant> {
        let operation = format!("{interface}.{method}");
        let slot = match bus {
            Bus::System => &self.system,
            Bus::Session => &self.session,
        };
        if slot.borrow().is_none() {
            let connection = gio::bus_get_sync(
                match bus {
                    Bus::System => gio::BusType::System,
                    Bus::Session => gio::BusType::Session,
                },
                None::<&gio::Cancellable>,
            )
            .map_err(|e| SystemError::dbus(&operation, e))?;
            *slot.borrow_mut() = Some(connection);
        }
        let borrowed = slot.borrow();
        let connection = borrowed.as_ref().unwrap();
        connection
            .call_sync(
                Some(service),
                path,
                interface,
                method,
                Some(&args),
                None,
                if interactive {
                    gio::DBusCallFlags::ALLOW_INTERACTIVE_AUTHORIZATION
                } else {
                    gio::DBusCallFlags::NONE
                },
                if interactive { 60_000 } else { 3_000 },
                None::<&gio::Cancellable>,
            )
            .map_err(|e| SystemError::dbus(&operation, e))
    }
}
pub(crate) fn get_all(
    t: &impl Transport,
    bus: Bus,
    service: &str,
    path: &str,
    interface: &str,
) -> Result<Properties> {
    decode::<(Properties,)>(
        t.call(
            bus,
            service,
            path,
            "org.freedesktop.DBus.Properties",
            "GetAll",
            (interface,).to_variant(),
            false,
        )?,
        "GetAll",
    )
    .map(|v| v.0)
}
pub(crate) fn set(
    t: &impl Transport,
    bus: Bus,
    service: &str,
    path: &str,
    interface: &str,
    property: &str,
    value: Variant,
) -> Result<()> {
    unit(
        t.call(
            bus,
            service,
            path,
            "org.freedesktop.DBus.Properties",
            "Set",
            (interface, property, value).to_variant(),
            true,
        )?,
        "Properties.Set",
    )
}
pub(crate) fn decode<T: FromVariant>(value: Variant, op: &str) -> Result<T> {
    value.get().ok_or_else(|| SystemError::protocol(op))
}
pub(crate) fn unit(value: Variant, op: &str) -> Result<()> {
    decode(value, op)
}
pub(crate) fn field<T: FromVariant>(props: &Properties, key: &str) -> Result<T> {
    props
        .get(key)
        .and_then(Variant::get)
        .ok_or_else(|| SystemError::protocol(key))
}
pub(crate) fn optional<T: FromVariant>(props: &Properties, key: &str) -> Option<T> {
    props.get(key).and_then(Variant::get)
}
pub(crate) fn path(value: &str, prefix: &str) -> Result<gio::glib::variant::ObjectPath> {
    if !value.starts_with(prefix) || value.len() <= prefix.len() {
        return Err(SystemError::invalid(
            "object path",
            "Select an object reported by the service",
        ));
    }
    gio::glib::variant::ObjectPath::try_from(value)
        .map_err(|_| SystemError::invalid("object path", "Invalid D-Bus object path"))
}
