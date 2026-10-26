use macstill::desktop;
use std::{fs, os::unix::fs::symlink};

#[test]
fn imports_preserve_originals_and_keep_both_names_on_collision() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("report.txt");
    let root = temp.path().join("Desktop");
    fs::write(&source, b"original").unwrap();
    desktop::import(&root, &[source.clone()]).unwrap();
    fs::write(&source, b"updated").unwrap();
    desktop::import(&root, &[source.clone()]).unwrap();
    assert_eq!(fs::read(root.join("report.txt")).unwrap(), b"original");
    assert_eq!(fs::read(root.join("report copy.txt")).unwrap(), b"updated");
    assert_eq!(fs::read(source).unwrap(), b"updated");
}
#[test]
fn folders_copy_recursively_without_following_symbolic_links() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("Project");
    let root = temp.path().join("Desktop");
    fs::create_dir_all(source.join("nested")).unwrap();
    fs::write(source.join("nested/file"), b"data").unwrap();
    symlink("..", source.join("nested/parent")).unwrap();
    desktop::import(&root, &[source.clone()]).unwrap();
    assert_eq!(fs::read(root.join("Project/nested/file")).unwrap(), b"data");
    assert_eq!(
        fs::read_link(root.join("Project/nested/parent")).unwrap(),
        std::path::Path::new("..")
    );
    assert!(source.exists());
}
#[test]
fn copying_an_ancestor_onto_its_own_desktop_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Desktop");
    assert!(desktop::import(&root, &[temp.path().to_path_buf()]).is_err());
    assert!(fs::read_dir(root).unwrap().next().is_none());
}
#[test]
fn desktop_items_are_not_duplicated_when_dropped_on_the_same_desktop() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("file");
    fs::write(&path, b"data").unwrap();
    desktop::import(temp.path(), &[path]).unwrap();
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}
#[test]
fn new_folders_and_external_changes_appear_in_the_desktop_listing() {
    let temp = tempfile::tempdir().unwrap();
    let a = desktop::new_folder(temp.path()).unwrap();
    let b = desktop::new_folder(temp.path()).unwrap();
    assert_ne!(a, b);
    fs::write(temp.path().join("notes.txt"), b"note").unwrap();
    fs::write(temp.path().join(".hidden"), b"hidden").unwrap();
    let entries = desktop::entries(temp.path()).unwrap();
    assert_eq!(entries.len(), 3);
    assert!(entries[0].directory && entries[1].directory && !entries[2].directory);
    fs::remove_file(temp.path().join("notes.txt")).unwrap();
    assert_eq!(desktop::entries(temp.path()).unwrap().len(), 2);
}
#[test]
fn dangling_destination_symlinks_are_preserved_on_name_collision() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Desktop");
    fs::create_dir(&root).unwrap();
    symlink("missing", root.join("file.txt")).unwrap();
    let source = temp.path().join("file.txt");
    fs::write(&source, b"safe").unwrap();
    desktop::import(&root, &[source]).unwrap();
    assert_eq!(
        fs::read_link(root.join("file.txt")).unwrap(),
        std::path::Path::new("missing")
    );
    assert_eq!(fs::read(root.join("file copy.txt")).unwrap(), b"safe");
}

#[test]
fn rename_rejects_collisions_and_paths_and_preserves_symlinks() {
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("a.txt");
    let b = temp.path().join("b.txt");
    fs::write(&a, "one").unwrap();
    fs::write(&b, "two").unwrap();
    assert!(desktop::rename(&a, "b.txt").is_err());
    for name in ["", ".", "..", "../outside", "nested/name"] {
        assert!(desktop::rename(&a, name).is_err());
    }
    assert_eq!(fs::read_to_string(&a).unwrap(), "one");
    assert_eq!(fs::read_to_string(&b).unwrap(), "two");
    symlink("missing", temp.path().join("link")).unwrap();
    let renamed = desktop::rename(&temp.path().join("link"), "renamed").unwrap();
    assert_eq!(
        fs::read_link(renamed).unwrap(),
        std::path::Path::new("missing")
    );
}
#[test]
fn duplicate_keeps_original_and_uses_unused_name() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("notes.txt");
    fs::write(&file, "hello").unwrap();
    let a = desktop::duplicate(&file).unwrap();
    let b = desktop::duplicate(&file).unwrap();
    assert_ne!(a, b);
    assert_ne!(a, file);
    assert_eq!(fs::read(a).unwrap(), b"hello");
    assert_eq!(fs::read(b).unwrap(), b"hello");
    assert_eq!(fs::read(file).unwrap(), b"hello");
}
#[test]
fn positions_roundtrip_and_stay_visible_after_display_resize() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("layouts/output.json");
    let mut positions = desktop::Positions::new();
    positions.insert(temp.path().join("file"), [900., 600.]);
    desktop::save_positions(&path, &positions).unwrap();
    assert_eq!(desktop::load_positions(&path), positions);
    assert_eq!(
        desktop::clamp_position([900., 600.], 800., 600.),
        [708., 420.]
    );
    assert_eq!(desktop::clamp_position([-10., -20.], 800., 600.), [0., 34.]);
}
