use crate::error::{Result, SystemError};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State<T> {
    pub value: Option<T>,
    pub error: Option<SystemError>,
}
impl<T> Default for State<T> {
    fn default() -> Self {
        Self {
            value: None,
            error: None,
        }
    }
}
impl<T> From<Result<T>> for State<T> {
    fn from(result: Result<T>) -> Self {
        match result {
            Ok(value) => Self {
                value: Some(value),
                error: None,
            },
            Err(error) => Self {
                value: None,
                error: Some(error),
            },
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkState {
    pub enabled: bool,
    pub wifi_enabled: bool,
    pub wifi_hardware_enabled: bool,
    pub connectivity: u32,
    pub state: u32,
    pub devices: Vec<NetworkDevice>,
    pub saved_connections: Vec<SavedConnection>,
    pub permissions: std::collections::HashMap<String, String>,
    pub warnings: Vec<SystemError>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkDevice {
    pub path: String,
    pub interface: String,
    pub kind: u32,
    pub state: u32,
    pub active_connection: Option<String>,
    #[serde(default)]
    pub active_profile: Option<String>,
    #[serde(default)]
    pub active_access_point: Option<String>,
    pub access_points: Vec<AccessPoint>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccessPoint {
    pub path: String,
    pub ssid: String,
    pub ssid_bytes: Vec<u8>,
    pub strength: u8,
    pub flags: u32,
    pub wpa_flags: u32,
    pub rsn_flags: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedConnection {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub uuid: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BluetoothState {
    pub adapters: Vec<BluetoothAdapter>,
    pub devices: Vec<BluetoothDevice>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BluetoothAdapter {
    pub path: String,
    pub name: String,
    pub powered: bool,
    pub discovering: bool,
    pub discoverable: bool,
    #[serde(default)]
    pub pairable: Option<bool>,
    #[serde(default)]
    pub discoverable_timeout_seconds: Option<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BluetoothDevice {
    pub path: String,
    pub adapter: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
    pub trusted: bool,
    pub rssi: Option<i16>,
    #[serde(default)]
    pub blocked: Option<bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PowerState {
    pub on_battery: bool,
    pub battery: Option<Battery>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Battery {
    pub percentage: f64,
    pub state: u32,
    pub time_to_empty_seconds: i64,
    pub time_to_full_seconds: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PowerProfiles {
    pub active: String,
    pub available: Vec<String>,
    pub performance_degraded: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DateTime {
    pub timezone: String,
    pub ntp: bool,
    pub can_ntp: bool,
    pub synchronized: bool,
    #[serde(default)]
    pub local_rtc: Option<bool>,
    #[serde(default)]
    pub unix_microseconds: Option<u64>,
    #[serde(default)]
    pub rtc_microseconds: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Host {
    pub hostname: String,
    pub static_hostname: String,
    pub pretty_hostname: String,
    pub operating_system: String,
    pub kernel: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Backlight {
    pub name: String,
    pub current: u32,
    pub maximum: u32,
    pub percent: f64,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudioDirection {
    Output,
    Input,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioDevice {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub direction: AudioDirection,
    pub percent: f64,
    pub muted: bool,
    pub is_default: bool,
    pub channels: Vec<AudioChannel>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioState {
    pub server: String,
    pub outputs: Vec<AudioDevice>,
    pub inputs: Vec<AudioDevice>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SystemSnapshot {
    #[serde(default)]
    pub pointer: State<crate::pointer::PointerSnapshot>,
    pub network: State<NetworkState>,
    pub bluetooth: State<BluetoothState>,
    pub audio: State<AudioState>,
    pub backlights: State<Vec<Backlight>>,
    pub power: State<PowerState>,
    pub power_profiles: State<PowerProfiles>,
    pub date_time: State<DateTime>,
    pub host: State<Host>,
    #[serde(default)]
    pub locale: State<LocaleState>,
    #[serde(default)]
    pub session: State<SessionState>,
    #[serde(default)]
    pub power_actions: State<PowerActions>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SystemCommand {
    SetPointer(crate::pointer::PointerCommand),
    SetNetworking(bool),
    SetWifi(bool),
    ScanWifi {
        device: String,
    },
    ActivateConnection {
        connection: String,
        device: String,
    },
    DeactivateConnection {
        active_connection: String,
    },
    SetBluetoothPowered {
        adapter: String,
        powered: bool,
    },
    StartBluetoothDiscovery {
        adapter: String,
    },
    StopBluetoothDiscovery {
        adapter: String,
    },
    ConnectBluetooth {
        device: String,
    },
    DisconnectBluetooth {
        device: String,
    },
    PairBluetooth {
        device: String,
    },
    CancelBluetoothPairing {
        device: String,
    },
    SetBluetoothTrusted {
        device: String,
        trusted: bool,
    },
    SetVolume {
        direction: AudioDirection,
        id: u32,
        percent: f64,
    },
    SetMute {
        direction: AudioDirection,
        id: u32,
        muted: bool,
    },
    SetDefaultAudio {
        direction: AudioDirection,
        id: u32,
    },
    SetBrightness {
        name: String,
        percent: f64,
    },
    SetPowerProfile(String),
    SetTimezone(String),
    SetNtp(bool),
    SetHostname(String),
    SetPrettyHostname(String),
    AddWifiConnection(WifiConnection),
    ForgetConnection {
        connection: String,
    },
    SetConnectionAutoconnect {
        connection: String,
        enabled: bool,
    },
    SetConnectionIp {
        connection: String,
        family: IpFamily,
        config: IpConfiguration,
    },
    RemoveBluetoothDevice {
        device: String,
    },
    SetBluetoothDiscoverable {
        adapter: String,
        discoverable: bool,
    },
    SetBluetoothDiscoverableTimeout {
        adapter: String,
        seconds: u32,
    },
    SetBluetoothPairable {
        adapter: String,
        pairable: bool,
    },
    SetBluetoothAdapterAlias {
        adapter: String,
        alias: String,
    },
    SetBluetoothDeviceAlias {
        device: String,
        alias: String,
    },
    SetBluetoothBlocked {
        device: String,
        blocked: bool,
    },
    SetLocale(Vec<String>),
    SetX11Keyboard {
        keyboard: X11Keyboard,
        convert: bool,
    },
    SetConsoleKeyboard {
        keyboard: ConsoleKeyboard,
        convert: bool,
    },
    SetTime {
        unix_microseconds: i64,
    },
    SetLocalRtc(bool),
    LockSession,
    PowerAction(PowerAction),
}

/// Queries kept out of the full snapshot because they may enumerate a large
/// database or require an explicitly selected connection.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SystemQuery {
    Connection { connection: String },
    Timezones,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", content = "value", rename_all = "snake_case")]
pub enum SystemQueryResult {
    Connection(Box<ConnectionDetails>),
    Timezones(Vec<String>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WifiConnection {
    pub name: String,
    /// The actual SSID bytes, not a lossy display name (1–32 bytes).
    pub ssid: Vec<u8>,
    pub hidden: bool,
    pub autoconnect: bool,
    pub security: WifiSecurity,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum WifiSecurity {
    Open {},
    WpaPsk { password: String },
    Sae { password: String },
}
impl std::fmt::Debug for WifiSecurity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Open {} => "Open",
            Self::WpaPsk { .. } => "WpaPsk { password: [redacted] }",
            Self::Sae { .. } => "Sae { password: [redacted] }",
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpFamily {
    Ipv4,
    Ipv6,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IpAddress {
    pub address: String,
    pub prefix: u32,
}
/// Replaces only the named family's address/DNS fields. Routes, metrics and
/// other advanced settings remain owned by NetworkManager/external editors.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IpConfiguration {
    /// Supported writes: auto, manual, link-local, disabled; IPv6 also dhcp.
    pub method: String,
    pub addresses: Vec<IpAddress>,
    pub gateway: Option<String>,
    pub dns: Vec<String>,
    pub dns_search: Vec<String>,
    pub ignore_auto_dns: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectionDetails {
    pub path: String,
    pub name: String,
    pub uuid: String,
    pub kind: String,
    pub autoconnect: bool,
    pub ipv4: Option<IpConfiguration>,
    pub ipv6: Option<IpConfiguration>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct X11Keyboard {
    pub layout: String,
    pub model: String,
    pub variant: String,
    pub options: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsoleKeyboard {
    pub keymap: String,
    pub keymap_toggle: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocaleState {
    pub locale: Vec<String>,
    pub x11: X11Keyboard,
    pub console: ConsoleKeyboard,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerAction {
    PowerOff,
    Reboot,
    Suspend,
    Hibernate,
    HybridSleep,
    SuspendThenHibernate,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PowerCapability {
    pub action: PowerAction,
    /// Preserve logind's yes/no/na/challenge/inhibited variants verbatim.
    pub availability: State<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Inhibitor {
    pub what: String,
    pub who: String,
    pub why: String,
    pub mode: String,
    pub uid: u32,
    pub pid: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PowerActions {
    pub capabilities: Vec<PowerCapability>,
    pub inhibitors: State<Vec<Inhibitor>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionState {
    pub id: String,
    pub user_id: u32,
    pub user_name: String,
    pub kind: String,
    pub desktop: String,
    pub state: String,
    pub active: bool,
    pub locked: bool,
    pub idle: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioChannel {
    pub name: String,
    pub volume: u32,
}

/// Optional battery health and daemon-recorded charge history for one system battery.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatteryReport {
    pub device: String,
    pub capacity_percent: Option<f64>,
    pub charge_cycles: Option<i32>,
    pub energy_full_wh: Option<f64>,
    pub energy_design_wh: Option<f64>,
    pub history: Vec<BatterySample>,
    pub history_error: Option<String>,
    pub battery_count: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatterySample {
    pub timestamp: u32,
    pub percentage: f64,
    pub state: u32,
}
