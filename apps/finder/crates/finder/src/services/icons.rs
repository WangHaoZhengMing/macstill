use std::path::Path;
/// What an icon is cached by: per file where files differ (apps, links,
/// anything with a thumbnail), else per extension.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Path(std::path::PathBuf, u32),
    Ext(String, u32),
}

const OWN_ICON: &[&str] = &["exe", "lnk", "url", "ico", "msi", "appref-ms", "cpl", "scr"];
const THUMBNAIL: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "heic", "tif", "tiff", "mp4", "mov", "mkv", "avi",
    "webm", "m4v", "pdf", "psd", "svg",
];

fn ext(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// Whether this file shows its own picture rather than its kind's icon.
pub fn has_thumbnail(path: &Path) -> bool {
    THUMBNAIL.contains(&ext(path).as_str())
}

pub fn key(path: &Path, px: u32) -> Key {
    let e = ext(path);
    if OWN_ICON.contains(&e.as_str()) || THUMBNAIL.contains(&e.as_str()) {
        Key::Path(path.to_path_buf(), px)
    } else {
        Key::Ext(e, px)
    }
}

pub fn load(path: &Path, px: u32) -> Option<image::RgbaImage> {
    if !has_thumbnail(path) {
        return None;
    }
    let image = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    Some(image.thumbnail(px.max(1), px.max(1)).into_rgba8())
}
