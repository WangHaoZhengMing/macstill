//! Custom Dock: SwiftDE proportions and motion, with real Linux app/window state.
use super::*;
use crate::{backend::Folder, dock as geometry};
use std::time::Instant;
use swiftui_gpui::{menu as context_menu, menu_header, menu_item, menu_separator};

const HEIGHT: f32 = 440.;
#[derive(Clone)]
enum Target {
    Window(String),
    Shortcut(String),
    App(String),
    Launchpad,
    Settings,
    Downloads,
    Trash,
}
#[derive(Clone)]
struct Entry {
    key: String,
    name: String,
    icon: Option<PathBuf>,
    target: Target,
    app_id: Option<String>,
    running: bool,
    pinned: bool,
}
struct Menu {
    entry: Option<Entry>,
    x: f32,
}
struct Press {
    entry: Entry,
    start: Point<Pixels>,
    moved: bool,
    x: f32,
    y: f32,
}
struct Hit {
    entry: Entry,
    x: f32,
    width: f32,
}
struct Tween {
    from: f32,
    to: f32,
    since: Instant,
    duration: f32,
}
impl Tween {
    fn new(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            since: Instant::now(),
            duration: 0.18,
        }
    }
    fn value(&self, now: Instant) -> f32 {
        self.from
            + (self.to - self.from)
                * geometry::ease_out_cubic(
                    now.duration_since(self.since).as_secs_f32() / self.duration,
                )
    }
    fn set(&mut self, target: f32, now: Instant, duration: f32) {
        if self.to != target {
            self.from = self.value(now);
            self.to = target;
            self.since = now;
            self.duration = duration;
        }
    }
    fn active(&self, now: Instant) -> bool {
        self.from != self.to && now.duration_since(self.since).as_secs_f32() < self.duration
    }
}
pub(super) struct DockState {
    inside: bool,
    mouse_x: f32,
    hovered: Option<String>,
    magnify: Tween,
    reveal: Tween,
    leave_at: Option<Instant>,
    bounce: Option<(String, Instant)>,
    menu: Option<Menu>,
    press: Option<Press>,
    hits: Vec<Hit>,
    drop_target: Option<String>,
    interaction: (f32, f32, f32, f32),
}
impl Default for DockState {
    fn default() -> Self {
        Self {
            inside: false,
            mouse_x: -1000.,
            hovered: None,
            magnify: Tween::new(0.),
            reveal: Tween::new(1.),
            leave_at: Some(Instant::now()),
            bounce: None,
            menu: None,
            press: None,
            hits: vec![],
            drop_target: None,
            interaction: (0., 0., 0., 0.),
        }
    }
}
impl DesktopView {
    fn dock_entries(&self, snap: &Snapshot) -> Vec<Option<Entry>> {
        let p = &snap.preferences;
        let mut pinned = vec![];
        for shortcut in p.shortcuts.iter().filter(|s| s.visible) {
            let app = shortcut
                .effective_app_id()
                .and_then(|id| snap.apps.iter().find(|a| a.id == id));
            pinned.push(Entry {
                key: format!("shortcut:{}", shortcut.id),
                name: shortcut.name.clone(),
                icon: shortcut
                    .custom_icon
                    .clone()
                    .or_else(|| Some(Self::asset(&shortcut.asset))),
                target: Target::Shortcut(shortcut.id.clone()),
                app_id: app.map(|a| a.id.clone()),
                running: app.is_some_and(|a| snap.clients.iter().any(|c| matches_app(a, c))),
                pinned: true,
            });
        }
        let builtin = |key: &str, name: &str, asset: &str, target: Target| Entry {
            key: key.into(),
            name: name.into(),
            icon: Some(Self::asset(asset)),
            target,
            app_id: None,
            running: false,
            pinned: true,
        };
        let launchpad = builtin("launchpad", "Launchpad", "launchpad.png", Target::Launchpad);
        pinned.insert(usize::from(!pinned.is_empty()), launchpad);
        let native = |app: &DesktopApp, pinned| Entry {
            key: format!("app:{}", app.id),
            name: app.name.clone(),
            icon: Self::app_icon(app, p),
            target: Target::App(app.id.clone()),
            app_id: Some(app.id.clone()),
            running: snap.clients.iter().any(|c| matches_app(app, c)),
            pinned,
        };
        for id in &p.dock_pinned_apps {
            if let Some(app) = snap.apps.iter().find(|a| &a.id == id)
                && !pinned.iter().any(|e| e.app_id.as_ref() == Some(id))
            {
                pinned.push(native(app, true));
            }
        }
        pinned.push(builtin(
            "settings",
            "System Settings",
            "settings.png",
            Target::Settings,
        ));
        pinned.sort_by_key(|e| {
            p.dock_order
                .iter()
                .position(|k| k == &e.key)
                .unwrap_or(usize::MAX)
        });
        let mut running: Vec<_> = if p.dock_show_running {
            snap.apps
                .iter()
                .filter(|app| {
                    !pinned.iter().any(|e| e.app_id.as_ref() == Some(&app.id))
                        && snap.clients.iter().any(|c| matches_app(app, c))
                })
                .map(|a| native(a, false))
                .collect()
        } else {
            vec![]
        };
        if p.dock_show_running {
            for client in &snap.clients {
                if client.auxiliary
                    || client.class.starts_with("xdg-desktop-portal")
                    || client.class == "org.macstill.Desktop"
                    || snap.apps.iter().any(|app| matches_app(app, client))
                {
                    continue;
                }
                running.push(Entry {
                    key: format!("window:{}", client.address),
                    name: if client.class.is_empty() {
                        client.title.clone()
                    } else {
                        client.class.clone()
                    },
                    icon: Some(Self::asset("folder.png")),
                    target: Target::Window(client.address.clone()),
                    app_id: None,
                    running: true,
                    pinned: false,
                });
            }
        }
        let mut entries: Vec<_> = pinned.into_iter().map(Some).collect();
        if !running.is_empty() {
            entries.push(None);
            entries.extend(running.into_iter().map(Some));
        }
        entries.push(None);
        entries.push(Some(builtin(
            "downloads",
            "Downloads",
            "folder.png",
            Target::Downloads,
        )));
        entries.push(Some(builtin(
            "trash",
            "Trash",
            "trash-empty.png",
            Target::Trash,
        )));
        entries
    }
    fn activate_dock_entry(&mut self, entry: &Entry, window: &mut Window, cx: &mut Context<Self>) {
        self.dock_state.menu = None;
        self.dock_state.bounce = Some((entry.key.clone(), Instant::now()));
        match &entry.target {
            Target::Launchpad => {
                if self.mode == Mode::Launchpad {
                    self.close_launchpad(window, cx);
                } else {
                    let _ = open_on(Mode::Launchpad, window.display(cx).map(|d| d.id()), cx);
                }
            }
            Target::Window(address) => self.send(Action::Focus(address.clone()), cx),
            Target::Settings => {
                let _ = open_settings(&[], cx);
            }
            Target::Downloads => self.send(Action::OpenFolder(Folder::Downloads), cx),
            Target::Trash => self.send(Action::OpenFolder(Folder::Trash), cx),
            Target::App(id) => self.launch_from_grid(id.clone(), window, cx),
            Target::Shortcut(id) => {
                if let Some(app) = &entry.app_id {
                    self.launch_from_grid(app.clone(), window, cx);
                } else {
                    self.configure_shortcut(id.clone(), cx);
                }
            }
        }
        cx.notify();
    }
    fn configure_shortcut(&mut self, id: String, cx: &mut Context<Self>) {
        self.dock_state.menu = None;
        if let Err(e) = open_settings(&["--bind", &id], cx) {
            self.message = Some(e.to_string());
        }
        cx.notify();
    }
    fn dock_motion(&mut self, event: &MouseMoveEvent, origin_y: f32, cx: &mut Context<Self>) {
        let state = &mut self.dock_state;
        state.mouse_x = f32::from(event.position.x);
        let y = f32::from(event.position.y) - origin_y;
        let (left, top, right, bottom) = state.interaction;
        let inside = state.mouse_x >= left && state.mouse_x <= right && y >= top && y <= bottom;
        if inside != state.inside {
            state.inside = inside;
            state.leave_at = if inside { None } else { Some(Instant::now()) };
        }

        if let Some(press) = &mut state.press {
            let dx = f32::from(event.position.x - press.start.x);
            let dy = f32::from(event.position.y - press.start.y);
            if event.dragging() && dx * dx + dy * dy > 36. {
                press.moved = true;
            }
            press.x = f32::from(event.position.x);
            press.y = f32::from(event.position.y) - origin_y;
            state.drop_target = state
                .hits
                .iter()
                .find(|h| {
                    h.entry.pinned
                        && press.x >= h.x
                        && press.x < h.x + h.width
                        && !matches!(h.entry.target, Target::Downloads | Target::Trash)
                })
                .map(|h| h.entry.key.clone());
        }
        state.hovered = state
            .hits
            .iter()
            .find(|h| state.mouse_x >= h.x && state.mouse_x < h.x + h.width)
            .map(|h| h.entry.key.clone());
        cx.notify();
    }
    fn dock_release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(press) = self.dock_state.press.take() {
            if press.moved {
                if let Some(target) = self.dock_state.drop_target.take() {
                    // Transient running apps become pinned only when deliberately dropped into the app group.
                    let active: Vec<_> = self
                        .dock_state
                        .hits
                        .iter()
                        .filter(|h| {
                            h.entry.pinned
                                && !matches!(h.entry.target, Target::Downloads | Target::Trash)
                        })
                        .map(|h| h.entry.key.clone())
                        .collect();
                    self.edit(cx, |p| {
                        let mut active = active;
                        if !press.entry.pinned
                            && let Target::App(id) = &press.entry.target
                        {
                            if !p.dock_pinned_apps.contains(id) {
                                p.dock_pinned_apps.push(id.clone());
                            }
                            active.push(press.entry.key.clone());
                        }
                        geometry::move_to(&mut p.dock_order, &active, &press.entry.key, &target);
                    });
                }
            } else {
                self.activate_dock_entry(&press.entry, window, cx);
            }
        }
        cx.notify();
    }
    pub(super) fn dock(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let snap = self.snapshot(cx);
        let prefs = &snap.preferences;
        let entries = self.dock_entries(&snap);
        let mut width = f32::from(window.bounds().size.width);
        let height = if self.mode == Mode::Preview {
            360.
        } else {
            HEIGHT
        };
        if self.mode == Mode::Dock
            && let Some(bounds) = super::displays::window_bounds(window, cx)
        {
            width = f32::from(bounds.size.width);
            let wanted = size(px(width), px(height));
            if window.bounds().size != wanted {
                window.resize(wanted);
            }
        }
        let origin_y = if self.mode != Mode::Dock {
            f32::from(window.bounds().size.height) - height
        } else {
            0.
        };
        let now = Instant::now();
        let reduce_motion = cx.reduce_motion();
        let state = &mut self.dock_state;
        let dragging = state.press.as_ref().is_some_and(|p| p.moved);
        let magnifying =
            state.inside && prefs.dock_magnification && state.menu.is_none() && !dragging;
        state.magnify.set(
            if magnifying { 1. } else { 0. },
            now,
            if magnifying { 0.18 } else { 0.16 },
        );
        let waiting = state
            .leave_at
            .is_some_and(|at| now.duration_since(at).as_secs_f32() < 0.5);
        let bouncing = state
            .bounce
            .as_ref()
            .is_some_and(|(_, at)| now.duration_since(*at).as_secs_f32() < 0.36);
        let shown = self.mode == Mode::Launchpad
            || !prefs.dock_auto_hide
            || state.inside
            || state.menu.is_some()
            || state.press.is_some()
            || waiting
            || bouncing;
        state.reveal.set(if shown { 1. } else { 0. }, now, 0.22);
        let progress = if reduce_motion {
            state.magnify.to
        } else {
            state.magnify.value(now)
        };
        let reveal = if reduce_motion {
            state.reveal.to
        } else {
            state.reveal.value(now)
        };
        let slots: Vec<_> = entries.iter().map(Option::is_some).collect();
        let layout = geometry::layout(
            &slots,
            prefs.dock_size as f32,
            width,
            state.mouse_x,
            progress,
        );
        let base = layout.base;
        let slide = (base + 31.) * (1. - reveal);
        let baseline = height - 20. + slide;
        let max_icon = layout
            .cells
            .iter()
            .filter(|c| c.icon)
            .map(|c| c.width)
            .fold(base, f32::max);
        if !reduce_motion && (state.magnify.active(now) || state.reveal.active(now) || bouncing)
            || prefs.dock_auto_hide && waiting && !state.inside
        {
            window.request_animation_frame();
        }
        if !bouncing {
            state.bounce = None;
        }
        if self.mode == Mode::Dock {
            window.set_exclusive_zone(px(if prefs.dock_auto_hide { 0. } else { base + 28. }));
            let region = if state.menu.is_some() || dragging {
                Bounds::new(point(px(0.), px(0.)), size(px(width), px(height)))
            } else if reveal < 0.01 {
                Bounds::new(
                    point(px(layout.left), px(height - 3.)),
                    size(px(layout.width), px(3.)),
                )
            } else {
                let top = (baseline - max_icon - 8.).clamp(0., height - 3.);
                Bounds::new(
                    point(px(layout.left - 6.), px(top)),
                    size(px(layout.width + 12.), px(height - top)),
                )
            };
            window.set_input_region(Some(&[region]));
        }
        let interaction_top = if reveal < 0.01 {
            height - 3.
        } else {
            (baseline - max_icon - 8.).clamp(0., height - 3.)
        };
        self.dock_state.interaction = (
            layout.left - 6.,
            interaction_top,
            layout.left + layout.width + 6.,
            height,
        );
        let dark = prefs.dark;
        let foreground = if dark { 0xf5f5f7 } else { 0x1d1d1f };
        let border = if dark { 0xffffff26 } else { 0x00000024 };
        let mut root = div()
            .id("dock-surface")
            .size_full()
            .relative()
            .font_family(cx.global::<Fonts>().text.clone())
            .text_color(rgb(foreground))
            // Track actual platform exit, not a layout-derived `on_hover`:
            // GPUI retains the last mouse coordinate after Wayland Leave. A new
            // frame could otherwise consider this full-width surface hovered again.
            .on_mouse_exit(cx.listener(|s, _, _, cx| {
                s.dock_state.inside = false;
                s.dock_state.leave_at = Some(Instant::now());
                s.dock_state.hovered = None;
                s.dock_state.menu = None;
                if !s.dock_state.press.as_ref().is_some_and(|p| p.moved) {
                    s.dock_state.press = None;
                }
                cx.notify();
            }))
            .on_mouse_move(cx.listener(move |s, event, _, cx| s.dock_motion(event, origin_y, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|s, _, w, cx| s.dock_release(w, cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| {
                    s.dock_state.press = None;
                    s.dock_state.drop_target = None;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .absolute()
                    .left(px(layout.left))
                    .bottom(px(8. - slide))
                    .w(px(layout.width))
                    .h(px(base + 18.))
                    .rounded(px(16.))
                    .border_1()
                    .border_color(rgba(border))
                    .bg(Palette::new(dark).material(swiftui_gpui::theme::Material::Chrome))
                    .shadow_lg()
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .rounded(px(15.))
                            .border_t_1()
                            .border_color(rgba(if dark { 0xffffff18 } else { 0xffffffb0 })),
                    )
                    .id("dock-material"),
            );
        self.dock_state.hits.clear();
        for (entry, cell) in entries.iter().zip(&layout.cells) {
            let Some(entry) = entry else {
                root = root.child(
                    div()
                        .absolute()
                        .left(px(cell.x + cell.width / 2.))
                        .bottom(px(18. - slide))
                        .w(px(1.))
                        .h(px(base * 0.72))
                        .bg(rgba(if dark { 0xffffff30 } else { 0x00000025 })),
                );
                continue;
            };
            self.dock_state.hits.push(Hit {
                entry: entry.clone(),
                x: cell.x,
                width: cell.width,
            });
            let lift = if reduce_motion {
                0.
            } else {
                self.dock_state
                    .bounce
                    .as_ref()
                    .filter(|(key, _)| key == &entry.key)
                    .map(|(_, at)| geometry::bounce(now.duration_since(*at).as_secs_f32()))
                    .unwrap_or(0.)
            };
            let pressed = self
                .dock_state
                .press
                .as_ref()
                .is_some_and(|p| p.entry.key == entry.key);
            let dragging_this = pressed && dragging;
            let down_entry = entry.clone();
            let menu_entry = entry.clone();
            root = root.child(
                div()
                    .id(eid("dock-icon", &entry.key))
                    .absolute()
                    .left(px(cell.x))
                    .top(px(baseline - cell.width - lift))
                    .size(px(cell.width))
                    .cursor_pointer()
                    .opacity(if dragging_this {
                        0.25
                    } else if pressed {
                        0.68
                    } else {
                        1.
                    })
                    .child(macstill_ui::icon(entry.icon.clone(), cell.width))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |s, event: &MouseDownEvent, _, cx| {
                            s.dock_state.menu = None;
                            s.dock_state.press = Some(Press {
                                entry: down_entry.clone(),
                                start: event.position,
                                moved: false,
                                x: f32::from(event.position.x),
                                y: f32::from(event.position.y) - origin_y,
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |s, event: &MouseDownEvent, _, cx| {
                            s.dock_state.menu = Some(Menu {
                                entry: Some(menu_entry.clone()),
                                x: f32::from(event.position.x),
                            });
                            s.dock_state.press = None;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );
            if entry.running {
                root = root.child(
                    div()
                        .absolute()
                        .left(px(cell.x + cell.width / 2. - 2.))
                        .bottom(px(12. - slide))
                        .size(px(4.))
                        .rounded_full()
                        .bg(rgb(if dark { 0xe3e3e8 } else { 0x3e3e43 })),
                );
            }
            if self.dock_state.menu.is_none()
                && !dragging
                && self.dock_state.inside
                && self.dock_state.hovered.as_ref() == Some(&entry.key)
            {
                let label = if matches!(entry.target, Target::Shortcut(_)) && entry.app_id.is_none()
                {
                    format!("{} · Choose app…", entry.name)
                } else {
                    entry.name.clone()
                };
                let tooltip_width = (label.chars().count() as f32 * 7. + 24.).clamp(76., 270.);
                let x = (cell.x + cell.width / 2. - tooltip_width / 2.)
                    .clamp(6., (width - tooltip_width - 6.).max(6.));
                root = root.child(
                    div()
                        .absolute()
                        .left(px(x))
                        .top(px(baseline - cell.width - lift - 42.))
                        .w(px(tooltip_width))
                        .h(px(28.))
                        .rounded(px(7.))
                        .border_1()
                        .border_color(rgba(border))
                        .bg(Palette::new(dark).material(swiftui_gpui::theme::Material::Menu))
                        .shadow_md()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(12.))
                        .child(label),
                );
            }
        }
        if let Some(press) = self.dock_state.press.as_ref().filter(|p| p.moved) {
            if let Some(target) = &self.dock_state.drop_target
                && target != &press.entry.key
                && let Some(hit) = self.dock_state.hits.iter().find(|h| &h.entry.key == target)
            {
                root = root.child(
                    div()
                        .absolute()
                        .left(px(hit.x - 3.))
                        .top(px(baseline - base))
                        .w(px(3.))
                        .h(px(base))
                        .rounded_full()
                        .bg(rgb(0x3478f6)),
                );
            }
            root = root.child(
                div()
                    .absolute()
                    .left(px(press.x - base / 2.))
                    .top(px(press.y - base / 2.))
                    .size(px(base))
                    .opacity(0.85)
                    .child(macstill_ui::icon(press.entry.icon.clone(), base)),
            );
        }
        if self.dock_state.menu.is_some() {
            root = root.child(self.dock_menu(&snap, width, height, base, cx));
        }
        // Surface errors are visible even when no Settings window is open.
        if let Some(error) = self.message.clone().or(snap.error) {
            root = root.child(
                div()
                    .absolute()
                    .bottom(px(base + 104.))
                    .left(px((width - 420.).max(0.) / 2.))
                    .w(px(420.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .bg(rgb(0xffe9cf))
                    .text_color(rgb(0x8d4d0d))
                    .text_size(px(12.))
                    .child(error),
            );
        }
        root.into_any_element()
    }
    fn dock_menu(
        &self,
        snap: &Snapshot,
        width: f32,
        height: f32,
        base: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let menu = self.dock_state.menu.as_ref().unwrap();
        let p = Palette::new(snap.preferences.dark);
        let x = (menu.x - 112.).clamp(8., (width - 232.).max(8.));
        let mut content = context_menu("dock-context-menu", p)
            .absolute()
            .left(px(x))
            .bottom(px(base + 35.))
            .max_h(px(height - base - 50.))
            .overflow_y_scroll()
            .on_mouse_down_out(cx.listener(|s, _, _, cx| {
                s.dock_state.menu = None;
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        let row = |id: &str, label: String| menu_item(SharedString::from(id.to_owned()), label, p);
        if let Some(entry) = &menu.entry {
            content = content.child(menu_header(entry.name.clone(), p));
            if let Some(id) = &entry.app_id {
                for (i, client) in snap
                    .clients
                    .iter()
                    .filter(|c| {
                        snap.apps
                            .iter()
                            .find(|a| &a.id == id)
                            .is_some_and(|a| matches_app(a, c))
                    })
                    .enumerate()
                {
                    let address = client.address.clone();
                    content = content.child(
                        row(
                            &format!("window-{i}"),
                            if client.title.is_empty() {
                                "Window".into()
                            } else {
                                client.title.chars().take(29).collect()
                            },
                        )
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.dock_state.menu = None;
                            s.send(Action::Focus(address.clone()), cx);
                        })),
                    );
                }
            }
            let selected = entry.clone();
            content = content.child(
                row(
                    "dock-open",
                    if matches!(entry.target, Target::Shortcut(_)) && entry.app_id.is_none() {
                        "Choose application…".into()
                    } else if entry.running {
                        "Show Application".into()
                    } else {
                        "Open".into()
                    },
                )
                .on_click(cx.listener(move |s, _, w, cx| s.activate_dock_entry(&selected, w, cx))),
            );
            if let Some(id) = &entry.app_id {
                let id = id.clone();
                content = content.child(row("dock-new", "Launch new instance".into()).on_click(
                    cx.listener(move |s, _, _, cx| {
                        s.dock_state.menu = None;
                        s.send(Action::LaunchNew(id.clone()), cx);
                    }),
                ));
            }
            match &entry.target {
                Target::Shortcut(id) => {
                    let binding = id.clone();
                    let hiding = id.clone();
                    content =
                        content
                            .child(row("dock-binding", "Choose application…".into()).on_click(
                                cx.listener(move |s, _, _, cx| {
                                    s.configure_shortcut(binding.clone(), cx)
                                }),
                            ))
                            .child(row("dock-hide", "Remove from Dock".into()).on_click(
                                cx.listener(move |s, _, _, cx| {
                                    s.dock_state.menu = None;
                                    s.edit(cx, |p| {
                                        if let Some(v) =
                                            p.shortcuts.iter_mut().find(|v| v.id == hiding)
                                        {
                                            v.visible = false;
                                        }
                                    });
                                }),
                            ));
                }
                Target::App(id) => {
                    let id = id.clone();
                    let pinned = entry.pinned;
                    content = content.child(
                        row(
                            "dock-keep",
                            if pinned {
                                "Remove from Dock".into()
                            } else {
                                "Keep in Dock".into()
                            },
                        )
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.dock_state.menu = None;
                            s.edit(cx, |p| {
                                if pinned {
                                    p.dock_pinned_apps.retain(|v| v != &id);
                                } else if !p.dock_pinned_apps.contains(&id) {
                                    p.dock_pinned_apps.push(id.clone());
                                }
                            });
                        })),
                    );
                }
                _ => {}
            }
            // Resolve the clicked application's most recently focused window, not
            // the globally focused window (which may belong to another app).
            let close_target = match &entry.target {
                Target::Window(address) => snap.clients.iter().find(|c| &c.address == address),
                _ => snap
                    .clients
                    .iter()
                    .filter(|client| {
                        entry
                            .app_id
                            .as_ref()
                            .and_then(|id| snap.apps.iter().find(|app| &app.id == id))
                            .is_some_and(|app| matches_app(app, client))
                    })
                    .min_by_key(|client| {
                        client
                            .focus_history_id
                            .filter(|id| *id >= 0)
                            .unwrap_or(i64::MAX)
                    }),
            }
            .map(|client| client.address.clone());
            if let Some(address) = close_target {
                content = content.child(menu_separator(p)).child(
                    row("dock-close-window", "Close Current Window".into()).on_click(cx.listener(
                        move |s, _, _, cx| {
                            s.dock_state.menu = None;
                            s.send(Action::CloseWindow(address.clone()), cx);
                        },
                    )),
                );
            }
            if entry.running {
                if let Some(id) = entry.app_id.clone() {
                    content = content.child(menu_separator(p)).child(
                        row("dock-quit", "Quit".into()).on_click(cx.listener(
                            move |s, _, _, cx| {
                                s.dock_state.menu = None;
                                s.send(Action::Quit(id.clone()), cx);
                            },
                        )),
                    );
                }
            }
        }
        content.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn exit_and_reentry_tweens_start_from_the_current_value() {
        let start = Instant::now();
        let mut tween = Tween::new(0.);
        tween.set(1., start, 0.18);
        let mid = start + Duration::from_millis(80);
        let before_leave = tween.value(mid);
        tween.set(0., mid, 0.16);
        assert_eq!(tween.value(mid), before_leave);
        assert_eq!(tween.value(mid + Duration::from_millis(180)), 0.);
        assert!(!tween.active(mid + Duration::from_millis(180)));
        tween.set(1., mid + Duration::from_millis(200), 0.18);
        assert_eq!(tween.value(mid + Duration::from_millis(200)), 0.);
    }
}
