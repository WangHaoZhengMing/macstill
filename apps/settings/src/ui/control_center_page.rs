use super::{widgets::*, *};
use gpui_kit::base::Popover;
use macstill_config::{ControlCenterModule as Module, MenuBarVisibility as Visibility};
use swiftui_gpui::{settings_divider, settings_group, toggle};
impl SettingsView {
    pub(super) fn control_center_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let preferences = self.snapshot(cx).preferences.control_center;
        let mut modules = settings_group(p);
        for (i, module) in Module::ALL.into_iter().enumerate() {
            if i > 0 {
                modules = modules.child(settings_divider(p));
            }
            let selected = preferences.visibility(module);
            let view = cx.entity().downgrade();
            let menu = Popover::new(eid("cc-module-menu", module.key()))
                .anchor(Anchor::TopRight)
                .offset(px(3.))
                .when(!module.supported(), |menu| menu.open(false))
                .trigger(
                    button(
                        eid("cc-module", module.key()),
                        if module.supported() {
                            selected.label()
                        } else {
                            "Not Available"
                        },
                        p,
                    )
                    .disabled(!module.supported())
                    .border_0()
                    .bg(transparent_black())
                    .text_size(px(12.))
                    .px(px(3.))
                    .py(px(2.))
                    .child(
                        div()
                            .ml(px(7.))
                            .rounded(px(4.))
                            .bg(rgb(p.line))
                            .child(chevrons(p)),
                    ),
                )
                .content(move |_, _, cx| {
                    let mut menu = div()
                        .w(px(225.))
                        .p(px(5.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(rgb(p.line))
                        .bg(rgb(p.surface))
                        .shadow_lg()
                        .flex()
                        .flex_col()
                        .gap(px(2.));
                    for choice in Visibility::ALL {
                        let view = view.clone();
                        menu = menu.child(
                            button(
                                eid(
                                    "cc-visibility",
                                    &format!("{}:{}", module.key(), choice.key()),
                                ),
                                format!(
                                    "{}  {}",
                                    if selected == choice { "✓" } else { "  " },
                                    choice.label()
                                ),
                                p,
                            )
                            .w_full()
                            .justify_start()
                            .border_0()
                            .bg(transparent_black())
                            .text_size(px(12.))
                            .disabled(!module.supported())
                            .on_click(cx.listener(
                                move |popover, _, window, cx| {
                                    let _ = view.update(cx, |view, cx| {
                                        view.edit(cx, |preferences| {
                                            preferences
                                                .control_center
                                                .menu_bar
                                                .insert(module, choice);
                                        })
                                    });
                                    popover.dismiss(window, cx);
                                },
                            )),
                        );
                    }
                    menu
                });
            modules = modules.child(
                div()
                    .min_h(px(44.))
                    .px(px(12.))
                    .py(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(module_icon(module, p))
                    .child(div().flex_1().min_w_0().child(module.title()))
                    .child(menu),
            );
        }
        let mut battery = settings_group(p).child(
            div()
                .px(px(12.))
                .py(px(11.))
                .flex()
                .items_center()
                .gap(px(10.))
                .child(super::sidebar::icon(Page::Battery, 22.))
                .child("Battery"),
        );
        for (id, label, value, field) in [
            (
                "cc-battery-menu-bar",
                "Show in Menu Bar",
                preferences.battery_in_menu_bar,
                0,
            ),
            (
                "cc-battery-control-center",
                "Show in Control Center",
                preferences.battery_in_control_center,
                1,
            ),
            (
                "cc-battery-percentage",
                "Show Percentage",
                preferences.battery_percentage,
                2,
            ),
        ] {
            battery =
                battery.child(settings_divider(p)).child(
                    div()
                        .pl(px(44.))
                        .pr(px(12.))
                        .py(px(9.))
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(div().flex_1().child(label))
                        .child(toggle(id, value, p).accessibility_label(label).on_change(
                            cx.listener(move |view, checked, _, cx| {
                                view.edit(cx, |preferences| match field {
                                    0 => preferences.control_center.battery_in_menu_bar = *checked,
                                    1 => {
                                        preferences.control_center.battery_in_control_center =
                                            *checked
                                    }
                                    _ => preferences.control_center.battery_percentage = *checked,
                                })
                            }),
                        )),
                );
        }
        let accessibility = settings_group(p)
            .child(
                div()
                    .px(px(12.))
                    .py(px(11.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(accessibility_icon())
                    .child(div().flex_1().child("Accessibility Shortcuts"))
                    .child(caption("Not Available", p).text_size(px(11.))),
            )
            .child(settings_divider(p))
            .child(
                div()
                    .pl(px(44.))
                    .pr(px(12.))
                    .py(px(9.))
                    .flex()
                    .items_center()
                    .child(div().flex_1().child("Show in Menu Bar"))
                    .child(
                        toggle("cc-accessibility-menu-bar", false, p)
                            .accessibility_label(
                                "Accessibility shortcuts in menu bar are unavailable",
                            )
                            .disabled(true),
                    ),
            )
            .child(settings_divider(p))
            .child(
                div()
                    .pl(px(44.))
                    .pr(px(12.))
                    .py(px(9.))
                    .flex()
                    .items_center()
                    .child(div().flex_1().child("Show in Control Center"))
                    .child(
                        toggle("cc-accessibility-control-center", false, p)
                            .accessibility_label(
                                "Accessibility shortcuts in Control Center are unavailable",
                            )
                            .disabled(true),
                    ),
            );
        stack().gap(px(12.))
            .child(heading("Control Center Modules","These modules are always visible in Control Center. You can choose when they should also show in the Menu Bar.",p))
            .child(modules)
            .child(heading("Other Modules","These modules can be added to Control Center and the Menu Bar.",p).mt(px(10.)))
            .child(accessibility)
            .child(battery)
            .into_any_element()
    }
}
fn heading(title: &str, detail: &str, p: Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .px(px(10.))
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        .child(
            caption(detail.to_string(), p)
                .text_size(px(12.))
                .line_height(px(17.)),
        )
}
pub(super) fn chevrons(p: Palette) -> AnyElement {
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="15" height="19" viewBox="0 0 15 19"><path d="m4 7 3.5-3.5L11 7m-7 5 3.5 3.5L11 12" fill="none" stroke="#{:06x}" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
        p.text
    );
    img(std::sync::Arc::new(Image::from_bytes(
        ImageFormat::Svg,
        svg.into_bytes(),
    )))
    .w(px(15.))
    .h(px(19.))
    .into_any_element()
}
fn module_icon(module: Module, _p: Palette) -> AnyElement {
    match module {
        Module::Wifi => return super::sidebar::icon(Page::Wifi, 22.),
        Module::Bluetooth => return super::sidebar::icon(Page::Bluetooth, 22.),
        Module::Display => return super::sidebar::icon(Page::Displays, 22.),
        Module::Sound => return super::sidebar::icon(Page::Sound, 22.),
        _ => {}
    }
    let (source, color) = match module {
        Module::AirDrop => (
            include_str!("../../../../assets/icons/control-center/cc_airdrop.svg"),
            0x7bc8e4,
        ),
        Module::Focus => (
            include_str!("../../../../assets/icons/control-center/cc_focus.svg"),
            0x7167dc,
        ),
        Module::StageManager => (
            include_str!("../../../../assets/icons/control-center/cc_stage.svg"),
            0x252525,
        ),
        Module::ScreenMirroring => (
            include_str!("../../../../assets/icons/control-center/cc_mirror.svg"),
            0x4ebcd2,
        ),
        Module::NowPlaying => (
            include_str!("../../../../assets/icons/control-center/cc_play.svg"),
            0xffaa20,
        ),
        _ => unreachable!(),
    };
    div()
        .size(px(22.))
        .flex_shrink_0()
        .rounded(px(5.))
        .bg(rgb(color))
        .flex()
        .items_center()
        .justify_center()
        .child(
            img(std::sync::Arc::new(Image::from_bytes(
                ImageFormat::Svg,
                source.replace("#202124", "#ffffff").into_bytes(),
            )))
            .size(px(17.)),
        )
        .into_any_element()
}

fn accessibility_icon() -> AnyElement {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 22 22"><rect width="22" height="22" rx="5" fill="#168af5"/><g fill="none" stroke="white" stroke-width="1.4" stroke-linecap="round"><circle cx="11" cy="11" r="8"/><circle cx="11" cy="6.3" r="1.2" fill="white" stroke="none"/><path d="M6.6 9h8.8M11 9v4m0 0-2.7 4m2.7-4 2.7 4"/></g></svg>"##;
    img(std::sync::Arc::new(Image::from_bytes(
        ImageFormat::Svg,
        svg.as_bytes().to_vec(),
    )))
    .size(px(22.))
    .flex_shrink_0()
    .into_any_element()
}
