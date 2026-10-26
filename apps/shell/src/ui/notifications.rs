use super::*;
use crate::notifications::{Command, Notification};

pub(super) fn sync_banner(output: Option<String>, cx: &mut App) {
    let store = cx.global::<SharedStore>().0.clone();
    let has_banners = !store.read(cx).notification_snapshot.banners().is_empty();
    let existing = cx.global::<OpenWindows>().notification_banner;
    let target = displays::output_named(output.as_deref(), cx);
    if !has_banners {
        if let Some(handle) = existing {
            let _ = handle.update(cx, |_, w, _| w.remove_window());
        }
        cx.global_mut::<OpenWindows>().notification_banner = None;
    } else if existing.is_none_or(|handle| {
        handle
            .update(cx, |view, window, cx| {
                if view.target_display == target {
                    cx.notify();
                    true
                } else {
                    window.remove_window();
                    false
                }
            })
            .ok()
            != Some(true)
    }) && let Err(error) = open_on(Mode::NotificationBanner, target, cx)
    {
        eprintln!("Cannot open notification banner: {error:#}");
    }
}
pub(super) fn banner_size(viewport: Size<Pixels>) -> Size<Pixels> {
    size(
        px(380.).min((viewport.width - px(32.)).max(px(1.))),
        px(540.).min((viewport.height - px(64.)).max(px(1.))),
    )
}
fn age(time: std::time::SystemTime) -> String {
    let seconds = time.elapsed().unwrap_or_default().as_secs();
    match seconds {
        0..60 => "now".into(),
        60..3600 => format!("{}m ago", seconds / 60),
        3600..86400 => format!("{}h ago", seconds / 3600),
        _ => chrono::DateTime::<chrono::Local>::from(time)
            .format("%b %-d")
            .to_string(),
    }
}
fn glass(p: Palette) -> Hsla {
    p.material(swiftui_gpui::theme::Material::Panel)
}
impl DesktopView {
    fn notification_command(&self, command: Command, cx: &App) {
        self.store.read(cx).notifications.send(command);
    }
    fn notification_card(
        &self,
        n: Notification,
        _banner: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = n.id;
        let expanded = self.notification_expanded.contains(&id);
        let show_actions = expanded || (n.active && n.resident && !n.actions.is_empty());
        let default_action = n.active && n.actions.iter().any(|a| a.0 == "default");
        let icon = if let Some(bytes) = n.icon {
            img(std::sync::Arc::new(Image::from_bytes(
                ImageFormat::Png,
                bytes.as_ref().clone(),
            )))
            .size(px(16.))
            .object_fit(ObjectFit::Contain)
            .into_any_element()
        } else {
            div()
                .size(px(16.))
                .rounded(px(4.))
                .bg(rgb(0x6284d8))
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(0xffffff))
                .text_size(px(10.))
                .child(
                    n.app
                        .chars()
                        .next()
                        .unwrap_or('N')
                        .to_uppercase()
                        .to_string(),
                )
                .into_any_element()
        };
        let mut actions = div().flex().flex_wrap().gap(px(5.)).mt(px(8.));
        if n.active && show_actions {
            for (index, (key, label)) in n.actions.into_iter().enumerate() {
                actions = actions.child(
                    button(
                        eid("notification-action", &format!("{id}:{index}")),
                        if label.is_empty() {
                            "Open".into()
                        } else {
                            label
                        },
                        p,
                    )
                    .h(px(25.))
                    .text_size(px(11.))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        cx.stop_propagation();
                        s.notification_command(Command::Invoke(id, key.clone()), cx);
                    })),
                );
            }
        }
        div()
            .id(eid("notification", &id.to_string()))
            .group("notification-card")
            .w_full()
            .flex_shrink_0()
            .rounded(px(13.))
            .bg(glass(p))
            .border_1()
            .border_color(if p.dark {
                rgba(0xffffff20)
            } else {
                rgba(0xffffff70)
            })
            .shadow_md()
            .px(px(13.))
            .py(px(10.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .when(default_action, |d| {
                d.cursor_pointer().on_click(cx.listener(move |s, _, _, cx| {
                    s.notification_command(Command::Invoke(id, "default".into()), cx)
                }))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(icon)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .text_size(px(10.))
                            .text_color(rgb(p.muted))
                            .child(n.app.to_uppercase()),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(p.muted))
                            .child(age(n.received)),
                    )
                    .child(
                        gpui_kit::base::Button::new(eid("notification-expand", &id.to_string()))
                            .accessibility_label("Show notification details")
                            .size(px(18.))
                            .rounded_full()
                            .text_size(px(12.))
                            .text_color(rgb(p.muted))
                            .child(if expanded { "⌃" } else { "⌄" })
                            .on_click(cx.listener(move |s, _, _, cx| {
                                cx.stop_propagation();
                                if s.notification_expanded.contains(&id) {
                                    s.notification_expanded.retain(|n| *n != id);
                                } else {
                                    s.notification_expanded.push(id);
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        gpui_kit::base::Button::new(eid("notification-dismiss", &id.to_string()))
                            .accessibility_label("Clear notification")
                            .size(px(18.))
                            .rounded_full()
                            .cursor_pointer()
                            .text_size(px(14.))
                            .text_color(rgb(p.muted))
                            .child("×")
                            .on_click(cx.listener(move |s, _, _, cx| {
                                cx.stop_propagation();
                                s.notification_command(Command::Dismiss(id), cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .when(!expanded, |d| d.line_clamp(1))
                    .child(n.summary),
            )
            .when(!n.body.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(px(12.))
                        .line_height(px(16.))
                        .when(!expanded, |d| d.line_clamp(2))
                        .child(n.body),
                )
            })
            .when(show_actions, |d| d.child(actions))
            .into_any_element()
    }
    pub(super) fn notification_banners(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(bounds) = window
            .display(cx)
            .and_then(|display| displays::bounds(display.id(), cx))
        {
            let size = banner_size(bounds.size);
            if window.bounds().size != size {
                window.resize(size);
            }
        }
        let p = Palette::new(self.snapshot(cx).preferences.dark);
        let banners = self.store.read(cx).notification_snapshot.banners();
        div()
            .size_full()
            .font_family(cx.global::<Fonts>().text.clone())
            .text_color(rgb(p.text))
            .child(
                div()
                    .on_children_prepainted(|bounds, w, _| {
                        let viewport = Bounds::new(point(px(0.), px(0.)), w.bounds().size);
                        let regions: Vec<_> = bounds
                            .into_iter()
                            .filter(|b| b.intersects(&viewport))
                            .map(|b| b.intersect(&viewport))
                            .collect();
                        w.set_input_region(Some(&regions));
                    })
                    .id("notification-banners")
                    .w_full()
                    .max_h_full()
                    .overflow_y_scroll()
                    .p(px(4.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .children(
                        banners
                            .into_iter()
                            .map(|n| self.notification_card(n, true, p, cx))
                            .collect::<Vec<_>>(),
                    ),
            )
            .into_any_element()
    }
    pub(super) fn notification_center_view(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(bounds) = window
            .display(cx)
            .and_then(|display| displays::bounds(display.id(), cx))
            && window.bounds().size != bounds.size
        {
            window.resize(bounds.size);
        }
        let p = Palette::new(self.snapshot(cx).preferences.dark);
        let state = self.store.read(cx).notification_snapshot.clone();
        let width = window.bounds().size.width;
        let height = window.bounds().size.height;
        let has_notifications = !state.notifications.is_empty();
        let mut list = div()
            .id("notification-history")
            .w_full()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(9.))
            .p(px(4.));
        if let Some(diagnostic) = state.diagnostic {
            let details = self.notification_expanded.contains(&0);
            list = list.child(
                div()
                    .id("notification-service-status")
                    .rounded(px(13.))
                    .bg(glass(p))
                    .px(px(13.))
                    .py(px(10.))
                    .text_size(px(12.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child("Notifications unavailable")
                            .child(
                                button(
                                    "notification-service-details",
                                    if details { "Hide" } else { "Details" },
                                    p,
                                )
                                .text_size(px(10.))
                                .h(px(22.))
                                .on_click(cx.listener(
                                    |s, _, _, cx| {
                                        if s.notification_expanded.contains(&0) {
                                            s.notification_expanded.retain(|n| *n != 0);
                                        } else {
                                            s.notification_expanded.push(0);
                                        }
                                        cx.notify();
                                    },
                                )),
                            ),
                    )
                    .when(details, |d| {
                        d.child(
                            div()
                                .mt(px(8.))
                                .text_size(px(11.))
                                .line_height(px(16.))
                                .text_color(rgb(p.muted))
                                .child(diagnostic),
                        )
                    }),
            );
        }
        if !has_notifications {
            list = list.child(
                div()
                    .mt(px(40.))
                    .self_center()
                    .rounded_full()
                    .bg(glass(p))
                    .px(px(14.))
                    .py(px(6.))
                    .text_center()
                    .text_size(px(13.))
                    .text_color(rgb(p.muted))
                    .child("No Notifications"),
            );
        } else {
            // Preserve newest-first app order; older cards tuck behind the newest card.
            let mut groups: Vec<(String, Vec<Notification>)> = Vec::new();
            for n in state.notifications {
                if let Some((_, items)) = groups.iter_mut().find(|(app, _)| *app == n.app) {
                    items.push(n);
                } else {
                    groups.push((n.app.clone(), vec![n]));
                }
            }
            for (app, items) in groups {
                let count = items.len();
                let expanded = self.notification_groups.contains(&app);
                let mut group = div().w_full().flex_shrink_0().flex().flex_col().gap(px(6.));
                for n in items.into_iter().take(if expanded { count } else { 1 }) {
                    group = group.child(self.notification_card(n, false, p, cx));
                }
                if count > 1 {
                    if !expanded {
                        group = group
                            .child(
                                div()
                                    .mx(px(7.))
                                    .mt(px(-7.))
                                    .h(px(5.))
                                    .rounded_b(px(7.))
                                    .bg(glass(p)),
                            )
                            .child(
                                div()
                                    .mx(px(14.))
                                    .mt(px(-7.))
                                    .h(px(4.))
                                    .rounded_b(px(6.))
                                    .bg(glass(p)),
                            );
                    }
                    group = group.child(
                        button(
                            eid("notification-group", &app),
                            if expanded {
                                "Show less".to_owned()
                            } else {
                                format!(
                                    "{} more notification{}",
                                    count - 1,
                                    if count == 2 { "" } else { "s" }
                                )
                            },
                            p,
                        )
                        .h(px(22.))
                        .self_center()
                        .bg(glass(p))
                        .text_size(px(10.))
                        .rounded_full()
                        .on_click(cx.listener(move |s, _, _, cx| {
                            if s.notification_groups.contains(&app) {
                                s.notification_groups.retain(|a| *a != app);
                            } else {
                                s.notification_groups.push(app.clone());
                            }
                            cx.notify();
                        })),
                    );
                }
                list = list.child(group);
            }
        }
        div()
            .size_full()
            .relative()
            .font_family(cx.global::<Fonts>().text.clone())
            .text_color(rgb(p.text))
            .track_focus(&self.notification_focus)
            .on_key_down(|event, w, _| {
                if event.keystroke.key == "escape" {
                    w.remove_window();
                }
            })
            .child(
                div()
                    .id("notification-center-dismiss")
                    .absolute()
                    .inset_0()
                    .on_mouse_down(MouseButton::Left, |_, w, _| w.remove_window()),
            )
            .child(
                div()
                    .absolute()
                    .top(px(42.))
                    .right(px(14.))
                    .w(px(350.).min((width - px(28.)).max(px(180.))))
                    .h((height - px(56.)).max(px(120.)))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(list)
                    .when(has_notifications, |d| {
                        d.child(
                            div().flex().justify_end().px(px(4.)).child(
                                button("notifications-clear-all", "Clear All", p)
                                    .rounded_full()
                                    .h(px(24.))
                                    .text_size(px(11.))
                                    .bg(glass(p))
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        s.notification_command(Command::Clear, cx)
                                    })),
                            ),
                        )
                    }),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Backend, Command, DesktopView, Mode, OpenWindows, SharedStore, Store, control_center, eid,
        open_on, toggle_popover,
    };
    use gpui_test::{
        self as gpui, App, ElementId, Entity, Task, TestAppContext, VisualTestContext, prelude::*,
        px, size,
    };
    fn store(cx: &mut App) -> (Entity<Store>, std::sync::mpsc::Receiver<Command>) {
        macstill_ui::init(cx);
        let backend = Backend::fixture();
        let snapshot = backend.snapshot();
        let (notifications, commands) = crate::notifications::Notifications::fixture();
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
        (store, commands)
    }
    fn setup(
        cx: &mut TestAppContext,
    ) -> (
        Entity<DesktopView>,
        &mut VisualTestContext,
        std::sync::mpsc::Receiver<Command>,
    ) {
        let (_, commands) = cx.update(store);
        let (view, cx) =
            cx.add_window_view(|w, cx| DesktopView::new(Mode::NotificationCenter, w, cx));
        cx.update(|w, cx| {
            w.resize(size(px(960.), px(720.)));
            w.bounds_changed(cx);
            w.draw(cx).clear(cx);
        });
        (view, cx, commands)
    }
    fn click(cx: &mut VisualTestContext, id: impl Into<ElementId>) {
        let id = id.into();
        let center = cx.update(|w, _| {
            gpui_base::test_support::find(w, &[], &id)
                .unwrap_or_else(|| panic!("missing element {id:?}"))
                .bounds()
                .center()
        });
        cx.simulate_click(center, gpui_test::Modifiers::default());
        cx.update(|w, cx| w.draw(cx).clear(cx));
    }
    #[test]
    fn banner_geometry_is_clamped_in_logical_coordinates() {
        assert_eq!(
            super::banner_size(size(px(1536.), px(864.))),
            size(px(380.), px(540.))
        );
        assert_eq!(
            super::banner_size(size(px(320.), px(360.))),
            size(px(288.), px(296.))
        );
        assert_eq!(
            super::banner_size(size(px(20.), px(40.))),
            size(px(1.), px(1.))
        );
    }
    #[gpui::test]
    fn controls_dispatch_notification_commands(cx: &mut TestAppContext) {
        let (_, cx, commands) = setup(cx);
        assert!(matches!(
            commands.try_recv().unwrap(),
            Command::Center(true)
        ));
        click(cx, eid("notification-expand", "1"));
        click(cx, eid("notification-action", "1:1"));
        assert!(matches!(commands.try_recv().unwrap(),Command::Invoke(1,key) if key=="reply"));
        click(cx, eid("notification-dismiss", "1"));
        assert!(matches!(commands.try_recv().unwrap(), Command::Dismiss(1)));
        click(cx, "notifications-clear-all");
        assert!(matches!(commands.try_recv().unwrap(), Command::Clear));
    }
    #[gpui::test]
    fn app_stacks_expand_and_collapse_and_service_details_are_disclosed(cx: &mut TestAppContext) {
        let (view, cx, _) = setup(cx);
        view.update(cx, |s, cx| {
            s.store.update(cx, |store, cx| {
                let mut older = store.notification_snapshot.notifications[0].clone();
                older.id = 99;
                store.notification_snapshot.notifications.push(older);
                store.notification_snapshot.diagnostic = Some("Test service conflict".into());
                cx.notify();
            })
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert!(cx.update(|w, _| {
            gpui_base::test_support::find(w, &[], &eid("notification-expand", "99").into())
                .is_none()
        }));
        click(cx, eid("notification-group", "Mail"));
        assert!(view.read_with(cx, |s, _| {
            s.notification_groups.contains(&"Mail".to_owned())
        }));
        assert!(cx.update(|w, _| {
            gpui_base::test_support::find(w, &[], &eid("notification-expand", "99").into())
                .is_some()
        }));
        click(cx, eid("notification-group", "Mail"));
        assert!(cx.update(|w, _| {
            gpui_base::test_support::find(w, &[], &eid("notification-expand", "99").into())
                .is_none()
        }));
        click(cx, "notification-service-details");
        assert!(view.read_with(cx, |s, _| s.notification_expanded.contains(&0)));
    }
    #[gpui::test]
    fn history_actions_are_inert_after_expiry_and_all_sizes_render(cx: &mut TestAppContext) {
        let (view, cx, commands) = setup(cx);
        commands.try_recv().unwrap();
        for dark in [false, true] {
            cx.simulate_scale_factor_change(if dark { 2. } else { 1.25 });
            for (width, height) in [(960., 720.), (760., 540.), (480., 360.)] {
                view.update(cx, |s, cx| {
                    s.store.update(cx, |store, cx| {
                        store.snapshot.preferences.dark = dark;
                        for n in &mut store.notification_snapshot.notifications {
                            n.active = false;
                            n.actions.clear();
                        }
                        cx.notify();
                    });
                });
                cx.update(|w, cx| {
                    w.resize(size(px(width), px(height)));
                    w.bounds_changed(cx);
                    w.draw(cx).clear(cx);
                });
                assert!(cx.update(|w, _| {
                    gpui_base::test_support::find(w, &[], &eid("notification-action", "1:0").into())
                        .is_none()
                }));
            }
        }
        assert!(commands.try_recv().is_err());
    }
    #[gpui::test]
    fn escape_closes_and_releases_center_visibility(cx: &mut TestAppContext) {
        let (view, cx, commands) = setup(cx);
        commands.try_recv().unwrap();
        view.update_in(cx, |s, w, cx| s.notification_focus.focus(w, cx));
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        drop(view);
        assert!(matches!(
            commands.try_recv().unwrap(),
            Command::Center(false)
        ));
    }
    #[gpui::test]
    fn losing_window_activation_closes_the_center(cx: &mut TestAppContext) {
        let (_view, cx, commands) = setup(cx);
        commands.try_recv().unwrap();
        cx.update(|w, _| w.activate_window());
        cx.run_until_parked();
        cx.deactivate_window();
        cx.run_until_parked();
        assert!(matches!(
            commands.try_recv().unwrap(),
            Command::Center(false)
        ));
    }
    #[gpui::test]
    fn clicking_outside_the_cards_closes_the_center(cx: &mut TestAppContext) {
        let (_view, cx, commands) = setup(cx);
        commands.try_recv().unwrap();
        cx.simulate_mouse_down(
            gpui_test::point(px(20.), px(120.)),
            gpui_test::MouseButton::Left,
            gpui_test::Modifiers::default(),
        );
        cx.run_until_parked();
        assert!(matches!(
            commands.try_recv().unwrap(),
            Command::Center(false)
        ));
    }
    #[gpui::test]
    fn notification_and_control_center_do_not_stay_open_together(cx: &mut TestAppContext) {
        let (_, _commands) = cx.update(store);
        let bar = cx.update(|cx| open_on(Mode::Bar, None, cx).unwrap());
        let control = cx.update(|cx| open_on(Mode::ControlCenter, None, cx).unwrap());
        bar.update(cx, |_, w, cx| {
            toggle_popover(Mode::NotificationCenter, w, cx)
        })
        .unwrap();
        cx.run_until_parked();
        assert!(control.update(cx, |_, _, _| ()).is_err());
        let center = cx.update(|cx| cx.global::<OpenWindows>().notification_center.unwrap());
        assert!(center.update(cx, |_, _, _| ()).is_ok());
        bar.update(cx, |_, w, cx| toggle_popover(Mode::ControlCenter, w, cx))
            .unwrap();
        cx.run_until_parked();
        assert!(center.update(cx, |_, _, _| ()).is_err());
    }
    #[test]
    #[ignore = "requires a real offscreen GPU renderer; saves PNGs for visual review"]
    fn notification_visual_review() {
        use gpui_kit::{AppContext, HeadlessAppContext};
        let mut context = HeadlessAppContext::with_platform(
            gpui_kit::platform::current_platform(true).text_system(),
            std::sync::Arc::new(()),
            gpui_platform_test::current_headless_renderer,
        );
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/notifications-review");
        std::fs::create_dir_all(&directory).unwrap();
        for (name, dark, empty, mode, width, height) in [
            ("center", false, false, Mode::NotificationCenter, 960., 720.),
            (
                "control-center",
                false,
                false,
                Mode::ControlCenter,
                960.,
                720.,
            ),
            (
                "control-center-dark",
                true,
                false,
                Mode::ControlCenter,
                960.,
                720.,
            ),
            (
                "center-dark",
                true,
                false,
                Mode::NotificationCenter,
                960.,
                720.,
            ),
            (
                "center-small",
                false,
                false,
                Mode::NotificationCenter,
                760.,
                540.,
            ),
            (
                "banners",
                false,
                false,
                Mode::NotificationBanner,
                380.,
                540.,
            ),
            ("empty", false, true, Mode::NotificationCenter, 960., 720.),
        ] {
            context.update(|cx| {
                let (store, _) = store(cx);
                store.update(cx, |store, _| {
                    store.snapshot.preferences.dark = dark;
                    if empty {
                        store.notification_snapshot.notifications.clear();
                    } else if mode == Mode::NotificationCenter {
                        let mut older = store.notification_snapshot.notifications[0].clone();
                        older.id = 99;
                        store.notification_snapshot.notifications.push(older);
                    }
                });
            });
            let handle = context
                .open_window(size(px(width), px(height)), |w, cx| {
                    cx.new(|cx| DesktopView::new(mode, w, cx))
                })
                .unwrap();
            context.run_until_parked();
            for _ in 0..2 {
                context
                    .update_window(handle.into(), |_, w, cx| w.draw(cx).clear(cx))
                    .unwrap();
                context.run_until_parked();
            }
            context
                .capture_screenshot(handle.into())
                .unwrap()
                .save(directory.join(format!("{name}.png")))
                .unwrap();
            handle
                .update(&mut context, |_, w, _| w.remove_window())
                .unwrap();
            context.run_until_parked();
        }
    }
}
