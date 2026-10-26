use macstill_hyprland::input::*;
use serde_json::json;
use std::{cell::RefCell, collections::HashMap};

fn fake_options() -> HashMap<String, serde_json::Value> {
    HashMap::from([
        ("input.sensitivity".into(), json!({"float":0.0})),
        ("input.natural_scroll".into(), json!({"bool":false})),
        ("input.left_handed".into(), json!({"int":0})),
        ("input.accel_profile".into(), json!({"str":"[[EMPTY]]"})),
        ("input.scroll_factor".into(), json!({"float":1.0})),
        (
            "input.touchpad.natural_scroll".into(),
            json!({"bool":false}),
        ),
        ("input.touchpad.tap_to_click".into(), json!({"bool":true})),
        (
            "input.touchpad.clickfinger_behavior".into(),
            json!({"bool":true}),
        ),
        ("input.touchpad.tap_and_drag".into(), json!({"bool":true})),
        ("input.touchpad.drag_lock".into(), json!({"int":2})),
        (
            "input.touchpad.disable_while_typing".into(),
            json!({"bool":true}),
        ),
        ("input.touchpad.scroll_factor".into(), json!({"float":1.0})),
        (
            "input.touchpad.middle_button_emulation".into(),
            json!({"bool":false}),
        ),
    ])
}
#[test]
fn reads_new_boolean_and_legacy_integer_formats_and_keeps_unsupported_controls_explicit() {
    let options = fake_options();
    let mut calls = vec![];
    let state = snapshot_with(&mut |message| {
        calls.push(message.to_string());
        Ok(options
            .get(message.strip_prefix("j/getoption ").unwrap())
            .unwrap()
            .to_string())
    })
    .unwrap();
    assert_eq!(
        state.boolean(PointerSetting::TrackpadTapToClick),
        Some(true)
    );
    assert_eq!(state.boolean(PointerSetting::TrackpadDragLock), Some(true));
    assert_eq!(
        state.text(PointerSetting::MouseAcceleration),
        Some("default")
    );
    assert!(state.shared_tracking_speed);
    assert!(!state.writable(PointerSetting::MouseDoubleClick));
    assert!(!state.writable(PointerSetting::TrackpadTwoFingerScroll));
    assert_eq!(
        calls.iter().filter(|v| v.contains("sensitivity")).count(),
        2
    ); // Detection plus one cached global read for both devices.
}
#[test]
fn validates_typed_options_before_sending_any_ipc_or_writing_files() {
    let dir = tempfile::tempdir().unwrap();
    for command in [
        PointerCommand {
            setting: PointerSetting::MouseSpeed,
            value: PointerValue::Number(f64::NAN),
        },
        PointerCommand {
            setting: PointerSetting::MouseSpeed,
            value: PointerValue::Number(2.),
        },
        PointerCommand {
            setting: PointerSetting::TrackpadTapToClick,
            value: PointerValue::Number(1.),
        },
        PointerCommand {
            setting: PointerSetting::MouseAcceleration,
            value: PointerValue::Text("flat\"});os.exit()".into()),
        },
        PointerCommand {
            setting: PointerSetting::MouseDoubleClick,
            value: PointerValue::Number(10.),
        },
    ] {
        assert!(
            apply_with(&command, dir.path(), &mut |_| panic!(
                "invalid input reached compositor"
            ))
            .is_err()
        );
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}
#[test]
fn saves_boolean_override_and_preserves_other_options() {
    let dir = tempfile::tempdir().unwrap();
    let live = RefCell::new(false);
    let mut sent = vec![];
    let mut transport = |message: &str| -> anyhow::Result<String> {
        sent.push(message.to_string());
        if message.contains("sensitivity") {
            return Ok(json!({"float":0.0}).to_string());
        }
        if message.starts_with("j/getoption") {
            return Ok(json!({"bool":*live.borrow()}).to_string());
        }
        assert!(message.starts_with("eval hl.config"));
        *live.borrow_mut() = true;
        Ok("ok".into())
    };
    let command = PointerCommand {
        setting: PointerSetting::TrackpadNaturalScroll,
        value: PointerValue::Bool(true),
    };
    apply_with(&command, dir.path(), &mut transport).unwrap();
    let saved = std::fs::read_to_string(dir.path().join("pointer-settings.lua")).unwrap();
    assert!(saved.contains("hl.config({[\"input.touchpad.natural_scroll\"]=true})"));
    assert!(*live.borrow());
    assert_eq!(sent.iter().filter(|s| s.starts_with("eval ")).count(), 1);
}
#[test]
fn rejected_readback_rolls_back_and_does_not_save() {
    let dir = tempfile::tempdir().unwrap();
    let mut writes = vec![];
    let result = apply_with(
        &PointerCommand {
            setting: PointerSetting::MouseSpeed,
            value: PointerValue::Number(0.5),
        },
        dir.path(),
        &mut |message| {
            if message.starts_with("j/getoption") {
                Ok(json!({"float":0.0}).to_string())
            } else {
                writes.push(message.to_string());
                Ok("ok".into())
            }
        },
    );
    assert!(result.is_err());
    assert_eq!(writes.len(), 2);
    assert!(writes[1].contains("=0.0"));
    assert!(!dir.path().join("pointer-settings.lua").exists());
}
#[test]
fn persistence_failure_restores_live_value_and_previous_saved_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pointer-settings.lua");
    let saved = "-- macstill-input-v1 {}\n-- Original\n";
    std::fs::write(&file, saved).unwrap();
    let mut writes = 0;
    let mut live = 0.0;
    let result = apply_with(
        &PointerCommand {
            setting: PointerSetting::MouseSpeed,
            value: PointerValue::Number(0.5),
        },
        dir.path(),
        &mut |message| {
            if message.starts_with("j/getoption") {
                Ok(json!({"float":live}).to_string())
            } else {
                writes += 1;
                live = if writes == 1 { 0.5 } else { 0.0 };
                if writes == 1 {
                    std::fs::remove_file(dir.path().join("pointer-settings.lua.tmp")).unwrap();
                    std::fs::create_dir(dir.path().join("pointer-settings.lua.tmp")).unwrap();
                }
                Ok("ok".into())
            }
        },
    );
    assert!(result.is_err());
    assert_eq!(live, 0.0);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), saved);
}
#[test]
fn legacy_syntax_is_selected_on_read_failure_and_does_not_retry_writes() {
    let dir = tempfile::tempdir().unwrap();
    let mut live = false;
    let mut calls = vec![];
    apply_with(
        &PointerCommand {
            setting: PointerSetting::TrackpadTapToClick,
            value: PointerValue::Bool(true),
        },
        dir.path(),
        &mut |message| {
            calls.push(message.to_string());
            if message == "j/getoption input.sensitivity" {
                return Ok("no such option".into());
            }
            if message.contains("sensitivity") {
                return Ok(json!({"float":0.0}).to_string());
            }
            if message.starts_with("j/getoption") {
                return Ok(json!({"int":i32::from(live)}).to_string());
            }
            assert_eq!(message, "keyword input:touchpad:tap_to_click 1");
            live = true;
            Ok("ok".into())
        },
    )
    .unwrap();
    assert!(live);
    assert_eq!(calls.iter().filter(|v| v.starts_with("keyword")).count(), 1);
    assert!(!calls.iter().any(|v| v.starts_with("eval")));
}

#[test]
fn a_lost_write_reply_restores_the_previous_value_instead_of_saving_uncertain_state() {
    let dir = tempfile::tempdir().unwrap();
    let mut writes = 0;
    let mut live = false;
    let result = apply_with(
        &PointerCommand {
            setting: PointerSetting::TrackpadTapToClick,
            value: PointerValue::Bool(true),
        },
        dir.path(),
        &mut |message| {
            if message.contains("sensitivity") {
                return Ok(json!({"float":0.0}).to_string());
            }
            if message.starts_with("j/getoption") {
                return Ok(json!({"bool":live}).to_string());
            }
            writes += 1;
            if writes == 1 {
                live = true;
                anyhow::bail!("lost write reply");
            }
            live = false;
            Ok("ok".into())
        },
    );
    assert!(result.is_err());
    assert_eq!(writes, 2);
    assert!(!live);
    assert!(!dir.path().join("pointer-settings.lua").exists());
}
