//! Regional window material, informed by AppleResearch window appearance data.
//! GPUI owns tint and clipping; Hyprland owns live backdrop blur and shadow.
use gpui_kit::{Hsla, Pixels, px, rgba};

pub(super) const CORNER_RADIUS: f32 = 12.;

#[derive(Clone, Copy)]
pub(super) struct SettingsMaterial {
    pub sidebar: Hsla,
    pub content: u32,
    pub search: Hsla,
    pub divider: Hsla,
    pub highlight: Hsla,
}

impl SettingsMaterial {
    pub fn new(dark: bool) -> Self {
        Self::for_window(dark, true)
    }

    pub fn for_window(dark: bool, active: bool) -> Self {
        // AppKit's private color/luminance filters are unavailable to a regular
        // Wayland client. Neutral coverage limits wallpaper color transmission
        // without changing global blur or making foreground controls translucent.
        // These alpha values are calibrated approximations, not Apple constants.
        if dark {
            Self {
                sidebar: rgba(if active { 0x29292ba8 } else { 0x29292be0 }).into(),
                content: 0x282828,
                search: rgba(0xffffff10).into(),
                divider: rgba(0xffffff12).into(),
                highlight: rgba(0xffffff1a).into(),
            }
        } else {
            Self {
                sidebar: rgba(if active { 0xe8e8e8b0 } else { 0xe8e8e8e0 }).into(),
                content: 0xf6f6f6,
                search: rgba(0x0000000e).into(),
                divider: rgba(0x00000018).into(),
                highlight: rgba(0xffffff3f).into(),
            }
        }
    }

    pub fn radius(expanded: bool) -> Pixels {
        px(if expanded { 0. } else { CORNER_RADIUS })
    }

    // One physical pixel, including fractional display scales.
    pub fn hairline(scale: f32) -> Pixels {
        px(1. / scale.max(1.))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::rgb;

    #[test]
    fn only_sidebar_material_is_translucent() {
        for dark in [false, true] {
            let material = SettingsMaterial::new(dark);
            assert!((0.65..0.7).contains(&material.sidebar.a));
            let content: Hsla = rgb(material.content).into();
            assert_eq!(content.a, 1.);
            assert!(material.divider.a < 0.1);
        }
    }

    #[test]
    fn inactive_material_suppresses_backdrop_without_changing_content() {
        for dark in [false, true] {
            let active = SettingsMaterial::for_window(dark, true);
            let inactive = SettingsMaterial::for_window(dark, false);
            assert!(inactive.sidebar.a > active.sidebar.a);
            assert!(inactive.sidebar.a < 1.);
            assert_eq!(active.content, inactive.content);
            // A saturated backdrop can contribute at most this fraction of
            // channel contrast through our neutralizing layer (before blur).
            assert!(1. - active.sidebar.a < 0.35);
            assert!(1. - inactive.sidebar.a < 0.13);
        }
    }

    #[test]
    fn material_geometry_tracks_expansion_and_display_scale() {
        assert_eq!(SettingsMaterial::radius(false), px(12.));
        assert_eq!(SettingsMaterial::radius(true), px(0.));
        for scale in [1., 1.25, 1.5, 2.] {
            assert_eq!(SettingsMaterial::hairline(scale) * scale, px(1.));
        }
    }
}
