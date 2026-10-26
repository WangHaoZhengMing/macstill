use gpui::{
    Context, Hsla, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, Render,
    TestAppContext, VisualTestContext, Window, div, point, prelude::*, px, rgb,
};
use gpui_test as gpui;
use swiftui_gpui::{Palette, button, toggle};

#[derive(Clone, Copy)]
enum Control {
    Button,
    Toggle,
}
struct Probe {
    kind: Control,
    disabled: bool,
    checked: bool,
    changes: usize,
    parent_clicks: usize,
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::new(false);
        let control = match self.kind {
            Control::Button => button("button", "Apply", p)
                .disabled(self.disabled)
                .on_click(cx.listener(|s, _, _, cx| {
                    s.changes += 1;
                    cx.notify();
                }))
                .into_any_element(),
            Control::Toggle => toggle("toggle", self.checked, p)
                .disabled(self.disabled)
                .accessibility_label("Enabled")
                .on_change(cx.listener(|s, checked: &bool, _, cx| {
                    s.checked = *checked;
                    s.changes += 1;
                    cx.notify();
                }))
                .into_any_element(),
        };
        div()
            .id("parent")
            .tab_group()
            .size(px(150.))
            .on_click(cx.listener(|s, _, _, _| s.parent_clicks += 1))
            .child(control)
    }
}
fn press(cx: &mut VisualTestContext, key: &str) {
    let keystroke = Keystroke::parse(key).unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
    cx.update(|w, cx| w.draw(cx).clear(cx));
}

#[gpui::test]
fn styled_controls_keep_pointer_keyboard_and_controlled_state_behavior(cx: &mut TestAppContext) {
    for kind in [Control::Button, Control::Toggle] {
        let (view, cx) = cx.add_window_view(move |_, _| Probe {
            kind,
            disabled: false,
            checked: false,
            changes: 0,
            parent_clicks: 0,
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        cx.simulate_click(point(px(8.), px(8.)), Modifiers::default());
        cx.update(|w, cx| {
            assert_eq!(view.read(cx).changes, 1);
            if matches!(kind, Control::Toggle) {
                assert!(view.read(cx).checked);
            }
            w.draw(cx).clear(cx);
        });
        press(cx, "space");
        cx.update(|_, cx| {
            assert_eq!(view.read(cx).changes, 2);
            if matches!(kind, Control::Toggle) {
                assert!(!view.read(cx).checked);
            }
        });
        press(cx, "enter");
        cx.update(|_, cx| assert_eq!(view.read(cx).changes, 3));
    }
}

#[gpui::test]
fn disabled_controls_do_not_activate_or_bubble_to_the_parent(cx: &mut TestAppContext) {
    for kind in [Control::Button, Control::Toggle] {
        let (view, cx) = cx.add_window_view(move |_, _| Probe {
            kind,
            disabled: true,
            checked: false,
            changes: 0,
            parent_clicks: 0,
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        cx.simulate_click(point(px(8.), px(8.)), Modifiers::default());
        press(cx, "space");
        press(cx, "enter");
        cx.update(|_, cx| {
            assert_eq!(view.read(cx).changes, 0);
            assert_eq!(view.read(cx).parent_clicks, 0);
        });
    }
}

#[gpui::test]
fn theme_updates_base_tokens_and_preserves_resolved_fonts(cx: &mut TestAppContext) {
    cx.update(|cx| {
        swiftui_gpui::init(cx);
        let sans = cx.global::<swiftui_gpui::Fonts>().text.clone();
        let mono = cx.global::<swiftui_gpui::Fonts>().mono.clone();
        for dark in [true, false] {
            let p = Palette::new(dark);
            swiftui_gpui::set_theme(p, cx);
            let base = gpui_kit::base::Theme::global(cx);
            assert_eq!(base.tokens.colors.foreground, Hsla::from(rgb(p.text)));
            assert_eq!(base.tokens.colors.ring, Hsla::from(rgb(p.accent)));
            assert_eq!(base.tokens.typography.sans, sans);
            assert_eq!(base.tokens.typography.mono, mono);
            assert_eq!(
                base.appearance == gpui_kit::base::ThemeAppearance::Dark,
                dark
            );
        }
    });
}

#[test]
fn missing_apple_families_fall_back_independently() {
    let fonts =
        swiftui_gpui::Fonts::from_installed(&["Noto Sans".into(), "DejaVu Sans Mono".into()]);
    assert_eq!(fonts.text.as_ref(), "Noto Sans");
    assert_eq!(fonts.display, fonts.text);
    assert_eq!(fonts.mono.as_ref(), "DejaVu Sans Mono");
    let fonts = swiftui_gpui::Fonts::from_installed(&[
        "Noto Sans".into(),
        "SF Pro Text".into(),
        "SF Pro Display".into(),
        "SF Mono".into(),
    ]);
    assert_eq!(fonts.text.as_ref(), "SF Pro Text");
    assert_eq!(fonts.display.as_ref(), "SF Pro Display");
    assert_eq!(fonts.mono.as_ref(), "SF Mono");
}

struct TextProbe {
    input: gpui::Entity<gpui_kit::base::input::InputState>,
}
impl Render for TextProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .tab_group()
            .size(px(250.))
            .child(swiftui_gpui::text_field(
                "text-field",
                &self.input,
                Palette::new(false),
                cx,
            ))
    }
}
#[gpui::test]
fn text_field_keeps_editor_focus_and_text_editing(cx: &mut TestAppContext) {
    cx.update(swiftui_gpui::init);
    let (view, cx) = cx.add_window_view(|w, cx| {
        let input = cx.new(|cx| gpui_kit::base::input::InputState::new(w, cx).placeholder("Name"));
        TextProbe { input }
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    // Click the padding, outside the text itself: the whole field should focus the editor.
    cx.simulate_click(point(px(4.), px(12.)), Modifiers::default());
    cx.simulate_keystrokes("a b c");
    cx.update(|_, cx| assert_eq!(view.read(cx).input.read(cx).value().as_ref(), "abc"));
    cx.simulate_keystrokes("backspace");
    cx.update(|_, cx| assert_eq!(view.read(cx).input.read(cx).value().as_ref(), "ab"));
}

struct NavigationLayoutProbe {
    icon_bounds: std::rc::Rc<std::cell::RefCell<Vec<gpui::Bounds<gpui::Pixels>>>>,
}
impl Render for NavigationLayoutProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::new(false);
        let mut rows = swiftui_gpui::settings_sidebar(p);
        for (i, label) in ["Appearance", "Desktop & Dock", "Applications"]
            .into_iter()
            .enumerate()
        {
            let bounds = self.icon_bounds.clone();
            let icon = gpui::canvas(move |b, _, _| bounds.borrow_mut().push(b), |_, _, _, _| {})
                .size(px(22.));
            rows = rows.child(swiftui_gpui::settings_navigation(
                ("nav", i),
                label,
                icon,
                i == 1,
                p,
            ));
        }
        div().size_full().child(rows)
    }
}

#[gpui::test]
fn settings_navigation_icons_share_a_leading_edge(cx: &mut TestAppContext) {
    cx.update(swiftui_gpui::init);
    let bounds = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let (_, cx) = cx.add_window_view(|_, _| NavigationLayoutProbe {
        icon_bounds: bounds.clone(),
    });
    bounds.borrow_mut().clear();
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let bounds = bounds.borrow();
    assert!(bounds.len() >= 3);
    for icon in bounds.iter() {
        assert_eq!(
            icon.origin.x,
            px(17.),
            "navigation icon should start at sidebar padding plus row padding"
        );
        assert_eq!(icon.size.width, px(22.));
    }
    assert_eq!(bounds[1].origin.y - bounds[0].origin.y, px(32.));
}

struct ControlCenterSliderProbe {
    state: gpui::Entity<gpui_kit::base::slider::SliderState>,
    available: bool,
}
impl Render for ControlCenterSliderProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(300.))
            .child(swiftui_gpui::control_center::control_center_slider(
                "Sound",
                &self.state,
                self.available,
                Palette::new(false),
                cx,
            ))
    }
}
#[gpui::test]
fn control_center_slider_maps_track_clicks_and_disables_missing_devices(cx: &mut TestAppContext) {
    cx.update(swiftui_gpui::init);
    let (view, cx) = cx.add_window_view(|_, cx| ControlCenterSliderProbe {
        state: cx.new(|_| {
            gpui_kit::base::slider::SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(25.)
        }),
        available: true,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let track = cx.update(|_, cx| view.read(cx).state.read(cx).bounds());
    assert!(track.size.width > px(200.));
    cx.simulate_mouse_move(
        point(track.origin.x + track.size.width * 0.75, track.center().y),
        None,
        Modifiers::default(),
    );
    cx.simulate_click(
        point(track.origin.x + track.size.width * 0.75, track.center().y),
        Modifiers::default(),
    );
    cx.update(|_, cx| {
        assert!(
            view.read(cx).state.read(cx).value().end() > 60.,
            "value {:?}",
            view.read(cx).state.read(cx).value()
        )
    });
    view.update(cx, |v, cx| {
        v.available = false;
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let before = cx.update(|_, cx| view.read(cx).state.read(cx).value());
    cx.simulate_click(
        point(track.origin.x + track.size.width * 0.1, track.center().y),
        Modifiers::default(),
    );
    cx.update(|_, cx| assert_eq!(view.read(cx).state.read(cx).value(), before));
}

struct MenuProbe {
    commands: usize,
    parent_clicks: usize,
}
impl Render for MenuProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::new(false);
        div()
            .id("menu-parent")
            .size_full()
            .on_click(cx.listener(|s, _, _, _| s.parent_clicks += 1))
            .child(
                swiftui_gpui::menu("context-menu", p)
                    .child(
                        swiftui_gpui::menu_item("open-item", "Open", p)
                            .on_click(cx.listener(|s, _, _, _| s.commands += 1)),
                    )
                    .child(
                        swiftui_gpui::menu_item("disabled-item", "Unavailable", p)
                            .disabled(true)
                            .on_click(cx.listener(|s, _, _, _| s.commands += 100)),
                    ),
            )
    }
}
#[gpui::test]
fn shared_menu_preserves_command_activation_and_stops_parent_clicks(cx: &mut TestAppContext) {
    cx.update(swiftui_gpui::init);
    let (view, cx) = cx.add_window_view(|_, _| MenuProbe {
        commands: 0,
        parent_clicks: 0,
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.simulate_click(point(px(20.), px(16.)), Modifiers::default());
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).commands, 1);
        assert_eq!(view.read(cx).parent_clicks, 0);
    });
    press(cx, "enter");
    cx.update(|_, cx| assert_eq!(view.read(cx).commands, 2));
    cx.simulate_click(point(px(20.), px(42.)), Modifiers::default());
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).commands, 2);
        assert_eq!(view.read(cx).parent_clicks, 0);
    });
}
