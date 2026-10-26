use anyhow::{Context, Result};
use gio::prelude::*;
use serde::Serialize;
use std::{
    collections::HashSet,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};
pub const FINDER_APP_ID: &str = "org.macstill.Finder.desktop";
#[derive(Clone, Debug, Serialize)]
pub struct DesktopApp {
    pub id: String,
    pub name: String,
    pub desktop_file: PathBuf,
    pub icon: Option<PathBuf>,
    pub startup_class: Option<String>,
    pub class_aliases: Vec<String>,
}
impl DesktopApp {
    /// Match compositor window classes against desktop IDs or StartupWMClass.
    pub fn matches_class(&self, class: &str) -> bool {
        !class.is_empty()
            && (self
                .id
                .trim_end_matches(".desktop")
                .eq_ignore_ascii_case(class)
                || self
                    .startup_class
                    .as_ref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(class))
                || self
                    .class_aliases
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(class)))
    }
}
pub fn roots() -> Vec<PathBuf> {
    let home = gio::glib::home_dir();
    let mut out = vec![
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"))
            .join("applications"),
    ];
    let dirs =
        std::env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    out.extend(std::env::split_paths(&dirs).map(|d| d.join("applications")));
    out.extend([
        home.join(".local/share/flatpak/exports/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        PathBuf::from("/var/lib/snapd/desktop/applications"),
    ]);
    let mut seen = HashSet::new();
    out.retain(|p| seen.insert(p.clone()));
    out
}
fn files(root: &Path, at: &Path, out: &mut Vec<(String, PathBuf)>) {
    if let Ok(entries) = fs::read_dir(at) {
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|x| x.file_name());
        for entry in entries {
            let p = entry.path();
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                files(root, &p, out)
            } else if p.extension().is_some_and(|e| e == "desktop")
                && let Ok(rel) = p.strip_prefix(root)
            {
                out.push((rel.to_string_lossy().replace('/', "-"), p))
            }
        }
    }
}
/// User entries (including Hidden=true tombstones) override system entries by desktop ID.
pub fn scan_roots(roots: &[PathBuf]) -> Vec<DesktopApp> {
    let mut seen = HashSet::new();
    let mut apps = vec![];
    for root in roots {
        let mut entries = vec![];
        files(root, root, &mut entries);
        for (id, path) in entries {
            if !seen.insert(id.clone()) {
                continue;
            }
            let Some(app) = gio::DesktopAppInfo::from_filename(&path) else {
                continue;
            };
            if app.is_hidden() || app.is_nodisplay() || !app.shows_in(Some("Hyprland")) {
                continue;
            }
            let Some(exec) = app.commandline() else {
                continue;
            };
            if exec.is_empty() {
                continue;
            }
            let icon = app.icon().and_then(|icon| {
                if let Some(file) = icon.downcast_ref::<gio::FileIcon>() {
                    return file.file().path();
                }
                if let Some(themed) = icon.downcast_ref::<gio::ThemedIcon>() {
                    for name in themed.names() {
                        for theme in ["Mac_icon", "hicolor", "Adwaita"] {
                            if let Some(p) = freedesktop_icons::lookup(name.as_str())
                                .with_size(64)
                                .with_theme(theme)
                                .with_cache()
                                .find()
                            {
                                return Some(p);
                            }
                        }
                    }
                }
                None
            });
            let mut class_aliases = Vec::new();
            if let Some(icon) = app.icon()
                && let Some(themed) = icon.downcast_ref::<gio::ThemedIcon>()
            {
                for name in themed.names() {
                    // Vendor-prefixed desktop IDs sometimes omit StartupWMClass.
                    // Require the icon ID to corroborate the short application ID.
                    if let Some((_, short)) = name.rsplit_once('.') {
                        let stem = id.trim_end_matches(".desktop");
                        if short.len() >= 3
                            && (stem.ends_with(&format!("-{short}"))
                                || stem.ends_with(&format!(".{short}")))
                        {
                            class_aliases.push(short.to_string());
                            class_aliases.push(name.to_string());
                        }
                    }
                }
            }
            apps.push(DesktopApp {
                class_aliases,
                id,
                name: app.display_name().to_string(),
                desktop_file: path,
                icon,
                startup_class: app.startup_wm_class().map(|s| s.to_string()),
            });
        }
    }
    apps.sort_by_cached_key(|a| a.name.to_lowercase());
    apps
}
pub fn scan() -> Vec<DesktopApp> {
    let mut apps = scan_roots(&roots());
    // The built-in component is available even in a development checkout or
    // before desktop entries have been installed. Keep installed artwork.
    if !apps.iter().any(|app| app.id == FINDER_APP_ID) {
        apps.push(DesktopApp {
            id: FINDER_APP_ID.into(),
            name: "Finder".into(),
            desktop_file: PathBuf::new(),
            icon: None,
            startup_class: Some("org.macstill.Finder".into()),
            class_aliases: Vec::new(),
        });
        apps.sort_by_cached_key(|a| a.name.to_lowercase());
    }
    apps
}
pub fn launch(app: &DesktopApp) -> Result<()> {
    if app.id == FINDER_APP_ID {
        return open_finder(&[]);
    }
    let info = gio::DesktopAppInfo::from_filename(&app.desktop_file)
        .context("Application was removed; refresh the app list")?;
    // GIO implements Exec quoting, field codes, DBus activation, Terminal and file arguments.
    info.launch(&[], None::<&gio::AppLaunchContext>)
        .with_context(|| format!("Could not open {}", app.name))
}

/// Launch the Finder shipped alongside this release, independently of the shell.
/// Do not resolve the generic name `finder` through PATH: it may be an old copy
/// or a different application. OS strings preserve arbitrary Linux filenames.
pub fn open_finder(args: &[OsString]) -> Result<()> {
    let program = std::env::current_exe()?.with_file_name("finder");
    let mut child = std::process::Command::new(&program)
        .args(args)
        .spawn()
        .with_context(|| {
            format!(
                "Cannot start built-in Finder at {}; build or reinstall the complete Macstill workspace",
                program.display()
            )
        })?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Start a companion executable next to this executable, falling back to PATH.
pub fn spawn_companion(name: &str, args: &[&str]) -> Result<()> {
    let sibling = std::env::current_exe()?.with_file_name(name);
    let program = if sibling.is_file() {
        sibling
    } else {
        PathBuf::from(name)
    };
    let mut child = std::process::Command::new(&program)
        .args(args)
        .spawn()
        .with_context(|| format!("Cannot start {}", program.display()))?;
    // Shell processes are long-lived; reap both app processes and short activation helpers.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
