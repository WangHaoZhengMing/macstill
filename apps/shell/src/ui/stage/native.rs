//! Layout/IPC only. Window content and spring integration belong to Hyprland.
use super::{layout, projection::Pose};
use crate::hyprland::stage::{Command, Snapshot, Window};
use gpui_kit::Pixels;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

// macOS onboarding reference; see docs/stage-manager-macos15-evidence.md.
const TILT: f64 = 65.0 * std::f64::consts::PI / 180.0;
pub(super) struct Presentation<'a> {
    pub height: f32,
    pub offset: f32,
    pub hidden: bool,
    pub drag: Option<(&'a str, f32, f32)>,
    pub hovered: Option<&'a str>,
    pub reduce: bool,
}
pub(super) struct Native {
    last: String,
    output: Option<String>,
    sent: Instant,
    acknowledged: Option<(u64, bool)>,
}
impl Native {
    pub fn connect() -> Option<Self> {
        crate::hyprland::stage::connect_native()?;
        Some(Self {
            last: String::new(),
            output: None,
            sent: Instant::now(),
            acknowledged: None,
        })
    }
    pub fn update(
        &mut self,
        stage: &Snapshot,
        presentation: Presentation<'_>,
    ) -> anyhow::Result<Option<Command>> {
        let Presentation {
            height,
            offset,
            hidden,
            drag,
            hovered,
            reduce,
        } = presentation;
        let Some(bounds) = &stage.bounds else {
            return Ok(None);
        };
        self.output = stage.output.clone();
        let strip = layout::Strip::new(
            height,
            stage
                .groups
                .len()
                .saturating_sub(usize::from(stage.active.is_some())),
        );
        let transition = stage.transition.as_ref();
        let mut windows = Vec::new();
        for (row, group) in stage
            .groups
            .iter()
            .filter(|g| Some(&g.id) != stage.active.as_ref())
            .enumerate()
        {
            // Back to front, matching the strip's pile and focused-window ordering.
            for (index, window) in group.windows.iter().enumerate().rev() {
                if transition.is_some_and(|t| t.lifecycle == Some(false)) {
                    continue;
                }
                let moving = transition
                    .is_some_and(|t| t.from.iter().any(|w| w.window.address == window.address));
                if !moving && (row >= strip.rows || index >= 3 || hidden) {
                    continue;
                }
                let mut rect = thumbnail(window, strip.slot(row), index);
                rect[0] += offset as f64;
                if row >= strip.rows || hidden {
                    rect[0] = -rect[2] - 20.;
                }
                if let Some((id, x, y)) = drag
                    && id == group.id
                {
                    rect[0] = x as f64 - 52. + index as f64 * 7.;
                    rect[1] = y as f64 - 38. - index as f64 * 3.;
                }
                let source = transition
                    .and_then(|t| t.from.iter().find(|w| w.window.address == window.address))
                    .map(|w| pose_array(Pose::rect(&w.rect, bounds.at)));
                let angle = if hovered == Some(group.id.as_str())
                    || drag.is_some_and(|(id, _, _)| id == group.id)
                {
                    0.
                } else {
                    TILT
                };
                windows.push(entry(
                    window,
                    rect,
                    source,
                    angle,
                    if moving { 0. } else { angle },
                ));
            }
        }
        if let Some(t) = transition {
            for item in &t.to {
                let live_source = t
                    .from
                    .iter()
                    .find(|w| w.window.address == item.window.address);
                let source = live_source
                    .map(|w| pose_array(Pose::rect(&w.rect, bounds.at)))
                    .unwrap_or_else(|| thumbnail(&item.window, strip.slot(t.incoming_slot), 0));
                windows.push(entry(
                    &item.window,
                    pose_array(Pose::rect(&item.rect, bounds.at)),
                    Some(source),
                    0.,
                    if live_source.is_some() { 0. } else { TILT },
                ));
            }
        }
        let request = json!({"owner":std::process::id(), "output":stage.output, "home":stage.workspace,
            "generation":transition.map_or(0, |t|t.generation), "animate":!reduce, "windows":windows}).to_string();
        if request != self.last || self.sent.elapsed() >= Duration::from_millis(500) {
            let reply = crate::hyprland::request(&format!("macstill-stage {request}"))?;
            anyhow::ensure!(reply.trim() == "ok", "Native Stage Manager: {reply}");
            self.last = request;
            self.sent = Instant::now();
        }
        if let Some(t) = transition {
            let key = (t.generation, t.ready);
            if self.acknowledged == Some(key) {
                return Ok(None);
            }
            if !t.ready {
                self.acknowledged = Some(key);
                return Ok(Some(Command::PresentTransition(t.generation)));
            }
            let status: Value = serde_json::from_str(&crate::hyprland::request(&format!(
                "macstill-stage {}",
                json!({"owner": std::process::id(), "output":stage.output, "status":true})
            ))?)?;
            if status["generation"].as_u64() == Some(t.generation)
                && status["settled"].as_bool() == Some(true)
            {
                self.acknowledged = Some(key);
                return Ok(Some(Command::FinishTransition(t.generation)));
            }
        } else {
            self.acknowledged = None;
        }
        Ok(None)
    }
}
fn pose_array(pose: Pose) -> [f64; 4] {
    [pose.x, pose.y, pose.width, pose.height]
}
fn entry(
    window: &Window,
    rect: [f64; 4],
    source: Option<[f64; 4]>,
    angle: f64,
    source_angle: f64,
) -> Value {
    let mut value = json!({"address":window.address,"pid":window.pid,"class":window.class,"rect":rect,"angle":angle,"source_angle":source_angle});
    if let Some(source) = source {
        value["source"] = json!(source);
    }
    value
}
fn thumbnail(window: &Window, bounds: gpui_kit::Bounds<Pixels>, index: usize) -> [f64; 4] {
    let ratio = window.size[1].max(1) as f64 / window.size[0].max(1) as f64;
    let width = (f64::from(bounds.size.width) - 20.)
        .min((f64::from(bounds.size.height) - 18.) / ratio)
        .max(1.);
    [
        f64::from(bounds.origin.x) + index as f64 * 7.,
        f64::from(bounds.origin.y) + (f64::from(bounds.size.height) - width * ratio) * 0.5
            - index as f64 * 3.,
        width,
        width * ratio,
    ]
}
impl Drop for Native {
    fn drop(&mut self) {
        if self.output.is_none() {
            return;
        }
        let _ = crate::hyprland::request(&format!(
            "macstill-stage {}",
            json!({"owner":std::process::id(),"output":self.output,"clear":true})
        ));
    }
}
