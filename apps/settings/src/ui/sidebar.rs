//! Settings-specific sidebar artwork; shared navigation behavior lives in swiftui_gpui.
use super::*;
pub(super) fn icon(page: Page, size: f32) -> AnyElement {
    if page == Page::Bluetooth {
        return super::reference::bluetooth()
            .size(px(size))
            .object_fit(ObjectFit::Contain)
            .into_any_element();
    }
    if matches!(
        page,
        Page::LanguageRegion | Page::Keyboard | Page::Mouse | Page::Trackpad
    ) {
        let kind = match page {
            Page::LanguageRegion => "network",
            Page::Keyboard => "keyboard",
            Page::Mouse => "mouse",
            Page::Trackpad => "trackpad",
            _ => "lock",
        };
        return div()
            .size(px(size))
            .rounded(px(5.))
            .bg(rgb(if page == Page::LockScreen {
                0x585b61
            } else {
                0x879099
            }))
            .flex()
            .items_center()
            .justify_center()
            .child(widgets::symbol(kind, size - 5., 0xffffff))
            .into_any_element();
    }
    let source: &[u8] = match page {
        Page::Wifi => include_bytes!("../../../../assets/icons/settings/sidebar-wifi.svg"),
        Page::Bluetooth => {
            include_bytes!("../../../../assets/icons/settings/sidebar-bluetooth.svg")
        }
        Page::Network => include_bytes!("../../../../assets/icons/settings/sidebar-network.png"),
        Page::Sound => include_bytes!("../../../../assets/icons/settings/sidebar-sound.svg"),
        Page::ControlCenter => {
            include_bytes!("../../../../assets/icons/settings/sidebar-control-center.svg")
        }
        Page::General | Page::AirDrop | Page::Storage | Page::LoginItems => {
            include_bytes!("../../../../assets/icons/settings/sidebar-general.png")
        }
        Page::Appearance => {
            include_bytes!("../../../../assets/icons/settings/sidebar-appearance.svg")
        }
        Page::Dock => include_bytes!("../../../../assets/icons/settings/sidebar-dock.png"),
        Page::Displays => include_bytes!("../../../../assets/icons/settings/sidebar-display.png"),
        Page::Wallpaper => {
            include_bytes!("../../../../assets/icons/settings/sidebar-wallpaper.svg")
        }
        Page::Battery => include_bytes!("../../../../assets/icons/settings/sidebar-battery.svg"),
        Page::DateTime => include_bytes!("../../../../assets/icons/settings/sidebar-datetime.svg"),
        Page::Shortcuts => {
            include_bytes!("../../../../assets/icons/settings/sidebar-shortcuts.svg")
        }
        Page::Applications => {
            include_bytes!("../../../../assets/icons/settings/sidebar-applications.svg")
        }
        Page::About => include_bytes!("../../../../assets/icons/settings/sidebar-about.svg"),
        Page::LockScreen => include_bytes!("../../../../assets/icons/settings/sidebar-lock.png"),
        Page::LanguageRegion | Page::Keyboard | Page::Mouse | Page::Trackpad => unreachable!(),
    };
    img(std::sync::Arc::new(Image::from_bytes(
        if matches!(
            page,
            Page::Network
                | Page::General
                | Page::Storage
                | Page::LoginItems
                | Page::Displays
                | Page::Dock
                | Page::LockScreen
        ) {
            ImageFormat::Png
        } else {
            ImageFormat::Svg
        },
        source.to_vec(),
    )))
    .size(px(size))
    .flex_shrink_0()
    .into_any_element()
}

pub(super) fn matches_search(page: Page, query: &str) -> bool {
    let keywords = match page {
        Page::Wifi => "wireless internet wlan",
        Page::Bluetooth => "pairing accessories headphones",
        Page::Sound => "audio speaker microphone volume mute",
        Page::Displays => "screen brightness monitor",
        Page::Battery => "power energy charging",
        Page::DateTime => "timezone ntp clock",
        Page::Shortcuts => "hide icons bind apple finder safari",
        Page::Applications => "launchpad installed icons",
        Page::Dock => "size magnification autohide mission control desktops workspaces gestures",
        Page::General => "computer hostname name",
        Page::Appearance => "theme light dark",
        Page::ControlCenter => {
            "control center menu bar modules icons visibility battery percentage"
        }
        Page::Wallpaper => "wallpaper background photo picture",
        Page::LanguageRegion => "language region locale formats date numbers currency",
        Page::Keyboard => "keyboard input layout xkb console keymap",
        Page::Mouse => {
            "mouse pointer tracking speed acceleration scrolling buttons left handed double click"
        }
        Page::Trackpad => "trackpad touchpad tap click scroll zoom drag gestures natural scrolling",
        Page::LockScreen => "lock screen session sleep suspend hibernate restart shutdown",
        _ => "",
    };
    format!("{} {keywords}", page.title())
        .to_lowercase()
        .contains(query)
}
