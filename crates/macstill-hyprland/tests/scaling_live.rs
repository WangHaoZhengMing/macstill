//! Run only in an isolated test compositor with a virtual SCALE-TEST output.
#[test]
#[ignore = "requires isolated Hyprland and MACSTILL_DISPLAY_TEST=1"]
fn applies_scaling_and_restores_it_after_config_reload() {
    assert_eq!(std::env::var("MACSTILL_DISPLAY_TEST").as_deref(), Ok("1"));
    use macstill_hyprland::{displays, request};
    displays::set_scale("SCALE-TEST", 1.5).unwrap();
    let find = || {
        displays::monitors()
            .unwrap()
            .into_iter()
            .find(|m| m.name == "SCALE-TEST")
            .unwrap()
    };
    assert_eq!(find().scale, 1.5);
    assert_eq!(request("reload").unwrap().trim(), "ok");
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert_eq!(find().scale, 1.5);
    displays::set_scale("SCALE-TEST", 1.).unwrap();
    assert_eq!(find().scale, 1.);
    let dir = std::path::PathBuf::from(std::env::var("XDG_CONFIG_HOME").unwrap()).join("macstill");
    let original_lua = std::fs::read(dir.join("display-scales.lua")).unwrap();
    let original_json = std::fs::read(dir.join("display-scales.json")).unwrap();
    std::fs::create_dir(dir.join("display-scales.json.tmp")).unwrap();
    assert!(displays::set_scale("SCALE-TEST", 1.25).is_err());
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!(find().scale, 1.);
    assert_eq!(
        std::fs::read(dir.join("display-scales.lua")).unwrap(),
        original_lua
    );
    assert_eq!(
        std::fs::read(dir.join("display-scales.json")).unwrap(),
        original_json
    );
    std::fs::remove_dir(dir.join("display-scales.json.tmp")).unwrap();
}
