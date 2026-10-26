mod control_center;
use anyhow::{Context, Result, bail};
pub use control_center::{ControlCenterModule, ControlCenterPreferences, MenuBarVisibility};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Preferences {
    pub version: u32,
    pub dark: bool,
    pub wallpaper: Option<PathBuf>,
    pub dock_size: u16,
    pub dock_magnification: bool,
    pub dock_auto_hide: bool,
    pub dock_show_running: bool,
    pub dock_pinned_apps: Vec<String>,
    pub dock_order: Vec<String>,
    pub shortcuts: Vec<Shortcut>,
    pub app_icons: BTreeMap<String, PathBuf>,
    pub control_center: ControlCenterPreferences,
    pub mission_control: MissionControlPreferences,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MissionControlPreferences {
    pub group_by_application: bool,
    pub animate: bool,
}
impl Default for MissionControlPreferences {
    fn default() -> Self {
        Self {
            group_by_application: false,
            animate: true,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Shortcut {
    pub id: String,
    pub name: String,
    pub asset: String,
    pub visible: bool,
    pub app_id: Option<String>,
    #[serde(default)]
    pub custom_icon: Option<PathBuf>,
}
impl Shortcut {
    /// Unbound Finder shortcuts from older preferences use the built-in app.
    /// An explicitly selected alternative file manager still takes precedence.
    pub fn effective_app_id(&self) -> Option<&str> {
        self.app_id
            .as_deref()
            .or_else(|| (self.id == "finder").then_some("org.macstill.Finder.desktop"))
    }
}
const ADDITIONAL_SHORTCUTS: [(&str, &str); 3] = [
    ("clock", "Uhr"),
    ("app_store", "App Store"),
    ("preview", "Preview"),
];
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            dark: false,
            wallpaper: None,
            dock_size: 42,
            dock_magnification: true,
            dock_auto_hide: false,
            dock_show_running: true,
            dock_pinned_apps: Vec::new(),
            dock_order: Vec::new(),
            app_icons: BTreeMap::new(),
            control_center: ControlCenterPreferences::default(),
            mission_control: MissionControlPreferences::default(),
            shortcuts: [
                ("finder", "Finder"),
                ("safari", "Safari"),
                ("messages", "Messages"),
                ("mail", "Mail"),
                ("photos", "Photos"),
                ("music", "Music"),
                ("calendar", "Calendar"),
                ("facetime", "FaceTime"),
                ("notes", "Notes"),
            ]
            .into_iter()
            .chain(ADDITIONAL_SHORTCUTS)
            .map(|(id, name)| Shortcut {
                id: id.into(),
                name: name.into(),
                asset: format!("{id}.png"),
                visible: matches!(id, "finder" | "safari" | "messages" | "notes"),
                app_id: None,
                custom_icon: None,
            })
            .collect(),
        }
    }
}
pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
        .join("macstill")
}
pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
pub fn config_path() -> PathBuf {
    config_dir().join("preferences.json")
}
pub fn read_at(path: &Path) -> Result<Preferences> {
    let mut p: Preferences = serde_json::from_slice(&fs::read(path)?).with_context(|| {
        format!(
            "Cannot read {}; the existing file was preserved",
            path.display()
        )
    })?;
    if p.version != 1 {
        bail!("Unsupported preferences version {}", p.version);
    }
    p.dock_size = p.dock_size.clamp(36, 72);
    // Append new built-ins without replacing bindings, visibility or custom icons.
    for (id, name) in ADDITIONAL_SHORTCUTS {
        if !p.shortcuts.iter().any(|shortcut| shortcut.id == id) {
            p.shortcuts.push(Shortcut {
                id: id.into(),
                name: name.into(),
                asset: format!("{id}.png"),
                visible: false,
                app_id: None,
                custom_icon: None,
            });
        }
    }
    Ok(p)
}
pub fn load() -> Result<Preferences> {
    let path = config_path();
    if path.exists() {
        read_at(&path)
    } else {
        Ok(Preferences::default())
    }
}
pub fn save_at(path: &Path, prefs: &Preferences) -> Result<()> {
    let parent = path.parent().context("Missing configuration directory")?;
    fs::create_dir_all(parent)?;
    let pending = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = fs::File::create(&pending)?;
    use std::io::Write;
    file.write_all(&serde_json::to_vec_pretty(prefs)?)?;
    file.sync_all()?;
    fs::rename(&pending, path)?;
    Ok(())
}
pub fn save(prefs: &Preferences) -> Result<()> {
    save_at(&config_path(), prefs)
}
/// Own a bounded-size copy, so moving the original image never breaks a user's choice.
pub fn import_icon(source: &Path) -> Result<PathBuf> {
    import_icon_at(source, &config_dir().join("icons"))
}
pub fn import_icon_at(source: &Path, folder: &Path) -> Result<PathBuf> {
    if fs::metadata(source)?.len() > 16 * 1024 * 1024 {
        bail!("Choose an image smaller than 16 MiB");
    }
    let bytes = fs::read(source)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    bytes.hash(&mut hasher);

    fs::create_dir_all(folder)?;
    let ext = source
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ext == "svg" {
        let text = std::str::from_utf8(&bytes)?;
        let doc = roxmltree::Document::parse(text).context("Invalid SVG document")?;
        if doc.root_element().tag_name().name() != "svg" {
            bail!("Not an SVG image");
        }
        // Only local self-contained artwork; SVG external references are not imported.
        if text.contains("<!ENTITY")
            || text.contains("<!DOCTYPE")
            || text.contains("href=")
            || text.contains("url(")
        {
            bail!("Use a self-contained SVG without external references");
        }
        let tree = resvg::usvg::Tree::from_data(&bytes, &resvg::usvg::Options::default())
            .context("Cannot decode SVG artwork")?;
        let mut pixmap = resvg::tiny_skia::Pixmap::new(128, 128).context("Cannot allocate icon")?;
        let dimensions = tree.size();
        let factor = 128.0 / dimensions.width().max(dimensions.height());
        let transform = resvg::tiny_skia::Transform::from_translate(
            (128. - dimensions.width() * factor) / 2.,
            (128. - dimensions.height() * factor) / 2.,
        )
        .pre_scale(factor, factor);
        resvg::render(&tree, transform, &mut pixmap.as_mut());
        let dst = folder.join(format!("{:016x}.png", hasher.finish()));
        pixmap.save_png(&dst)?;
        Ok(dst)
    } else {
        let mut reader =
            image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        let img = reader
            .decode()
            .context("Choose PNG, JPEG, WebP or SVG artwork")?
            .thumbnail(128, 128);
        let dst = folder.join(format!("{:016x}.png", hasher.finish()));
        img.save(&dst)?;
        Ok(dst)
    }
}

/// Import a full-resolution wallpaper under a content-specific filename so that
/// the renderer sees new artwork immediately and source files can be moved.
pub fn import_wallpaper(source: &Path) -> Result<PathBuf> {
    import_wallpaper_at(source, &config_dir().join("wallpapers"))
}
pub fn import_wallpaper_at(source: &Path, folder: &Path) -> Result<PathBuf> {
    use std::hash::{Hash, Hasher};
    if fs::metadata(source)?.len() > 64 * 1024 * 1024 {
        bail!("Choose a wallpaper smaller than 64 MiB");
    }
    let bytes = fs::read(source)?;
    if bytes.len() > 64 * 1024 * 1024 {
        bail!("Choose a wallpaper smaller than 64 MiB");
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format()?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
    ) {
        bail!("Choose a PNG, JPEG or WebP wallpaper");
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .context("Cannot decode wallpaper (maximum 8192 × 8192)")?;
    if image::ImageDecoder::total_bytes(&decoder) > 256 * 1024 * 1024 {
        bail!("Wallpaper requires too much memory");
    }
    let orientation = image::ImageDecoder::orientation(&mut decoder)?;
    let mut image =
        image::DynamicImage::from_decoder(decoder).context("Cannot decode wallpaper")?;
    image.apply_orientation(orientation);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    fs::create_dir_all(folder)?;
    let destination = folder.join(format!("{:016x}.png", hasher.finish()));
    if !destination.is_file() {
        let pending = folder.join(format!(
            ".{:016x}-{}.png",
            hasher.finish(),
            std::process::id()
        ));
        image.save(&pending).context("Cannot save wallpaper")?;
        fs::rename(&pending, &destination).context("Cannot finish saving wallpaper")?;
    }
    Ok(destination)
}
