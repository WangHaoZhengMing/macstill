//! Finder's look: macOS 26 (Tahoe), light and dark, in points, measured
//! from Tahoe screenshots. One place for every color, size and font, so
//! the app can be tuned without touching views.

use crate::model::tag::Tag;
use gpui_kit::{BoxShadow, Hsla, Rgba, hsla, point, px, rgb, rgba};
use std::sync::atomic::{AtomicBool, Ordering};

static DARK: AtomicBool = AtomicBool::new(false);

/// Follow the window's appearance (set before each frame is drawn).
pub fn set_dark(dark: bool) {
    DARK.store(dark, Ordering::Relaxed);
}

pub fn dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

/// Preferred UI fonts, first installed wins: SF Pro (loaded from the local
/// fonts folder) for testing, Inter as the shippable stand-in, then
/// Linux font fallbacks.
pub const FONTS: &[&str] = &[
    "SF Pro Text",
    "SF Pro",
    "Inter",
    "Ubuntu",
    "Noto Sans",
    "DejaVu Sans",
];

pub fn ui_font(installed: &[String]) -> &'static str {
    FONTS
        .iter()
        .find(|f| installed.iter().any(|i| i == *f))
        .copied()
        .unwrap_or("sans-serif")
}

// Window
pub const WINDOW_W: f32 = 1280.;
pub const WINDOW_H: f32 = 720.;
pub const WINDOW_MIN_W: f32 = 1024.;
pub const WINDOW_MIN_H: f32 = 420.;
pub const TOOLBAR_H: f32 = 52.;

// Traffic lights
pub const LIGHT_D: f32 = 14.;
pub const LIGHT_GAP: f32 = 8.;
/// From the window's top-left corner.
pub const LIGHTS_X: f32 = 19.;
pub const LIGHTS_Y: f32 = (TOOLBAR_H - LIGHT_D) / 2.;
pub fn light_close() -> Rgba {
    rgb(0xFF5F57)
}
pub fn light_min() -> Rgba {
    rgb(0xFEBC2E)
}
pub fn light_max() -> Rgba {
    rgb(0x28C840)
}
pub fn light_inactive() -> Rgba {
    if dark() { rgb(0x4A4A4C) } else { rgb(0xDDDDDD) }
}
pub fn light_border() -> Rgba {
    if dark() {
        rgba(0x00000040)
    } else {
        rgba(0x00000014)
    }
}
pub fn light_glyph() -> Rgba {
    if dark() {
        rgba(0x00000099)
    } else {
        rgba(0x00000099)
    }
}

// Sidebar: full height, flush with the window's edges.
pub const SIDEBAR_W: f32 = 206.;
/// SF Symbols' point size in the sidebar.

pub fn sidebar_material(active: bool) -> Rgba {
    match (dark(), active) {
        (true, true) => rgba(0x29292ba8),
        (false, true) => rgba(0xe8e8e8b0),
        (true, false) => rgba(0x29292be0),
        (false, false) => rgba(0xe8e8e8e0),
    }
}
pub fn sidebar_selected() -> Rgba {
    if dark() {
        rgba(0xFFFFFF1A)
    } else {
        rgba(0x0000000F)
    }
}

pub fn accent() -> Rgba {
    if dark() { rgb(0x3D9BFF) } else { rgb(0x0A7AFF) }
}

// Content
pub const ROW_H: f32 = 22.;
pub const ROW_INSET: f32 = 10.;
pub const ROW_RADIUS: f32 = 6.;
pub const ROW_ICON: f32 = 16.;
pub const INDENT: f32 = 18.;
pub const HEADER_H: f32 = 28.;
/// "Search: File System | Folder", under the toolbar while searching.
pub const SCOPE_BAR_H: f32 = 32.;
pub const PATH_BAR_H: f32 = 26.;
pub const STATUS_BAR_H: f32 = 26.;
pub const INSPECTOR_W: f32 = 300.;
pub const DATE_W: f32 = 200.;
pub const SIZE_W: f32 = 80.;
pub const KIND_W: f32 = 140.;
pub fn content_bg() -> Rgba {
    if dark() { rgb(0x1E1E1E) } else { rgb(0xFFFFFF) }
}
pub fn zebra() -> Rgba {
    if dark() { rgb(0x262627) } else { rgb(0xF4F5F5) }
}
pub fn selection() -> Rgba {
    if dark() { rgb(0x0058D0) } else { rgb(0x0056DF) }
}
pub fn selection_inactive() -> Rgba {
    if dark() { rgb(0x464648) } else { rgb(0xDCDCDC) }
}
/// Behind a selected icon in the grid.
pub fn icon_selected() -> Rgba {
    if dark() {
        rgba(0xFFFFFF1F)
    } else {
        rgba(0x00000014)
    }
}
pub fn inspector_bg() -> Rgba {
    if dark() { rgb(0x1E1E1E) } else { rgb(0xFFFFFF) }
}
pub fn separator() -> Rgba {
    if dark() {
        rgba(0xFFFFFF1A)
    } else {
        rgba(0x00000017)
    }
}
/// Selected text in fields.

/// The ring around a focused field.
pub fn focus_ring() -> Rgba {
    if dark() {
        rgba(0x3D9BFF99)
    } else {
        rgba(0x0A7AFF80)
    }
}
pub fn bar_bg() -> Rgba {
    if dark() { rgb(0x232324) } else { rgb(0xF9F9F9) }
}

// Text
pub const TEXT_SIZE: f32 = 13.;
pub const SMALL_SIZE: f32 = 11.;
pub const HEADER_SIZE: f32 = 12.;
pub const TITLE_SIZE: f32 = 15.;
pub fn text() -> Rgba {
    if dark() {
        rgba(0xFFFFFFE0)
    } else {
        rgba(0x000000DE)
    }
}
pub fn text_secondary() -> Rgba {
    if dark() {
        rgba(0xFFFFFF8C)
    } else {
        rgba(0x00000080)
    }
}
pub fn text_tertiary() -> Rgba {
    if dark() {
        rgba(0xFFFFFF4D)
    } else {
        rgba(0x00000047)
    }
}

// Toolbar layout and shared control colors.
pub const CAPSULE_H: f32 = 36.;
pub const CAPSULE_GAP: f32 = 10.;
pub fn glass_border() -> Rgba {
    if dark() {
        rgba(0xFFFFFF14)
    } else {
        rgba(0x0000000A)
    }
}
pub fn glass_pressed() -> Rgba {
    if dark() {
        rgba(0xFFFFFF1F)
    } else {
        rgba(0x00000012)
    }
}
/// A folder something is being dragged over.
pub fn drop_target() -> Rgba {
    if dark() {
        rgba(0x3D9BFF40)
    } else {
        rgba(0x0A7AFF2E)
    }
}
/// Menus are glass too: frostier than the toolbar's, bending less, with a
/// deeper shadow as they float higher.

/// Quick Look's panel.
pub fn menu_bg() -> Rgba {
    if dark() {
        rgba(0x2C2C2EF5)
    } else {
        rgba(0xFAFAFAF5)
    }
}
pub fn menu_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: hsla(0., 0., 0., if dark() { 0.5 } else { 0.18 }),
        offset: point(px(0.), px(8.)),
        blur_radius: px(24.),
        spread_radius: px(0.),
        inset: false,
    }]
}
pub fn glass_icon() -> Hsla {
    if dark() {
        hsla(0., 0., 1., 0.85)
    } else {
        hsla(0., 0., 0.12, 1.)
    }
}

/// Shared geometry for list headers and rows. Keep filenames readable on smaller windows.
#[derive(Clone, Copy, Debug)]
pub struct ListColumns {
    pub date: f32,
    pub size: f32,
    pub kind: f32,
}
impl ListColumns {
    pub fn for_width(width: f32) -> Self {
        let usable = (width - 2. * ROW_INSET).max(0.);
        let metadata = (usable * 0.55).min(DATE_W + SIZE_W + KIND_W);
        let scale = metadata / (DATE_W + SIZE_W + KIND_W);
        Self {
            date: DATE_W * scale,
            size: SIZE_W * scale,
            kind: KIND_W * scale,
        }
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    #[test]
    fn list_keeps_filename_space_at_supported_window_widths() {
        for width in [1024., 1280., 1600.] {
            let content = width - SIDEBAR_W - INSPECTOR_W;
            let columns = ListColumns::for_width(content);
            let name = content - 2. * ROW_INSET - columns.date - columns.size - columns.kind;
            assert!(name >= 200., "filename column collapsed at {width}: {name}");
            assert!(columns.date <= DATE_W && columns.size <= SIZE_W && columns.kind <= KIND_W);
        }
    }
}

impl crate::model::tag::Tag {
    /// macOS's tag colors.
    pub fn color(self) -> Hsla {
        let rgb = match self {
            Tag::Red => 0xFF3B30,
            Tag::Orange => 0xFF9500,
            Tag::Yellow => 0xFFCC00,
            Tag::Green => 0x34C759,
            Tag::Blue => 0x007AFF,
            Tag::Purple => 0xAF52DE,
            Tag::Gray => 0x8E8E93,
        };
        gpui_kit::rgb(rgb).into()
    }
}
