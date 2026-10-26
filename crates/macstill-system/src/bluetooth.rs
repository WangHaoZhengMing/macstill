use crate::{dbus::*, error::*, model::*};
use gio::glib::variant::ToVariant;
const SERVICE: &str = "org.bluez";
const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";
pub struct Bluetooth<'a, T: Transport>(pub &'a T);
impl<T: Transport> Bluetooth<'_, T> {
    pub fn snapshot(&self) -> Result<BluetoothState> {
        let (objects,): (Objects,) = decode(
            self.0.call(
                Bus::System,
                SERVICE,
                "/",
                "org.freedesktop.DBus.ObjectManager",
                "GetManagedObjects",
                ().to_variant(),
                false,
            )?,
            "BlueZ.GetManagedObjects",
        )?;
        let mut adapters = vec![];
        let mut devices = vec![];
        for (path, interfaces) in objects {
            if let Some(p) = interfaces.get(ADAPTER) {
                adapters.push(BluetoothAdapter {
                    path: path.to_string(),
                    name: optional(p, "Alias")
                        .or_else(|| optional(p, "Name"))
                        .unwrap_or_else(|| "Bluetooth".into()),
                    powered: field(p, "Powered")?,
                    discovering: field(p, "Discovering")?,
                    discoverable: field(p, "Discoverable")?,
                    pairable: optional(p, "Pairable"),
                    discoverable_timeout_seconds: optional(p, "DiscoverableTimeout"),
                });
            }
            if let Some(p) = interfaces.get(DEVICE) {
                let adapter: gio::glib::variant::ObjectPath = field(p, "Adapter")?;
                devices.push(BluetoothDevice {
                    path: path.to_string(),
                    adapter: adapter.to_string(),
                    name: optional(p, "Alias")
                        .or_else(|| optional(p, "Name"))
                        .unwrap_or_else(|| "Bluetooth device".into()),
                    paired: field(p, "Paired")?,
                    connected: field(p, "Connected")?,
                    trusted: field(p, "Trusted")?,
                    rssi: optional(p, "RSSI"),
                    blocked: optional(p, "Blocked"),
                });
            }
        }
        adapters.sort_by(|a, b| a.path.cmp(&b.path));
        devices.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(BluetoothState { adapters, devices })
    }
    fn adapter_path(&self, adapter: &str) -> Result<()> {
        path(adapter, "/org/bluez/")?;
        if self.snapshot()?.adapters.iter().any(|a| a.path == adapter) {
            Ok(())
        } else {
            Err(SystemError::new(
                ErrorKind::NotFound,
                "Bluetooth adapter",
                "Adapter is no longer available",
            ))
        }
    }
    fn device_path(&self, device: &str) -> Result<()> {
        path(device, "/org/bluez/")?;
        if self.snapshot()?.devices.iter().any(|d| d.path == device) {
            Ok(())
        } else {
            Err(SystemError::new(
                ErrorKind::NotFound,
                "Bluetooth device",
                "Device is no longer available",
            ))
        }
    }
    pub fn set_powered(&self, adapter: &str, powered: bool) -> Result<()> {
        self.adapter_path(adapter)?;
        set(
            self.0,
            Bus::System,
            SERVICE,
            adapter,
            ADAPTER,
            "Powered",
            powered.to_variant(),
        )
    }
    pub fn start_discovery(&self, adapter: &str) -> Result<()> {
        self.adapter_path(adapter)?;
        unit(
            self.0.call(
                Bus::System,
                SERVICE,
                adapter,
                ADAPTER,
                "StartDiscovery",
                ().to_variant(),
                false,
            )?,
            "StartDiscovery",
        )
    }
    /// Call on the same SystemApi/connection which started this discovery session.
    pub fn stop_discovery(&self, adapter: &str) -> Result<()> {
        self.adapter_path(adapter)?;
        unit(
            self.0.call(
                Bus::System,
                SERVICE,
                adapter,
                ADAPTER,
                "StopDiscovery",
                ().to_variant(),
                false,
            )?,
            "StopDiscovery",
        )
    }
    fn device_call(&self, device: &str, method: &str) -> Result<()> {
        self.device_path(device)?;
        unit(
            self.0.call(
                Bus::System,
                SERVICE,
                device,
                DEVICE,
                method,
                ().to_variant(),
                true,
            )?,
            method,
        )
    }
    pub fn connect(&self, device: &str) -> Result<()> {
        self.device_call(device, "Connect")
    }
    pub fn disconnect(&self, device: &str) -> Result<()> {
        self.device_call(device, "Disconnect")
    }
    /// Uses BlueZ's default pairing agent; never auto-confirms passkeys or trusts a device.
    pub fn pair(&self, device: &str) -> Result<()> {
        self.device_call(device, "Pair")
    }
    pub fn cancel_pairing(&self, device: &str) -> Result<()> {
        self.device_call(device, "CancelPairing")
    }
    pub fn set_trusted(&self, device: &str, trusted: bool) -> Result<()> {
        self.device_path(device)?;
        set(
            self.0,
            Bus::System,
            SERVICE,
            device,
            DEVICE,
            "Trusted",
            trusted.to_variant(),
        )
    }
    /// Removes the device AND its pairing keys using the adapter reported by BlueZ.
    pub fn remove_device(&self, device: &str) -> Result<()> {
        let device_path = path(device, "/org/bluez/")?;
        let state = self.snapshot()?;
        let entry = state
            .devices
            .iter()
            .find(|d| d.path == device)
            .ok_or_else(|| {
                SystemError::new(
                    ErrorKind::NotFound,
                    "Bluetooth device",
                    "Device is no longer available",
                )
            })?;
        path(&entry.adapter, "/org/bluez/")?;
        unit(
            self.0.call(
                Bus::System,
                SERVICE,
                &entry.adapter,
                ADAPTER,
                "RemoveDevice",
                (device_path,).to_variant(),
                true,
            )?,
            "RemoveDevice",
        )
    }
    pub fn set_discoverable(&self, adapter: &str, discoverable: bool) -> Result<()> {
        self.adapter_property(adapter, "Discoverable", discoverable.to_variant())
    }
    /// Zero means indefinitely discoverable, according to BlueZ semantics.
    pub fn set_discoverable_timeout(&self, adapter: &str, seconds: u32) -> Result<()> {
        self.adapter_property(adapter, "DiscoverableTimeout", seconds.to_variant())
    }
    pub fn set_pairable(&self, adapter: &str, pairable: bool) -> Result<()> {
        self.adapter_property(adapter, "Pairable", pairable.to_variant())
    }
    pub fn set_adapter_alias(&self, adapter: &str, alias: &str) -> Result<()> {
        validate_alias(alias)?;
        self.adapter_property(adapter, "Alias", alias.to_variant())
    }
    pub fn set_device_alias(&self, device: &str, alias: &str) -> Result<()> {
        validate_alias(alias)?;
        self.device_path(device)?;
        set(
            self.0,
            Bus::System,
            SERVICE,
            device,
            DEVICE,
            "Alias",
            alias.to_variant(),
        )
    }
    pub fn set_blocked(&self, device: &str, blocked: bool) -> Result<()> {
        self.device_path(device)?;
        set(
            self.0,
            Bus::System,
            SERVICE,
            device,
            DEVICE,
            "Blocked",
            blocked.to_variant(),
        )
    }
    fn adapter_property(
        &self,
        adapter: &str,
        property: &str,
        value: gio::glib::Variant,
    ) -> Result<()> {
        self.adapter_path(adapter)?;
        set(
            self.0,
            Bus::System,
            SERVICE,
            adapter,
            ADAPTER,
            property,
            value,
        )
    }
}
fn validate_alias(alias: &str) -> Result<()> {
    if alias.len() > 248 || alias.chars().any(char::is_control) {
        Err(SystemError::invalid(
            "Bluetooth alias",
            "Use at most 248 UTF-8 bytes without control characters; empty resets the alias",
        ))
    } else {
        Ok(())
    }
}
