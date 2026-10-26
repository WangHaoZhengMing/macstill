//! Preferences shared by System Settings and the desktop renderer.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlCenterModule {
    Wifi,
    Bluetooth,
    AirDrop,
    Focus,
    StageManager,
    ScreenMirroring,
    Display,
    Sound,
    NowPlaying,
}
impl ControlCenterModule {
    pub const ALL: [Self; 9] = [
        Self::Wifi,
        Self::Bluetooth,
        Self::AirDrop,
        Self::Focus,
        Self::StageManager,
        Self::ScreenMirroring,
        Self::Display,
        Self::Sound,
        Self::NowPlaying,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Wifi => "Wi-Fi",
            Self::Bluetooth => "Bluetooth",
            Self::AirDrop => "AirDrop",
            Self::Focus => "Focus",
            Self::StageManager => "Stage Manager",
            Self::ScreenMirroring => "Screen Mirroring",
            Self::Display => "Display",
            Self::Sound => "Sound",
            Self::NowPlaying => "Now Playing",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Wifi => "wifi",
            Self::Bluetooth => "bluetooth",
            Self::AirDrop => "airdrop",
            Self::Focus => "focus",
            Self::StageManager => "stage_manager",
            Self::ScreenMirroring => "screen_mirroring",
            Self::Display => "display",
            Self::Sound => "sound",
            Self::NowPlaying => "now_playing",
        }
    }
    pub fn supported(self) -> bool {
        !matches!(self, Self::ScreenMirroring)
    }
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MenuBarVisibility {
    Always,
    WhenActive,
    #[default]
    Never,
}
impl MenuBarVisibility {
    pub const ALL: [Self; 3] = [Self::Always, Self::WhenActive, Self::Never];
    pub fn label(self) -> &'static str {
        match self {
            Self::Always => "Show in Menu Bar",
            Self::WhenActive => "Show When Active",
            Self::Never => "Don’t Show in Menu Bar",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::WhenActive => "when_active",
            Self::Never => "never",
        }
    }
    pub fn visible(self, active: bool) -> bool {
        match self {
            Self::Always => true,
            Self::WhenActive => active,
            Self::Never => false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ControlCenterPreferences {
    pub menu_bar: BTreeMap<ControlCenterModule, MenuBarVisibility>,
    pub battery_in_menu_bar: bool,
    pub battery_in_control_center: bool,
    pub battery_percentage: bool,
}
impl Default for ControlCenterPreferences {
    fn default() -> Self {
        Self {
            menu_bar: BTreeMap::new(),
            battery_in_menu_bar: true,
            battery_in_control_center: false,
            battery_percentage: true,
        }
    }
}
impl ControlCenterPreferences {
    pub fn visibility(&self, module: ControlCenterModule) -> MenuBarVisibility {
        self.menu_bar.get(&module).copied().unwrap_or_default()
    }
    pub fn visible(&self, module: ControlCenterModule, active: bool) -> bool {
        module.supported() && self.visibility(module).visible(active)
    }
}
