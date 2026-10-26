//! Test the actual CLI without starting a renderer or touching the user's session.
use std::{
    fs,
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    process::Command,
    time::{Duration, Instant},
};

#[test]
fn finder_entry_uses_matching_release_and_forwards_arguments_without_a_shell() {
    let temp = tempfile::tempdir_in(
        std::path::Path::new(env!("CARGO_BIN_EXE_macstill"))
            .parent()
            .unwrap(),
    )
    .unwrap();
    let shell = temp.path().join("macstill");
    fs::hard_link(env!("CARGO_BIN_EXE_macstill"), &shell).unwrap();
    let record = temp.path().join("arguments");
    let finder = temp.path().join("finder");
    fs::write(
        &finder,
        "#!/bin/sh\nprintf '%s\\0' \"$@\" > \"$FINDER_TEST_RECORD\"\n",
    )
    .unwrap();
    fs::set_permissions(&finder, fs::Permissions::from_mode(0o755)).unwrap();
    // Spaces, shell punctuation and non-UTF-8 bytes are all legal in Linux paths.
    let path =
        std::ffi::OsString::from_vec(b"/tmp/folder with spaces/$(echo unexpected)-\xff".to_vec());
    let output = Command::new(&shell)
        .arg("finder")
        .arg("--reveal")
        .arg(&path)
        .env("FINDER_TEST_RECORD", &record)
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut expected = b"--reveal\0".to_vec();
    expected.extend(path.into_vec());
    expected.push(0);
    let deadline = Instant::now() + Duration::from_secs(5);
    while fs::read(&record).ok().as_deref() != Some(&expected) {
        assert!(
            Instant::now() < deadline,
            "Finder did not receive the original arguments"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    // A missing bundled component must not silently launch an older PATH copy.
    let old_bin = temp.path().join("old-bin");
    fs::create_dir(&old_bin).unwrap();
    fs::rename(&finder, old_bin.join("finder")).unwrap();
    fs::remove_file(&record).unwrap();
    let failed = Command::new(&shell)
        .arg("finder")
        .env("PATH", old_bin)
        .env("FINDER_TEST_RECORD", &record)
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("Cannot start built-in Finder"));
    assert!(!record.exists());
}

#[test]
fn catalog_contains_builtin_finder_without_installing_a_desktop_file() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_macstill"))
        .arg("apps")
        .env("XDG_DATA_HOME", temp.path())
        .env("XDG_DATA_DIRS", temp.path())
        .env("HOME", temp.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let apps: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let finder: Vec<_> = apps
        .as_array()
        .unwrap()
        .iter()
        .filter(|app| app["id"] == macstill::catalog::FINDER_APP_ID)
        .collect();
    assert_eq!(finder.len(), 1);
    assert_eq!(finder[0]["startup_class"], "org.macstill.Finder");
}
