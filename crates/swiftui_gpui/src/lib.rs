//! Apple-style presentation on top of GPUI Kit's behavior primitives.
//! Application state and platform integration belong to the consuming application.
pub mod containers;
pub mod control_center;
pub mod controls;
pub mod menu;
pub mod settings;
pub mod theme;
pub mod typography;
pub mod window;

pub use containers::{caption, form, panel, section, sidebar};
pub use controls::{Button, Toggle, button, text_field, toggle};
pub use theme::{Palette, Theme, set_theme};
pub use typography::Fonts;
pub use window::traffic_lights;

/// Initialize GPUI Kit, resolve installed fonts, and install the default light theme.
/// Call once inside the application's startup callback, before opening windows.
pub fn init(cx: &mut gpui_kit::App) {
    gpui_kit::init(cx);
    cx.set_global(Fonts::detect(cx));
    set_theme(Palette::new(false), cx);
}

pub use settings::{
    appearance_option, settings_divider, settings_group, settings_navigation, settings_row,
    settings_sidebar, settings_toolbar,
};

pub use menu::{menu, menu_header, menu_item, menu_separator};
