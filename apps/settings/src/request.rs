//! Navigation requests shared by the CLI, local IPC, and Settings view.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Page {
    Wifi,
    Bluetooth,
    Network,
    General,
    AirDrop,
    Storage,
    LoginItems,
    Appearance,
    ControlCenter,
    Dock,
    Displays,
    Wallpaper,
    Sound,
    Battery,
    DateTime,
    Shortcuts,
    Applications,
    About,
    LanguageRegion,
    Keyboard,
    LockScreen,
    Mouse,
    Trackpad,
}
impl Page {
    pub const ALL: [Self; 23] = [
        Self::Wifi,
        Self::Bluetooth,
        Self::Network,
        Self::Sound,
        Self::General,
        Self::AirDrop,
        Self::Storage,
        Self::LoginItems,
        Self::Appearance,
        Self::ControlCenter,
        Self::Dock,
        Self::Displays,
        Self::Wallpaper,
        Self::Battery,
        Self::LockScreen,
        Self::Keyboard,
        Self::Mouse,
        Self::Trackpad,
        Self::Shortcuts,
        Self::Applications,
        Self::About,
        Self::LanguageRegion,
        Self::DateTime,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Wifi => "Wi-Fi",
            Self::Bluetooth => "Bluetooth",
            Self::Network => "Network",
            Self::General => "General",
            Self::AirDrop => "AirDrop",
            Self::Storage => "Storage",
            Self::LoginItems => "Login Items",
            Self::Appearance => "Appearance",
            Self::ControlCenter => "Control Center",
            Self::Dock => "Desktop & Dock",
            Self::Displays => "Displays",
            Self::Wallpaper => "Wallpaper",
            Self::Sound => "Sound",
            Self::Battery => "Battery",
            Self::DateTime => "Date & Time",
            Self::Shortcuts => "App Shortcuts",
            Self::Applications => "Applications",
            Self::About => "About",
            Self::LanguageRegion => "Language & Region",
            Self::Keyboard => "Keyboard",
            Self::LockScreen => "Lock Screen",
            Self::Mouse => "Mouse",
            Self::Trackpad => "Trackpad",
        }
    }
    pub fn group(self) -> u8 {
        match self {
            Self::Wifi | Self::Bluetooth | Self::Network => 0,
            Self::Sound => 1,
            Self::Dock
            | Self::Displays
            | Self::Wallpaper
            | Self::Battery
            | Self::DateTime
            | Self::Keyboard
            | Self::Mouse
            | Self::Trackpad
            | Self::LockScreen => 3,
            Self::Shortcuts | Self::Applications | Self::About => 4,
            _ => 2,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Request {
    Show(Page),
    Bind(String),
    Icon(String),
}
impl Request {
    pub fn parse(args: &[String]) -> anyhow::Result<Self> {
        Ok(match args {
            [] => Self::Show(Page::General),
            [flag, page] if flag == "--page" => Self::Show(match page.as_str() {
                "appearance" => Page::Appearance,
                "control-center" => Page::ControlCenter,
                "dock" => Page::Dock,
                "shortcuts" => Page::Shortcuts,
                "applications" => Page::Applications,
                "about" => Page::About,
                "wifi" => Page::Wifi,
                "bluetooth" => Page::Bluetooth,
                "network" => Page::Network,
                "general" => Page::General,
                "airdrop" => Page::AirDrop,
                "storage" => Page::Storage,
                "login-items" => Page::LoginItems,
                "displays" => Page::Displays,
                "wallpaper" => Page::Wallpaper,
                "sound" => Page::Sound,
                "battery" => Page::Battery,
                "date-time" => Page::DateTime,
                "language-region" => Page::LanguageRegion,
                "keyboard" => Page::Keyboard,
                "lock-screen" => Page::LockScreen,
                "mouse" => Page::Mouse,
                "trackpad" => Page::Trackpad,
                _ => anyhow::bail!("Unknown Settings page: {page}"),
            }),
            [flag, id] if flag == "--bind" && !id.is_empty() => Self::Bind(id.clone()),
            [flag, id] if flag == "--icon" && !id.is_empty() => Self::Icon(id.clone()),
            _ => anyhow::bail!("Invalid arguments; run macstill-settings --help"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> anyhow::Result<Request> {
        Request::parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }
    #[test]
    fn shell_entry_points_reach_the_intended_settings_page() {
        assert_eq!(
            parse(&["--page", "mouse"]).unwrap(),
            Request::Show(Page::Mouse)
        );
        assert_eq!(
            parse(&["--page", "trackpad"]).unwrap(),
            Request::Show(Page::Trackpad)
        );
        assert_eq!(parse(&[]).unwrap(), Request::Show(Page::General));
        assert_eq!(
            parse(&["--page", "control-center"]).unwrap(),
            Request::Show(Page::ControlCenter)
        );
        assert_eq!(
            parse(&["--page", "dock"]).unwrap(),
            Request::Show(Page::Dock)
        );
        assert_eq!(
            parse(&["--bind", "finder"]).unwrap(),
            Request::Bind("finder".into())
        );
        assert_eq!(
            parse(&["--icon", "org.example.App.desktop"]).unwrap(),
            Request::Icon("org.example.App.desktop".into())
        );
    }
    #[test]
    fn incomplete_or_unknown_navigation_is_rejected() {
        for args in [
            vec!["--bind"],
            vec!["--icon", ""],
            vec!["--page", "unknown"],
            vec!["--page", "dock", "extra"],
        ] {
            assert!(parse(&args).is_err());
        }
    }
}
