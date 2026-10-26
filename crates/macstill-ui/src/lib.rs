//! Macstill-specific artwork and application setup. Generic controls live in swiftui_gpui.
use gpui_kit::App;

pub fn init(cx: &mut App) {
    swiftui_gpui::init(cx);
}

pub fn asset_root() -> std::path::PathBuf {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default();
    let installed = home.join(".local/share/macstill/icons");
    if installed.is_dir() {
        return installed;
    }
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icons")
}

mod icon;
pub use icon::icon;

/// Resolve the shared desktop/Launchpad wallpaper, with an installed/repo fallback.
pub fn wallpaper(custom: Option<&std::path::Path>) -> std::path::PathBuf {
    if let Some(path) = custom.filter(|p| p.is_file()) {
        return path.to_path_buf();
    }
    let installed = asset_root().join("../launchpad-wallpaper.jpg");
    if installed.is_file() {
        installed
    } else {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/launchpad-wallpaper.jpg")
    }
}

/// Bundled artwork for real installed applications, shared by all app lists.
pub fn application_icon_override(id: &str) -> Option<std::path::PathBuf> {
    let name = match id.to_ascii_lowercase().as_str() {
        "org.macstill.finder.desktop" => "finder.png",
        "org.gnome.clocks.desktop" | "org.kde.kclock.desktop" => "clock.png",
        "snap-store_snap-store.desktop"
        | "org.gnome.software.desktop"
        | "org.kde.discover.desktop" => "app_store.png",
        _ => return None,
    };
    let installed = asset_root().join(name);
    if installed.is_file() {
        return Some(installed);
    }
    let bundled = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/icons")
        .join(name);
    bundled.is_file().then_some(bundled)
}

/// Resolve individual assets even when an older local installation lacks new artwork.
pub fn asset(name: &str) -> std::path::PathBuf {
    let installed = asset_root().join(name);
    if installed.is_file() {
        installed
    } else {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/icons")
            .join(name)
    }
}
