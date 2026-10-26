use macstill_config::{
    ControlCenterModule as Module, MenuBarVisibility as Visibility, Preferences, read_at, save_at,
};
#[test]
fn older_preferences_keep_the_existing_menu_bar_and_accept_new_module_choices() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("preferences.json");
    std::fs::write(&path, r#"{"version":1,"dark":true,"dock_size":52}"#).unwrap();
    let mut preferences = read_at(&path).unwrap();
    assert!(preferences.dark);
    assert_eq!(preferences.dock_size, 52);
    assert!(preferences.control_center.battery_in_menu_bar);
    assert!(preferences.control_center.battery_percentage);
    assert!(!preferences.control_center.battery_in_control_center);
    assert!(!preferences.control_center.visible(Module::Wifi, true));
    preferences
        .control_center
        .menu_bar
        .insert(Module::Wifi, Visibility::Always);
    preferences
        .control_center
        .menu_bar
        .insert(Module::Focus, Visibility::WhenActive);
    preferences.control_center.battery_in_menu_bar = false;
    preferences.control_center.battery_in_control_center = true;
    save_at(&path, &preferences).unwrap();
    assert_eq!(read_at(&path).unwrap(), preferences);
}
#[test]
fn visibility_respects_activity_and_cannot_enable_unimplemented_modules() {
    let mut preferences = Preferences::default().control_center;
    preferences
        .menu_bar
        .insert(Module::Focus, Visibility::WhenActive);
    assert!(!preferences.visible(Module::Focus, false));
    assert!(preferences.visible(Module::Focus, true));
    preferences
        .menu_bar
        .insert(Module::Sound, Visibility::Always);
    assert!(preferences.visible(Module::Sound, false));
    preferences
        .menu_bar
        .insert(Module::StageManager, Visibility::Always);
    assert!(preferences.visible(Module::StageManager, true));
    preferences
        .menu_bar
        .insert(Module::ScreenMirroring, Visibility::Always);
    assert!(!preferences.visible(Module::ScreenMirroring, true));
}
