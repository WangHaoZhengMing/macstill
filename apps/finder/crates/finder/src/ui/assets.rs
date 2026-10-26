//! Embedded assets, and SF Symbols by name.
//!
//! `icons/sf/` holds SF Symbols extracted from a local SF Pro font
//! (assets/extract_symbols.py). They may not be redistributed, so the
//! folder is git-ignored and every symbol has a stand-in in `icons/`.

use gpui_kit::{AssetSource, SharedString};
use rust_embed::Embed;
use std::borrow::Cow;

#[derive(Embed)]
#[folder = "assets/icons"]
#[prefix = "icons/"]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if let Some(file) = Self::get(path) {
            return Ok(Some(file.data));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut assets = gpui_kit::assets::Assets.list(path)?;
        assets.extend(
            Self::iter()
                .filter(|p| p.starts_with(path))
                .map(|p| p.to_string().into()),
        );
        Ok(assets)
    }
}

/// Our drawing of each symbol, for builds without the SF set.
const STAND_INS: &[(&str, &str)] = &[
    ("sidebar.right", "sidebar-right"),
    ("chevron.left", "chevron-left"),
    ("chevron.right", "chevron-right"),
    ("chevron.down", "chevron-down"),
    ("chevron.up", "chevron-up"),
    ("chevron.forward", "disclosure"),
    ("square.grid.2x2", "grid"),
    ("list.bullet", "list"),
    ("rectangle.split.3x1", "columns"),
    ("squares.below.rectangle", "gallery"),
    ("square.grid.3x1.below.line.grid.1x2", "group"),
    ("square.and.arrow.up", "share"),
    ("tag", "tag"),
    ("ellipsis", "ellipsis"),
    ("ellipsis.circle", "ellipsis-circle"),
    ("magnifyingglass", "search"),
    ("xmark.circle.fill", "xmark-circle"),
    ("menubar.dock.rectangle", "desktop"),
    ("document", "document"),
    ("arrow.down.circle", "download"),
    ("hammer", "hammer"),
    ("house", "house"),
    ("icloud", "cloud"),
    ("internaldrive", "drive"),
    ("externaldrive", "external-drive"),
    ("network", "globe"),
    ("globe", "globe"),
    ("trash", "trash"),
    ("eject.fill", "eject"),
    ("clock", "clock"),
    ("xmark", "tl-close"),
    ("minus", "tl-min"),
    ("plus", "tl-max"),
];

/// The asset path of SF Symbol `name` (with an optional "@medium" or
/// "@semibold" weight), or of our stand-in, or of a custom icon given by
/// file stem (applications, airdrop: Finder's own, not public symbols).
pub fn symbol(name: &str) -> SharedString {
    let sf = format!("icons/sf/{name}.svg");
    if Assets::get(&sf).is_some() {
        return sf.into();
    }
    let base = name.split('@').next().unwrap_or(name);
    let file = STAND_INS
        .iter()
        .find(|(n, _)| *n == base)
        .map_or(base, |(_, f)| f);
    format!("icons/{file}.svg").into()
}
