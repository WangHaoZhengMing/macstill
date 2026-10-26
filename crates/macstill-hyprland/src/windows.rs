//! Desktop actions use compositor geometry, including output scale and reserved bar/Dock space.
use crate::{dispatch, focus, request, validate_address};
use anyhow::{Result, bail, ensure};
use serde::Deserialize;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCommand {
    Next,
    Previous,
    SameApp,
    Center,
    Left,
    Right,
    Maximize,
    Fullscreen,
    Close,
    Float,
}

impl FromStr for WindowCommand {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> Result<Self> {
        Ok(match value {
            "next" => Self::Next,
            "previous" => Self::Previous,
            "same-app" => Self::SameApp,
            "center" => Self::Center,
            "left" => Self::Left,
            "right" => Self::Right,
            "maximize" => Self::Maximize,
            "fullscreen" => Self::Fullscreen,
            "close" => Self::Close,
            "float" => Self::Float,
            _ => bail!("Unknown window action: {value}"),
        })
    }
}


#[derive(Deserialize)]
struct Workspace {
    id: i64,
}
#[derive(Deserialize)]
struct ManagedWindow {
    address: String,
    class: String,
    workspace: Workspace,
    monitor: i64,
    size: [i32; 2],
    #[serde(default)]
    hidden: bool,
    #[serde(default = "mapped")]
    mapped: bool,
}
fn mapped() -> bool {
    true
}
#[derive(Deserialize)]
struct Monitor {
    id: i64,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    scale: f64,
    #[serde(default)]
    transform: u8,
    #[serde(default)]
    reserved: [i32; 4],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}
impl Monitor {
    fn work_area(&self) -> Result<Rect> {
        ensure!(
            self.scale.is_finite() && self.scale > 0. && self.width > 0 && self.height > 0,
            "Invalid monitor geometry"
        );
        let (w, h) = if self.transform % 2 == 1 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        };
        let [left, top, right, bottom] = self.reserved;
        let area = Rect {
            x: self.x + left,
            y: self.y + top,
            width: (w as f64 / self.scale).round() as i32 - left - right,
            height: (h as f64 / self.scale).round() as i32 - top - bottom,
        };
        ensure!(area.width > 0 && area.height > 0, "No usable monitor area");
        Ok(area)
    }
}
fn placement(area: Rect, size: [i32; 2], action: WindowCommand) -> Rect {
    let margin = 12.min((area.width.min(area.height) - 1) / 4);
    let x = area.x + margin;
    let y = area.y + margin;
    let width = area.width - 2 * margin;
    let height = area.height - 2 * margin;
    match action {
        WindowCommand::Left | WindowCommand::Right => {
            let half = (width - margin).max(2) / 2;
            Rect {
                x: if action == WindowCommand::Right {
                    x + width - half
                } else {
                    x
                },
                y,
                width: half,
                height,
            }
        }
        _ => {
            let w = size[0].clamp(1, width);
            let h = size[1].clamp(1, height);
            Rect {
                x: x + (width - w) / 2,
                y: y + (height - h) / 2,
                width: w,
                height: h,
            }
        }
    }
}
fn cycle_target<'a>(
    all: &'a [ManagedWindow],
    active: &ManagedWindow,
    action: WindowCommand,
) -> Option<&'a str> {
    let visible: Vec<_> = all
        .iter()
        .filter(|w| {
            w.mapped
                && !w.hidden
                && w.workspace.id == active.workspace.id
                && (action != WindowCommand::SameApp || w.class == active.class)
        })
        .collect();
    if visible.len() < 2 {
        return None;
    }
    let index = visible.iter().position(|w| w.address == active.address)?;
    let next = if action == WindowCommand::Previous {
        (index + visible.len() - 1) % visible.len()
    } else {
        (index + 1) % visible.len()
    };
    Some(&visible[next].address)
}

pub fn manage_window(action: WindowCommand) -> Result<()> {
    let active: serde_json::Value = serde_json::from_str(&request("j/activewindow")?)?;
    if active.get("address").is_none() {
        return Ok(());
    }
    let active: ManagedWindow = serde_json::from_value(active)?;
    validate_address(&active.address)?;
    let target = format!("address:{}", active.address);
    let lua_target = format!("window = \"{target}\"");
    match action {
        WindowCommand::Next | WindowCommand::Previous | WindowCommand::SameApp => {
            let windows = serde_json::from_str::<Vec<ManagedWindow>>(&request("j/clients")?)?;
            if let Some(address) = cycle_target(&windows, &active, action) {
                focus(address)?;
            }
        }
        WindowCommand::Maximize => dispatch(
            "fullscreen 1",
            &format!("hl.dsp.window.fullscreen({{mode = \"maximized\", {lua_target}}})"),
        )?,
        WindowCommand::Fullscreen => dispatch(
            "fullscreen 0",
            &format!("hl.dsp.window.fullscreen({{{lua_target}}})"),
        )?,
        WindowCommand::Close => dispatch(
            &format!("closewindow {target}"),
            &format!("hl.dsp.window.close({{{lua_target}}})"),
        )?,
        WindowCommand::Float => dispatch(
            &format!("togglefloating {target}"),
            &format!("hl.dsp.window.float({{{lua_target}}})"),
        )?,
        _ => {
            let monitors: Vec<Monitor> = serde_json::from_str(&request("j/monitors")?)?;
            let monitor = monitors
                .iter()
                .find(|m| m.id == active.monitor)
                .ok_or_else(|| anyhow::anyhow!("Window monitor is unavailable"))?;
            let rect = placement(monitor.work_area()?, active.size, action);
            // Placement is explicit floating geometry, even when invoked on a tiled/fullscreen client.
            dispatch(
                "fullscreenstate 0 0",
                &format!(
                    "hl.dsp.window.fullscreen_state({{internal = 0, client = 0, {lua_target}}})"
                ),
            )?;
            dispatch(
                &format!("setfloating {target}"),
                &format!("hl.dsp.window.float({{action = \"on\", {lua_target}}})"),
            )?;
            dispatch(
                &format!(
                    "resizewindowpixel exact {} {},{target}",
                    rect.width, rect.height
                ),
                &format!(
                    "hl.dsp.window.resize({{x = {}, y = {}, relative = false, {lua_target}}})",
                    rect.width, rect.height
                ),
            )?;
            dispatch(
                &format!("movewindowpixel exact {} {},{target}", rect.x, rect.y),
                &format!(
                    "hl.dsp.window.move({{x = {}, y = {}, relative = false, {lua_target}}})",
                    rect.x, rect.y
                ),
            )?;
            focus(&active.address)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scaled_and_rotated_outputs_preserve_reserved_space_and_negative_origins() {
        let mut m = Monitor {
            id: 1,
            x: -1440,
            y: 0,
            width: 2880,
            height: 1800,
            scale: 2.,
            transform: 0,
            reserved: [0, 30, 0, 70],
        };
        assert_eq!(
            m.work_area().unwrap(),
            Rect {
                x: -1440,
                y: 30,
                width: 1440,
                height: 800
            }
        );
        m.transform = 1;
        assert_eq!(m.work_area().unwrap().width, 900);
        m.scale = 0.;
        assert!(m.work_area().is_err());
    }
    #[test]
    fn side_by_side_windows_do_not_overlap_or_cover_the_bar_and_dock() {
        let area = Rect {
            x: 0,
            y: 30,
            width: 1400,
            height: 800,
        };
        let left = placement(area, [960, 720], WindowCommand::Left);
        let right = placement(area, [960, 720], WindowCommand::Right);
        assert!(left.x + left.width < right.x);
        assert_eq!(left.width, right.width);
        assert!(left.y >= area.y && left.y + left.height <= area.y + area.height);
        assert!(right.x + right.width <= area.width);
        let center = placement(area, [3000, 2000], WindowCommand::Center);
        assert_eq!(
            center,
            Rect {
                x: 12,
                y: 42,
                width: 1376,
                height: 776
            }
        );
    }
    #[test]
    fn switching_wraps_and_excludes_hidden_and_other_workspace_windows() {
        let make = |address: &str, class: &str, workspace| ManagedWindow {
            address: address.into(),
            class: class.into(),
            workspace: Workspace { id: workspace },
            monitor: 0,
            size: [800, 600],
            hidden: false,
            mapped: true,
        };
        let active = make("0x1", "one", 1);
        let mut all = vec![
            make("0x1", "one", 1),
            make("0x2", "two", 1),
            make("0x3", "one", 1),
            make("0x4", "four", 2),
            make("0x5", "five", 1),
        ];
        all[4].hidden = true;
        assert_eq!(
            cycle_target(&all, &active, WindowCommand::Next),
            Some("0x2")
        );
        assert_eq!(
            cycle_target(&all, &active, WindowCommand::Previous),
            Some("0x3")
        );
        assert_eq!(
            cycle_target(&all, &active, WindowCommand::SameApp),
            Some("0x3")
        );
        assert!("next; exit".parse::<WindowCommand>().is_err());
    }
}
