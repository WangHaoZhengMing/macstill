//! Pointer defaults for Hyprland. Saved overrides are loaded by Macstill's Lua profile.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerSetting {
    MouseSpeed,
    MouseNaturalScroll,
    MouseLeftHanded,
    MouseAcceleration,
    MouseScrollSpeed,
    MouseDoubleClick,
    MouseCursorSize,
    TrackpadSpeed,
    TrackpadNaturalScroll,
    TrackpadTapToClick,
    TrackpadSecondaryClick,
    TrackpadTapAndDrag,
    TrackpadDragLock,
    TrackpadDisableWhileTyping,
    TrackpadScrollSpeed,
    TrackpadTwoFingerScroll,
    TrackpadMiddleClick,
}
impl PointerSetting {
    pub const ALL: [Self; 17] = [
        Self::MouseSpeed,
        Self::MouseNaturalScroll,
        Self::MouseLeftHanded,
        Self::MouseAcceleration,
        Self::MouseScrollSpeed,
        Self::MouseDoubleClick,
        Self::MouseCursorSize,
        Self::TrackpadSpeed,
        Self::TrackpadNaturalScroll,
        Self::TrackpadTapToClick,
        Self::TrackpadSecondaryClick,
        Self::TrackpadTapAndDrag,
        Self::TrackpadDragLock,
        Self::TrackpadDisableWhileTyping,
        Self::TrackpadScrollSpeed,
        Self::TrackpadTwoFingerScroll,
        Self::TrackpadMiddleClick,
    ];
    pub fn is_trackpad(self) -> bool {
        !matches!(
            self,
            Self::MouseSpeed
                | Self::MouseNaturalScroll
                | Self::MouseLeftHanded
                | Self::MouseAcceleration
                | Self::MouseScrollSpeed
                | Self::MouseCursorSize
                | Self::MouseDoubleClick
        )
    }
    fn key(self) -> Option<&'static str> {
        Some(match self {
            Self::MouseSpeed | Self::TrackpadSpeed => "input.sensitivity",
            Self::MouseNaturalScroll => "input.natural_scroll",
            Self::MouseLeftHanded => "input.left_handed",
            Self::MouseAcceleration => "input.accel_profile",
            Self::MouseScrollSpeed => "input.scroll_factor",
            Self::TrackpadNaturalScroll => "input.touchpad.natural_scroll",
            Self::TrackpadTapToClick => "input.touchpad.tap_to_click",
            Self::TrackpadSecondaryClick => "input.touchpad.clickfinger_behavior",
            Self::TrackpadTapAndDrag => "input.touchpad.tap_and_drag",
            Self::TrackpadDragLock => "input.touchpad.drag_lock",
            Self::TrackpadDisableWhileTyping => "input.touchpad.disable_while_typing",
            Self::TrackpadScrollSpeed => "input.touchpad.scroll_factor",
            Self::TrackpadMiddleClick => "input.touchpad.middle_button_emulation",
            Self::MouseCursorSize | Self::MouseDoubleClick | Self::TrackpadTwoFingerScroll => {
                return None;
            }
        })
    }
    pub fn choices(self) -> Vec<String> {
        match self {
            Self::MouseAcceleration => vec!["default", "flat", "adaptive"],
            Self::TrackpadSecondaryClick => vec!["default", "fingers", "areas", "none"],
            _ => vec![],
        }
        .into_iter()
        .map(str::to_string)
        .collect()
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PointerValue {
    Bool(bool),
    Number(f64),
    Text(String),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointerCommand {
    pub setting: PointerSetting,
    pub value: PointerValue,
}
impl PointerCommand {
    pub fn validate(&self) -> Result<()> {
        use PointerSetting::*;
        let valid = match (&self.setting, &self.value) {
            (MouseSpeed | TrackpadSpeed, PointerValue::Number(v)) => {
                v.is_finite() && (-1.0..=1.0).contains(v)
            }
            (MouseScrollSpeed | TrackpadScrollSpeed, PointerValue::Number(v)) => {
                v.is_finite() && (0.1..=5.0).contains(v)
            }
            (MouseCursorSize, PointerValue::Number(v)) => {
                v.is_finite() && (24.0..=96.0).contains(v) && v.fract() == 0.0
            }
            (MouseDoubleClick, PointerValue::Number(v)) => {
                v.is_finite() && (100.0..=1000.0).contains(v) && v.fract() == 0.0
            }
            (MouseAcceleration | TrackpadSecondaryClick, PointerValue::Text(v)) => {
                self.setting.choices().contains(v)
            }
            (
                MouseNaturalScroll
                | MouseLeftHanded
                | TrackpadNaturalScroll
                | TrackpadTapToClick
                | TrackpadTapAndDrag
                | TrackpadDragLock
                | TrackpadDisableWhileTyping
                | TrackpadTwoFingerScroll
                | TrackpadMiddleClick,
                PointerValue::Bool(_),
            ) => true,
            _ => false,
        };
        ensure!(valid, "Invalid value for {:?}", self.setting);
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PointerControl {
    pub setting: PointerSetting,
    pub value: Option<PointerValue>,
    pub writable: bool,
    pub choices: Vec<String>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PointerDevice {
    pub name: String,
    pub trackpad: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PointerSnapshot {
    pub backend: String,
    pub controls: Vec<PointerControl>,
    pub devices: Vec<PointerDevice>,
    pub devices_error: Option<String>,
    pub shared_tracking_speed: bool,
}
impl PointerSnapshot {
    pub fn control(&self, setting: PointerSetting) -> Option<&PointerControl> {
        self.controls.iter().find(|c| c.setting == setting)
    }
    pub fn boolean(&self, setting: PointerSetting) -> Option<bool> {
        match self.control(setting)?.value.as_ref()? {
            PointerValue::Bool(v) => Some(*v),
            _ => None,
        }
    }
    pub fn number(&self, setting: PointerSetting) -> Option<f64> {
        match self.control(setting)?.value.as_ref()? {
            PointerValue::Number(v) => Some(*v),
            _ => None,
        }
    }
    pub fn text(&self, setting: PointerSetting) -> Option<&str> {
        match self.control(setting)?.value.as_ref()? {
            PointerValue::Text(v) => Some(v),
            _ => None,
        }
    }
    pub fn writable(&self, setting: PointerSetting) -> bool {
        self.control(setting)
            .is_some_and(|c| c.writable && c.value.is_some())
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Syntax {
    Lua,
    Legacy,
}
fn detect(transport: &mut impl FnMut(&str) -> Result<String>) -> Result<Syntax> {
    let value = transport("j/getoption input.sensitivity")?;
    if serde_json::from_str::<serde_json::Value>(&value)
        .ok()
        .and_then(|v| v.get("float").and_then(|v| v.as_f64()))
        .is_some()
    {
        return Ok(Syntax::Lua);
    }
    let value = transport("j/getoption input:sensitivity")?;
    ensure!(
        serde_json::from_str::<serde_json::Value>(&value)
            .ok()
            .and_then(|v| v.get("float").and_then(|v| v.as_f64()))
            .is_some(),
        "This Hyprland session cannot report pointer options"
    );
    Ok(Syntax::Legacy)
}
fn read_raw(
    key: &str,
    syntax: Syntax,
    transport: &mut impl FnMut(&str) -> Result<String>,
) -> Result<serde_json::Value> {
    let key = if syntax == Syntax::Legacy {
        key.replace('.', ":")
    } else {
        key.into()
    };
    let response = transport(&format!("j/getoption {key}"))?;
    let value = serde_json::from_str::<serde_json::Value>(&response)
        .context("Hyprland returned an invalid option response")?;
    ensure!(value.is_object(), "Hyprland option is unavailable");
    Ok(value)
}
fn value_from_raw(setting: PointerSetting, raw: &serde_json::Value) -> Result<PointerValue> {
    use PointerSetting::*;
    Ok(match setting {
        MouseSpeed | TrackpadSpeed | MouseScrollSpeed | TrackpadScrollSpeed => {
            PointerValue::Number(
                raw.get("float")
                    .and_then(|v| v.as_f64())
                    .filter(|v| v.is_finite())
                    .context("Missing numeric option")?,
            )
        }
        MouseAcceleration => PointerValue::Text(
            raw.get("str")
                .and_then(|v| v.as_str())
                .map(|s| {
                    if s.is_empty() || s == "[[EMPTY]]" {
                        "default"
                    } else {
                        s
                    }
                })
                .context("Missing acceleration profile")?
                .into(),
        ),
        TrackpadSecondaryClick => PointerValue::Text(
            if integer(raw)? != 0 {
                "fingers"
            } else {
                "areas"
            }
            .into(),
        ),
        _ => PointerValue::Bool(integer(raw)? != 0),
    })
}
fn integer(raw: &serde_json::Value) -> Result<i64> {
    raw.get("int")
        .and_then(|v| v.as_i64())
        .or_else(|| raw.get("bool").and_then(|v| v.as_bool()).map(i64::from))
        .context("Missing boolean option")
}
pub fn snapshot() -> Result<PointerSnapshot> {
    snapshot_with(&mut crate::request)
}
pub fn snapshot_with(
    transport: &mut impl FnMut(&str) -> Result<String>,
) -> Result<PointerSnapshot> {
    let syntax = detect(transport)?;
    let mut cache = BTreeMap::new();
    let mut controls = vec![];
    for setting in PointerSetting::ALL {
        let mut choices = setting.choices();
        if setting == PointerSetting::TrackpadSecondaryClick {
            choices = vec!["fingers".into(), "areas".into()];
        }
        let result = if let Some(key) = setting.key() {
            if !cache.contains_key(key) {
                cache.insert(
                    key,
                    read_raw(key, syntax, transport).map_err(|e| e.to_string()),
                );
            }
            cache[key]
                .as_ref()
                .map_err(|e| anyhow::anyhow!("{e}"))
                .and_then(|value| value_from_raw(setting, value))
        } else {
            Err(anyhow::anyhow!("Not configurable through this compositor"))
        };
        let (value, error) = match result {
            Ok(value) => (Some(value), None),
            Err(e) => (None, Some(e.to_string())),
        };
        controls.push(PointerControl {
            setting,
            writable: value.is_some(),
            value,
            choices,
            error,
        });
    }
    Ok(PointerSnapshot {
        backend: "Hyprland".into(),
        controls,
        devices: vec![],
        devices_error: None,
        shared_tracking_speed: true,
    })
}
fn directory() -> Result<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(value) => PathBuf::from(value),
        None => PathBuf::from(std::env::var_os("HOME").context("HOME is missing")?).join(".config"),
    };
    ensure!(
        base.is_absolute(),
        "The configuration directory must be absolute"
    );
    Ok(base.join("macstill"))
}
pub fn apply(command: &PointerCommand) -> Result<()> {
    apply_with(command, &directory()?, &mut crate::request)
}
fn wire_value(command: &PointerCommand) -> Result<serde_json::Value> {
    command.validate()?;
    use PointerSetting::*;
    Ok(match (&command.setting, &command.value) {
        (TrackpadSecondaryClick, PointerValue::Text(v)) => {
            ensure!(
                matches!(v.as_str(), "fingers" | "areas"),
                "This compositor supports finger or button-area clicking"
            );
            serde_json::json!(v == "fingers")
        }
        (TrackpadDragLock, PointerValue::Bool(v)) => serde_json::json!(i32::from(*v)),
        (MouseAcceleration, PointerValue::Text(v)) => {
            serde_json::json!(if v == "default" { "" } else { v })
        }
        (_, PointerValue::Bool(v)) => serde_json::json!(v),
        (_, PointerValue::Number(v)) => serde_json::json!(v),
        (_, PointerValue::Text(v)) => serde_json::json!(v),
    })
}
fn lua_literal(value: &serde_json::Value) -> Result<String> {
    match value {
        serde_json::Value::Bool(v) => Ok(v.to_string()),
        serde_json::Value::Number(v) => Ok(v.to_string()),
        serde_json::Value::String(v) => {
            let mut out = String::from("\"");
            for byte in v.bytes() {
                if (32..=126).contains(&byte) && byte != b'"' && byte != b'\\' {
                    out.push(byte as char);
                } else {
                    out.push_str(&format!("\\{byte:03}"));
                }
            }
            out.push('"');
            Ok(out)
        }
        _ => bail!("Invalid saved input value"),
    }
}
fn raw_wire(setting: PointerSetting, raw: &serde_json::Value) -> Result<serde_json::Value> {
    use PointerSetting::*;
    Ok(match setting {
        MouseSpeed | TrackpadSpeed | MouseScrollSpeed | TrackpadScrollSpeed => raw
            .get("float")
            .filter(|v| v.as_f64().is_some_and(f64::is_finite))
            .context("Missing previous speed")?
            .clone(),
        MouseAcceleration => {
            let value = raw
                .get("str")
                .and_then(|v| v.as_str())
                .context("Missing previous acceleration profile")?;
            serde_json::json!(if value == "[[EMPTY]]" { "" } else { value })
        }
        TrackpadDragLock => serde_json::json!(integer(raw)?),
        _ => serde_json::json!(integer(raw)? != 0),
    })
}

fn send(
    key: &str,
    value: &serde_json::Value,
    syntax: Syntax,
    transport: &mut impl FnMut(&str) -> Result<String>,
) -> Result<()> {
    let literal = lua_literal(value)?;
    let message = match syntax {
        Syntax::Lua => format!("eval hl.config({{[\"{key}\"]={literal}}})"),
        Syntax::Legacy => {
            let value = match value {
                serde_json::Value::String(value) => value.clone(),
                serde_json::Value::Bool(v) => i32::from(*v).to_string(),
                _ => literal,
            };
            ensure!(
                !value.contains(['\n', '\r', ';']),
                "Invalid compositor option response"
            );
            format!("keyword {} {value}", key.replace('.', ":"))
        }
    };
    ensure!(
        transport(&message)?.trim() == "ok",
        "Hyprland rejected the input change"
    );
    Ok(())
}
/// Apply one option, read it back, then atomically save one generated Lua file.
/// On failure restore the previous live value; never report a failed rollback as success.
pub fn apply_with(
    command: &PointerCommand,
    directory: &Path,
    transport: &mut impl FnMut(&str) -> Result<String>,
) -> Result<()> {
    let next = wire_value(command)?;
    let key = command
        .setting
        .key()
        .context("This setting is not supported by Hyprland")?;
    let syntax = detect(transport)?;
    let previous = raw_wire(command.setting, &read_raw(key, syntax, transport)?)?;
    std::fs::create_dir_all(directory)?;
    let path = directory.join("pointer-settings.lua");
    // Only read our own generated value dictionary, never parse or execute arbitrary Lua.
    let original = match std::fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let mut saved: BTreeMap<String, serde_json::Value> = if let Some(original) = &original {
        let header = original
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("-- macstill-input-v1 "))
            .context("The input settings file was edited externally; refusing to overwrite it")?;
        serde_json::from_str(header)?
    } else {
        BTreeMap::new()
    };
    // Validate persisted keys and values before re-emitting them as code.
    for (saved_key, value) in &saved {
        ensure!(
            PointerSetting::ALL
                .iter()
                .any(|setting| setting.key() == Some(saved_key)),
            "Unknown saved input option"
        );
        lua_literal(value)?;
    }
    saved.insert(key.into(), next.clone());
    let mut contents = format!(
        "-- macstill-input-v1 {}\n-- Generated by Macstill Mouse & Trackpad Settings.\n",
        serde_json::to_string(&saved)?
    );
    for (key, value) in &saved {
        contents.push_str(&format!(
            "hl.config({{[\"{key}\"]={}}})\n",
            lua_literal(value)?
        ));
    }
    let temporary = directory.join("pointer-settings.lua.tmp");
    // Stage the file first so a permissions error cannot change the pointer.
    std::fs::write(&temporary, contents)?;
    if let Err(error) = send(key, &next, syntax, transport) {
        let _ = std::fs::remove_file(&temporary);
        return match send(key, &previous, syntax, transport) {
            Ok(()) => Err(error),
            Err(rollback) => Err(anyhow::anyhow!(
                "{error}; restoring the previous value also failed: {rollback}"
            )),
        };
    }
    let finish = (|| -> Result<()> {
        let actual = raw_wire(command.setting, &read_raw(key, syntax, transport)?)?;
        let matches = match (&actual, &next) {
            (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
                (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 0.0001
            }
            _ => actual == next,
        };
        ensure!(matches, "The compositor did not accept this input setting");
        std::fs::rename(&temporary, &path)?;
        Ok(())
    })();
    if let Err(error) = finish {
        let _ = std::fs::remove_file(&temporary);
        return match send(key, &previous, syntax, transport) {
            Ok(()) => Err(error),
            Err(rollback) => Err(anyhow::anyhow!(
                "{error}; restoring the previous value also failed: {rollback}"
            )),
        };
    }
    Ok(())
}
