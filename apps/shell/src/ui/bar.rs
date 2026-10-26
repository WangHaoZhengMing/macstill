use super::*;
impl DesktopView {
    // Reuse SwiftDE's original PNG alpha masks and tint them like its ColorOverlay.
    pub(super) fn bar_icon(name: &str, color: u32) -> AnyElement {
        let asset: Option<(&[u8], f32, f32)> = match name {
            "apple" => Some((
                include_bytes!("../../../../assets/icons/menubar/apple_2x.png"),
                13.,
                16.,
            )),
            "search" => Some((
                include_bytes!("../../../../assets/icons/menubar/Spotlight_native.png"),
                14.,
                14.,
            )),
            "control-center" => Some((
                include_bytes!("../../../../assets/icons/menubar/ControlCenter_native.png"),
                14.,
                14.,
            )),
            _ => None,
        };
        if let Some((bytes, width, height)) = asset {
            let mut pixels = image::load_from_memory(bytes)
                .expect("bundled menu bar PNG must be valid")
                .into_rgba8();
            for pixel in pixels.pixels_mut() {
                pixel.0[..3].copy_from_slice(&[
                    (color >> 16) as u8,
                    (color >> 8) as u8,
                    color as u8,
                ]);
            }
            let mut png = std::io::Cursor::new(Vec::new());
            pixels
                .write_to(&mut png, image::ImageFormat::Png)
                .expect("menu bar PNG encoding must succeed");
            return img(std::sync::Arc::new(Image::from_bytes(
                ImageFormat::Png,
                png.into_inner(),
            )))
            .w(px(width))
            .h(px(height))
            .object_fit(ObjectFit::Contain)
            .into_any_element();
        }
        unreachable!("unknown menu bar PNG icon: {name}")
    }
    pub(super) fn battery_icon(level: u8, charging: bool, color: u32) -> AnyElement {
        let fill = if charging {
            0x34c759
        } else if level <= 10 {
            0xff453a
        } else {
            color
        };
        let width = 20.0 * f32::from(level.min(100)) / 100.0;
        let bolt = if charging {
            r##"<path d="M15 2.5 9 10h4l-1 5.5L19 8h-4l1-5.5Z" fill="#ffffff" stroke="#176b32" stroke-width="0.65" stroke-linejoin="round"/>"##
        } else {
            ""
        };
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="29" height="18" viewBox="0 0 29 18">
<rect x="1" y="3" width="24" height="12" rx="3" fill="none" stroke="#{color:06x}" stroke-width="1.2" stroke-opacity="0.65"/>
<path d="M27 6v6c1.2-.4 1.8-1.4 1.8-3S28.2 6.4 27 6" fill="#{color:06x}" fill-opacity="0.65"/>
<rect x="3" y="5" width="{width}" height="8" rx="1.4" fill="#{fill:06x}"/>
{bolt}</svg>"##
        );
        img(std::sync::Arc::new(Image::from_bytes(
            ImageFormat::Svg,
            svg.into_bytes(),
        )))
        .w(px(29.))
        .h(px(18.))
        .into_any_element()
    }
    pub(super) fn bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let snap = self.snapshot(cx);
        let p = Palette::new(snap.preferences.dark);
        div()
            .font_family(cx.global::<Fonts>().text.clone())
            .w_full()
            .h(px(30.))
            .cursor(CursorStyle::Arrow)
            .bg(p.material(swiftui_gpui::theme::Material::Chrome))
            .text_color(rgb(p.text))
            .flex()
            .items_center()
            .gap(px(0.))
            .text_size(px(13.))
            .child(
                div()
                    .id("apple-menu")
                    .w(px(49.))
                    .h(px(30.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor(CursorStyle::Arrow)
                    .child(Self::bar_icon("apple", p.text))
                    .on_click(|_, w, cx| {
                        let application_menu = cx.global::<OpenWindows>().apple_menu
                            .and_then(|h| h.update(cx, |v, _, _| v.window_menu.is_some()).ok())
                            .unwrap_or(false);
                        toggle_popover(Mode::AppleMenu, w, cx);
                        if application_menu {
                            let _ = open_on(Mode::AppleMenu, w.display(cx).map(|d| d.id()), cx);
                        }
                    }),
            )
            .child(
                div()
                    .id("application-menu-open")
                    .h(px(30.)).px(px(8.)).flex().items_center()
                    .cursor(CursorStyle::Arrow)
                    .on_click(cx.listener(|s, e: &ClickEvent, w, cx| s.open_window_menu(window_menu::Kind::Application, f32::from(e.position().x), w, cx)))
                    .min_w_0()
                    .text_ellipsis()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(if snap.active_title.is_empty() {
                        "Macstill".into()
                    } else {
                        snap.active_title
                    }),
            )
            .child(self.application_menus(cx))
            .child(self.tray_icons(cx))
            .when_some(self.control_center_menu_bar(cx), |bar, icons| {
                bar.child(icons)
            })
            .child(
                div()
                    .id("search-open")
                    .h(px(30.))
                    .px(px(9.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor(CursorStyle::Arrow)
                    .child(Self::bar_icon("search", p.text))
                    .on_click(|_, w, cx| {
                        let _ = open_on(Mode::Launchpad, w.display(cx).map(|d| d.id()), cx);
                    }),
            )
            .when_some(
                snap.battery
                    .filter(|_| snap.preferences.control_center.battery_in_menu_bar),
                |d, b| {
                    d.child(
                        gpui_kit::base::Button::new("menu-bar-battery")
                            .accessibility_label("Battery settings")
                            .h(px(30.))
                            .px(px(9.))
                            .flex_none()
                            .cursor(CursorStyle::Arrow)
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .on_click(|_, _, cx| {
                                let _ = open_settings(&["--page", "battery"], cx);
                            })
                            .child(Self::battery_icon(b, snap.battery_charging, p.text))
                            .when(snap.preferences.control_center.battery_percentage, |d| {
                                d.child(format!("{b}%"))
                            }),
                    )
                },
            )
            .child(
                div()
                    .id("control-center-open")
                    .h(px(30.))
                    .px(px(9.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor(CursorStyle::Arrow)
                    .child(Self::bar_icon("control-center", p.text))
                    .on_click(|_, w, cx| toggle_popover(Mode::ControlCenter, w, cx)),
            )
            .child(
                div()
                    .id("notification-center-open")
                    .h(px(30.))
                    .px(px(9.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor(CursorStyle::Arrow)
                    .child(chrono::Local::now().format("%a %e %b  %H:%M").to_string())
                    .on_click(|_, w, cx| toggle_popover(Mode::NotificationCenter, w, cx)),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        Backend, DesktopView, Mode, OpenWindows, SharedStore, Store, control_center,
    };
    use gpui_test::{self as gpui, ElementId, Task, TestAppContext, prelude::*};
    use macstill_config::{ControlCenterModule as Module, MenuBarVisibility as Visibility};
    #[gpui::test]
    fn menu_bar_module_and_battery_visibility_use_saved_preferences(cx: &mut TestAppContext) {
        let store = cx.update(|cx| {
            macstill_ui::init(cx);
            let backend = Backend::fixture();
            let mut snapshot = backend.snapshot();
            snapshot.battery = Some(73);
            snapshot
                .preferences
                .control_center
                .menu_bar
                .insert(Module::Wifi, Visibility::Always);
            snapshot
                .preferences
                .control_center
                .menu_bar
                .insert(Module::Focus, Visibility::WhenActive);
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
                menu_snapshot: Default::default(),
                _refresh: Task::ready(()),
                _notification_refresh: Task::ready(()),
                clock_minute: String::new(),
                _control: Task::ready(()),
            });
            cx.set_global(SharedStore(store.clone()));
            cx.set_global(OpenWindows::default());
            store
        });
        let (_, cx) = cx.add_window_view(|window, cx| DesktopView::new(Mode::Bar, window, cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let present = |cx: &mut gpui_test::VisualTestContext, id: &str| {
            cx.update(|window, _| {
                gpui_base::test_support::find(window, &[], &ElementId::from(id.to_string()))
                    .is_some()
            })
        };
        assert!(present(cx, "menu-bar-module:wifi"));
        assert!(!present(cx, "menu-bar-module:focus"));
        assert!(present(cx, "menu-bar-battery"));
        store.update(cx, |store, cx| {
            store
                .snapshot
                .preferences
                .control_center
                .menu_bar
                .insert(Module::Wifi, Visibility::Never);
            store
                .snapshot
                .preferences
                .control_center
                .battery_in_menu_bar = false;
            store.notification_snapshot.focus = true;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(!present(cx, "menu-bar-module:wifi"));
        assert!(present(cx, "menu-bar-module:focus"));
        assert!(!present(cx, "menu-bar-battery"));
    }
}
