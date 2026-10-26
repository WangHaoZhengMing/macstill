//! Shell-owned Control Center. System access goes through macstill-system;
//! the shell retains window, focus, notification and Stage Manager coordination.
use super::*;
use gpui_kit::base::slider::{SliderEvent, SliderState};
use macstill_system::{SystemApi, SystemCommand};
use swiftui_gpui::control_center::{
    control_center_panel, control_center_slider_content, control_center_tile,
};
mod actions;
mod media;
mod menu_bar;
mod state;
#[cfg(test)]
mod tests;
mod view;
use actions::ControlAction;
use state::Status;
pub(super) use state::{ControlCenter, StatusCache};
