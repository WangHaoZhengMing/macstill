//! Linux system settings for background workers. This crate has no GUI dependency.
//! Snapshot reads never request authentication. Writes use the session's policy
//! authority; permission errors are returned, never worked around with sudo.
pub mod audio;
pub mod bluetooth;
pub mod brightness;
pub mod dbus;
pub mod error;
pub mod host;
pub mod storage;
pub mod login_items;
pub mod locale;
pub mod media;
pub mod model;
pub mod network;
mod network_config;
pub mod pointer;
pub mod power;
pub mod session;
use audio::{Audio, CommandRunner, PactlRunner};
use bluetooth::Bluetooth;
use brightness::Brightness;
use dbus::{GioTransport, Transport};
pub use error::{ErrorKind, Result, SystemError};
use host::HostSettings;
pub use model::*;
use network::Network;
use power::Power;

pub struct SystemApi<T = GioTransport, R = PactlRunner> {
    transport: T,
    audio: R,
}
impl Default for SystemApi {
    fn default() -> Self {
        Self::new()
    }
}
impl SystemApi {
    pub fn new() -> Self {
        Self::with_backends(GioTransport::default(), PactlRunner)
    }
}
impl<T: Transport, R: CommandRunner> SystemApi<T, R> {
    pub fn with_backends(transport: T, audio: R) -> Self {
        Self { transport, audio }
    }
    pub fn media(&self) -> media::Media<'_, T> {
        media::Media(&self.transport)
    }
    pub fn network(&self) -> Network<'_, T> {
        Network(&self.transport)
    }
    pub fn bluetooth(&self) -> Bluetooth<'_, T> {
        Bluetooth(&self.transport)
    }
    pub fn audio(&self) -> Audio<'_, R> {
        Audio(&self.audio)
    }
    pub fn brightness(&self) -> Brightness<'_, T> {
        Brightness::new(&self.transport)
    }
    pub fn power(&self) -> Power<'_, T> {
        Power(&self.transport)
    }
    pub fn host(&self) -> HostSettings<'_, T> {
        HostSettings(&self.transport)
    }
    pub fn locale(&self) -> locale::LocaleSettings<'_, T> {
        locale::LocaleSettings(&self.transport)
    }
    pub fn session(&self) -> session::SessionSettings<'_, T> {
        session::SessionSettings(&self.transport)
    }
    pub fn pointer(&self) -> pointer::PointerSettings {
        pointer::PointerSettings
    }
    pub fn snapshot(&self) -> SystemSnapshot {
        // Each domain reports its own availability; missing hardware/daemons
        // must not prevent unrelated settings from being used.
        SystemSnapshot {
            network: self.network().snapshot().into(),
            bluetooth: self.bluetooth().snapshot().into(),
            audio: self.audio().snapshot().into(),
            backlights: self.brightness().snapshot().into(),
            power: self.power().snapshot().into(),
            power_profiles: self.power().profiles().into(),
            date_time: self.host().date_time().into(),
            host: self.host().snapshot().into(),
            locale: self.locale().snapshot().into(),
            session: self.session().snapshot().into(),
            power_actions: self.session().power_actions().into(),
            pointer: self.pointer().snapshot().into(),
        }
    }
    pub fn query(&self, query: SystemQuery) -> Result<SystemQueryResult> {
        match query {
            SystemQuery::Connection { connection } => self
                .network()
                .connection(&connection)
                .map(Box::new)
                .map(SystemQueryResult::Connection),
            SystemQuery::Timezones => self.host().timezones().map(SystemQueryResult::Timezones),
        }
    }
    /// Completion means the daemon accepted the request. Re-read state to observe
    /// asynchronous activation/pairing; do not optimistically claim connectivity.
    pub fn apply(&self, command: SystemCommand) -> Result<()> {
        match command {
            SystemCommand::SetPointer(command) => self.pointer().apply(&command),
            SystemCommand::SetNetworking(v) => self.network().set_enabled(v),
            SystemCommand::SetWifi(v) => self.network().set_wifi(v),
            SystemCommand::ScanWifi { device } => self.network().scan(&device),
            SystemCommand::ActivateConnection { connection, device } => {
                self.network().activate(&connection, &device).map(|_| ())
            }
            SystemCommand::DeactivateConnection { active_connection } => {
                self.network().deactivate(&active_connection)
            }
            SystemCommand::SetBluetoothPowered { adapter, powered } => {
                self.bluetooth().set_powered(&adapter, powered)
            }
            SystemCommand::StartBluetoothDiscovery { adapter } => {
                self.bluetooth().start_discovery(&adapter)
            }
            SystemCommand::StopBluetoothDiscovery { adapter } => {
                self.bluetooth().stop_discovery(&adapter)
            }
            SystemCommand::ConnectBluetooth { device } => self.bluetooth().connect(&device),
            SystemCommand::DisconnectBluetooth { device } => self.bluetooth().disconnect(&device),
            SystemCommand::PairBluetooth { device } => self.bluetooth().pair(&device),
            SystemCommand::CancelBluetoothPairing { device } => {
                self.bluetooth().cancel_pairing(&device)
            }
            SystemCommand::SetBluetoothTrusted { device, trusted } => {
                self.bluetooth().set_trusted(&device, trusted)
            }
            SystemCommand::SetVolume {
                direction,
                id,
                percent,
            } => self.audio().set_volume(direction, id, percent),
            SystemCommand::SetMute {
                direction,
                id,
                muted,
            } => self.audio().set_mute(direction, id, muted),
            SystemCommand::SetDefaultAudio { direction, id } => {
                self.audio().set_default(direction, id)
            }
            SystemCommand::SetBrightness { name, percent } => self.brightness().set(&name, percent),
            SystemCommand::SetPowerProfile(v) => self.power().set_profile(&v),
            SystemCommand::SetTimezone(v) => self.host().set_timezone(&v),
            SystemCommand::SetNtp(v) => self.host().set_ntp(v),
            SystemCommand::SetHostname(v) => self.host().set_hostname(&v),
            SystemCommand::SetPrettyHostname(v) => self.host().set_pretty_hostname(&v),
            SystemCommand::AddWifiConnection(v) => self.network().add_wifi(&v).map(|_| ()),
            SystemCommand::ForgetConnection { connection } => {
                self.network().forget_connection(&connection)
            }
            SystemCommand::SetConnectionAutoconnect {
                connection,
                enabled,
            } => self.network().set_autoconnect(&connection, enabled),
            SystemCommand::SetConnectionIp {
                connection,
                family,
                config,
            } => self.network().set_ip(&connection, family, &config),
            SystemCommand::RemoveBluetoothDevice { device } => {
                self.bluetooth().remove_device(&device)
            }
            SystemCommand::SetBluetoothDiscoverable {
                adapter,
                discoverable,
            } => self.bluetooth().set_discoverable(&adapter, discoverable),
            SystemCommand::SetBluetoothDiscoverableTimeout { adapter, seconds } => {
                self.bluetooth().set_discoverable_timeout(&adapter, seconds)
            }
            SystemCommand::SetBluetoothPairable { adapter, pairable } => {
                self.bluetooth().set_pairable(&adapter, pairable)
            }
            SystemCommand::SetBluetoothAdapterAlias { adapter, alias } => {
                self.bluetooth().set_adapter_alias(&adapter, &alias)
            }
            SystemCommand::SetBluetoothDeviceAlias { device, alias } => {
                self.bluetooth().set_device_alias(&device, &alias)
            }
            SystemCommand::SetBluetoothBlocked { device, blocked } => {
                self.bluetooth().set_blocked(&device, blocked)
            }
            SystemCommand::SetLocale(v) => self.locale().set_locale(&v),
            SystemCommand::SetX11Keyboard { keyboard, convert } => {
                self.locale().set_x11_keyboard(&keyboard, convert)
            }
            SystemCommand::SetConsoleKeyboard { keyboard, convert } => {
                self.locale().set_console_keyboard(&keyboard, convert)
            }
            SystemCommand::SetTime { unix_microseconds } => self.host().set_time(unix_microseconds),
            SystemCommand::SetLocalRtc(v) => self.host().set_local_rtc(v),
            SystemCommand::LockSession => self.session().lock(),
            SystemCommand::PowerAction(v) => self.session().power_action(v),
        }
    }
}
