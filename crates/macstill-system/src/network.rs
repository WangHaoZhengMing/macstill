use crate::{dbus::*, error::*, model::*};
use gio::glib::variant::{ObjectPath, ToVariant};
use std::collections::HashMap;
pub(crate) const SERVICE: &str = "org.freedesktop.NetworkManager";
const ROOT: &str = "/org/freedesktop/NetworkManager";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
pub(crate) const CONNECTIONS: &str = "/org/freedesktop/NetworkManager/Settings";
pub(crate) const SETTINGS: &str = "org.freedesktop.NetworkManager.Settings";
pub struct Network<'a, T: Transport>(pub &'a T);
impl<T: Transport> Network<'_, T> {
    pub fn snapshot(&self) -> Result<NetworkState> {
        let p = get_all(self.0, Bus::System, SERVICE, ROOT, SERVICE)?;
        let mut warnings = vec![];
        let paths: Vec<ObjectPath> = field(&p, "Devices")?;
        let mut devices = vec![];
        for path in paths {
            let props = match get_all(self.0, Bus::System, SERVICE, path.as_str(), DEVICE) {
                Ok(p) => p,
                Err(e) => {
                    warnings.push(e);
                    continue;
                }
            };
            let kind = field(&props, "DeviceType")?;
            let active: ObjectPath = field(&props, "ActiveConnection")?;
            let mut access_points = vec![];
            let mut active_access_point = None;
            let mut active_profile = None;
            if kind == 2 {
                match self.access_points(path.as_str()) {
                    Ok(a) => access_points = a,
                    Err(e) => warnings.push(e),
                }
                match get_all(self.0, Bus::System, SERVICE, path.as_str(), WIRELESS) {
                    Ok(properties) => {
                        active_access_point =
                            optional::<ObjectPath>(&properties, "ActiveAccessPoint")
                                .filter(|path| path.as_str() != "/")
                                .map(|path| path.to_string());
                    }
                    Err(error) => warnings.push(error),
                }
                if active.as_str() != "/" {
                    match get_all(
                        self.0,
                        Bus::System,
                        SERVICE,
                        active.as_str(),
                        "org.freedesktop.NetworkManager.Connection.Active",
                    ) {
                        Ok(properties) => {
                            active_profile = optional::<ObjectPath>(&properties, "Connection")
                                .filter(|path| path.as_str() != "/")
                                .map(|path| path.to_string());
                        }
                        Err(error) => warnings.push(error),
                    }
                }
            }
            devices.push(NetworkDevice {
                path: path.to_string(),
                interface: field(&props, "Interface")?,
                kind,
                state: field(&props, "State")?,
                active_connection: (active.as_str() != "/").then(|| active.to_string()),
                active_profile,
                active_access_point,
                access_points,
            });
        }
        let saved_connections = match self.saved_connections() {
            Ok(v) => v,
            Err(e) => {
                warnings.push(e);
                vec![]
            }
        };
        let permissions = match self
            .0
            .call(
                Bus::System,
                SERVICE,
                ROOT,
                SERVICE,
                "GetPermissions",
                ().to_variant(),
                false,
            )
            .and_then(|v| decode::<(HashMap<String, String>,)>(v, "GetPermissions"))
        {
            Ok(v) => v.0,
            Err(e) => {
                warnings.push(e);
                HashMap::new()
            }
        };
        Ok(NetworkState {
            enabled: field(&p, "NetworkingEnabled")?,
            wifi_enabled: field(&p, "WirelessEnabled")?,
            wifi_hardware_enabled: field(&p, "WirelessHardwareEnabled")?,
            connectivity: field(&p, "Connectivity")?,
            state: field(&p, "State")?,
            devices,
            saved_connections,
            permissions,
            warnings,
        })
    }
    pub fn access_points(&self, device: &str) -> Result<Vec<AccessPoint>> {
        let device = path(device, "/org/freedesktop/NetworkManager/Devices/")?;
        let (paths,): (Vec<ObjectPath>,) = decode(
            self.0.call(
                Bus::System,
                SERVICE,
                device.as_str(),
                WIRELESS,
                "GetAllAccessPoints",
                ().to_variant(),
                false,
            )?,
            "GetAllAccessPoints",
        )?;
        let mut points = vec![];
        for path in paths {
            let p = match get_all(
                self.0,
                Bus::System,
                SERVICE,
                path.as_str(),
                "org.freedesktop.NetworkManager.AccessPoint",
            ) {
                Ok(p) => p,
                Err(e) if e.kind == ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            let bytes: Vec<u8> = field(&p, "Ssid")?;
            points.push(AccessPoint {
                path: path.to_string(),
                ssid: String::from_utf8_lossy(&bytes).into_owned(),
                ssid_bytes: bytes,
                strength: field(&p, "Strength")?,
                flags: field(&p, "Flags")?,
                wpa_flags: field(&p, "WpaFlags")?,
                rsn_flags: field(&p, "RsnFlags")?,
            });
        }
        points.sort_by_key(|p| std::cmp::Reverse(p.strength));
        Ok(points)
    }
    pub fn saved_connections(&self) -> Result<Vec<SavedConnection>> {
        let (paths,): (Vec<ObjectPath>,) = decode(
            self.0.call(
                Bus::System,
                SERVICE,
                CONNECTIONS,
                SETTINGS,
                "ListConnections",
                ().to_variant(),
                false,
            )?,
            "ListConnections",
        )?;
        let mut profiles = vec![];
        for path in paths {
            // GetSettings intentionally excludes secrets. GetSecrets is never called.
            let (settings,): (HashMap<String, Properties>,) = decode(
                self.0.call(
                    Bus::System,
                    SERVICE,
                    path.as_str(),
                    "org.freedesktop.NetworkManager.Settings.Connection",
                    "GetSettings",
                    ().to_variant(),
                    false,
                )?,
                "GetSettings",
            )?;
            let p = settings
                .get("connection")
                .ok_or_else(|| SystemError::protocol("connection settings"))?;
            profiles.push(SavedConnection {
                path: path.to_string(),
                name: field(p, "id")?,
                kind: field(p, "type")?,
                uuid: field(p, "uuid")?,
            });
        }
        Ok(profiles)
    }
    pub fn set_enabled(&self, enabled: bool) -> Result<()> {
        unit(
            self.0.call(
                Bus::System,
                SERVICE,
                ROOT,
                SERVICE,
                "Enable",
                (enabled,).to_variant(),
                true,
            )?,
            "Enable",
        )
    }
    pub fn set_wifi(&self, enabled: bool) -> Result<()> {
        set(
            self.0,
            Bus::System,
            SERVICE,
            ROOT,
            SERVICE,
            "WirelessEnabled",
            enabled.to_variant(),
        )
    }
    pub fn scan(&self, device: &str) -> Result<()> {
        let device = path(device, "/org/freedesktop/NetworkManager/Devices/")?;
        let options: Properties = HashMap::new();
        unit(
            self.0.call(
                Bus::System,
                SERVICE,
                device.as_str(),
                WIRELESS,
                "RequestScan",
                (options,).to_variant(),
                true,
            )?,
            "RequestScan",
        )
    }
    /// Returns an activation object, not a claim that DHCP/authentication finished.
    pub fn activate(&self, connection: &str, device: &str) -> Result<String> {
        let connection = path(connection, "/org/freedesktop/NetworkManager/Settings/")?;
        let device = path(device, "/org/freedesktop/NetworkManager/Devices/")?;
        let specific = ObjectPath::try_from("/").unwrap();
        let (active,): (ObjectPath,) = decode(
            self.0.call(
                Bus::System,
                SERVICE,
                ROOT,
                SERVICE,
                "ActivateConnection",
                (connection, device, specific).to_variant(),
                true,
            )?,
            "ActivateConnection",
        )?;
        Ok(active.to_string())
    }
    pub fn deactivate(&self, active: &str) -> Result<()> {
        let active = path(active, "/org/freedesktop/NetworkManager/ActiveConnection/")?;
        unit(
            self.0.call(
                Bus::System,
                SERVICE,
                ROOT,
                SERVICE,
                "DeactivateConnection",
                (active,).to_variant(),
                true,
            )?,
            "DeactivateConnection",
        )
    }
}
