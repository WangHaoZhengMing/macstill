use super::*;
use crate::hyprland::stage::Command;
use swiftui_gpui::{Button, caption, menu, menu_item};
mod content;
mod layout;
mod native;
mod projection;
mod spring;
use std::{collections::BTreeMap, time::Instant};

#[derive(Default)]
pub(super) struct State {
    content: Option<content::Content>,
    pub(super) output: Option<String>,
    hovered: Option<String>,
    menu: Option<(String, f32)>,
    tilts: BTreeMap<String, (spring::Spring, Instant)>,
    press: Option<Press>,
    suppress_click: bool,
    revealed: bool,
    edge_enter: Option<Instant>,
    leave_at: Option<Instant>,
    slide: Option<(spring::Spring, Instant)>,
}

struct Press {
    group: String,
    start: Point<Pixels>,
    position: Point<Pixels>,
    moved: bool,
}

#[cfg(test)]
mod tests;

pub(super) fn sync(cx: &mut App) {
    let snapshot = cx.global::<SharedStore>().0.read(cx).snapshot.stage.clone();
    let snapshots = if snapshot.outputs.is_empty() {
        vec![snapshot]
    } else {
        snapshot.outputs
    };
    let mut wanted = std::collections::HashSet::new();
    for stage in snapshots.into_iter().filter(|s| s.enabled && s.visible) {
        let Some(display) = displays::output_named(stage.output.as_deref(), cx) else {
            continue;
        };
        wanted.insert(display);
        match open_on(Mode::Stage, Some(display), cx) {
            Ok(handle) => {
                let _ = handle.update(cx, |view, _, cx| {
                    view.stage_state.output = stage.output.clone();
                    let content = view
                        .stage_state
                        .content
                        .get_or_insert_with(content::Content::new);
                    content.refresh(&stage);
                    if content.native.is_none()
                        && let Some(t) = &stage.transition
                    {
                        let command = Command::FinishTransition(t.generation);
                        let command = stage.output.clone().map_or(command.clone(), |name| {
                            Command::OnOutput(name, Box::new(command))
                        });
                        let _ = view.store.read(cx).backend.send(Action::Stage(command));
                    }
                    cx.notify();
                });
            }
            Err(error) => eprintln!("Cannot open Stage Manager: {error:#}"),
        }
    }
    let removed: Vec<_> = cx
        .global::<OpenWindows>()
        .stage
        .keys()
        .copied()
        .filter(|id| !wanted.contains(id))
        .collect();
    for id in removed {
        if let Some(handle) = cx.global_mut::<OpenWindows>().stage.remove(&id) {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    }
}

impl DesktopView {
    fn output_command(&self, command: Command) -> Command {
        self.stage_state
            .output
            .clone()
            .map_or(command.clone(), |name| {
                Command::OnOutput(name, Box::new(command))
            })
    }

    pub(super) fn stage_command(&mut self, command: Command, cx: &mut Context<Self>) {
        self.message = None;
        self.stage_state.menu = None;
        let can_animate = !cx.reduce_motion()
            && matches!(
                command,
                Command::Activate(_)
                    | Command::Next
                    | Command::ShowDesktop
                    | Command::Enable
                    | Command::Disable
                    | Command::Toggle
            )
            && self.stage_state.content.as_ref().map_or(
                matches!(
                    command,
                    Command::Enable | Command::Toggle | Command::ShowDesktop
                ),
                |c| c.native.is_some(),
            );
        let command = if can_animate {
            Command::Animate(Box::new(command))
        } else {
            command
        };
        if let Err(error) = self
            .store
            .read(cx)
            .backend
            .send(Action::Stage(self.output_command(command)))
        {
            self.message = Some(error.to_string());
        }
        let snapshot = self.store.read(cx).backend.snapshot();
        self.store.update(cx, |store, cx| {
            store.snapshot = snapshot;
            cx.notify();
        });
        cx.notify();
    }
    pub(super) fn stage_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let snap = self.snapshot(cx);
        let stage = snap
            .stage
            .outputs
            .iter()
            .find(|s| s.output == self.stage_state.output)
            .cloned()
            .unwrap_or(snap.stage);
        let p = Palette::new(snap.preferences.dark);
        let now = Instant::now();
        let height = f32::from(window.bounds().size.height);
        // Sparse Wayland input regions can leave the entire surface before a
        // button receives a local mouse-exit event. Clear the stale hover then.
        if !window.is_window_hovered() {
            self.stage_state.hovered = None;
        }
        if stage.fullscreen {
            self.stage_state.press = None;
            self.stage_state.menu = None;
            self.stage_state.hovered = None;
            self.stage_state.revealed = false;
            self.stage_state.edge_enter = None;
            self.stage_state.leave_at = None;
        }
        let strip = layout::Strip::new(
            height,
            stage
                .groups
                .len()
                .saturating_sub(usize::from(stage.active.is_some())),
        );
        self.stage_state
            .tilts
            .retain(|id, _| stage.groups.iter().any(|g| &g.id == id));
        if self
            .stage_state
            .menu
            .as_ref()
            .is_some_and(|(id, _)| !stage.groups.iter().any(|g| &g.id == id))
        {
            self.stage_state.menu = None;
        }
        if self
            .stage_state
            .press
            .as_ref()
            .is_some_and(|press| !stage.groups.iter().any(|g| g.id == press.group))
        {
            self.stage_state.press = None;
        }
        let auto_hide = stage.recent_hidden || stage.occluded;
        if self
            .stage_state
            .edge_enter
            .is_some_and(|time| now.duration_since(time).as_millis() >= 160)
        {
            self.stage_state.revealed = true;
            self.stage_state.edge_enter = None;
        }
        if self
            .stage_state
            .leave_at
            .is_some_and(|time| now.duration_since(time).as_millis() >= 400)
            && self.stage_state.press.is_none()
            && self.stage_state.menu.is_none()
        {
            self.stage_state.revealed = false;
            self.stage_state.leave_at = None;
        }
        let hidden = stage.fullscreen
            || (auto_hide && !self.stage_state.revealed && self.stage_state.press.is_none());
        let target = if hidden { -(layout::WIDTH as f64) } else { 0. };
        let (slide, last) = self
            .stage_state
            .slide
            .get_or_insert_with(|| (spring::Spring::new(target, 2., 300., 50.), now));
        slide.retarget(target);
        if cx.reduce_motion() {
            slide.position = target;
            slide.velocity = 0.;
        } else {
            slide.step(now.duration_since(*last).as_secs_f64().min(0.1));
        }
        *last = now;
        let offset = slide.position as f32;
        if !slide.settled(0.1, 0.1)
            || self.stage_state.edge_enter.is_some()
            || (self.stage_state.leave_at.is_some()
                && self.stage_state.menu.is_none()
                && self.stage_state.press.is_none())
        {
            window.request_animation_frame();
        }
        let native = self
            .stage_state
            .content
            .as_ref()
            .is_some_and(|c| c.native.is_some());
        if native {
            let drag = self
                .stage_state
                .press
                .as_ref()
                .filter(|p| p.moved)
                .map(|p| {
                    (
                        p.group.as_str(),
                        f32::from(p.position.x),
                        f32::from(p.position.y),
                    )
                });
            let reduce = cx.reduce_motion();
            let result = self
                .stage_state
                .content
                .as_mut()
                .unwrap()
                .native
                .as_mut()
                .unwrap()
                .update(
                    &stage,
                    native::Presentation {
                        height,
                        offset,
                        hidden,
                        drag,
                        reduce,
                        hovered: self.stage_state.hovered.as_deref(),
                    },
                );
            match result {
                Ok(Some(command)) => {
                    let _ = self
                        .store
                        .read(cx)
                        .backend
                        .send(Action::Stage(self.output_command(command)));
                }
                Ok(None) => {}
                Err(error) => {
                    self.message = Some(error.to_string());
                    self.stage_state.content.as_mut().unwrap().fallback();
                    if let Some(t) = &stage.transition {
                        let _ = self.store.read(cx).backend.send(Action::Stage(
                            self.output_command(Command::FinishTransition(t.generation)),
                        ));
                    }
                }
            }
        }
        let dragging = self.stage_state.press.as_ref().is_some_and(|p| p.moved);
        let mut hits = Vec::new();
        if !stage.fullscreen && strip.rows > 0 {
            hits.push(Bounds::new(
                point(px(0.), px(layout::TOP)),
                size(px(2.), px((height - layout::TOP - layout::BOTTOM).max(0.))),
            ));
        }
        if self.stage_state.revealed && !stage.fullscreen {
            hits.push(Bounds::new(
                point(px(0.), px(layout::TOP)),
                size(
                    px(layout::WIDTH),
                    px((height - layout::TOP - layout::BOTTOM).max(0.)),
                ),
            ));
        }
        if self.stage_state.press.is_some() || self.stage_state.menu.is_some() {
            hits.push(Bounds::new(point(px(0.), px(0.)), window.bounds().size));
        }
        let mut root = div()
            .id("stage-strip")
            .relative()
            .size_full()
            .font_family(cx.global::<Fonts>().text.clone())
            .text_color(rgb(p.text))
            .on_mouse_move(cx.listener(|s, event: &MouseMoveEvent, _, cx| {
                if f32::from(event.position.x) <= 2. {
                    s.stage_state.edge_enter.get_or_insert_with(Instant::now);
                    s.stage_state.leave_at = None;
                } else if f32::from(event.position.x) < layout::WIDTH {
                    s.stage_state.leave_at = None;
                } else if s.stage_state.revealed {
                    s.stage_state.leave_at.get_or_insert_with(Instant::now);
                }
                if let Some(press) = &mut s.stage_state.press {
                    let delta = event.position - press.start;
                    if event.dragging()
                        && f32::from(delta.x).powi(2) + f32::from(delta.y).powi(2) > 36.
                    {
                        press.moved = true;
                        s.stage_state.suppress_click = true;
                    }
                    press.position = event.position;
                }
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|s, event: &MouseUpEvent, window, cx| {
                    if let Some(press) = s.stage_state.press.take() {
                        if press.moved
                            && f32::from(event.position.x) > layout::WIDTH + 24.
                            && f32::from(event.position.y) >= layout::TOP
                            && f32::from(event.position.y)
                                < f32::from(window.bounds().size.height) - layout::BOTTOM
                        {
                            s.stage_command(Command::Merge(press.group), cx);
                        }
                        cx.notify();
                    }
                }),
            )
            .on_mouse_exit(cx.listener(|s, _, _, cx| {
                s.stage_state.hovered = None;
                s.stage_state.edge_enter = None;
                s.stage_state.leave_at = Some(Instant::now());
                cx.notify();
            }));
        for (row, group) in stage
            .groups
            .iter()
            .filter(|g| Some(&g.id) != stage.active.as_ref())
            .take(strip.rows)
            .enumerate()
        {
            let mut bounds = strip.slot(row);
            bounds.origin.x += px(offset);
            if !hidden {
                hits.push(bounds);
            }
            let tilt = self
                .stage_state
                .tilts
                .entry(group.id.clone())
                .or_insert_with(|| (spring::Spring::new(projection::TILT, 2., 300., 50.), now));
            tilt.0
                .retarget(if self.stage_state.hovered.as_ref() == Some(&group.id) {
                    0.
                } else {
                    projection::TILT
                });
            if cx.reduce_motion() {
                tilt.0.position = tilt.0.target;
                tilt.0.velocity = 0.;
            } else {
                tilt.0
                    .step(now.duration_since(tilt.1).as_secs_f64().min(0.1));
            }
            tilt.1 = now;
            if !tilt.0.settled(0.002, 0.002) {
                window.request_animation_frame();
            }
            let angle = tilt.0.position;
            let apps: Vec<_> = group
                .classes
                .iter()
                .map(|class| snap.apps.iter().find(|app| app.matches_class(class)))
                .collect();
            let title = group
                .classes
                .iter()
                .zip(&apps)
                .map(|(class, app)| {
                    app.map(|app| app.name.clone())
                        .unwrap_or_else(|| class.clone())
                })
                .collect::<Vec<_>>()
                .join(" + ");
            let badges: Vec<_> = apps
                .iter()
                .take(3)
                .map(|app| {
                    app.and_then(|app| Self::app_icon(app, &snap.preferences))
                        .unwrap_or_else(|| Self::asset("folder.png"))
                })
                .collect();
            let live: Vec<_> = self
                .stage_state
                .content
                .as_ref()
                .or_else(|| cx.try_global::<content::Content>())
                .map(|content| {
                    group
                        .windows
                        .iter()
                        .take(3)
                        .filter_map(|w| content.image(w))
                        .collect()
                })
                .unwrap_or_default();
            let source_dragged = self
                .stage_state
                .press
                .as_ref()
                .is_some_and(|press| press.moved && press.group == group.id);
            let mut preview = div().relative().size_full();
            if !live.is_empty() && !source_dragged {
                preview = preview.child(projection::preview(live.clone(), angle));
            }
            // Badges belong to settled strip cards, not their destination slots
            // while the corresponding window is still travelling.
            let badges_visible = !stage.transition.as_ref().is_some_and(|t| {
                t.lifecycle.is_some()
                    || t.from
                        .iter()
                        .any(|w| group.windows.iter().any(|g| g.address == w.window.address))
            });
            for (index, badge) in badges.into_iter().enumerate().filter(|_| badges_visible) {
                preview = preview.child(
                    img(badge)
                        .absolute()
                        .left(px(index as f32 * 22.))
                        .bottom(px(1.))
                        .size(px(if live.is_empty() && !native { 40. } else { 24. }))
                        .object_fit(ObjectFit::Contain),
                );
            }
            let id = group.id.clone();
            let hover_id = id.clone();
            let menu_id = id.clone();
            let press_id = id.clone();
            root = root.child(Button::new(eid("stage-select", &id))
                .accessibility_label(format!("Switch to {title}, {} windows. Drag or Shift-click to add to current group.", group.count))
                .absolute().left(bounds.origin.x).top(bounds.origin.y).w(bounds.size.width).h(bounds.size.height)
                .disabled(stage.busy || hidden).cursor_pointer()
                .focus_visible(move |style| style.rounded(px(9.)).border_1().border_color(rgb(p.accent)))
                .on_hover(cx.listener(move |s, hovered: &bool, _, cx| {
                    if *hovered { s.stage_state.hovered = Some(hover_id.clone()); s.stage_state.leave_at = None; }
                    else if s.stage_state.hovered.as_ref() == Some(&hover_id) { s.stage_state.hovered = None; }
                    cx.notify();
                }))
                .on_mouse_down(MouseButton::Left, cx.listener(move |s, event: &MouseDownEvent, _, cx| {
                    s.stage_state.suppress_click = false;
                    s.stage_state.press = Some(Press { group: press_id.clone(), start: event.position, position: event.position, moved: false });
                    cx.notify();
                }))
                .on_mouse_down(MouseButton::Right, cx.listener(move |s, event: &MouseDownEvent, _, cx| {
                    s.stage_state.press = None;
                    s.stage_state.menu = Some((menu_id.clone(), f32::from(event.position.y)));
                    cx.stop_propagation(); cx.notify();
                }))
                .child(preview)
                .on_click(cx.listener(move |s, event: &ClickEvent, _, cx| {
                    if event.is_right_click() || s.stage_state.suppress_click { return; }
                    s.stage_command(if event.modifiers().shift { Command::Merge(id.clone()) } else { Command::Activate(id.clone()) }, cx);
                })));
            if source_dragged && let Some(press) = &self.stage_state.press {
                root = root.child(
                    div()
                        .absolute()
                        .left(press.position.x - px(52.))
                        .top(press.position.y - px(38.))
                        .w(px(136.))
                        .h(px(112.))
                        .child(projection::preview(live, 0.)),
                );
            }
        }
        if dragging {
            root = root.child(
                div()
                    .absolute()
                    .left(px(layout::WIDTH + 24.))
                    .top(px(layout::TOP + 8.))
                    .text_size(px(12.))
                    .px(px(12.))
                    .py(px(7.))
                    .rounded(px(9.))
                    .bg(rgba(0x202020c0))
                    .text_color(rgb(0xffffff))
                    .child("Release to add to this group"),
            );
        }
        if let Some((id, y)) = self.stage_state.menu.clone() {
            let separate_id = id.clone();
            let y = y.clamp(layout::TOP, (height - 180.).max(layout::TOP));
            hits.push(Bounds::new(point(px(8.), px(y)), size(px(184.), px(170.))));
            let mut actions = menu("stage-actions", p)
                .absolute()
                .left(px(8.))
                .top(px(y))
                .w(px(184.))
                .on_mouse_down_out(cx.listener(|s, _, _, cx| {
                    s.stage_state.menu = None;
                    cx.notify();
                }))
                .child(
                    menu_item(eid("stage-add", &id), "Add to group", p)
                        .disabled(stage.busy)
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.stage_command(Command::Merge(id.clone()), cx)
                        })),
                );
            if stage
                .groups
                .iter()
                .any(|g| g.id == separate_id && g.classes.len() > 1)
            {
                actions = actions.child(
                    menu_item("stage-ungroup", "Separate apps", p)
                        .disabled(stage.busy)
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.stage_command(Command::Separate(separate_id.clone()), cx)
                        })),
                );
            }
            actions = actions.child(
                menu_item(
                    "stage-recent",
                    if stage.recent_hidden {
                        "Show recent apps"
                    } else {
                        "Hide recent apps"
                    },
                    p,
                )
                .disabled(stage.busy)
                .on_click(cx.listener(move |s, _, _, cx| {
                    s.stage_command(Command::SetRecentHidden(!stage.recent_hidden), cx)
                })),
            );
            root = root.child(
                actions.child(
                    menu_item("stage-off", "Turn off", p)
                        .disabled(stage.busy)
                        .on_click(cx.listener(|s, _, _, cx| s.stage_command(Command::Disable, cx))),
                ),
            );
        }
        if let Some(error) = stage.error.or_else(|| self.message.clone()) {
            root = root.child(
                caption(error, p)
                    .absolute()
                    .left(px(8.))
                    .top(px(layout::TOP))
                    .w(px(180.))
                    .text_size(px(10.))
                    .p(px(7.))
                    .rounded(px(7.))
                    .bg(rgb(p.surface)),
            );
        }
        div()
            .size_full()
            .on_children_prepainted(move |_, window, _| {
                let viewport = Bounds::new(point(px(0.), px(0.)), window.bounds().size);
                let regions: Vec<_> = hits
                    .iter()
                    .filter(|b| b.intersects(&viewport))
                    .map(|b| b.intersect(&viewport))
                    .collect();
                window.set_input_region(Some(&regions));
            })
            .child(root)
            .into_any_element()
    }
}
