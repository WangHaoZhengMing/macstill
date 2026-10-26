use super::*;
use crate::{
    dock::ease_out_cubic,
    launchpad::{Direction, Grid},
};
use gpui_kit::base::input::InputEditorStyle;
use std::time::Instant;
use swiftui_gpui::{menu as context_menu, menu_item, menu_separator};

struct PageSlide {
    from: usize,
    to: usize,
    began: Instant,
}
struct AppMenu {
    id: String,
    x: f32,
    y: f32,
}
pub(super) struct LaunchpadState {
    opened: Instant,
    closing: Option<Instant>,
    slide: Option<PageSlide>,
    selected: usize,
    capacity: usize,
    keyboard_selection: bool,
    pressed: Option<String>,
    menu: Option<AppMenu>,
    scroll_delta: f32,
    last_scroll: Instant,
    pub(super) pending_launch: Option<String>,
}
impl Default for LaunchpadState {
    fn default() -> Self {
        Self {
            opened: Instant::now(),
            closing: None,
            slide: None,
            selected: 0,
            capacity: 28,
            keyboard_selection: false,
            pressed: None,
            menu: None,
            scroll_delta: 0.,
            last_scroll: Instant::now() - Duration::from_secs(1),
            pending_launch: None,
        }
    }
}
impl LaunchpadState {
    pub(super) fn reset_search(&mut self) {
        self.selected = 0;
        self.keyboard_selection = false;
        self.slide = None;
        self.menu = None;
    }
    pub(super) fn is_closing(&self) -> bool {
        self.closing.is_some()
    }
}
impl DesktopView {
    fn launcher_apps(&self, cx: &App) -> Vec<DesktopApp> {
        let query = self.search.read(cx).value().to_lowercase();
        let snap = &self.store.read(cx).snapshot;
        crate::launchpad::applications(&snap.apps, &snap.preferences)
            .into_iter()
            .filter(|a| {
                crate::launchpad::matches(&a.name, &a.id, &query)
                    || a.id
                        .strip_prefix(crate::launchpad::SHORTCUT_PREFIX)
                        .and_then(|id| snap.preferences.shortcuts.iter().find(|s| s.id == id))
                        .and_then(|s| s.effective_app_id())
                        .and_then(|id| snap.apps.iter().find(|a| a.id == id))
                        .is_some_and(|a| crate::launchpad::matches(&a.name, &a.id, &query))
            })
            .collect()
    }
    pub(super) fn launch_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let apps = self.launcher_apps(cx);
        if let Some(app) = apps.get(
            self.launchpad_state
                .selected
                .min(apps.len().saturating_sub(1)),
        ) {
            self.launch_from_grid(app.id.clone(), window, cx);
        }
    }
    pub(super) fn close_launchpad(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.launchpad_state.is_closing() {
            return;
        }
        self.launchpad_state.closing = Some(Instant::now());
        self.launchpad_state.menu = None;
        if cx.reduce_motion() {
            window.remove_window();
        } else {
            cx.notify();
        }
    }
    fn launcher_page(&mut self, page: usize, pages: usize, cx: &mut Context<Self>) {
        let page = page.min(pages.saturating_sub(1));
        if self.page_index == page || self.launchpad_state.is_closing() {
            return;
        }
        self.launchpad_state.slide = if cx.reduce_motion() {
            None
        } else {
            Some(PageSlide {
                from: self.page_index,
                to: page,
                began: Instant::now(),
            })
        };
        self.page_index = page;
        self.launchpad_state.selected = page * self.launchpad_state.capacity;
        self.launchpad_state.pressed = None;
        self.launchpad_state.menu = None;
        self.launchpad_state.scroll_delta = 0.;
        cx.notify();
    }
    fn launcher_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modified = event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
            || event.keystroke.modifiers.platform;
        if modified {
            return;
        }
        let apps = self.launcher_apps(cx);
        let grid = Grid::new(
            f32::from(window.bounds().size.width),
            f32::from(window.bounds().size.height),
        );
        match key {
            "escape" => {
                if self.launchpad_state.menu.take().is_some() {
                    cx.notify();
                } else if !self.search.read(cx).value().is_empty() {
                    self.search.update(cx, |s, cx| s.set_value("", window, cx));
                } else {
                    self.close_launchpad(window, cx);
                }
            }
            "enter" => self.launch_selected(window, cx),
            "pageup" | "pagedown" => {
                let page = if key == "pageup" {
                    self.page_index.saturating_sub(1)
                } else {
                    self.page_index + 1
                };
                self.launcher_page(page, grid.pages(apps.len()), cx);
                self.launchpad_state.selected =
                    (self.page_index * grid.capacity()).min(apps.len().saturating_sub(1));
                self.launchpad_state.keyboard_selection = true;
            }
            "left" | "right" | "up" | "down" | "tab" => {
                let direction = match key {
                    "left" => Direction::Left,
                    "up" => Direction::Up,
                    "down" => Direction::Down,
                    "tab" if event.keystroke.modifiers.shift => Direction::Left,
                    _ => Direction::Right,
                };
                let index = grid.navigate(self.launchpad_state.selected, apps.len(), direction);
                self.launcher_page(grid.page_for(index), grid.pages(apps.len()), cx);
                self.launchpad_state.selected = index;
                self.launchpad_state.keyboard_selection = true;
                cx.notify();
            }
            _ => return,
        }
        cx.stop_propagation();
    }
    fn launcher_scroll(&mut self, event: &ScrollWheelEvent, pages: usize, cx: &mut Context<Self>) {
        let now = Instant::now();
        if now
            .duration_since(self.launchpad_state.last_scroll)
            .as_secs_f32()
            < 0.32
        {
            return;
        }
        if now
            .duration_since(self.launchpad_state.last_scroll)
            .as_secs_f32()
            > 0.8
        {
            self.launchpad_state.scroll_delta = 0.;
        }
        let delta = event.delta.pixel_delta(px(24.));
        let x = f32::from(delta.x);
        let y = f32::from(delta.y);
        let amount = if x.abs() > y.abs() { x } else { y };
        if self.launchpad_state.scroll_delta.signum() != amount.signum() {
            self.launchpad_state.scroll_delta = 0.;
        }
        self.launchpad_state.scroll_delta += amount;
        if self.launchpad_state.scroll_delta.abs() >= 30. {
            let page = if self.launchpad_state.scroll_delta < 0. {
                self.page_index + 1
            } else {
                self.page_index.saturating_sub(1)
            };
            self.launcher_page(page, pages, cx);
            self.launchpad_state.last_scroll = now;
            self.launchpad_state.scroll_delta = 0.;
            self.launchpad_state.keyboard_selection = false;
        }
        cx.stop_propagation();
    }
    pub(super) fn launchpad(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
            && let Some(bounds) = super::displays::window_bounds(window, cx)
        {
            let wanted = bounds.size;
            if window.bounds().size != wanted {
                window.resize(wanted);
            }
        }
        let size = window.bounds().size;
        let width = f32::from(size.width);
        let height = f32::from(size.height);
        let grid = Grid::new(width, height);
        self.launchpad_state.capacity = grid.capacity();
        let apps = self.launcher_apps(cx);
        let snap = self.snapshot(cx);
        let pages = grid.pages(apps.len());
        self.page_index = self.page_index.min(pages - 1);
        self.launchpad_state.selected = self
            .launchpad_state
            .selected
            .min(apps.len().saturating_sub(1));
        let now = Instant::now();
        let opening = if cx.reduce_motion() {
            1.
        } else {
            ease_out_cubic(
                now.duration_since(self.launchpad_state.opened)
                    .as_secs_f32()
                    / 0.22,
            )
        };
        let closing = self
            .launchpad_state
            .closing
            .map(|at| {
                if cx.reduce_motion() {
                    1.
                } else {
                    ease_out_cubic(now.duration_since(at).as_secs_f32() / 0.18)
                }
            })
            .unwrap_or(0.);
        if closing >= 1. {
            window.remove_window();
            return div().into_any_element();
        }
        if opening < 1. || closing > 0. {
            window.request_animation_frame();
        }
        let visibility = opening * (1. - closing);
        let zoom = 1. + (1. - opening) * 0.06 + closing * 0.06;
        self.search.update(cx, |s, _| {
            s.set_editor_style(InputEditorStyle {
                foreground: rgb(0xffffff).into(),
                muted_foreground: rgba(0xffffffaa).into(),
                caret: rgb(0xffffff).into(),
                selection: rgba(0x3478f680).into(),
                ..Default::default()
            })
        });
        let query = self.search.read(cx).value();
        let mut root = div()
            .id("launchpad-root")
            .size_full()
            .relative()
            .overflow_hidden()
            .font_family(cx.global::<Fonts>().text.clone())
            .text_color(rgb(0xffffff))
            .capture_key_down(cx.listener(|s, event, w, cx| s.launcher_key(event, w, cx)))
            .on_scroll_wheel(
                cx.listener(move |s, event, _, cx| s.launcher_scroll(event, pages, cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| {
                    s.launchpad_state.pressed = None;
                    cx.notify();
                }),
            )
            .on_mouse_exit(cx.listener(|s, _, _, cx| {
                s.launchpad_state.pressed = None;
                cx.notify();
            }))
            .child(
                div().absolute().inset_0().opacity(visibility).child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(rgba(if snap.preferences.dark {
                            0x00000066
                        } else {
                            0x00000052
                        })),
                ),
            )
            .child(
                div()
                    .id("launchpad-backdrop")
                    .absolute()
                    .inset_0()
                    .on_click(cx.listener(|s, _, w, cx| {
                        if s.launchpad_state.menu.take().is_some() {
                            cx.notify();
                        } else {
                            s.close_launchpad(w, cx);
                        }
                    })),
            );
        let search_width = (width * 0.16).clamp(210., 260.);
        root = root.child(
            div()
                .id("launchpad-search")
                .absolute()
                .left(px((width - search_width) / 2.))
                .top(px(height * 0.038))
                .w(px(search_width))
                .h(px(28.))
                .px(px(9.))
                .rounded(px(6.))
                .border_1()
                .border_color(rgba(if query.is_empty() {
                    0xffffff50
                } else {
                    0x77b5ffff
                }))
                .bg(rgba(0x00000048))
                .opacity(visibility)
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.))
                .child(Self::bar_icon("search", 0xffffff))
                .child(div().flex_1().min_w_0().child(Input::new(&self.search)))
                .when(!query.is_empty(), |d| {
                    d.child(
                        div()
                            .id("launchpad-clear")
                            .size(px(16.))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(15.))
                            .cursor_pointer()
                            .text_color(rgba(0xffffffbb))
                            .child("×")
                            .on_click(cx.listener(|s, _, w, cx| {
                                s.search.update(cx, |v, cx| {
                                    v.set_value("", w, cx);
                                    v.focus(w, cx);
                                });
                                cx.stop_propagation();
                            })),
                    )
                })
                .on_click(|_, _, cx| cx.stop_propagation()),
        );
        let mut visible_pages = vec![(self.page_index, 0., 1.)];
        if let Some(slide) = &self.launchpad_state.slide {
            let t = ease_out_cubic(now.duration_since(slide.began).as_secs_f32() / 0.24);
            if t < 1. && slide.to == self.page_index {
                let direction = if slide.to > slide.from { 1. } else { -1. };
                visible_pages = vec![
                    (slide.from, -t * direction * width * 0.85, 1. - t),
                    (slide.to, (1. - t) * direction * width * 0.85, t),
                ];
                window.request_animation_frame();
            } else {
                self.launchpad_state.slide = None;
            }
        }
        for (page, offset, alpha) in visible_pages {
            for (slot, app) in apps
                .iter()
                .skip(page * grid.capacity())
                .take(grid.capacity())
                .enumerate()
            {
                let (x, y) = grid.center(slot);
                let x = width / 2. + (x - width / 2.) * zoom + offset;
                let y = height / 2. + (y - height / 2.) * zoom;
                let selected = self.launchpad_state.keyboard_selection
                    && self.launchpad_state.selected == page * grid.capacity() + slot;
                let pressed = self.launchpad_state.pressed.as_ref() == Some(&app.id)
                    || self.launchpad_state.pending_launch.as_ref() == Some(&app.id);
                let art_size = grid.icon * zoom * if pressed { 0.93 } else { 1. };
                let app_id = app.id.clone();
                let down_id = app.id.clone();
                let menu_id = app.id.clone();
                let tile_width = grid.label_width.max(grid.icon + 20.);
                root = root.child(
                    div()
                        .id(eid("launchpad-app", &format!("{page}:{}", app.id)))
                        .absolute()
                        .left(px(x - tile_width / 2.))
                        .top(px(y - grid.icon / 2. - 7.))
                        .w(px(tile_width))
                        .h(px(grid.icon + 45.))
                        .rounded(px(14.))
                        .cursor_pointer()
                        .opacity(visibility * alpha)
                        .child(
                            div()
                                .relative()
                                .mx_auto()
                                .w(px(grid.icon + 14.))
                                .h(px(grid.icon + 14.))
                                .rounded(px(grid.icon * 0.24))
                                .border_2()
                                .border_color(if selected {
                                    rgba(0xffffffbb)
                                } else {
                                    rgba(0xffffff00)
                                })
                                .bg(if selected {
                                    rgba(0xffffff22)
                                } else {
                                    rgba(0xffffff00)
                                })
                                .child(
                                    div()
                                        .absolute()
                                        .left(px((grid.icon + 14. - art_size) / 2. - 2.))
                                        .top(px((grid.icon + 14. - art_size) / 2. - 2.))
                                        .child(macstill_ui::icon(
                                            Self::app_icon(app, &snap.preferences),
                                            art_size,
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .w_full()
                                .text_center()
                                .text_size(px(12.))
                                .line_height(px(17.))
                                .font_weight(FontWeight::MEDIUM)
                                .truncate()
                                .child(app.name.clone()),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |s, _, _, cx| {
                                s.launchpad_state.pressed = Some(down_id.clone());
                                s.launchpad_state.menu = None;
                                cx.notify();
                            }),
                        )
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|s, _, _, cx| {
                                s.launchpad_state.pressed = None;
                                cx.notify();
                            }),
                        )
                        .on_click(cx.listener(move |s, _, w, cx| {
                            if page == s.page_index {
                                s.launch_from_grid(app_id.clone(), w, cx);
                            }
                            cx.stop_propagation();
                        }))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |s, event: &MouseDownEvent, _, cx| {
                                s.launchpad_state.menu = Some(AppMenu {
                                    id: menu_id.clone(),
                                    x: f32::from(event.position.x),
                                    y: f32::from(event.position.y),
                                });
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                );
            }
        }
        if apps.is_empty() {
            root = root.child(
                div()
                    .absolute()
                    .top(px(height * 0.37))
                    .w_full()
                    .text_center()
                    .text_size(px(20.))
                    .opacity(visibility * 0.8)
                    .child(if query.is_empty() {
                        "No applications found"
                    } else {
                        "No results"
                    }),
            );
        }
        if pages > 1 {
            let mut dots = div()
                .absolute()
                .bottom(px(
                    (snap.preferences.dock_size as f32 + 52.).max(height * 0.13)
                ))
                .left(px((width - pages as f32 * 24.) / 2.))
                .h(px(24.))
                .flex()
                .opacity(visibility);
            for page in 0..pages {
                dots = dots.child(
                    div()
                        .id(("launchpad-page", page))
                        .size(px(24.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .child(div().size(px(6.)).rounded_full().bg(rgba(
                            if self.page_index == page {
                                0xffffffff
                            } else {
                                0xffffff66
                            },
                        )))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.launcher_page(page, pages, cx);
                            s.launchpad_state.keyboard_selection = false;
                            cx.stop_propagation();
                        })),
                );
            }
            root = root.child(dots);
        }
        if self.launchpad_state.menu.is_some() {
            root = root.child(self.launcher_menu(width, height, &snap, cx));
        }
        if let Some(error) = self.message.clone().or(snap.error) {
            root = root.child(
                div()
                    .absolute()
                    .bottom(px(snap.preferences.dock_size as f32 + 130.))
                    .left(px((width - 440.).max(0.) / 2.))
                    .w(px(440.))
                    .p(px(12.))
                    .rounded(px(9.))
                    .bg(rgba(0x351d16ee))
                    .text_size(px(12.))
                    .child(error),
            );
        }
        root.into_any_element()
    }
    fn launcher_menu(
        &self,
        width: f32,
        height: f32,
        snap: &Snapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let menu = self.launchpad_state.menu.as_ref().unwrap();
        let id = menu.id.clone();
        let pin_id = id.clone();
        let icon_id = id.clone();
        let reset_id = id.clone();
        let pinned = snap.preferences.dock_pinned_apps.contains(&id);
        let p = Palette::new(snap.preferences.dark);
        let row = |key: &str, title: &str| {
            menu_item(SharedString::from(key.to_owned()), title.to_owned(), p)
        };
        if let Some(shortcut) = id.strip_prefix(crate::launchpad::SHORTCUT_PREFIX) {
            let shortcut = shortcut.to_owned();
            let launch = id.clone();
            return context_menu("launchpad-shortcut-menu", p)
                .absolute()
                .left(px(menu.x.clamp(12., (width - 240.).max(12.))))
                .top(px(menu.y.clamp(12., (height - 100.).max(12.))))
                .child(row("launcher-shortcut-open", "Open").on_click(cx.listener(
                    move |s, _, w, cx| {
                        s.launchpad_state.menu = None;
                        s.launch_from_grid(launch.clone(), w, cx);
                    },
                )))
                .child(
                    row("launcher-shortcut-configure", "Configure Shortcut…").on_click(
                        cx.listener(move |s, _, w, cx| {
                            s.launchpad_state.menu = None;
                            match open_settings(&["--bind", &shortcut], cx) {
                                Ok(_) => s.close_launchpad(w, cx),
                                Err(e) => s.message = Some(e.to_string()),
                            }
                            cx.notify();
                        }),
                    ),
                )
                .on_mouse_down_out(cx.listener(|s, _, _, cx| {
                    s.launchpad_state.menu = None;
                    cx.notify();
                }))
                .on_click(|_, _, cx| cx.stop_propagation())
                .into_any_element();
        }
        context_menu("launchpad-app-menu", p)
            .absolute()
            .left(px(menu.x.clamp(8., (width - 232.).max(8.))))
            .top(px(menu.y.clamp(8., (height - 175.).max(8.))))
            .child(
                row("launcher-open", "Open").on_click(cx.listener(move |s, _, w, cx| {
                    s.launchpad_state.menu = None;
                    s.launch_from_grid(id.clone(), w, cx);
                    cx.stop_propagation();
                })),
            )
            .child(
                row(
                    "launcher-pin",
                    if pinned {
                        "Remove from Dock"
                    } else {
                        "Keep in Dock"
                    },
                )
                .on_click(cx.listener(move |s, _, _, cx| {
                    s.launchpad_state.menu = None;
                    s.edit(cx, |p| {
                        if pinned {
                            p.dock_pinned_apps.retain(|v| v != &pin_id);
                        } else {
                            p.dock_pinned_apps.push(pin_id.clone());
                        }
                    });
                    cx.stop_propagation();
                })),
            )
            .child(menu_separator(p))
            .child(
                row("launcher-artwork", "Change icon…").on_click(cx.listener(
                    move |_, _, _, cx| {
                        let _ = open_settings(&["--icon", &icon_id], cx);
                        cx.stop_propagation();
                    },
                )),
            )
            .when(snap.preferences.app_icons.contains_key(&menu.id), |d| {
                d.child(
                    row("launcher-reset", "Restore original icon").on_click(cx.listener(
                        move |s, _, _, cx| {
                            s.launchpad_state.menu = None;
                            s.edit(cx, |p| {
                                p.app_icons.remove(&reset_id);
                            });
                            cx.stop_propagation();
                        },
                    )),
                )
            })
            .on_mouse_down_out(cx.listener(|s, _, _, cx| {
                s.launchpad_state.menu = None;
                cx.notify();
            }))
            .on_click(|_, _, cx| cx.stop_propagation())
            .into_any_element()
    }
}
