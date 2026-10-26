use super::*;

pub(crate) struct Arrangement {
    monitors: Vec<Monitor>,
    original: Vec<Monitor>,
    store: Entity<Store>,
    palette: Palette,
    drag: Option<(usize, Point<Pixels>, i32, i32, f32, f32, f32)>,
    message: Option<String>,
    pending: bool,
    _subscription: Subscription,
}
impl Arrangement {
    pub fn new(
        monitors: Vec<Monitor>,
        store: Entity<Store>,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscription = cx.observe(&store, |s, _, cx| {
            let state = &s.store.read(cx).system_snapshot;
            if s.pending && !state.busy {
                s.pending = false;
                s.message = state
                    .error
                    .clone()
                    .or_else(|| Some("Arrangement saved.".into()));
                if state.error.is_none() {
                    s.original = s.monitors.clone();
                }
            }
            cx.notify();
        });
        Self {
            original: monitors.clone(),
            monitors,
            store,
            palette,
            drag: None,
            message: None,
            pending: false,
            _subscription: subscription,
        }
    }
    fn geometry(&self) -> (f32, f32, f32) {
        let x = self.monitors.iter().map(|m| m.x).min().unwrap_or(0) as f32;
        let y = self.monitors.iter().map(|m| m.y).min().unwrap_or(0) as f32;
        let right = self
            .monitors
            .iter()
            .map(|m| m.x as f32 + m.logical_size().0 as f32)
            .fold(x + 1., f32::max);
        let bottom = self
            .monitors
            .iter()
            .map(|m| m.y as f32 + m.logical_size().1 as f32)
            .fold(y + 1., f32::max);
        ((340. / (right - x)).min(180. / (bottom - y)), x, y)
    }
    fn release(&mut self) {
        if let Some((index, ..)) = self.drag.take() {
            snap(&mut self.monitors, index);
        }
    }
}
fn snap(monitors: &mut [Monitor], index: usize) {
    let m = &monitors[index];
    let (w, h) = m.logical_size();
    let (x, y) = (m.x, m.y);
    let mut candidates = Vec::new();
    for (i, other) in monitors.iter().enumerate() {
        if i == index {
            continue;
        }
        let (ow, oh) = other.logical_size();
        for cy in [
            y.clamp(other.y - h as i32 + 1, other.y + oh as i32 - 1),
            other.y,
            other.y + oh as i32 - h as i32,
        ] {
            candidates.push((other.x - w as i32, cy));
            candidates.push((other.x + ow as i32, cy));
        }
        for cx in [
            x.clamp(other.x - w as i32 + 1, other.x + ow as i32 - 1),
            other.x,
            other.x + ow as i32 - w as i32,
        ] {
            candidates.push((cx, other.y - h as i32));
            candidates.push((cx, other.y + oh as i32));
        }
    }
    candidates
        .sort_by_key(|(cx, cy)| (*cx as i64 - x as i64).pow(2) + (*cy as i64 - y as i64).pow(2));
    for (cx, cy) in candidates {
        monitors[index].x = cx;
        monitors[index].y = cy;
        if macstill_hyprland::displays::validate_arrangement(monitors).is_ok() {
            return;
        }
    }
    monitors[index].x = x;
    monitors[index].y = y;
}
impl Render for Arrangement {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let (factor, x, y) = self
            .drag
            .as_ref()
            .map(|d| (d.4, d.5, d.6))
            .unwrap_or_else(|| self.geometry());
        let mut diagram = div()
            .id("arrangement-canvas")
            .relative()
            .w(px(380.))
            .h(px(220.))
            .overflow_hidden()
            .rounded(px(6.))
            .bg(rgb(p.background))
            .on_mouse_move(cx.listener(|s, e: &MouseMoveEvent, _, cx| {
                if let Some((i, start, x, y, factor, ..)) = s.drag {
                    s.monitors[i].x = (x as f32 + f32::from(e.position.x - start.x) / factor)
                        .round()
                        .clamp(-100_000., 100_000.) as i32;
                    s.monitors[i].y = (y as f32 + f32::from(e.position.y - start.y) / factor)
                        .round()
                        .clamp(-100_000., 100_000.) as i32;
                    s.message = None;
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| {
                    s.release();
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| {
                    s.release();
                    cx.notify();
                }),
            );
        for (i, m) in self.monitors.iter().enumerate() {
            let selector = format!("arrange-output-{}", m.name);
            let (w, h) = m.logical_size();
            diagram = diagram.child(
                div()
                    .id(format!("arrange-output-{}", m.name))
                    .debug_selector(move || selector.clone())
                    .absolute()
                    .left(px(20. + (m.x as f32 - x) * factor))
                    .top(px(20. + (m.y as f32 - y) * factor))
                    .w(px(w as f32 * factor))
                    .h(px(h as f32 * factor))
                    .border_2()
                    .border_color(rgb(p.accent))
                    .rounded(px(4.))
                    .bg(rgb(if p.dark { 0x254271 } else { 0xd5e5fd }))
                    .flex()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .cursor_pointer()
                    .text_size(px(12.))
                    .child(m.name.clone())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |s, e: &MouseDownEvent, _, cx| {
                            if s.pending {
                                return;
                            }
                            let (f, x, y) = s.geometry();
                            let m = &s.monitors[i];
                            s.drag = Some((i, e.position, m.x, m.y, f, x, y));
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );
        }
        let validation = macstill_hyprland::displays::validate_arrangement(&self.monitors)
            .err()
            .map(|e| e.to_string());
        div()
            .p(px(14.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .rounded(px(9.))
            .border_1()
            .border_color(rgb(p.line))
            .bg(rgb(p.surface))
            .shadow_lg()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Display arrangement"),
            )
            .child(diagram)
            .child(
                caption(
                    "Drag displays to match their physical positions. Edges snap together.",
                    p,
                )
                .max_w(px(380.)),
            )
            .when(self.message.is_some() || validation.is_some(), |d| {
                d.child(
                    caption(self.message.clone().or(validation.clone()).unwrap(), p)
                        .max_w(px(380.)),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        button("arrangement-reset", "Reset", p)
                            .disabled(self.pending)
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.monitors = s.original.clone();
                                s.message = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        button(
                            "arrangement-apply",
                            if self.pending { "Applying…" } else { "Apply" },
                            p,
                        )
                        .disabled(
                            self.pending
                                || self.drag.is_some()
                                || validation.is_some()
                                || self.monitors == self.original,
                        )
                        .on_click(cx.listener(|s, _, _, cx| {
                            let positions = s
                                .monitors
                                .iter()
                                .map(|m| (m.name.clone(), m.x, m.y))
                                .collect();
                            match s.store.read(cx).system.arrange_displays(positions) {
                                Ok(()) => {
                                    s.pending = true;
                                    s.message = None;
                                }
                                Err(e) => s.message = Some(e.to_string()),
                            }
                            cx.notify();
                        })),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn monitor(name: &str, x: i32, y: i32) -> Monitor {
        Monitor {
            name: name.into(),
            description: String::new(),
            width: 1920,
            height: 1080,
            x,
            y,
            scale: 1.,
            refresh_rate: 60.,
            transform: 0,
        }
    }
    #[::core::prelude::v1::test]
    fn snapping_supports_left_right_above_below() {
        for (x, y, expected) in [
            (1950, 0, (1920, 0)),
            (-1940, 0, (-1920, 0)),
            (0, -1100, (0, -1080)),
            (0, 1100, (0, 1080)),
        ] {
            let mut displays = vec![monitor("A", 0, 0), monitor("B", x, y)];
            snap(&mut displays, 1);
            assert_eq!((displays[1].x, displays[1].y), expected);
            macstill_hyprland::displays::validate_arrangement(&displays).unwrap();
        }
    }
    #[::core::prelude::v1::test]
    fn invalid_layouts_are_rejected() {
        for (x, y) in [(100, 0), (3000, 0), (1920, 1080)] {
            assert!(
                macstill_hyprland::displays::validate_arrangement(&[
                    monitor("A", 0, 0),
                    monitor("B", x, y)
                ])
                .is_err()
            );
        }
    }
    #[::core::prelude::v1::test]
    fn rotated_scaled_outputs_use_logical_edges() {
        let mut b = monitor("B", 0, -1300);
        b.scale = 1.5;
        b.transform = 1;
        let mut displays = vec![monitor("A", 0, 0), b];
        snap(&mut displays, 1);
        assert_eq!(displays[1].y, -1280);
        assert_eq!(displays[1].scale, 1.5);
        assert_eq!(displays[1].transform, 1);
    }
}
