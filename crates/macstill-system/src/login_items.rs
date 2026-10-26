//! XDG autostart entries. System entries are changed through user overrides only.
use crate::{ErrorKind, Result, SystemError};
use gio::{
    glib::{self, KeyFile, KeyFileFlags},
    prelude::*,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
const GROUP: &str = "Desktop Entry";
#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub applicable: bool,
}
#[derive(Clone, Debug)]
pub struct Application {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub items: Vec<Item>,
    pub applications: Vec<Application>,
}
fn failure(e: impl std::fmt::Display) -> SystemError {
    SystemError::new(ErrorKind::Failed, "Login items", e.to_string())
}
fn user_dir() -> PathBuf {
    glib::user_config_dir().join("autostart")
}
fn entries() -> Result<BTreeMap<String, PathBuf>> {
    let mut result = BTreeMap::new();
    let mut dirs: Vec<_> = glib::system_config_dirs()
        .into_iter()
        .map(|p| p.join("autostart"))
        .collect();
    dirs.reverse();
    dirs.push(user_dir());
    for dir in dirs {
        let iter = match std::fs::read_dir(dir) {
            Ok(iter) => iter,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(SystemError::io("Read login items", e)),
        };
        for entry in iter {
            let entry = entry.map_err(|e| SystemError::io("Read login items", e))?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "desktop") {
                result.insert(entry.file_name().to_string_lossy().into_owned(), path);
            }
        }
    }
    Ok(result)
}
fn read(path: &Path) -> Result<KeyFile> {
    let key = KeyFile::new();
    key.load_from_file(
        path,
        KeyFileFlags::KEEP_COMMENTS | KeyFileFlags::KEEP_TRANSLATIONS,
    )
    .map_err(failure)?;
    Ok(key)
}
fn applicable(key: &KeyFile) -> bool {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let matches = |names: &str| {
        names
            .split(';')
            .filter(|s| !s.is_empty())
            .any(|name| desktop.split(':').any(|d| d == name))
    };
    if let Ok(names) = key.string(GROUP, "OnlyShowIn") {
        if !matches(&names) {
            return false;
        }
    }
    if let Ok(names) = key.string(GROUP, "NotShowIn") {
        if matches(&names) {
            return false;
        }
    }
    if let Ok(exec) = key.string(GROUP, "TryExec") {
        if glib::find_program_in_path(exec.as_str()).is_none() {
            return false;
        }
    }
    true
}
pub fn snapshot() -> Result<Snapshot> {
    let mut items = Vec::new();
    for (id, path) in entries()? {
        let Ok(key) = read(&path) else {
            continue;
        };
        if key.string(GROUP, "Type").ok().as_deref() != Some("Application") {
            continue;
        }
        items.push(Item {
            name: key
                .locale_string(GROUP, "Name", None)
                .map(|s| s.to_string())
                .unwrap_or_else(|_| id.clone()),
            id,
            enabled: !key.boolean(GROUP, "Hidden").unwrap_or(false)
                && key
                    .boolean(GROUP, "X-GNOME-Autostart-enabled")
                    .unwrap_or(true),
            applicable: applicable(&key),
        });
    }
    items.sort_by_key(|i| i.name.to_lowercase());
    let mut applications: Vec<_> = gio::AppInfo::all()
        .into_iter()
        .filter(|app| app.should_show())
        .filter_map(|app| {
            let id = app.id()?.to_string();
            if items.iter().any(|item| item.id == id)
                || gio::DesktopAppInfo::new(&id)?.filename().is_none()
            {
                return None;
            }
            Some(Application {
                id,
                name: app.display_name().to_string(),
            })
        })
        .collect();
    applications.sort_by_key(|a| a.name.to_lowercase());
    Ok(Snapshot {
        items,
        applications,
    })
}
fn save(id: &str, key: &KeyFile) -> Result<()> {
    if !id.ends_with(".desktop") || id.contains('/') || id.contains('\\') || id == ".desktop" {
        return Err(SystemError::invalid(
            "Login items",
            "Invalid application ID",
        ));
    }
    std::fs::create_dir_all(user_dir()).map_err(|e| SystemError::io("Save login item", e))?;
    let file = gio::File::for_path(user_dir().join(id));
    file.replace_contents(
        key.to_data().as_bytes(),
        None,
        false,
        gio::FileCreateFlags::PRIVATE | gio::FileCreateFlags::REPLACE_DESTINATION,
        gio::Cancellable::NONE,
    )
    .map_err(failure)?;
    Ok(())
}
pub fn set_enabled(id: &str, enabled: bool) -> Result<()> {
    let paths = entries()?;
    let path = paths
        .get(id)
        .ok_or_else(|| SystemError::invalid("Login items", "Application no longer exists"))?;
    let key = read(path)?;
    key.set_boolean(GROUP, "Hidden", !enabled);
    key.set_boolean(GROUP, "X-GNOME-Autostart-enabled", enabled);
    save(id, &key)
}
pub fn add(id: &str) -> Result<()> {
    let app = gio::DesktopAppInfo::new(id)
        .ok_or_else(|| SystemError::invalid("Login items", "Application no longer exists"))?;
    let path = app
        .filename()
        .ok_or_else(|| SystemError::invalid("Login items", "Application has no desktop entry"))?;
    let key = read(&path)?;
    key.set_boolean(GROUP, "Hidden", false);
    key.set_boolean(GROUP, "X-GNOME-Autostart-enabled", true);
    // A user's explicit selection applies to their current desktop.
    let _ = key.remove_key(GROUP, "OnlyShowIn");
    let _ = key.remove_key(GROUP, "NotShowIn");
    save(id, &key)
}
/// Run once from the session startup script, never when opening Settings.
pub fn launch() -> Result<()> {
    for (_, path) in entries()? {
        let Ok(key) = read(&path) else {
            continue;
        };
        if key.boolean(GROUP, "Hidden").unwrap_or(false)
            || !key
                .boolean(GROUP, "X-GNOME-Autostart-enabled")
                .unwrap_or(true)
            || !applicable(&key)
        {
            continue;
        }
        if let Some(app) = gio::DesktopAppInfo::from_filename(path) {
            if let Err(error) = app.launch(&[], None::<&gio::AppLaunchContext>) {
                eprintln!("Cannot launch login item: {error}");
            }
        }
    }
    Ok(())
}
