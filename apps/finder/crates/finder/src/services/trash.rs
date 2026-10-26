//! Freedesktop Trash data, adapted to the existing Finder file entries.
use crate::model::entry::Entry;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

pub fn storage_path(item: &trash::TrashItem) -> Option<PathBuf> {
    let info = Path::new(&item.id);
    if info.extension()? != "trashinfo" || info.parent()?.file_name()? != "info" {
        return None;
    }
    Some(
        info.parent()?
            .parent()?
            .join("files")
            .join(info.file_stem()?),
    )
}

pub fn list() -> Result<(Vec<Entry>, HashMap<PathBuf, trash::TrashItem>), String> {
    let mut entries = Vec::new();
    let mut items = HashMap::new();
    for item in trash::os_limited::list().map_err(|e| e.to_string())? {
        let Some(path) = storage_path(&item) else {
            continue;
        };
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        let deleted = u64::try_from(item.time_deleted)
            .ok()
            .and_then(|secs| UNIX_EPOCH.checked_add(Duration::from_secs(secs)));
        let mut entry = Entry::from_parts(
            item.name.to_string_lossy().into_owned(),
            path.clone(),
            metadata.is_dir(),
            metadata.len(),
            deleted,
        );
        entry.created = metadata.created().ok();
        entry.accessed = metadata.accessed().ok();
        entries.push(entry);
        items.insert(path, item);
    }
    Ok((entries, items))
}

pub fn restore(items: Vec<trash::TrashItem>) -> Result<(), String> {
    for item in &items {
        let path = item.original_path();
        // symlink_metadata also rejects overwriting a dangling symlink.
        match std::fs::symlink_metadata(&path) {
            Ok(_) => return Err(format!("{} already exists", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    trash::os_limited::restore_all(items).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item(id: &str) -> trash::TrashItem {
        trash::TrashItem {
            id: id.into(),
            name: "original.txt".into(),
            original_parent: "/tmp".into(),
            time_deleted: 0,
        }
    }
    #[test]
    fn storage_paths_support_home_and_both_volume_layouts() {
        for root in [
            "/home/u/.local/share/Trash",
            "/custom/data/Trash",
            "/media/usb/.Trash/1000",
            "/mnt/data/.Trash-1000",
        ] {
            assert_eq!(
                storage_path(&item(&format!("{root}/info/report.txt.2.trashinfo"))),
                Some(PathBuf::from(format!("{root}/files/report.txt.2")))
            );
        }
        assert_eq!(storage_path(&item("/tmp/unrelated/info.txt")), None);
    }
    #[test]
    fn restore_uses_storage_identity_and_preserves_colliding_items() {
        let root = std::env::temp_dir().join(format!(
            "finder-trash-store-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let bin = root.join("Trash");
        let destination = root.join("original");
        std::fs::create_dir_all(bin.join("info")).unwrap();
        std::fs::create_dir_all(bin.join("files")).unwrap();
        std::fs::create_dir_all(&destination).unwrap();
        let make = |stored: &str, content: &[u8]| {
            let info = bin.join("info").join(format!("{stored}.trashinfo"));
            std::fs::write(&info, b"[Trash Info]\n").unwrap();
            std::fs::write(bin.join("files").join(stored), content).unwrap();
            trash::TrashItem {
                id: info.into_os_string(),
                name: "report.txt".into(),
                original_parent: destination.clone(),
                time_deleted: 0,
            }
        };
        let first = make("report.txt", b"first");
        let second = make("report.txt.2", b"second");
        restore(vec![second.clone()]).unwrap();
        assert_eq!(
            std::fs::read(destination.join("report.txt")).unwrap(),
            b"second"
        );
        assert!(!Path::new(&second.id).exists());
        assert!(restore(vec![first.clone()]).is_err());
        assert_eq!(
            std::fs::read(storage_path(&first).unwrap()).unwrap(),
            b"first"
        );
        assert!(Path::new(&first.id).exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
