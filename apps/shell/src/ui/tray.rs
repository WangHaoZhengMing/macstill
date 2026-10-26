use super::*;
use crate::tray::{Command, Entry};
use swiftui_gpui::{menu, menu_item, menu_separator};
impl DesktopView {
    pub(super) fn tray_icons(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = self.store.read(cx).tray.snapshot();
        let mut row = div().h(px(30.)).flex().items_center();
        for item in state.items {
            let key = item.key.clone();
            let right = key.clone();
            let middle = key.clone();
            let scroll = key.clone();
            let is_menu = item.menu;
            let icon = if let Some(bytes) = item.icon {
                let format = if bytes.starts_with(b"\x89PNG") {
                    ImageFormat::Png
                } else {
                    ImageFormat::Svg
                };
                img(std::sync::Arc::new(Image::from_bytes(format, bytes)))
                    .size(px(18.))
                    .object_fit(ObjectFit::Contain)
                    .into_any_element()
            } else {
                div()
                    .text_size(px(13.))
                    .child(item.title.chars().next().unwrap_or('●').to_string())
                    .into_any_element()
            };
            row = row.child(
                div()
                    .id(eid("tray", &key))
                    .w(px(34.)).h(px(30.)).flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor(CursorStyle::Arrow)
                    .child(icon)
                    .on_click(cx.listener(move |s, e: &ClickEvent, w, cx| {
                        if is_menu {
                            s.open_tray_menu(&key, f32::from(e.position().x), w, cx)
                        } else {
                            s.store.read(cx).tray.send(Command::Activate(key.clone()));
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |s, e: &MouseDownEvent, w, cx| {
                            cx.stop_propagation();
                            s.open_tray_menu(&right, f32::from(e.position.x), w, cx)
                        }),
                    )
                    .on_scroll_wheel(cx.listener(move |s, e: &ScrollWheelEvent, _, cx| {
                        let delta = e.delta.pixel_delta(px(20.));
                        let x: f32 = delta.x.into();
                        let y: f32 = delta.y.into();
                        let (amount, orientation) = if x.abs() > y.abs() {
                            (x, "horizontal")
                        } else {
                            (y, "vertical")
                        };
                        s.store.read(cx).tray.send(Command::Scroll(
                            scroll.clone(),
                            amount as i32,
                            orientation.into(),
                        ));
                    }))
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(move |s, _, _, cx| {
                            s.store
                                .read(cx)
                                .tray
                                .send(Command::Secondary(middle.clone()))
                        }),
                    ),
            );
        }
        row.into_any_element()
    }
    fn open_tray_menu(&self, key: &str, x: f32, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .store
            .read(cx)
            .tray
            .snapshot()
            .items
            .iter()
            .any(|i| i.key == key && !i.exported_menu)
        {
            self.store.read(cx).tray.send(Command::Menu(key.into()));
            return;
        }
        cx.global_mut::<OpenWindows>().tray_x = x;
        self.store.read(cx).tray.send(Command::Close);
        self.store.read(cx).tray.send(Command::Menu(key.into()));
        let _ = open_on(Mode::Tray, window.display(cx).map(|d| d.id()), cx);
    }
    fn tray_entries(
        &self,
        entries: &[Entry],
        key: &str,
        depth: usize,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = div().flex().flex_col();
        for entry in entries {
            if entry.separator {
                list = list.child(menu_separator(p));
                continue;
            }
            let k = key.to_string();
            let id = entry.id;
            let submenu = entry.submenu;
            list = list.child(
                menu_item(
                    eid("tray-menu", &format!("{depth}:{id}")),
                    format!(
                        "{}{}{}",
                        if entry.checked == Some(true) {
                            "✓  "
                        } else {
                            ""
                        },
                        entry.label,
                        if submenu { "   ›" } else { "" }
                    ),
                    p,
                )
                .pl(px(9. + depth as f32 * 12.))
                .disabled(!entry.enabled)
                .on_click(cx.listener(move |s, _, w, cx| {
                    if submenu {
                        if s.tray_expanded.contains(&id) {
                            s.tray_expanded.retain(|v| *v != id);
                        } else {
                            s.tray_expanded.push(id);
                            s.store.read(cx).tray.send(Command::Expand(k.clone(), id));
                        }
                        cx.notify();
                        return;
                    }
                    s.store.read(cx).tray.send(Command::Event(k.clone(), id));
                    s.store.read(cx).tray.send(Command::Close);
                    w.remove_window();
                })),
            );
            if entry.enabled && self.tray_expanded.contains(&entry.id) {
                list = list.child(self.tray_entries(&entry.children, key, depth + 1, p, cx));
            }
        }
        list.into_any_element()
    }
    pub(super) fn tray_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if let Some(bounds) = super::displays::window_bounds(window, cx) {
            let wanted = bounds.size;
            if window.bounds().size != wanted {
                window.resize(wanted);
            }
        }
        let state = self.store.read(cx).tray.snapshot();
        let p = Palette::new(self.snapshot(cx).preferences.dark);
        let mut panel = menu("tray-menu-panel", p);
        if let Some(key) = state.selected {
            panel = panel.child(self.tray_entries(&state.entries, &key, 0, p, cx));
        } else {
            panel = panel.child(div().p(px(9.)).child("Loading menu…"));
        }
        let x = cx.global::<OpenWindows>().tray_x;
        let width: f32 = window.viewport_size().width.into();
        let height: f32 = window.viewport_size().height.into();
        div()
            .relative()
            .size_full()
            .font_family(cx.global::<Fonts>().text.clone())
            .track_focus(&self.apple_menu.focus)
            .on_key_down(|e, w, _| {
                if e.keystroke.key == "escape" {
                    w.remove_window();
                }
            })
            .child(
                div()
                    .id("tray-dismiss")
                    .absolute()
                    .inset_0()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|s, _, w, cx| {
                            s.store.read(cx).tray.send(Command::Close);
                            w.remove_window();
                        }),
                    )
                    .on_mouse_down(MouseButton::Right, |_, w, _| w.remove_window()),
            )
            .child(
                div()
                    .absolute()
                    .top(px(32.))
                    .left(px((x - 112.).clamp(8., (width - 232.).max(8.))))
                    .w(px(224.))
                    .child(panel.max_h(px((height - 40.).max(40.))).overflow_y_scroll()),
            )
            .into_any_element()
    }
}
