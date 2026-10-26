//! Shared AirDrop IPC contract. No GUI or radio ownership lives here.
use anyhow::{Context, Result};
use gio::{glib::variant::ToVariant, prelude::*};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};
pub const NAME: &str = "org.macstill.AirDrop";
pub const PATH: &str = "/org/macstill/AirDrop";
pub const INTERFACE: &str = "org.macstill.AirDrop1";
/// User-visible discovery policy, independent of the radio's sharing mode.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Off,
    EveryoneTenMinutes,
    Everyone,
}
impl Visibility {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "No One",
            Self::EveryoneTenMinutes => "Everyone for 10 Minutes",
            Self::Everyone => "Everyone Until Turned Off",
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Snapshot {
    pub visibility: Visibility,
    pub available: bool,
    pub state: String,
    pub mode: String,
    pub error: Option<String>,
    pub warning: Option<String>,
    pub expires_at: u64,
    pub receive_directory: PathBuf,
    pub peers: Vec<Peer>,
    pub transfers: Vec<Transfer>,
    pub revision: u64,
}
impl Snapshot {
    pub fn enabled(&self) -> bool {
        self.state == "on"
    }
    pub fn busy(&self) -> bool {
        ["starting", "stopping", "finishing"].contains(&self.state.as_str())
    }
    pub fn detail(&self) -> String {
        if self.enabled() {
            self.visibility.label().into()
        } else if self.state == "finishing" {
            "Finishing transfers…".into()
        } else if self.busy() {
            "Connecting…".into()
        } else if self.available {
            "Off".into()
        } else {
            "Setup required".into()
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub model: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Transfer {
    pub id: String,
    pub peer: String,
    pub incoming: bool,
    pub files: Vec<String>,
    pub state: String,
    pub sent: u64,
    pub total: u64,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Enable { seconds: u32 },
    SetVisibility { visibility: Visibility },
    SetReceiveDirectory { path: PathBuf },
    Disable,
    Refresh,
    Send { peer: String, files: Vec<PathBuf> },
    Answer { id: String, accept: bool },
    Cancel { id: String },
}
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn downloads() -> PathBuf {
    gio::glib::user_special_dir(gio::glib::UserDirectory::Downloads).unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Downloads")
    })
}
pub fn executable(name: &str) -> PathBuf {
    let sibling = std::env::current_exe()
        .unwrap_or_default()
        .with_file_name(name);
    if sibling.is_file() {
        sibling
    } else {
        PathBuf::from(name)
    }
}
fn proxy() -> Result<gio::DBusProxy> {
    Ok(gio::DBusProxy::for_bus_sync(
        gio::BusType::Session,
        gio::DBusProxyFlags::DO_NOT_LOAD_PROPERTIES,
        None,
        NAME,
        PATH,
        INTERFACE,
        gio::Cancellable::NONE,
    )?)
}
pub fn snapshot() -> Result<Snapshot> {
    let v = proxy()?.call_sync(
        "GetSnapshot",
        None,
        gio::DBusCallFlags::NONE,
        1500,
        gio::Cancellable::NONE,
    )?;
    let (json,): (String,) = v.get().context("Invalid AirDrop service response")?;
    Ok(serde_json::from_str(&json)?)
}
pub fn send(command: Command) -> Result<()> {
    let json = serde_json::to_string(&command)?;
    proxy()?.call_sync(
        "Execute",
        Some(&(json,).to_variant()),
        gio::DBusCallFlags::NONE,
        2000,
        gio::Cancellable::NONE,
    )?;
    Ok(())
}
/// D-Bus activation is preferred. Development checkouts fall back to the sibling daemon.
pub fn ensure_service() -> Result<()> {
    if snapshot().is_ok() {
        return Ok(());
    }
    let mut child = std::process::Command::new(executable("macstill-airdropd"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("AirDrop service is not installed")?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    for _ in 0..30 {
        if snapshot().is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!("AirDrop service did not start")
}
pub fn open_app() -> Result<()> {
    macstill_apps::open_finder(&["--airdrop".into()])
}
pub fn open_settings() -> Result<()> {
    let mut child = std::process::Command::new(executable("macstill-settings"))
        .args(["--page", "airdrop"])
        .spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
/// Show a received file in the file manager; URLs are never opened automatically.
pub fn show_received(files: &[String]) -> Result<()> {
    let file = files.iter().map(PathBuf::from).find(|p| p.is_absolute());
    let args = if let Some(file) = file {
        vec!["--reveal".into(), file.into_os_string()]
    } else {
        vec![downloads().into_os_string()]
    };
    macstill_apps::open_finder(&args)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unknown_commands() {
        assert!(
            serde_json::from_str::<Command>(
                r#"{"command":"enable","seconds":600,"execute":"bad"}"#
            )
            .is_err()
        );
    }
    #[test]
    fn off_is_not_reported_as_on() {
        assert!(!Snapshot::default().enabled());
    }
}
