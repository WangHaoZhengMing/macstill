use crate::{dbus::*, error::*, model::*};
use gio::glib::variant::ToVariant;
use std::path::{Path, PathBuf};
pub struct Brightness<'a, T: Transport> {
    transport: &'a T,
    root: PathBuf,
}
impl<'a, T: Transport> Brightness<'a, T> {
    pub fn new(transport: &'a T) -> Self {
        Self::with_root(transport, "/sys/class/backlight")
    }
    /// Alternate root is useful for sysfs fixtures and hardware-independent tests.
    pub fn with_root(transport: &'a T, root: impl Into<PathBuf>) -> Self {
        Self {
            transport,
            root: root.into(),
        }
    }
    pub fn snapshot(&self) -> Result<Vec<Backlight>> {
        let entries = match std::fs::read_dir(&self.root) {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(SystemError::io("read backlights", e)),
        };
        let mut result = vec![];
        for entry in entries {
            let entry = entry.map_err(|e| SystemError::io("read backlights", e))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            validate_name(&name)?;
            let maximum = read_number(&entry.path().join("max_brightness"))?;
            let current = read_number(&entry.path().join("brightness"))?;
            if maximum == 0 {
                continue;
            }
            result.push(Backlight {
                name,
                current,
                maximum,
                percent: current as f64 / maximum as f64 * 100.,
            });
        }
        result.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(result)
    }
    pub fn set(&self, name: &str, percent: f64) -> Result<()> {
        validate_name(name)?;
        if !percent.is_finite() || !(0. ..=100.).contains(&percent) {
            return Err(SystemError::invalid(
                "brightness",
                "Brightness must be between 0 and 100 percent",
            ));
        }
        let device = self
            .snapshot()?
            .into_iter()
            .find(|d| d.name == name)
            .ok_or_else(|| {
                SystemError::new(
                    ErrorKind::NotFound,
                    "brightness",
                    "Backlight is no longer available",
                )
            })?;
        let raw = (device.maximum as f64 * percent / 100.).round() as u32;
        // logind resolves the caller's own session; no direct privileged sysfs write.
        unit(
            self.transport.call(
                Bus::System,
                "org.freedesktop.login1",
                "/org/freedesktop/login1/session/auto",
                "org.freedesktop.login1.Session",
                "SetBrightness",
                ("backlight", name, raw).to_variant(),
                false,
            )?,
            "SetBrightness",
        )
    }
}
fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-.:".contains(&c))
    {
        return Err(SystemError::invalid(
            "backlight name",
            "Select a backlight reported by this machine",
        ));
    }
    Ok(())
}
fn read_number(path: &Path) -> Result<u32> {
    std::fs::read_to_string(path)
        .map_err(|e| SystemError::io("backlight", e))?
        .trim()
        .parse()
        .map_err(|_| SystemError::protocol("backlight value"))
}
