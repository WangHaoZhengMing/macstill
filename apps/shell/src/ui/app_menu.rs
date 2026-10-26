use super::*;
use macstill_menu::{
    Endpoint, Entry,
    host::{Session, Window as MenuWindow},
};
use swiftui_gpui::{menu, menu_item, menu_separator};

pub(super) fn windows() -> anyhow::Result<Vec<MenuWindow>> {
    let records: Vec<serde_json::Value> =
        serde_json::from_str(&crate::hyprland::request("macstill-stage menus")?)?;
    Ok(records
        .into_iter()
        .filter_map(|r| {
            Some(MenuWindow {
                address: r["address"].as_str()?.into(),
                xid: r["xid"].as_u64().and_then(|n| u32::try_from(n).ok()),
                endpoint: r.get("endpoint").and_then(|e| {
                    Some(Endpoint {
                        owner: e["owner"].as_str()?.into(),
                        path: e["path"].as_str()?.into(),
                    })
                }),
            })
        })
        .collect())
}
pub(super) struct State {
    session: Session,
    path: Vec<i32>,
    selected: Option<i32>,
    x: f32,
}
impl DesktopView {
    pub(super) fn application_menus(&self, cx: &mut Context<Self>) -> AnyElement {
        let snapshot = self.store.read(cx).menu_snapshot.clone();
        let mut row = div()
            .id("application-menus")
            .min_w_0()
            .flex_1()
            .h(px(30.))
            .flex()
            .items_center()
            .overflow_x_scroll();
        if let (Some(session), Some(root)) = (snapshot.session, snapshot.root) {
            for entry in root.children.into_iter().filter(|e| !e.separator) {
                let id = entry.id;
                let submenu = entry.submenu;
                let revision = snapshot.revision;
                let selected = self
                    .app_menu
                    .as_ref()
                    .is_some_and(|s| s.path.first() == Some(&id));
                let hover_session = session.clone();
                let click_session = session.clone();
                row = row.child(
                    menu_item(
                        eid("application-menu", &id.to_string()),
                        entry.label,
                        Palette::new(self.snapshot(cx).preferences.dark),
                    )
                    .w_auto()
                    .h(px(30.))
                    .px(px(9.))
                    .rounded(px(0.))
                    .flex_none()
                    .disabled(!entry.enabled)
                    .when(selected, |d| d.bg(rgba(0x80808030)))
                    .on_click(cx.listener(move |s, e: &ClickEvent, w, cx| {
                        if !submenu {
                            s.store
                                .read(cx)
                                .menus
                                .click(click_session.clone(), revision, id);
                            if s.mode == Mode::AppleMenu {
                                w.remove_window();
                            }
                            return;
                        }
                        if s.app_menu
                            .as_ref()
                            .is_some_and(|s| s.path.first() == Some(&id))
                        {
                            w.remove_window();
                            return;
                        }
                        s.open_app_menu(
                            click_session.clone(),
                            id,
                            f32::from(e.position().x),
                            w,
                            cx,
                        );
                    }))
                    .on_mouse_move(cx.listener(
                        move |s, e: &MouseMoveEvent, w, cx| {
                            if submenu
                                && s.app_menu
                                    .as_ref()
                                    .is_some_and(|s| s.path.first() != Some(&id))
                            {
                                s.open_app_menu(
                                    hover_session.clone(),
                                    id,
                                    f32::from(e.position.x),
                                    w,
                                    cx,
                                );
                            }
                        },
                    )),
                );
            }
        } else {
            row = row.child(
                menu_item(
                    "fallback-window-menu",
                    "Window",
                    Palette::new(self.snapshot(cx).preferences.dark),
                )
                .w_auto()
                .h(px(30.))
                .flex_none()
                .on_click(cx.listener(|s, e: &ClickEvent, w, cx| {
                    s.open_window_menu(window_menu::Kind::Window, f32::from(e.position().x), w, cx)
                })),
            );
        }
        row.into_any_element()
    }
    fn open_app_menu(
        &mut self,
        session: Session,
        id: i32,
        x: f32,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.store.read(cx).menu_snapshot.session.as_ref() != Some(&session) {
            return;
        }
        self.close_app_menu(cx);
        self.store.read(cx).menus.show(session.clone(), id);
        let state = State {
            session,
            path: vec![id],
            selected: None,
            x,
        };
        if self.mode == Mode::AppleMenu {
            self.window_menu = None;
            self.app_menu = Some(state);
            cx.notify();
            return;
        }
        for handle in [
            cx.global::<OpenWindows>().control_center,
            cx.global::<OpenWindows>().notification_center,
            cx.global::<OpenWindows>().tray,
        ]
        .into_iter()
        .flatten()
        {
            let _ = handle.update(cx, |_, w, _| w.remove_window());
        }
        if let Ok(handle) = open_on(Mode::AppleMenu, w.display(cx).map(|d| d.id()), cx) {
            let _ = handle.update(cx, |s, _, cx| {
                s.window_menu = None;
                s.app_menu = Some(state);
                cx.notify();
            });
        }
    }
    pub(super) fn close_app_menu(&self, cx: &App) {
        if let Some(state) = &self.app_menu {
            for id in state.path.iter().rev() {
                self.store.read(cx).menus.hide(state.session.clone(), *id);
            }
        }
    }
    fn app_menu_back(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.app_menu.as_mut() {
            if state.path.len() > 1 {
                if let Some(id) = state.path.pop() {
                    self.store.read(cx).menus.hide(state.session.clone(), id);
                }
                state.selected = None;
            }
        }
        cx.notify();
    }
    fn app_menu_activate(&mut self, id: i32, w: &mut Window, cx: &mut Context<Self>) {
        let snapshot = self.store.read(cx).menu_snapshot.clone();
        let Some(state) = self.app_menu.as_mut() else {
            return;
        };
        if snapshot.session.as_ref() != Some(&state.session) {
            w.remove_window();
            return;
        }
        let Some(entry) = snapshot.root.as_ref().and_then(|r| r.find(id)) else {
            return;
        };
        if !entry.enabled || entry.separator {
            return;
        }
        if entry.submenu {
            state.path.push(id);
            state.selected = None;
            self.store.read(cx).menus.show(state.session.clone(), id);
            cx.notify();
        } else {
            self.store
                .read(cx)
                .menus
                .click(state.session.clone(), snapshot.revision, id);
            w.remove_window();
        }
    }
    fn app_menu_key(&mut self, key: &str, w: &mut Window, cx: &mut Context<Self>) {
        if key == "escape" {
            w.remove_window();
            return;
        }
        let snapshot = self.store.read(cx).menu_snapshot.clone();
        let Some(state) = self.app_menu.as_mut() else {
            return;
        };
        let Some(root) = snapshot.root else {
            return;
        };
        let Some(current) = state.path.last().and_then(|id| root.find(*id)) else {
            return;
        };
        let available: Vec<_> = current
            .children
            .iter()
            .filter(|e| e.enabled && !e.separator)
            .map(|e| e.id)
            .collect();
        match key {
            "up" | "down" if !available.is_empty() => {
                let index = state
                    .selected
                    .and_then(|id| available.iter().position(|i| *i == id));
                let next = match (key, index) {
                    ("up", Some(i)) => (i + available.len() - 1) % available.len(),
                    ("up", None) => available.len() - 1,
                    (_, Some(i)) => (i + 1) % available.len(),
                    _ => 0,
                };
                state.selected = Some(available[next]);
            }
            "enter" | "space" | "right"
                if state.selected.is_some_and(|id| {
                    key != "right" || root.find(id).is_some_and(|e| e.submenu)
                }) =>
            {
                let id = state.selected.unwrap();
                self.app_menu_activate(id, w, cx);
                return;
            }
            "left" if state.path.len() > 1 => {
                self.app_menu_back(cx);
                return;
            }
            "left" | "right" => {
                let tops: Vec<&Entry> = root
                    .children
                    .iter()
                    .filter(|e| e.enabled && !e.separator)
                    .collect();
                if let Some(index) = tops.iter().position(|e| Some(&e.id) == state.path.first()) {
                    let next = if key == "left" {
                        (index + tops.len() - 1) % tops.len()
                    } else {
                        (index + 1) % tops.len()
                    };
                    for id in state.path.iter().rev() {
                        self.store.read(cx).menus.hide(state.session.clone(), *id);
                    }
                    state.path = vec![tops[next].id];
                    state.selected = None;
                    self.store
                        .read(cx)
                        .menus
                        .show(state.session.clone(), tops[next].id);
                }
            }
            _ => {}
        }
        cx.notify();
    }
    pub(super) fn app_menu_view(&mut self, w: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let snapshot = self.store.read(cx).menu_snapshot.clone();
        let state = self.app_menu.as_ref().unwrap();
        if snapshot.session.as_ref() != Some(&state.session) {
            w.remove_window();
            return div().into_any_element();
        }
        let p = Palette::new(self.snapshot(cx).preferences.dark);
        let mut panel = menu("app-menu-panel", p).w(px(280.));
        if state.path.len() > 1 {
            panel = panel
                .child(
                    menu_item("app-menu-back", "‹ Back", p).on_click(cx.listener(|s, _, _, cx| {
                        s.app_menu_back(cx);
                    })),
                )
                .child(menu_separator(p));
        }
        if let Some(entry) = snapshot
            .root
            .as_ref()
            .and_then(|r| state.path.last().and_then(|id| r.find(*id)))
        {
            for item in &entry.children {
                if item.separator {
                    panel = panel.child(menu_separator(p));
                    continue;
                }
                let id = item.id;
                let check = if item.checked == Some(true) {
                    if item.radio { "● " } else { "✓ " }
                } else {
                    ""
                };
                panel = panel.child(
                    menu_item(
                        eid("app-menu-item", &id.to_string()),
                        format!("{check}{}", item.label),
                        p,
                    )
                    .disabled(!item.enabled || !entry.enabled)
                    .when(state.selected == Some(id), |d| {
                        d.bg(rgb(p.accent)).text_color(rgb(p.accent_text))
                    })
                    .child(div().text_size(px(11.)).child(item.shortcut_label()))
                    .when(item.submenu, |d| d.child("›"))
                    .on_click(cx.listener(move |s, _, w, cx| s.app_menu_activate(id, w, cx))),
                );
            }
            if entry.children.is_empty() {
                panel = panel
                    .child(menu_item("app-menu-empty", "No actions available", p).disabled(true));
            }
        } else {
            panel = panel
                .child(menu_item("app-menu-unavailable", "Menu unavailable", p).disabled(true));
        }
        let width: f32 = w.viewport_size().width.into();
        let height: f32 = w.viewport_size().height.into();
        let x = state.x.clamp(0., (width - 288.).max(0.));
        div()
            .size_full()
            .relative()
            .font_family(cx.global::<Fonts>().text.clone())
            .text_size(px(13.))
            .text_color(rgb(p.text))
            .track_focus(&self.apple_menu.focus)
            .on_key_down(cx.listener(|s, e: &KeyDownEvent, w, cx| {
                s.app_menu_key(&e.keystroke.key, w, cx);
                cx.stop_propagation();
            }))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .on_mouse_down(MouseButton::Left, |_, w, _| w.remove_window())
                    .on_mouse_down(MouseButton::Right, |_, w, _| w.remove_window()),
            )
            .child(
                div()
                    .absolute()
                    .top(px(0.))
                    .left(px(0.))
                    .w_full()
                    .h(px(30.))
                    .flex()
                    .child(div().w(px(49.)).flex_none())
                    .child(
                        div()
                            .px(px(8.))
                            .h(px(30.))
                            .flex()
                            .items_center()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.snapshot(cx).active_title),
                    )
                    .child(self.application_menus(cx)),
            )
            .child(
                div()
                    .absolute()
                    .top(px(32.))
                    .left(px(x))
                    .child(panel.max_h(px((height - 40.).max(40.))).overflow_y_scroll()),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Backend, DesktopView, Endpoint, Entry, Mode, OpenWindows, Session, SharedStore, State,
        Store, control_center,
    };
    use gpui_test::{self as gpui, Task, TestAppContext, prelude::*};
    #[gpui::test]
    fn keyboard_skips_disabled_items_and_traverses_submenus(cx: &mut TestAppContext) {
        let session = Session {
            window: "window-a".into(),
            endpoint: Endpoint {
                owner: ":1.42".into(),
                path: "/Menu".into(),
            },
            generation: 1,
        };
        cx.update(|cx| {
            macstill_ui::init(cx);
            let backend = Backend::fixture();
            let snapshot = backend.snapshot();
            let (notifications, _) = crate::notifications::Notifications::fixture();
            let notification_snapshot = notifications.snapshot();
            let control_status = cx.new(|_| control_center::StatusCache::fixture());
            let store = cx.new(|_| Store {
                backend,
                snapshot,
                notifications,
                notification_snapshot,
                control_status,
                tray: crate::tray::Tray::fixture(),
                menus: macstill_menu::host::Host::disabled(),
                menu_snapshot: macstill_menu::host::Snapshot {
                    session: Some(session.clone()),
                    root: Some(Entry::menu(
                        0,
                        "",
                        vec![Entry::menu(
                            100,
                            "File",
                            vec![
                                Entry {
                                    enabled: false,
                                    ..Entry::item(1, "Disabled")
                                },
                                Entry::menu(2, "Nested", vec![Entry::item(3, "Action")]),
                                Entry::item(4, "Other"),
                            ],
                        )],
                    )),
                    ..Default::default()
                },
                _refresh: Task::ready(()),
                _notification_refresh: Task::ready(()),
                _control: Task::ready(()),
                clock_minute: String::new(),
            });
            cx.set_global(SharedStore(store));
            cx.set_global(OpenWindows::default());
        });
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = DesktopView::new(Mode::AppleMenu, window, cx);
            view.app_menu = Some(State {
                session,
                path: vec![100],
                selected: None,
                x: 100.,
            });
            view
        });
        cx.update(|w, cx| {
            view.update(cx, |v, cx| {
                v.app_menu_key("down", w, cx);
                assert_eq!(v.app_menu.as_ref().unwrap().selected, Some(2));
                v.app_menu_key("right", w, cx);
                assert_eq!(v.app_menu.as_ref().unwrap().path, [100, 2]);
                v.app_menu_key("left", w, cx);
                assert_eq!(v.app_menu.as_ref().unwrap().path, [100]);
                v.app_menu_key("up", w, cx);
                assert_eq!(v.app_menu.as_ref().unwrap().selected, Some(4));
            })
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
    }
}
