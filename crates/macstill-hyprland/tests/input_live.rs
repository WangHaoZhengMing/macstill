//! Explicitly opt in with a disposable nested compositor and an isolated config directory.
#[test]
#[ignore = "requires isolated Hyprland and MACSTILL_POINTER_TEST=1"]
fn pointer_settings_apply_read_back_and_survive_reload() {
    use macstill_hyprland::{input::*, request};
    assert_eq!(std::env::var("MACSTILL_POINTER_TEST").as_deref(), Ok("1"));
    assert_eq!(
        std::env::var("XDG_RUNTIME_DIR").as_deref(),
        Ok("/tmp/macstill-input-test")
    );
    let original = snapshot().unwrap();
    let old_speed = original.number(PointerSetting::MouseSpeed).unwrap();
    let old_tap = original
        .boolean(PointerSetting::TrackpadTapToClick)
        .unwrap();
    let old_natural = original
        .boolean(PointerSetting::TrackpadNaturalScroll)
        .unwrap();
    let set = |setting, value| apply(&PointerCommand { setting, value }).unwrap();
    set(PointerSetting::MouseSpeed, PointerValue::Number(0.35));
    set(
        PointerSetting::TrackpadTapToClick,
        PointerValue::Bool(!old_tap),
    );
    set(
        PointerSetting::TrackpadNaturalScroll,
        PointerValue::Bool(!old_natural),
    );
    let state = snapshot().unwrap();
    assert_eq!(state.number(PointerSetting::MouseSpeed), Some(0.35));
    assert_eq!(state.number(PointerSetting::TrackpadSpeed), Some(0.35));
    assert_eq!(
        state.boolean(PointerSetting::TrackpadTapToClick),
        Some(!old_tap)
    );
    assert_eq!(
        state.boolean(PointerSetting::TrackpadNaturalScroll),
        Some(!old_natural)
    );
    assert_eq!(request("reload").unwrap().trim(), "ok");
    let state = snapshot().unwrap();
    assert_eq!(state.number(PointerSetting::MouseSpeed), Some(0.35));
    assert_eq!(
        state.boolean(PointerSetting::TrackpadTapToClick),
        Some(!old_tap)
    );
    assert_eq!(
        state.boolean(PointerSetting::TrackpadNaturalScroll),
        Some(!old_natural)
    );
    let errors: Vec<String> = serde_json::from_str(&request("j/configerrors").unwrap()).unwrap();
    assert!(
        errors.iter().all(|error| error.trim().is_empty()),
        "{errors:?}"
    );
    set(PointerSetting::MouseSpeed, PointerValue::Number(old_speed));
    set(
        PointerSetting::TrackpadTapToClick,
        PointerValue::Bool(old_tap),
    );
    set(
        PointerSetting::TrackpadNaturalScroll,
        PointerValue::Bool(old_natural),
    );
}
