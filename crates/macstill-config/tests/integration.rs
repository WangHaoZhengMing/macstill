use macstill_config::{self as config, Preferences};
use std::fs;
use tempfile::tempdir;
#[test]
fn unbound_finder_uses_builtin_without_changing_saved_preferences() {
    let old: Preferences = serde_json::from_str(
        r#"{"shortcuts":[{"id":"finder","name":"Finder","asset":"finder.png","visible":true,"app_id":null}]}"#,
    ).unwrap();
    assert_eq!(old.shortcuts[0].effective_app_id(), Some("org.macstill.Finder.desktop"));
    assert!(old.shortcuts[0].app_id.is_none());
    let mut custom = old.shortcuts[0].clone();
    custom.app_id = Some("other-files.desktop".into());
    assert_eq!(custom.effective_app_id(), Some("other-files.desktop"));
    assert!(Preferences::default().shortcuts[1].effective_app_id().is_none());
}

#[test]
fn hiding_shortcut_keeps_binding_and_custom_artwork() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    let mut p = Preferences::default();
    p.shortcuts[0].app_id = Some("org.example.App.desktop".into());
    p.shortcuts[0].custom_icon = Some("/example/icon.png".into());
    p.shortcuts[0].visible = false;
    config::save_at(&path, &p).unwrap();
    let mut restored = config::read_at(&path).unwrap();
    assert_eq!(p, restored);
    restored.shortcuts[0].visible = true;
    assert_eq!(restored.shortcuts[0].app_id, p.shortcuts[0].app_id);
    assert_eq!(
        restored.shortcuts[0].custom_icon,
        p.shortcuts[0].custom_icon
    );
}
#[test]
fn malformed_and_future_configuration_are_not_silently_accepted() {
    let dir = tempdir().unwrap();
    let p = dir.path().join("prefs.json");
    fs::write(&p, "{broken}").unwrap();
    assert!(config::read_at(&p).is_err());
    assert_eq!(fs::read_to_string(&p).unwrap(), "{broken}");
    fs::write(&p, r#"{"version":42}"#).unwrap();
    assert!(config::read_at(&p).is_err());
}
#[test]
fn imported_icon_is_owned_and_bounded() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("large.png");
    image::RgbaImage::from_pixel(512, 256, image::Rgba([40, 90, 240, 255]))
        .save(&source)
        .unwrap();
    let copy = config::import_icon_at(&source, &dir.path().join("owned")).unwrap();
    fs::remove_file(source).unwrap();
    let img = image::open(copy).unwrap();
    assert_eq!((img.width(), img.height()), (128, 64));
}
#[test]
fn svg_import_rejects_invalid_xml_and_external_assets() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("icon.svg");
    fs::write(&source, "<svg>broken").unwrap();
    assert!(config::import_icon_at(&source, &dir.path().join("owned")).is_err());
    fs::write(&source,r#"<svg xmlns="http://www.w3.org/2000/svg"><image href="https://example.com/image.png"/></svg>"#).unwrap();
    assert!(config::import_icon_at(&source, &dir.path().join("owned")).is_err());
}

#[test]
fn existing_preferences_without_wallpaper_keep_the_default() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    fs::write(&path, r#"{"version":1,"dark":true,"dock_size":48}"#).unwrap();
    let preferences = config::read_at(&path).unwrap();
    assert!(preferences.wallpaper.is_none());
    assert!(preferences.dark);
    assert_eq!(preferences.dock_size, 48);
}
#[test]
fn wallpaper_import_keeps_full_resolution_and_survives_source_removal() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("photo.png");
    image::RgbImage::from_pixel(1920, 1080, image::Rgb([23, 45, 67]))
        .save(&source)
        .unwrap();
    let imported = config::import_wallpaper_at(&source, &dir.path().join("wallpapers")).unwrap();
    fs::remove_file(&source).unwrap();
    assert_eq!(image::image_dimensions(&imported).unwrap(), (1920, 1080));
    let mut preferences = Preferences::default();
    preferences.wallpaper = Some(imported);
    let path = dir.path().join("preferences.json");
    config::save_at(&path, &preferences).unwrap();
    assert_eq!(config::read_at(&path).unwrap(), preferences);
}
#[test]
fn new_wallpaper_content_gets_a_new_render_cache_path() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("photo.png");
    let target = dir.path().join("wallpapers");
    image::RgbImage::from_pixel(10, 10, image::Rgb([255, 0, 0]))
        .save(&source)
        .unwrap();
    let first = config::import_wallpaper_at(&source, &target).unwrap();
    image::RgbImage::from_pixel(10, 10, image::Rgb([0, 0, 255]))
        .save(&source)
        .unwrap();
    let second = config::import_wallpaper_at(&source, &target).unwrap();
    assert_ne!(first, second);
    assert!(first.is_file() && second.is_file());
    fs::write(&source, b"not an image").unwrap();
    assert!(config::import_wallpaper_at(&source, &target).is_err());
    assert!(second.is_file());
}

#[test]
fn new_shortcuts_are_added_to_existing_preferences_without_resetting_choices() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    let mut original = Preferences::default();
    original
        .shortcuts
        .retain(|s| !["clock", "app_store", "preview"].contains(&s.id.as_str()));
    original.shortcuts[0].app_id = Some("my-files.desktop".into());
    original.shortcuts[0].visible = false;
    original.shortcuts[0].custom_icon = Some("/my/finder.png".into());
    config::save_at(&path, &original).unwrap();
    let restored = config::read_at(&path).unwrap();
    assert_eq!(
        &restored.shortcuts[..original.shortcuts.len()],
        &original.shortcuts
    );
    for id in ["clock", "app_store", "preview"] {
        let new = restored.shortcuts.iter().find(|s| s.id == id).unwrap();
        assert_eq!(new.asset, format!("{id}.png"));
        assert!(!new.visible);
        assert!(new.app_id.is_none());
    }
    let mut customized = restored.clone();
    let clock = customized
        .shortcuts
        .iter_mut()
        .find(|s| s.id == "clock")
        .unwrap();
    clock.app_id = Some("org.gnome.clocks.desktop".into());
    clock.visible = true;
    config::save_at(&path, &customized).unwrap();
    assert_eq!(config::read_at(&path).unwrap(), customized);
}
