//! Installed desktop applications, aggregated without moving package files.
use crate::model::entry::Entry;
use macstill_apps::DesktopApp;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{OnceLock, RwLock},
};
fn registry() -> &'static RwLock<HashMap<PathBuf, DesktopApp>> {
    static APPS: OnceLock<RwLock<HashMap<PathBuf, DesktopApp>>> = OnceLock::new();
    APPS.get_or_init(Default::default)
}
pub fn list() -> Vec<Entry> {
    list_roots(&macstill_apps::roots())
}
fn list_roots(roots: &[PathBuf]) -> Vec<Entry> {
    let apps = macstill_apps::scan_roots(roots);
    let entries = apps
        .iter()
        .map(|app| {
            let mut entry =
                Entry::from_parts(app.name.clone(), app.desktop_file.clone(), false, 0, None);
            entry.kind = "Application".into();
            entry
        })
        .collect();
    *registry().write().unwrap() = apps
        .into_iter()
        .map(|app| (app.desktop_file.clone(), app))
        .collect();
    entries
}
pub fn icon(path: &Path) -> Option<PathBuf> {
    registry()
        .read()
        .unwrap()
        .get(path)
        .and_then(|app| app.icon.clone())
}
pub fn launch(path: &Path) -> Result<(), String> {
    let app = registry()
        .read()
        .unwrap()
        .get(path)
        .cloned()
        .ok_or("Application was removed; refresh Applications")?;
    macstill_apps::launch(&app).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_entries_have_application_names_and_keep_launcher_paths() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("org.example.Test.desktop");
        std::fs::write(
            &path,
            "[Desktop Entry]\nType=Application\nName=Example App\nExec=/bin/true\n",
        )
        .unwrap();
        let entries = list_roots(&[dir.path().into(), dir.path().join("missing")]);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Example App");
        assert_eq!(entries[0].kind, "Application");
        assert_eq!(entries[0].path, path);
        assert!(!entries[0].is_dir);
    }
}
