use gpui_kit::{App, Global, SharedString};

/// Resolve installed families once so every window and control uses the same fonts.
pub struct Fonts {
    pub text: SharedString,
    pub display: SharedString,
    pub mono: SharedString,
}

impl Global for Fonts {}

impl Fonts {
    pub fn detect(cx: &App) -> Self {
        Self::from_installed(&cx.text_system().all_font_names())
    }

    /// Resolve fonts from an inventory, useful for hosts with their own font discovery.
    pub fn from_installed(installed: &[String]) -> Self {
        let text = choose(
            installed,
            &[
                "SF Pro Text",
                "SF Pro",
                "Helvetica Neue",
                "Inter",
                "Noto Sans",
                "DejaVu Sans",
                "Liberation Sans",
            ],
            ".SystemUIFont",
        );
        let display = choose(installed, &["SF Pro Display", "SF Pro"], &text);
        let mono = choose(
            installed,
            &[
                "SF Mono",
                "Menlo",
                "Cascadia Mono",
                "Noto Sans Mono",
                "DejaVu Sans Mono",
                "Liberation Mono",
            ],
            "monospace",
        );
        Self {
            text,
            display,
            mono,
        }
    }
}

fn choose(installed: &[String], preferred: &[&str], fallback: &str) -> SharedString {
    preferred
        .iter()
        .find_map(|name| {
            installed
                .iter()
                .find(|family| family.eq_ignore_ascii_case(name))
        })
        .map(|family| family.clone().into())
        .unwrap_or_else(|| fallback.to_owned().into())
}
