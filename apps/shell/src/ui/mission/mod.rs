//! Mission Control is a transient shell surface. Workspaces and clients stay owned by Hyprland.
use super::{SharedStore, displays};
use crate::hyprland::{mission as api, stage};
use gpui_kit::{prelude::*, *};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use swiftui_gpui::Fonts;
mod layout;
mod native;
use layout::Rect;
#[cfg(test)]
mod tests;
#[derive(Default)]
struct Windows {
    views: Vec<WindowHandle<Overview>>,
    opening: bool,
    generation: u64,
    preparing: bool,
    prepared: Option<Prepared>,
    opening_gesture: bool,
}
type Prepared = (
    Arc<api::Scene>,
    std::collections::BTreeMap<String, anyhow::Result<native::Native>>,
);
impl Global for Windows {}
struct Overview {
    scene: Arc<api::Scene>,
    layout: layout::Cache,
    native: Option<native::Native>,
    output: String,
    workspace: i64,
    space_page: usize,
    focus: FocusHandle,
    selected: Option<String>,
    drag: Option<(String, Point<Pixels>, Point<Pixels>, bool)>,
    progress: f32,
    from: f32,
    target: f32,
    started: Instant,
    gesture_seq: u64,
    gesture_active: bool,
    closing: bool,
    releasing_native: bool,
    after: Option<stage::Window>,
    busy: bool,
    error: Option<String>,
    _poll: Task<()>,
    _scene_poll: Task<()>,
}
fn gesture_path() -> Option<std::path::PathBuf> {
    crate::hyprland::socket("macstill-mission-gesture.json").ok()
}
#[derive(Clone, serde::Deserialize)]
struct Gesture {
    seq: u64,
    direction: String,
    progress: f32,
    active: bool,
    cancelled: bool,
}
fn read_gesture() -> Option<Gesture> {
    let path = gesture_path()?;
    let m = std::fs::metadata(&path).ok()?;
    if m.modified().ok()?.elapsed().ok()?.as_secs() > 3 {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}
fn close(cx: &mut App) {
    if !cx.has_global::<Windows>() {
        return;
    }
    cx.global_mut::<Windows>().opening = false;
    cx.global_mut::<Windows>().generation += 1;
    cx.global_mut::<Windows>().preparing = false;
    cx.global_mut::<Windows>().prepared = None;
    let views = cx.global::<Windows>().views.clone();
    for view in views {
        let _ = view.update(cx, |s, _, cx| s.exit(None, cx));
    }
}
pub(super) fn command(command: &str, cx: &mut App) {
    if !cx.has_global::<Windows>() {
        cx.set_global(Windows::default());
    }
    let views = cx.global::<Windows>().views.clone();
    let alive = views.iter().any(|v| v.read(cx).is_ok());
    if command == "mission-unprepare" {
        if !alive && !cx.global::<Windows>().opening {
            close(cx);
        }
        return;
    }
    if command == "mission-close"
        || command == "mission-toggle" && (alive || cx.global::<Windows>().opening)
    {
        close(cx);
        return;
    }
    if alive {
        if command == "mission-gesture" {
            let gesture = read_gesture();
            for view in views {
                let _ = view.update(cx, |s, _, cx| {
                    if !s.closing {
                        s.update_gesture(gesture.clone(), cx);
                        cx.notify();
                    }
                });
            }
        }
        return;
    }
    if command == "mission-gesture" && !gesture_wants_open(read_gesture().as_ref()) {
        close(cx);
        return;
    }
    if cx.global::<Windows>().opening {
        return;
    }
    let prepare = command == "mission-prepare";
    cx.global_mut::<Windows>().opening = !prepare;
    cx.global_mut::<Windows>().opening_gesture = command == "mission-gesture";
    if let Some((scene, natives)) = cx.global_mut::<Windows>().prepared.take() {
        if prepare {
            cx.global_mut::<Windows>().prepared = Some((scene, natives));
        } else {
            cx.global_mut::<Windows>().opening = false;
            open(scene, natives, command == "mission-gesture", cx);
        }
        return;
    }
    if cx.global::<Windows>().preparing {
        return;
    }
    cx.global_mut::<Windows>().preparing = true;
    let generation = cx.global::<Windows>().generation;
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async {
                let scene = api::cache::shared().snapshot()?;
                let natives = scene
                    .monitors
                    .iter()
                    .map(|m| (m.name.clone(), native::Native::connect(&m.name)))
                    .collect::<std::collections::BTreeMap<_, _>>();
                anyhow::Ok((scene, natives))
            })
            .await;
        cx.update(|cx| {
            if cx.global::<Windows>().generation != generation {
                return;
            }
            cx.global_mut::<Windows>().preparing = false;
            let opening = cx.global::<Windows>().opening;
            let gesture = cx.global::<Windows>().opening_gesture;
            cx.global_mut::<Windows>().opening = false;
            if opening && gesture && !gesture_wants_open(read_gesture().as_ref()) {
                return; // Fingers lifted/cancelled while preparation was in flight.
            }
            match result {
                Ok((scene, natives)) if opening => open(scene, natives, gesture, cx),
                Ok(prepared) => cx.global_mut::<Windows>().prepared = Some(prepared),
                Err(e) => eprintln!("Mission Control: {e:#}"),
            }
        });
    })
    .detach();
    // Abandoned holds/pinches must not retain worker threads or stale scenes.
    cx.spawn(async move |cx| {
        cx.background_executor().timer(Duration::from_secs(3)).await;
        cx.update(|cx| {
            let windows = cx.global_mut::<Windows>();
            if windows.generation == generation && !windows.opening && !windows.preparing {
                windows.prepared = None;
            }
        });
    })
    .detach();
}
fn gesture_wants_open(gesture: Option<&Gesture>) -> bool {
    gesture.is_some_and(|g| g.direction == "up" && (g.active || !g.cancelled && g.progress >= 0.35))
}
fn open(
    scene: Arc<api::Scene>,
    mut natives: std::collections::BTreeMap<String, anyhow::Result<native::Native>>,
    gesture: bool,
    cx: &mut App,
) {
    let mut views = Vec::new();
    for monitor in &scene.monitors {
        let Some(id) = displays::output_named(Some(&monitor.name), cx) else {
            continue;
        };
        let (width, height) = if monitor.transform % 2 == 1 {
            (monitor.height, monitor.width)
        } else {
            (monitor.width, monitor.height)
        };
        let bounds = Bounds::new(
            point(px(monitor.x as f32), px(monitor.y as f32)),
            size(
                px(width as f32 / monitor.scale as f32),
                px(height as f32 / monitor.scale as f32),
            ),
        );
        use layer_shell::*;
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            display_id: Some(id),
            titlebar: None,
            window_background: WindowBackgroundAppearance::Transparent,
            app_id: Some("org.macstill.MissionControl".into()),
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: "macstill-mission-control".into(),
                layer: Layer::Overlay,
                anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                exclusive_zone: Some(px(-1.)),
                keyboard_interactivity: KeyboardInteractivity::Exclusive,
                ..Default::default()
            }),
            ..Default::default()
        };
        let output = monitor.name.clone();
        let native = natives.remove(&output).expect("prepared monitor");
        let workspace = monitor.workspace.id;
        let scene = scene.clone();

        match cx.open_window(options, |w, cx| {
            cx.new(|cx| {
                let view = Overview::new(scene, native, output, workspace, gesture, w, cx);
                view
            })
        }) {
            Ok(handle) => views.push(handle),
            Err(e) => eprintln!("Mission Control: {e:#}"),
        }
    }
    cx.global_mut::<Windows>().views = views;
}
impl Overview {
    fn new(
        scene: Arc<api::Scene>,
        native: anyhow::Result<native::Native>,
        output: String,
        workspace: i64,
        gesture: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        w.focus(&focus, cx);
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                // Gesture transport uses a small runtime file; never stat/read
                // it on the UI thread while animating or dragging windows.
                let gesture = cx
                    .background_executor()
                    .spawn(async { read_gesture() })
                    .await;
                if this
                    .update(cx, |s, cx| {
                        if s.closing {
                            return;
                        }
                        let before = (s.progress, s.gesture_active, s.gesture_seq);
                        s.update_gesture(gesture, cx);
                        if before != (s.progress, s.gesture_active, s.gesture_seq) {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let scene_changes = api::cache::shared().subscribe();
        let scene_poll = cx.spawn(async move |this, cx| {
            while scene_changes.recv().await.is_ok() {
                let scene = Some(api::cache::shared().latest());
                if this
                    .update(cx, |s, cx| {
                        if s.closing {
                            return;
                        }
                        if let Some(scene) = scene.filter(|_| !s.busy) {
                            match scene {
                                Ok(scene) => {
                                    if let Some(m) =
                                        scene.monitors.iter().find(|m| m.name == s.output)
                                    {
                                        s.workspace = m.workspace.id;
                                    }
                                    if !scene.monitors.iter().any(|m| m.name == s.output) {
                                        s.exit(None, cx);
                                        return;
                                    }
                                    s.scene = scene;
                                }
                                Err(e) => s.error = Some(e.to_string()),
                            }
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let mut s = Self {
            scene,
            layout: layout::Cache::default(),
            error: native.as_ref().err().map(|e| e.to_string()),
            native: native.ok(),
            output,
            workspace,
            space_page: 0,
            focus,
            selected: None,
            drag: None,
            progress: 0.,
            from: 0.,
            target: 1.,
            started: Instant::now(),
            gesture_seq: 0,
            gesture_active: false,
            closing: false,
            releasing_native: false,
            after: None,
            busy: false,
            _poll: poll,
            _scene_poll: scene_poll,
        };
        if gesture {
            s.update_gesture(read_gesture(), cx);
        } else if let Some(g) = read_gesture() {
            s.gesture_seq = g.seq;
        }
        s
    }
    fn monitor(&self) -> Option<&stage::Monitor> {
        self.scene.monitors.iter().find(|m| m.name == self.output)
    }
    fn stage_home(&self, w: &stage::Window, cx: &App) -> Option<i64> {
        if !w.workspace.name.starts_with("macstill-stage-") {
            return None;
        }
        let snapshot = &cx.global::<SharedStore>().0.read(cx).snapshot.stage;
        snapshot
            .outputs
            .iter()
            .chain(std::iter::once(snapshot))
            .find(|s| {
                s.groups
                    .iter()
                    .any(|g| g.windows.iter().any(|v| v.address == w.address))
            })
            .and_then(|s| s.workspace)
    }
    fn visible(&self, cx: &App) -> Vec<stage::Window> {
        let mut windows: Vec<_> = self
            .scene
            .windows
            .iter()
            .filter(|w| {
                let home = self.stage_home(w, cx).unwrap_or(w.workspace.id);
                (home == self.workspace
                    || w.pinned && self.monitor().is_some_and(|m| m.id == w.monitor))
                    && w.class != "org.macstill.MissionControl"
            })
            .cloned()
            .collect();
        if cx
            .global::<SharedStore>()
            .0
            .read(cx)
            .snapshot
            .preferences
            .mission_control
            .group_by_application
        {
            windows.sort_by(|a, b| a.class.cmp(&b.class).then(a.title.cmp(&b.title)));
        }
        windows
    }
    fn spaces(&self) -> Vec<api::DesktopWorkspace> {
        self.scene
            .spaces
            .iter()
            .filter(|s| s.monitor == self.output || s.id == self.workspace)
            .cloned()
            .collect()
    }
    fn displayed_spaces(&self) -> Vec<api::DesktopWorkspace> {
        self.spaces()
            .into_iter()
            .skip(self.space_page * 8)
            .take(8)
            .collect()
    }
    fn animate(&mut self, to: f32) {
        self.from = self.progress;
        self.target = to;
        self.started = Instant::now();
        self.gesture_active = false;
    }
    fn update_gesture(&mut self, gesture: Option<Gesture>, cx: &mut Context<Self>) {
        let Some(g) = gesture else { return };
        if g.seq < self.gesture_seq || g.seq == self.gesture_seq && !self.gesture_active {
            return;
        }
        self.gesture_seq = g.seq;
        let progress = g.progress.clamp(0., 1.);
        if g.active {
            self.gesture_active = true;
            self.progress = if g.direction == "up" {
                progress
            } else {
                1. - progress
            };
        } else {
            let open = if g.cancelled {
                g.direction != "up"
            } else if g.direction == "up" {
                progress >= 0.35
            } else {
                progress < 0.35
            };
            if open {
                self.animate(1.);
            } else {
                self.exit(None, cx);
            }
        }
    }
    fn exit(&mut self, after: Option<stage::Window>, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        self.after = after;
        self.closing = true;
        self.animate(0.);
        cx.notify();
    }
    fn action(&mut self, action: Operation, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let output = self.output.clone();
        let parked: Vec<_> = if let Operation::Remove(id, _) = &action {
            self.scene
                .windows
                .iter()
                .filter(|w| self.stage_home(w, cx) == Some(*id))
                .cloned()
                .collect()
        } else {
            vec![]
        };
        self.busy = true;
        self.error = None;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::focus_output(&output)?;
                    let selected = match action {
                        Operation::Switch(id) => {
                            api::switch(id)?;
                            Some(id)
                        }
                        Operation::Create => Some(api::create()?),
                        Operation::Remove(id, to) => {
                            for window in &parked {
                                api::move_window(window, to)?;
                            }
                            api::remove(id, to)?;
                            Some(to)
                        }
                        Operation::Move(w, id) => {
                            api::move_window(&w, id)?;
                            None
                        }
                    };
                    anyhow::Ok((api::cache::shared().refresh()?, selected))
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.busy = false;
                match result {
                    Ok((scene, id)) => {
                        s.scene = scene;
                        if let Some(id) = id {
                            s.workspace = id;
                            s.space_page =
                                s.spaces().iter().position(|v| v.id == id).unwrap_or(0) / 8;
                            s.selected = None;
                        }
                    }
                    Err(e) => s.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn stage_action(&self, window: &stage::Window, cx: &App) -> Option<stage::Command> {
        if !window.workspace.name.starts_with("macstill-stage-") {
            return None;
        }
        let snapshot = &cx.global::<SharedStore>().0.read(cx).snapshot.stage;
        snapshot
            .outputs
            .iter()
            .chain(std::iter::once(snapshot))
            .find_map(|s| {
                s.groups
                    .iter()
                    .find(|g| g.windows.iter().any(|w| w.address == window.address))
                    .map(|_| {
                        let action = stage::Command::ActivateWindow(window.clone());
                        if let Some(output) = &s.output {
                            stage::Command::OnOutput(output.clone(), Box::new(action))
                        } else {
                            action
                        }
                    })
            })
    }
    fn activate_after_unmap(&self, window: stage::Window, cx: &mut Context<Self>) {
        let action = self.stage_action(&window, cx);
        let store = cx.global::<SharedStore>().0.clone();
        cx.spawn(async move |_, cx| {
            // remove_window queues a Wayland unmap. Exclusive keyboard focus is
            // still held until the compositor processes it; activating earlier
            // can silently focus the wrong member when that layer disappears.
            let mut unmapped = false;
            for _ in 0..100 {
                if cx
                    .background_executor()
                    .spawn(async { native::overlay_unmapped() })
                    .await
                {
                    unmapped = true;
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(20))
                    .await;
            }
            if !unmapped {
                eprintln!("Mission Control overlay did not release keyboard focus");
                return;
            }
            if let Some(action) = action {
                for _ in 0..50 {
                    if store
                        .read_with(cx, |s, _| {
                            s.backend
                                .send(crate::backend::Action::Stage(action.clone()))
                        })
                        .is_ok()
                    {
                        return;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(20))
                        .await;
                }
                eprintln!("Mission Control could not activate the selected stage window");
            } else {
                if let Err(e) = cx
                    .background_executor()
                    .spawn(async move { api::activate(&window) })
                    .await
                {
                    eprintln!("Mission Control window activation: {e:#}");
                }
            }
        })
        .detach();
    }
    fn choose(&mut self, window: stage::Window, cx: &mut Context<Self>) {
        self.exit(Some(window), cx);
        cx.defer(|cx| close(cx));
    }
}
enum Operation {
    Switch(i64),
    Create,
    Remove(i64, i64),
    Move(stage::Window, i64),
}
fn space_rect(index: usize, count: usize, width: f32) -> Rect {
    let tile = (140_f32)
        .min(((width - 160.) / (count.max(1) as f32)) - 16.)
        .max(48.);
    let total = count as f32 * (tile + 16.);
    Rect {
        x: (width - total - 48.) / 2. + index as f32 * (tile + 16.),
        y: 18.,
        w: tile,
        h: 86.,
    }
}
fn glyph(bytes: &'static [u8], size: f32) -> impl IntoElement {
    img(Arc::new(Image::from_bytes(
        ImageFormat::Png,
        bytes.to_vec(),
    )))
    .size(px(size))
}
impl Render for Overview {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.gesture_active {
            let t = (self.started.elapsed().as_secs_f32() / 0.32).clamp(0., 1.);
            let animate = cx
                .global::<SharedStore>()
                .0
                .read(cx)
                .snapshot
                .preferences
                .mission_control
                .animate;
            let t = if cx.reduce_motion() || !animate {
                1.
            } else {
                t
            };
            let ease = 1. - (1. - t).powi(3);
            self.progress = self.from + (self.target - self.from) * ease;
            if t < 1. {
                w.request_animation_frame();
            } else if self.closing {
                if let Some(native) = self.native.take() {
                    self.releasing_native = true;
                    cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .spawn(async move {
                                drop(native);
                            })
                            .await;
                        let _ = this.update(cx, |s, cx| {
                            s.releasing_native = false;
                            cx.notify();
                        });
                    })
                    .detach();
                } else if !self.releasing_native {
                    let after = self.after.take();
                    w.remove_window();
                    if let Some(window) = after {
                        self.activate_after_unmap(window, cx);
                    }
                }
            }
        }
        let size = w.viewport_size();
        let width = f32::from(size.width);
        let height = f32::from(size.height);
        let progress = self.progress;
        let prefs = cx
            .global::<SharedStore>()
            .0
            .read(cx)
            .snapshot
            .preferences
            .clone();
        let wallpaper = macstill_ui::wallpaper(prefs.wallpaper.as_deref());
        let windows = self.visible(cx);
        let targets = self
            .layout
            .arrange(windows.iter().map(|w| w.size).collect(), width, height)
            .to_vec();
        let monitor = self.monitor();
        let origin = monitor
            .map(|m| (m.x as f32, m.y as f32))
            .unwrap_or_default();
        let all_spaces = self.spaces();
        let spaces = self.displayed_spaces();
        let count = spaces.len();
        let mut root = div()
            .id("mission-control")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .overflow_hidden()
            .font_family(cx.global::<Fonts>().text.clone())
            .text_size(px(13.))
            .text_color(rgb(0xffffff))
            .child(
                div().absolute().size_full().opacity(progress).child(
                    img(wallpaper.clone())
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                ),
            )
            .child(
                div()
                    .absolute()
                    .size_full()
                    .bg(rgba(0x00000042))
                    .opacity(progress),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|s, e: &MouseDownEvent, w, cx| {
                    s.focus.focus(w, cx);
                    if f32::from(e.position.y) > 145. && s.drag.is_none() {
                        s.exit(None, cx);
                        cx.defer(|cx| close(cx));
                    }
                }),
            )
            .on_key_down(cx.listener(|s, e: &KeyDownEvent, _, cx| {
                match e.keystroke.key.as_str() {
                    "escape" => cx.defer(|cx| close(cx)),
                    "enter" => {
                        if let Some(window) = s
                            .visible(cx)
                            .iter()
                            .find(|w| Some(&w.address) == s.selected.as_ref())
                            .cloned()
                        {
                            s.choose(window, cx);
                        }
                    }
                    "left" | "right" if e.keystroke.modifiers.control => {
                        let spaces = s.spaces();
                        if let Some(i) = spaces.iter().position(|w| w.id == s.workspace) {
                            let i = if e.keystroke.key == "left" {
                                i.saturating_sub(1)
                            } else {
                                (i + 1).min(spaces.len() - 1)
                            };
                            s.action(Operation::Switch(spaces[i].id), cx);
                        }
                    }
                    "tab" | "right" | "left" | "up" | "down" => {
                        let windows = s.visible(cx);
                        if !windows.is_empty() {
                            let i = windows
                                .iter()
                                .position(|w| Some(&w.address) == s.selected.as_ref());
                            let backwards = e.keystroke.modifiers.shift
                                || e.keystroke.key == "left"
                                || e.keystroke.key == "up";
                            let i = match i {
                                None => 0,
                                Some(i) if backwards => (i + windows.len() - 1) % windows.len(),
                                Some(i) => (i + 1) % windows.len(),
                            };
                            s.selected = Some(windows[i].address.clone());
                            cx.notify();
                        }
                    }
                    _ => {}
                }
            }))
            .on_mouse_move(cx.listener(|s, e: &MouseMoveEvent, _, cx| {
                if let Some((_, start, pos, moved)) = &mut s.drag {
                    *pos = e.position;
                    let dx = f32::from(pos.x - start.x);
                    let dy = f32::from(pos.y - start.y);
                    *moved |= dx * dx + dy * dy > 25.;
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |s, e: &MouseUpEvent, _, cx| {
                    if let Some((address, _, _, moved)) = s.drag.take() {
                        if let Some(window) = s
                            .scene
                            .windows
                            .iter()
                            .find(|w| w.address == address)
                            .cloned()
                        {
                            if moved {
                                let spaces = s.displayed_spaces();
                                if let Some((_, space)) =
                                    spaces.iter().enumerate().find(|(i, _)| {
                                        space_rect(*i, spaces.len(), width).contains(
                                            f32::from(e.position.x),
                                            f32::from(e.position.y),
                                        )
                                    })
                                {
                                    s.action(Operation::Move(window, space.id), cx);
                                }
                            } else {
                                s.choose(window, cx);
                            }
                        }
                        cx.notify();
                    }
                }),
            );
        root = root.child(
            div()
                .absolute()
                .top(px(-145. * (1. - progress)))
                .w_full()
                .h(px(142.))
                .bg(rgba(0x17171be8))
                .border_b_1()
                .border_color(rgba(0xffffff26))
                .opacity(progress),
        );
        for (i, space) in spaces.iter().enumerate() {
            let id = space.id;
            let r = space_rect(i, count, width);
            let selected = id == self.workspace;
            let is_drop = self.drag.as_ref().is_some_and(|(_, _, pos, moved)| {
                *moved && r.contains(f32::from(pos.x), f32::from(pos.y))
            });
            let mut tile = div()
                .id(("mission-space", id as usize))
                .absolute()
                .left(px(r.x))
                .top(px(r.y - 145. * (1. - progress)))
                .w(px(r.w))
                .h(px(r.h))
                .rounded(px(5.))
                .overflow_hidden()
                .border_2()
                .border_color(if selected || is_drop {
                    rgb(0x67aaff)
                } else {
                    rgba(0xffffff50)
                })
                .cursor_pointer()
                .child(
                    img(wallpaper.clone())
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                );
            for window in self.scene.windows.iter().filter(|w| w.workspace.id == id) {
                let sx = r.w / width;
                let sy = r.h / height;
                let x = (window.at[0] as f32 - origin.0) * sx;
                let y = (window.at[1] as f32 - origin.1) * sy;
                tile = tile.child(
                    div()
                        .absolute()
                        .left(px(x))
                        .top(px(y))
                        .w(px((window.size[0] as f32 * sx).max(3.)))
                        .h(px((window.size[1] as f32 * sy).max(3.)))
                        .rounded(px(1.))
                        .bg(rgba(0xdce6f0cc)),
                );
            }
            root = root.child(tile.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |s, _, _, cx| {
                    cx.stop_propagation();
                    if s.drag.is_none() {
                        s.action(Operation::Switch(id), cx);
                    }
                }),
            ));
            root = root.child(
                div()
                    .absolute()
                    .left(px(r.x))
                    .top(px(r.y + r.h + 7. - 145. * (1. - progress)))
                    .w(px(r.w))
                    .text_center()
                    .opacity(progress)
                    .child(format!("Desktop {}", self.space_page * 8 + i + 1)),
            );
            if all_spaces.len() > 1 {
                let at = all_spaces.iter().position(|v| v.id == id).unwrap_or(0);
                let destination = all_spaces[if at == 0 { 1 } else { at - 1 }].id;
                root = root.child(
                    div()
                        .id(("mission-remove", id as usize))
                        .absolute()
                        .left(px(r.x - 7.))
                        .top(px(r.y - 7. - 145. * (1. - progress)))
                        .size(px(22.))
                        .cursor_pointer()
                        .child(glyph(
                            include_bytes!("../../../../../assets/mission/SpacesBarClose@2x.png"),
                            22.,
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |s, _, _, cx| {
                                cx.stop_propagation();
                                s.action(Operation::Remove(id, destination), cx);
                            }),
                        ),
                );
            }
        }
        if all_spaces.len() > 8 {
            let pages = all_spaces.len().div_ceil(8);
            root = root.child(
                div()
                    .id("mission-spaces-next")
                    .absolute()
                    .right(px(62.))
                    .top(px(112.))
                    .cursor_pointer()
                    .child(format!("{} / {} ›", self.space_page + 1, pages))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |s, _, _, cx| {
                            cx.stop_propagation();
                            s.space_page = (s.space_page + 1) % pages;
                            cx.notify();
                        }),
                    ),
            );
        }
        root = root.child(
            div()
                .id("mission-add")
                .absolute()
                .right(px(25.))
                .top(px(45. - 145. * (1. - progress)))
                .size(px(28.))
                .cursor_pointer()
                .child(glyph(
                    include_bytes!("../../../../../assets/mission/SpacesBarSpacePlus@2x.png"),
                    28.,
                ))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|s, _, _, cx| {
                        cx.stop_propagation();
                        s.action(Operation::Create, cx);
                    }),
                ),
        );
        let mut entries = Vec::new();
        for (window, rect) in windows.iter().zip(targets) {
            let source = Rect {
                x: window.at[0] as f32 - origin.0,
                y: window.at[1] as f32 - origin.1,
                w: window.size[0] as f32,
                h: window.size[1] as f32,
            };
            let (source, angle) = self
                .native
                .as_ref()
                .map(|n| n.source(window, source))
                .unwrap_or((source, 0.));
            let mut r = source.mix(rect, progress);
            let address = window.address.clone();
            if let Some((dragged, start, pos, true)) = &self.drag {
                if dragged == &address {
                    r.x += f32::from(pos.x - start.x);
                    r.y += f32::from(pos.y - start.y);
                }
            }
            let selected = self.selected.as_ref() == Some(&address);
            entries.push(
                serde_json::json!({"address":window.address,"pid":window.pid,"class":window.class,
                "rect":[r.x,r.y,r.w,r.h],"angle":angle * (1. - progress as f64)}),
            );
            root = root.child(
                div()
                    .id(SharedString::from(format!("mission-window-{address}")))
                    .absolute()
                    .left(px(r.x))
                    .top(px(r.y))
                    .w(px(r.w))
                    .h(px(r.h))
                    .rounded(px(7.))
                    .border_2()
                    .border_color(if selected {
                        rgb(0x79b7ff)
                    } else {
                        rgba(0xffffff35)
                    })
                    .shadow_lg()
                    .overflow_hidden()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |s, e: &MouseDownEvent, w, cx| {
                            cx.stop_propagation();
                            s.focus.focus(w, cx);
                            s.selected = Some(address.clone());
                            s.drag = Some((address.clone(), e.position, e.position, false));
                            cx.notify();
                        }),
                    ),
            );
            root = root.child(
                div()
                    .absolute()
                    .left(px(r.x))
                    .top(px(r.y + r.h + 5.))
                    .w(px(r.w))
                    .text_center()
                    .truncate()
                    .opacity(progress)
                    .child(window.title.clone()),
            );
        }
        if let Some(native) = &self.native {
            if let Err(e) = native.update(self.workspace, entries) {
                self.error = Some(e.to_string());
            }
        }
        if windows.is_empty() {
            root = root.child(
                div()
                    .absolute()
                    .top(px(height / 2.))
                    .w_full()
                    .text_center()
                    .text_size(px(22.))
                    .opacity(progress)
                    .child("No windows on this desktop"),
            );
        }
        root = root.child(
            div()
                .absolute()
                .bottom(px(20.))
                .w_full()
                .text_center()
                .text_size(px(12.))
                .opacity(progress)
                .child(self.error.clone().unwrap_or_else(|| {
                    if self.busy {
                        "Updating desktop…".into()
                    } else {
                        "Drag a window to a desktop · Esc to return".into()
                    }
                })),
        );
        root
    }
}
