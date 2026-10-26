//! Session pointer preferences. GNOME uses GSettings; Macstill/Hyprland uses IPC.
use crate::{ErrorKind, Result, SystemError};
use gio::{glib::variant::ToVariant, prelude::*};
pub use macstill_hyprland::input::{
    PointerCommand, PointerControl, PointerDevice, PointerSetting, PointerSnapshot, PointerValue,
};
use std::path::Path;

pub struct PointerSettings;
impl PointerSettings {
    pub fn snapshot(&self) -> Result<PointerSnapshot> {
        let mut snapshot = if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
            macstill_hyprland::input::snapshot().map_err(|e| {
                SystemError::new(ErrorKind::Unavailable, "pointer settings", e.to_string())
            })?
        } else if is_gnome() {
            gnome_snapshot(None)?
        } else {
            return Err(SystemError::new(
                ErrorKind::Unsupported,
                "pointer settings",
                "Mouse and trackpad controls are available in Macstill/Hyprland and GNOME sessions",
            ));
        };
        if snapshot.backend == "Hyprland" {
            if let Ok(gnome) = gnome_snapshot(None) {
                if let Some(control) = gnome
                    .controls
                    .into_iter()
                    .find(|c| c.setting == PointerSetting::MouseCursorSize)
                {
                    snapshot
                        .controls
                        .retain(|c| c.setting != PointerSetting::MouseCursorSize);
                    snapshot.controls.push(control);
                }
            }
        }
        match devices(Path::new("/sys/class/input"), Path::new("/run/udev/data")) {
            Ok(devices) => snapshot.devices = devices,
            Err(error) => snapshot.devices_error = Some(error.to_string()),
        };
        Ok(snapshot)
    }
    pub fn apply(&self, command: &PointerCommand) -> Result<()> {
        command
            .validate()
            .map_err(|e| SystemError::invalid("pointer settings", &e.to_string()))?;
        if command.setting == PointerSetting::MouseCursorSize {
            if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
                let previous = gnome_snapshot(None)?
                    .number(PointerSetting::MouseCursorSize)
                    .unwrap_or(32.);
                let PointerValue::Number(size) = command.value else {
                    unreachable!()
                };
                let theme = std::env::var("HYPRCURSOR_THEME")
                    .or_else(|_| std::env::var("XCURSOR_THEME"))
                    .unwrap_or_else(|_| "default".into());
                if theme.contains(['\n', '\r', '\0']) {
                    return Err(SystemError::invalid(
                        "pointer settings",
                        "Invalid cursor theme",
                    ));
                }
                let apply = |size: f64| -> Result<()> {
                    let reply = macstill_hyprland::request(&format!("setcursor {theme} {size:.0}"))
                        .map_err(|e| {
                            SystemError::new(ErrorKind::Failed, "cursor size", e.to_string())
                        })?;
                    if reply.trim() != "ok" {
                        return Err(SystemError::new(ErrorKind::Failed, "cursor size", reply));
                    }
                    Ok(())
                };
                apply(size)?;
                if let Err(error) = gnome_apply(command, None) {
                    let _ = apply(previous);
                    return Err(error);
                }
                return Ok(());
            }
        }
        if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
            macstill_hyprland::input::apply(command)
                .map_err(|e| SystemError::new(ErrorKind::Failed, "pointer settings", e.to_string()))
        } else if is_gnome() {
            gnome_apply(command, None)
        } else {
            Err(SystemError::new(
                ErrorKind::Unsupported,
                "pointer settings",
                "No supported desktop input backend is active",
            ))
        }
    }
}
fn is_gnome() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_ascii_uppercase()
        .split([':', ';'])
        .any(|s| s == "GNOME" || s == "GNOME-FLASHBACK")
}
fn key(setting: PointerSetting) -> &'static str {
    use PointerSetting::*;
    match setting {
        MouseSpeed | TrackpadSpeed => "speed",
        MouseNaturalScroll | TrackpadNaturalScroll => "natural-scroll",
        MouseLeftHanded => "left-handed",
        MouseAcceleration => "accel-profile",
        MouseScrollSpeed | TrackpadScrollSpeed => "scroll-speed",
        MouseDoubleClick => "double-click",
        MouseCursorSize => "cursor-size",
        TrackpadTapToClick => "tap-to-click",
        TrackpadSecondaryClick => "click-method",
        TrackpadTapAndDrag => "tap-and-drag",
        TrackpadDragLock => "tap-and-drag-lock",
        TrackpadDisableWhileTyping => "disable-while-typing",
        TrackpadTwoFingerScroll => "two-finger-scrolling-enabled",
        TrackpadMiddleClick => "middle-click-emulation",
    }
}
fn schema(setting: PointerSetting) -> &'static str {
    if setting == PointerSetting::MouseCursorSize {
        "org.gnome.desktop.interface"
    } else if setting.is_trackpad() {
        "org.gnome.desktop.peripherals.touchpad"
    } else {
        "org.gnome.desktop.peripherals.mouse"
    }
}
fn settings(
    setting: PointerSetting,
    backend: Option<&gio::SettingsBackend>,
) -> Result<(gio::Settings, gio::SettingsSchema)> {
    let schema = gio::SettingsSchemaSource::default()
        .and_then(|source| source.lookup(schema(setting), true))
        .ok_or_else(|| {
            SystemError::new(
                ErrorKind::Unavailable,
                "pointer settings",
                "The desktop input settings schema is not installed",
            )
        })?;
    Ok((gio::Settings::new_full(&schema, backend, None), schema))
}
fn from_variant(setting: PointerSetting, value: &gio::glib::Variant) -> Result<PointerValue> {
    use PointerSetting::*;
    match setting {
        MouseSpeed | TrackpadSpeed | MouseScrollSpeed | TrackpadScrollSpeed => {
            value.get().map(PointerValue::Number)
        }
        MouseDoubleClick | MouseCursorSize => {
            value.get::<i32>().map(|v| PointerValue::Number(v as f64))
        }
        MouseAcceleration | TrackpadSecondaryClick => value.get().map(PointerValue::Text),
        _ => value.get().map(PointerValue::Bool),
    }
    .ok_or_else(|| {
        SystemError::new(
            ErrorKind::Failed,
            "pointer settings",
            "The desktop returned an invalid input setting",
        )
    })
}
fn gnome_snapshot(backend: Option<&gio::SettingsBackend>) -> Result<PointerSnapshot> {
    let mut controls = vec![];
    let mut found = false;
    for setting in PointerSetting::ALL {
        let result = settings(setting, backend).and_then(|(settings, schema)| {
            let name = key(setting);
            if !schema.has_key(name) {
                return Err(SystemError::new(
                    ErrorKind::Unsupported,
                    "pointer settings",
                    "Not available in this desktop version",
                ));
            }
            found = true;
            Ok((
                from_variant(setting, &settings.value(name))?,
                settings.is_writable(name),
            ))
        });
        let (value, writable, error) = match result {
            Ok((value, writable)) => (Some(value), writable, None),
            Err(error) => (None, false, Some(error.message)),
        };
        controls.push(PointerControl {
            setting,
            value,
            writable,
            choices: setting.choices(),
            error,
        });
    }
    if !found {
        return Err(SystemError::new(
            ErrorKind::Unavailable,
            "pointer settings",
            "No desktop input schemas are installed",
        ));
    }
    Ok(PointerSnapshot {
        backend: "GNOME".into(),
        controls,
        devices: vec![],
        devices_error: None,
        shared_tracking_speed: false,
    })
}
fn gnome_apply(command: &PointerCommand, backend: Option<&gio::SettingsBackend>) -> Result<()> {
    command
        .validate()
        .map_err(|e| SystemError::invalid("pointer settings", &e.to_string()))?;
    let (settings, schema) = settings(command.setting, backend)?;
    let name = key(command.setting);
    if !schema.has_key(name) {
        return Err(SystemError::new(
            ErrorKind::Unsupported,
            "pointer settings",
            "This desktop version does not provide this control",
        ));
    }
    if !settings.is_writable(name) {
        return Err(SystemError::new(
            ErrorKind::PermissionDenied,
            "pointer settings",
            "This input setting is locked by system policy",
        ));
    }
    let value = match &command.value {
        PointerValue::Bool(v) => v.to_variant(),
        PointerValue::Number(v)
            if matches!(
                command.setting,
                PointerSetting::MouseDoubleClick | PointerSetting::MouseCursorSize
            ) =>
        {
            (*v as i32).to_variant()
        }
        PointerValue::Number(v) => v.to_variant(),
        PointerValue::Text(v) => v.to_variant(),
    };
    if !schema.key(name).range_check(&value) {
        return Err(SystemError::invalid(
            "pointer settings",
            "This desktop does not accept the selected value",
        ));
    }
    settings
        .set_value(name, &value)
        .map_err(|e| SystemError::new(ErrorKind::Failed, "pointer settings", e.to_string()))?;
    gio::Settings::sync();
    if settings.value(name) != value {
        return Err(SystemError::new(
            ErrorKind::Failed,
            "pointer settings",
            "The desktop did not retain the selected value",
        ));
    }
    Ok(())
}
/// udev classifies devices; names alone are never used to guess touchpad support.
fn devices(sysfs: &Path, udev: &Path) -> std::io::Result<Vec<PointerDevice>> {
    std::fs::read_dir(udev)?;
    let mut devices = vec![];
    for entry in std::fs::read_dir(sysfs)? {
        let entry = entry?;
        if !entry.file_name().to_string_lossy().starts_with("event") {
            continue;
        }
        let Ok(dev) = std::fs::read_to_string(entry.path().join("dev")) else {
            continue;
        };
        let Ok(data) = std::fs::read_to_string(udev.join(format!("c{}", dev.trim()))) else {
            continue;
        };
        let trackpad = data.lines().any(|s| s == "E:ID_INPUT_TOUCHPAD=1");
        let mouse = data.lines().any(|s| s == "E:ID_INPUT_MOUSE=1");
        if !trackpad && !mouse {
            continue;
        }
        let Ok(name) = std::fs::read_to_string(entry.path().join("device/name")) else {
            continue;
        };
        devices.push(PointerDevice {
            name: name.trim().into(),
            trackpad,
        });
    }
    devices.sort_by(|a, b| a.name.cmp(&b.name).then(a.trackpad.cmp(&b.trackpad)));
    devices.dedup_by(|a, b| a.name == b.name && a.trackpad == b.trackpad);
    Ok(devices)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gnome_round_trips_through_memory_without_touching_the_users_settings() {
        let backend = gio::memory_settings_backend_new();
        // The schema is supplied by the Linux desktop; never create one in the user's profile.
        if gio::SettingsSchemaSource::default()
            .and_then(|s| s.lookup("org.gnome.desktop.peripherals.touchpad", true))
            .is_none()
        {
            return;
        }
        let command = PointerCommand {
            setting: PointerSetting::TrackpadTapToClick,
            value: PointerValue::Bool(false),
        };
        gnome_apply(&command, Some(&backend)).unwrap();
        let state = gnome_snapshot(Some(&backend)).unwrap();
        assert_eq!(state.boolean(command.setting), Some(false));
        gnome_apply(
            &PointerCommand {
                setting: PointerSetting::MouseSpeed,
                value: PointerValue::Number(0.65),
            },
            Some(&backend),
        )
        .unwrap();
        let state = gnome_snapshot(Some(&backend)).unwrap();
        assert_eq!(state.number(PointerSetting::MouseSpeed), Some(0.65));
        assert_ne!(state.number(PointerSetting::TrackpadSpeed), Some(0.65));
        assert!(
            gnome_apply(
                &PointerCommand {
                    setting: PointerSetting::MouseSpeed,
                    value: PointerValue::Number(5.0)
                },
                Some(&backend)
            )
            .is_err()
        );
    }
    #[test]
    fn cursor_size_round_trips_and_rejects_invalid_values() {
        let backend = gio::memory_settings_backend_new();
        for size in [24., 48., 96.] {
            let command = PointerCommand {
                setting: PointerSetting::MouseCursorSize,
                value: PointerValue::Number(size),
            };
            gnome_apply(&command, Some(&backend)).unwrap();
            assert_eq!(
                gnome_snapshot(Some(&backend))
                    .unwrap()
                    .number(command.setting),
                Some(size)
            );
        }
        for size in [0., 23., 97., 48.5, f64::NAN] {
            assert!(
                gnome_apply(
                    &PointerCommand {
                        setting: PointerSetting::MouseCursorSize,
                        value: PointerValue::Number(size),
                    },
                    Some(&backend)
                )
                .is_err()
            );
        }
        assert_eq!(
            gnome_snapshot(Some(&backend))
                .unwrap()
                .number(PointerSetting::MouseCursorSize),
            Some(96.)
        );
    }
    #[test]
    fn udev_classification_distinguishes_touchpads_and_mice() {
        let dir = tempfile::tempdir().unwrap();
        let sys = dir.path().join("input");
        let udev = dir.path().join("udev");
        std::fs::create_dir_all(&udev).unwrap();
        for (i, name, properties) in [
            (1, "Built-in Touchpad", "E:ID_INPUT_TOUCHPAD=1"),
            (2, "USB Mouse", "E:ID_INPUT_MOUSE=1"),
            (3, "Keyboard", "E:ID_INPUT_KEYBOARD=1"),
        ] {
            let event = sys.join(format!("event{i}"));
            std::fs::create_dir_all(event.join("device")).unwrap();
            std::fs::write(event.join("dev"), format!("13:{i}")).unwrap();
            std::fs::write(event.join("device/name"), name).unwrap();
            std::fs::write(udev.join(format!("c13:{i}")), properties).unwrap();
        }
        let devices = devices(&sys, &udev).unwrap();
        assert_eq!(devices.len(), 2);
        assert!(devices[0].trackpad);
        assert!(!devices[1].trackpad);
    }
}
