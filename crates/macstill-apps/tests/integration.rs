use macstill_apps as catalog;
use std::fs;
use tempfile::tempdir;

#[test]
fn catalog_obeys_user_overrides_hidden_tombstones_and_nested_ids() {
    let dir = tempdir().unwrap();
    let user = dir.path().join("user");
    let system = dir.path().join("system");
    fs::create_dir_all(user.join("vendor")).unwrap();
    fs::create_dir_all(&system).unwrap();
    let desktop = |name: &str| {
        format!(
            "[Desktop Entry]\nType=Application\nName={name}\nExec=/bin/true %U\nIcon=nonexistent-macstill-test\n"
        )
    };
    fs::write(system.join("one.desktop"), desktop("System One")).unwrap();
    fs::write(user.join("one.desktop"), desktop("User One")).unwrap();
    fs::write(system.join("hidden.desktop"), desktop("Must not appear")).unwrap();
    fs::write(
        user.join("hidden.desktop"),
        "[Desktop Entry]\nHidden=true\n",
    )
    .unwrap();
    fs::write(
        user.join("helper.desktop"),
        format!("{}NoDisplay=true\n", desktop("Helper")),
    )
    .unwrap();
    fs::write(user.join("vendor/two.desktop"), desktop("Nested App")).unwrap();
    let apps = catalog::scan_roots(&[user, system]);
    assert_eq!(apps.len(), 2);
    assert!(apps.iter().any(|a| a.name == "User One"));
    assert!(apps.iter().any(|a| a.id == "vendor-two.desktop"));
}

#[test]
fn window_matching_honors_desktop_id_and_startup_class() {
    let app = catalog::DesktopApp {
        id: "org.example.App.desktop".into(),
        name: "Example".into(),
        desktop_file: "/example.desktop".into(),
        icon: None,
        class_aliases: Vec::new(),
        startup_class: Some("ExampleWindow".into()),
    };
    assert!(app.matches_class("ORG.EXAMPLE.APP"));
    assert!(app.matches_class("examplewindow"));
    assert!(!app.matches_class("OtherApp"));
}

#[test]
fn dehelper_window_matches_vendor_desktop_entry_using_icon_identity() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("eusoft-dehelper.desktop"),
        "[Desktop Entry]\nType=Application\nName=德语助手\nExec=/bin/true %F\nIcon=com.eusoft.dehelper\n").unwrap();
    let apps = catalog::scan_roots(&[dir.path().to_path_buf()]);
    let app = &apps[0];
    assert!(app.matches_class("dehelper"));
    assert!(app.matches_class("com.eusoft.dehelper"));
    assert!(!app.matches_class("helper"));
    assert!(!app.matches_class(""));
}
