use super::*;
use crate::hyprland::window_menu::Action as WindowAction;
use swiftui_gpui::{menu, menu_item, menu_separator};
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Kind {
    Application,
    Window,
}
pub(super) struct State {
    kind: Kind,
    target: crate::hyprland::Client,
    x: f32,
    monitors: Vec<crate::hyprland::displays::Monitor>,
}
impl DesktopView {
    pub(super) fn open_window_menu(
        &mut self,
        kind: Kind,
        x: f32,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self
            .snapshot(cx)
            .clients
            .into_iter()
            .find(|c| c.focus_history_id == Some(0) && !matches!(c.class.as_str(), "org.macstill.Desktop" | "org.macstill.MissionControl"))
        else {
            return;
        };
        let old = cx.global::<OpenWindows>().apple_menu;
        let same = old
            .and_then(|h| {
                h.update(cx, |v, w, _| {
                    let same = v.window_menu.as_ref().is_some_and(|s| s.kind == kind);
                    w.remove_window();
                    same
                })
                .ok()
            })
            .unwrap_or(false);
        if same {
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
        // Capture the target before the keyboard-interactive overlay takes focus.
        let state = State {
            kind,
            target,
            x,
            monitors: crate::hyprland::displays::monitors().unwrap_or_default(),
        };
        if let Ok(handle) = open_on(Mode::AppleMenu, w.display(cx).map(|d| d.id()), cx) {
            let _ = handle.update(cx, |v, _, cx| {
                v.window_menu = Some(state);
                cx.notify();
            });
        }
    }
    pub(super) fn window_menu_view(&self, w: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let state = self.window_menu.as_ref().unwrap();
        let p = Palette::new(self.snapshot(cx).preferences.dark);
        let mut entries = Vec::new();
        entries.extend([
            ("Minimize".to_owned(), WindowAction::Minimize),
            ("Zoom / Restore".to_owned(), WindowAction::Maximize),
            ("Toggle Full Screen".to_owned(), WindowAction::Fullscreen),
            ("Close Window".to_owned(), WindowAction::Close),
        ]);
        let mut content = menu("window-menu-content", p).w(px(260.));
        for (index, (label, action)) in entries.into_iter().enumerate() {
            let target = state.target.clone();
            content = content.child(
                menu_item(eid("window-menu-action", &index.to_string()), label, p)
                    .cursor(CursorStyle::Arrow)
                    .on_click(cx.listener(move |s, _, w, cx| {
                        let backend = s.store.clone();
                        let target = target.clone();
                        let action = action.clone();
                        w.remove_window();
                        cx.defer(move |cx| {
                            let _ = backend.read(cx).backend.send(Action::WindowMenu {
                                address: target.address,
                                class: target.class,
                                action,
                            });
                        });
                    })),
            );
        }
        if state.kind == Kind::Window {
            content = content.child(menu_separator(p)).child(
                div()
                    .px(px(10.))
                    .py(px(5.))
                    .text_size(px(11.))
                    .child("Move to Display"),
            );
            if state.monitors.is_empty() {
                content = content
                    .child(menu_item("no-displays", "No displays available", p).disabled(true));
            }
            for monitor in &state.monitors {
                let target = state.target.clone();
                let name = monitor.name.clone();
                let label = if monitor.description.is_empty() {
                    name.clone()
                } else {
                    format!("{} ({name})", monitor.description)
                };
                content = content.child(
                    menu_item(eid("window-menu-monitor", &name), label, p)
                        .cursor(CursorStyle::Arrow)
                        .on_click(cx.listener(move |s, _, w, cx| {
                            let backend = s.store.clone();
                            let target = target.clone();
                            let name = name.clone();
                            w.remove_window();
                            cx.defer(move |cx| {
                                let _ = backend.read(cx).backend.send(Action::WindowMenu {
                                    address: target.address,
                                    class: target.class,
                                    action: WindowAction::MoveTo(name),
                                });
                            });
                        })),
                );
            }
        }
        let width: f32 = w.bounds().size.width.into();
        div()
            .size_full()
            .relative()
            .cursor(CursorStyle::Arrow)
            .font_family(cx.global::<Fonts>().text.clone())
            .track_focus(&self.apple_menu.focus)
            .on_key_down(|e, w, _| {
                if e.keystroke.key == "escape" {
                    w.remove_window();
                }
            })
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
                    .top(px(32.))
                    .left(px(state.x.clamp(0., (width - 268.).max(0.))))
                    .child(content),
            )
            .into_any_element()
    }
}
