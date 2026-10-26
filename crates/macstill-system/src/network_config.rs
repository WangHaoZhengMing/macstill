//! Typed profile editing. GetSettings never asks for secrets. NetworkManager
//! retains existing secrets when Update contains no secrets at all.
use crate::{
    dbus::*,
    error::*,
    model::*,
    network::{CONNECTIONS, Network, SERVICE, SETTINGS},
};
use gio::glib::variant::{ObjectPath, ToVariant};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};
const CONNECTION: &str = "org.freedesktop.NetworkManager.Settings.Connection";
type Settings = HashMap<String, Properties>;

impl<T: Transport> Network<'_, T> {
    fn connection_settings(&self, connection: &str) -> Result<Settings> {
        let connection = path(connection, "/org/freedesktop/NetworkManager/Settings/")?;
        decode::<(Settings,)>(
            self.0.call(
                Bus::System,
                SERVICE,
                connection.as_str(),
                CONNECTION,
                "GetSettings",
                ().to_variant(),
                false,
            )?,
            "GetSettings",
        )
        .map(|v| v.0)
    }
    /// Read persistent configuration, not the active lease; never includes passwords.
    pub fn connection(&self, connection: &str) -> Result<ConnectionDetails> {
        let settings = self.connection_settings(connection)?;
        let p = settings
            .get("connection")
            .ok_or_else(|| SystemError::protocol("connection"))?;
        Ok(ConnectionDetails {
            path: connection.into(),
            name: field(p, "id")?,
            uuid: field(p, "uuid")?,
            kind: field(p, "type")?,
            autoconnect: optional(p, "autoconnect").unwrap_or(true),
            ipv4: settings
                .get("ipv4")
                .map(|p| read_ip(p, IpFamily::Ipv4))
                .transpose()?,
            ipv6: settings
                .get("ipv6")
                .map(|p| read_ip(p, IpFamily::Ipv6))
                .transpose()?,
        })
    }
    /// Saves a profile. Autoconnect=true may cause NetworkManager to activate it.
    /// The returned path can also be explicitly passed to activate().
    pub fn add_wifi(&self, config: &WifiConnection) -> Result<String> {
        if config.name.is_empty()
            || config.name.len() > 256
            || config.name.chars().any(char::is_control)
            || config.ssid.is_empty()
            || config.ssid.len() > 32
        {
            return Err(SystemError::invalid(
                "Wi-Fi profile",
                "Use a profile name (1–256 bytes) and SSID (1–32 bytes)",
            ));
        }
        let mut settings = Settings::from([
            (
                "connection".into(),
                Properties::from([
                    ("id".into(), config.name.to_variant()),
                    ("type".into(), "802-11-wireless".to_variant()),
                    ("autoconnect".into(), config.autoconnect.to_variant()),
                ]),
            ),
            (
                "802-11-wireless".into(),
                Properties::from([
                    ("ssid".into(), config.ssid.to_variant()),
                    ("mode".into(), "infrastructure".to_variant()),
                    ("hidden".into(), config.hidden.to_variant()),
                ]),
            ),
            (
                "ipv4".into(),
                Properties::from([("method".into(), "auto".to_variant())]),
            ),
            (
                "ipv6".into(),
                Properties::from([("method".into(), "auto".to_variant())]),
            ),
        ]);
        let secret = match &config.security {
            WifiSecurity::Open {} => None,
            WifiSecurity::WpaPsk { password } => {
                let passphrase = (8..=63).contains(&password.len())
                    && password.bytes().all(|c| (32..=126).contains(&c));
                let raw_key =
                    password.len() == 64 && password.bytes().all(|c| c.is_ascii_hexdigit());
                if !passphrase && !raw_key {
                    return Err(SystemError::invalid(
                        "Wi-Fi password",
                        "WPA-PSK requires 8–63 printable ASCII bytes or 64 hexadecimal digits",
                    ));
                }
                Some(("wpa-psk", password))
            }
            WifiSecurity::Sae { password } => {
                if password.is_empty()
                    || password.len() > 1024
                    || password.chars().any(char::is_control)
                {
                    return Err(SystemError::invalid(
                        "Wi-Fi password",
                        "SAE requires a nonempty passphrase of at most 1024 UTF-8 bytes without control characters",
                    ));
                }
                Some(("sae", password))
            }
        };
        if let Some((key_mgmt, password)) = secret {
            settings.insert(
                "802-11-wireless-security".into(),
                Properties::from([
                    ("key-mgmt".into(), key_mgmt.to_variant()),
                    ("psk".into(), password.to_variant()),
                    // Explicitly system-owned: NM stores this under its own policy.
                    ("psk-flags".into(), 0u32.to_variant()),
                ]),
            );
        }
        let (connection,): (ObjectPath,) = decode(
            self.0.call(
                Bus::System,
                SERVICE,
                CONNECTIONS,
                SETTINGS,
                "AddConnection",
                (settings,).to_variant(),
                true,
            )?,
            "AddConnection",
        )?;
        Ok(connection.to_string())
    }
    /// Deletes the saved profile and its stored credentials, not just a connection.
    pub fn forget_connection(&self, connection: &str) -> Result<()> {
        let connection = path(connection, "/org/freedesktop/NetworkManager/Settings/")?;
        unit(
            self.0.call(
                Bus::System,
                SERVICE,
                connection.as_str(),
                CONNECTION,
                "Delete",
                ().to_variant(),
                true,
            )?,
            "Delete connection",
        )
    }
    pub fn set_autoconnect(&self, connection: &str, enabled: bool) -> Result<()> {
        let mut settings = self.connection_settings(connection)?;
        settings
            .get_mut("connection")
            .ok_or_else(|| SystemError::protocol("connection"))?
            .insert("autoconnect".into(), enabled.to_variant());
        self.update_connection(connection, settings)
    }
    /// Persists configuration. Re-activate the profile for IP changes to take
    /// effect; this never disconnects the current network as a hidden side effect.
    pub fn set_ip(
        &self,
        connection: &str,
        family: IpFamily,
        config: &IpConfiguration,
    ) -> Result<()> {
        validate_ip(family, config)?;
        let mut settings = self.connection_settings(connection)?;
        let p = settings
            .entry(
                match family {
                    IpFamily::Ipv4 => "ipv4",
                    IpFamily::Ipv6 => "ipv6",
                }
                .into(),
            )
            .or_default();
        // Remove legacy representations so they cannot override modern fields.
        for key in [
            "addresses",
            "address-data",
            "gateway",
            "dns",
            "dns-data",
            "dns-search",
            "ignore-auto-dns",
        ] {
            p.remove(key);
        }
        p.insert("method".into(), config.method.to_variant());
        let addresses: Vec<Properties> = config
            .addresses
            .iter()
            .map(|a| {
                Properties::from([
                    ("address".into(), a.address.to_variant()),
                    ("prefix".into(), a.prefix.to_variant()),
                ])
            })
            .collect();
        p.insert("address-data".into(), addresses.to_variant());
        if let Some(gateway) = &config.gateway {
            p.insert("gateway".into(), gateway.to_variant());
        }
        // Legacy DNS wire formats work on both old and current NM versions.
        // Validation above makes these conversions infallible.
        let dns = match family {
            IpFamily::Ipv4 => config
                .dns
                .iter()
                .map(|ip| u32::from_ne_bytes(ip.parse::<Ipv4Addr>().unwrap().octets()))
                .collect::<Vec<_>>()
                .to_variant(),
            IpFamily::Ipv6 => config
                .dns
                .iter()
                .map(|ip| ip.parse::<Ipv6Addr>().unwrap().octets().to_vec())
                .collect::<Vec<_>>()
                .to_variant(),
        };
        p.insert("dns".into(), dns);
        p.insert("dns-search".into(), config.dns_search.to_variant());
        p.insert(
            "ignore-auto-dns".into(),
            config.ignore_auto_dns.to_variant(),
        );
        self.update_connection(connection, settings)
    }
    fn update_connection(&self, connection: &str, settings: Settings) -> Result<()> {
        unit(
            self.0.call(
                Bus::System,
                SERVICE,
                connection,
                CONNECTION,
                "Update",
                (settings,).to_variant(),
                true,
            )?,
            "Update connection",
        )
    }
}
fn validate_ip(family: IpFamily, config: &IpConfiguration) -> Result<()> {
    let valid_address = |s: &str| match s.parse::<IpAddr>() {
        Ok(IpAddr::V4(_)) => family == IpFamily::Ipv4,
        Ok(IpAddr::V6(_)) => family == IpFamily::Ipv6,
        Err(_) => false,
    };
    let max_prefix = if family == IpFamily::Ipv4 { 32 } else { 128 };
    if !(matches!(
        config.method.as_str(),
        "auto" | "manual" | "link-local" | "disabled"
    ) || (family == IpFamily::Ipv6 && config.method == "dhcp"))
        || (config.method == "manual" && config.addresses.is_empty())
        || config.addresses.len() > 32
        || config.dns.len() > 16
        || config.dns_search.len() > 16
        || config
            .addresses
            .iter()
            .any(|a| !valid_address(&a.address) || a.prefix == 0 || a.prefix > max_prefix)
        || config.gateway.as_deref().is_some_and(|s| !valid_address(s))
        || config.dns.iter().any(|s| !valid_address(s))
        || config.dns_search.iter().any(|s| !valid_search_domain(s))
        || (matches!(config.method.as_str(), "disabled" | "link-local")
            && (!config.addresses.is_empty()
                || config.gateway.is_some()
                || !config.dns.is_empty()
                || !config.dns_search.is_empty()
                || config.ignore_auto_dns))
    {
        return Err(SystemError::invalid(
            "IP configuration",
            "Choose a supported method, matching IP family and prefixes, valid DNS/search domains; manual needs an address and disabled/link-local must have empty address/DNS fields",
        ));
    }
    Ok(())
}
fn valid_search_domain(value: &str) -> bool {
    let name = value.strip_prefix('~').unwrap_or(value);
    if value == "~." {
        return true;
    }
    !name.is_empty()
        && name.len() <= 253
        && name
            .strip_suffix('.')
            .unwrap_or(name)
            .split('.')
            .all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            })
}
fn read_ip(p: &Properties, family: IpFamily) -> Result<IpConfiguration> {
    let mut addresses = vec![];
    if let Some(value) = p.get("address-data") {
        let data: Vec<Properties> = decode(value.clone(), "address-data")?;
        for a in data {
            addresses.push(IpAddress {
                address: field(&a, "address")?,
                prefix: field(&a, "prefix")?,
            });
        }
    }
    let dns = if let Some(value) = p.get("dns-data") {
        decode(value.clone(), "dns-data")?
    } else if let Some(value) = p.get("dns") {
        match family {
            IpFamily::Ipv4 => decode::<Vec<u32>>(value.clone(), "dns")?
                .into_iter()
                .map(|n| Ipv4Addr::from(n.to_ne_bytes()).to_string())
                .collect(),
            IpFamily::Ipv6 => {
                let mut result = vec![];
                for bytes in decode::<Vec<Vec<u8>>>(value.clone(), "dns")? {
                    let octets: [u8; 16] = bytes
                        .try_into()
                        .map_err(|_| SystemError::protocol("IPv6 DNS"))?;
                    result.push(Ipv6Addr::from(octets).to_string());
                }
                result
            }
        }
    } else {
        vec![]
    };
    Ok(IpConfiguration {
        method: field(p, "method")?,
        addresses,
        gateway: optional::<String>(p, "gateway").filter(|s| !s.is_empty()),
        dns,
        dns_search: optional(p, "dns-search").unwrap_or_default(),
        ignore_auto_dns: optional(p, "ignore-auto-dns").unwrap_or(false),
    })
}
