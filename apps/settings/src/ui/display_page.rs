use super::widgets::{SliderTarget, symbol};
use super::*;
use gpui_kit::base::Popover;
use macstill_hyprland::displays::Monitor;
use std::path::Path;
use swiftui_gpui::{settings_divider, settings_group, settings_row, toggle};

pub(super) mod arrangement;
#[cfg(test)]
#[path = "display_tests.rs"]
mod tests;

// Keep the native Displays presentation local to this page.
fn group(p: Palette) -> Div {
    settings_group(p)
        .rounded(px(7.))
        .bg(rgb(if p.dark { 0x2d2d2f } else { 0xefeff0 }))
}

fn choice_button(id: impl Into<ElementId>, p: Palette) -> swiftui_gpui::Button {
    swiftui_gpui::Button::new(id)
        .rounded(px(7.))
        .cursor_pointer()
        .hover(move |s| s.bg(rgb(p.hover)))
        .focus_visible(move |s| s.border_1().border_color(rgb(p.accent)))
        .styles(|s| s.disabled(|s| s.opacity(0.45).cursor_default()))
}

fn display_name(display: &Monitor) -> String {
    if display.name.starts_with("eDP") || display.name.starts_with("LVDS") {
        "Built-in Display".into()
    } else if display.description.is_empty() {
        display.name.clone()
    } else {
        display.description.clone()
    }
}

fn preview(display: Option<&Monitor>, wallpaper: &Path, selected: bool, p: Palette) -> Div {
    let (width, height) = display.map(Monitor::logical_size).unwrap_or((1920, 1080));
    let ratio = (width as f32 / height.max(1) as f32).clamp(0.5, 2.4);
    let screen_h = 108_f32.min(154. / ratio);
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_end()
        .child(
            div()
                .w(px(screen_h * ratio))
                .h(px(screen_h))
                .relative()
                .flex_shrink_0()
                .overflow_hidden()
                .p(px(4.))
                .rounded(px(7.))
                .bg(rgb(0x19191b))
                .border_2()
                .border_color(rgb(if selected { p.accent } else { 0x454548 }))
                .shadow_sm()
                .child(
                    img(wallpaper.to_path_buf())
                        .absolute()
                        .left(px(4.))
                        .top(px(4.))
                        .w(px(screen_h * ratio - 12.))
                        .h(px(screen_h - 12.))
                        .object_fit(ObjectFit::Cover)
                        .rounded(px(2.)),
                ),
        )
        .child(div().w(px(17.)).h(px(6.)).bg(rgb(0x9c9ca0)))
        .child(div().w(px(43.)).h(px(3.)).rounded(px(2.)).bg(rgb(0xb3b3b7)))
}

fn scale_choices(display: &Monitor) -> Vec<f64> {
    let mut scales = display.scales();
    // Five samples fit the same row even in the smallest supported window.
    let selected = scales
        .iter()
        .position(|v| (*v - display.scale).abs() < 0.01);
    let start = selected.unwrap_or(0).saturating_sub(4);
    scales = scales.into_iter().skip(start).take(5).collect();
    scales.reverse();
    scales
}

fn text_sample(index: usize, selected: bool, p: Palette) -> Div {
    div()
        .w(px(60.))
        .h(px(62.))
        .flex_shrink_0()
        .p(px(2.))
        .rounded(px(9.))
        .border_2()
        .border_color(if selected {
            rgb(p.accent).into()
        } else {
            transparent_black()
        })
        .child(
            super::reference::resolution(index, p.dark)
                .size_full()
                .object_fit(ObjectFit::Contain),
        )
}

impl SettingsView {
    pub(super) fn displays(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let snap = self.snapshot(cx);
        let p = settings_palette(snap.preferences.dark);
        let state = self.system_state(cx);
        let wallpaper = macstill_ui::wallpaper(snap.preferences.wallpaper.as_deref());
        let selected_index = self
            .display_selection
            .as_ref()
            .and_then(|name| {
                state
                    .monitors
                    .iter()
                    .position(|monitor| &monitor.name == name)
            })
            .unwrap_or(0);
        let selected = state.monitors.get(selected_index);
        let mut screens = div()
            .flex()
            .flex_wrap()
            .justify_center()
            .items_end()
            .gap(px(12.));
        if state.monitors.is_empty() {
            screens = screens.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(10.))
                    .child(preview(None, &wallpaper, false, p))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Display")),
            );
        }
        for (index, monitor) in state.monitors.iter().enumerate() {
            let name = monitor.name.clone();
            screens = screens.child(
                choice_button(format!("display-select-{}", monitor.name), p)
                    .accessibility_label(format!("Select {}", display_name(monitor)))
                    .border_0()
                    .bg(transparent_black())
                    .p(px(6.))
                    .w(px(180.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(7.))
                    .child(div().h(px(120.)).flex().items_end().child(preview(
                        Some(monitor),
                        &wallpaper,
                        index == selected_index,
                        p,
                    )))
                    .child(
                        div()
                            .w_full()
                            .text_center()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(13.))
                            .child(display_name(monitor)),
                    )
                    .child(caption(monitor.name.clone(), p).text_size(px(12.)))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.display_selection = Some(name.clone());
                        cx.notify();
                    })),
            );
        }
        let monitors = state.monitors.clone();
        let store = self.store.clone();
        let owner = cx.entity();
        let header = div()
            .mx(px(-20.))
            .px(px(20.))
            .pt(px(10.))
            .pb(px(12.))
            .border_b_1()
            .border_color(rgb(p.line))
            .bg(rgb(if p.dark { 0x29292b } else { 0xeeeef0 }))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(screens)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        Popover::new("display-arrangement-popover")
                            .trigger(
                                button("display-arrange", "Arrange…", p)
                                    .disabled(state.monitors.is_empty()),
                            )
                            .on_open_change(cx.listener(|s, open: &bool, _, _| {
                                if !open {
                                    s.display_arrangement = None;
                                }
                            }))
                            .content(move |_, _, cx| {
                                owner.update(cx, |s, cx| {
                                    if s.display_arrangement.is_none() {
                                        s.display_arrangement = Some(cx.new(|cx| {
                                            arrangement::Arrangement::new(
                                                monitors.clone(),
                                                store.clone(),
                                                p,
                                                cx,
                                            )
                                        }));
                                    }
                                    s.display_arrangement.clone().unwrap()
                                })
                            }),
                    )
                    .child(
                        button("display-add", "+  ⌄", p)
                            .accessibility_label("Add display — connect a display to your computer")
                            .disabled(true),
                    ),
            );
        let mut content = div().flex().flex_col().gap(px(16.)).child(header);
        if let Some(display) = selected {
            let mut choices = div()
                .flex()
                .items_start()
                .gap(px(4.))
                .px(px(10.))
                .pt(px(18.))
                .pb(px(15.));
            let scales = scale_choices(display);
            let default_scale = if display.width >= 2560 && scales.contains(&2.) {
                2.
            } else {
                1.
            };
            for (index, scale) in scales.iter().copied().enumerate() {
                let active = (display.scale - scale).abs() < 0.01;
                let name = display.name.clone();
                let label = if (scale - default_scale).abs() < 0.01 {
                    "Default"
                } else if index == 0 {
                    "Larger Text"
                } else if index + 1 == scales.len() {
                    "More Space"
                } else {
                    ""
                };
                choices = choices.child(
                    choice_button(
                        format!("display-scale-{}-{:.0}%", display.name, scale * 100.),
                        p,
                    )
                    .accessibility_label(format!(
                        "{}: {:.0}% scaling{}",
                        display.name,
                        scale * 100.,
                        if active { ", selected" } else { "" }
                    ))
                    .disabled(state.busy || state.loading)
                    .flex_1()
                    .min_w_0()
                    .p(px(3.))
                    .border_0()
                    .bg(transparent_black())
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.))
                    .child(text_sample(index, active, p))
                    .child(
                        div()
                            .h(px(17.))
                            .text_size(px(11.))
                            .text_center()
                            .when(active, |d| d.font_weight(FontWeight::SEMIBOLD))
                            .child(label),
                    )
                    .on_click(cx.listener(move |s, _, _, cx| {
                        if let Err(error) = s.store.read(cx).system.set_scale(name.clone(), scale) {
                            s.message = Some(error.to_string());
                        }
                        let next = s.store.read(cx).system.snapshot();
                        s.store.update(cx, |store, cx| {
                            store.system_snapshot = next;
                            cx.notify();
                        });
                        cx.notify();
                    })),
                );
            }
            content = content.child(
                group(p)
                    .child(
                        settings_row("Use as", p).child(
                            caption(
                                if state.monitors.len() == 1 {
                                    "Main display"
                                } else {
                                    "Extended display"
                                },
                                p,
                            )
                            .text_color(rgb(p.text)),
                        ),
                    )
                    .child(settings_divider(p))
                    .child(choices)
                    .child(
                        caption(
                            format!(
                                "Looks like {} × {}",
                                display.logical_size().0,
                                display.logical_size().1
                            ),
                            p,
                        )
                        .text_size(px(11.))
                        .text_center()
                        .pb(px(12.)),
                    ),
            );
        } else {
            content = content.child(group(p).p(px(14.)).child(caption(
                if state.display_error.is_some() {
                    "Display settings are available in a Macstill / Hyprland session."
                } else {
                    "Detecting displays…"
                },
                p,
            )));
        }
        // These values are not exposed for editing by the current display backend.
        content = content.child(
            group(p).child(settings_row("Color profile", p).child(caption("Not available", p))),
        );
        let refresh = selected
            .map(|m| format!("{} Hertz", (m.refresh_rate * 100.).round() / 100.))
            .unwrap_or_else(|| "Not available".into());
        content = content.child(group(p)
            .child(settings_row("Refresh rate", p).child(div().text_size(px(13.)).child(refresh)))
            .child(settings_divider(p))
            .child(div().px(px(12.)).py(px(10.)).flex().items_center().gap(px(18.))
                .child(div().flex_1().flex().flex_col().gap(px(3.))
                    .child("High Dynamic Range")
                    .child(caption("Automatically adjust the display to show high dynamic range content.", p).text_size(px(11.))))
                .child(toggle("display-hdr", false, p).disabled(true)
                    .accessibility_label("High Dynamic Range is not available"))));
        let rotation = selected
            .map(|m| match m.transform % 4 {
                1 => "90°",
                2 => "180°",
                3 => "270°",
                _ => "Standard",
            })
            .unwrap_or("Not available");
        content = content
            .child(group(p).child(
                settings_row("Rotation", p).child(div().text_size(px(13.)).child(rotation)),
            ));
        let lights = state.system.backlights.value.unwrap_or_default();
        if !lights.is_empty() {
            let mut brightness = group(p);
            for light in lights {
                let slider = self.slider(
                    format!("brightness-{}", light.name),
                    light.percent,
                    SliderTarget::Brightness(light.name.clone()),
                    window,
                    cx,
                );
                brightness = brightness.child(
                    settings_row("Brightness", p)
                        .child(symbol("sun", 16., p.muted))
                        .child(slider),
                );
            }
            content = content.child(brightness);
        }
        content.child(div().flex().justify_end().items_center().gap(px(8.)).pt(px(3.))
            .child(button("display-advanced", "Advanced…", p).disabled(true))
            .child(button("display-night-shift", "Night Shift…", p).disabled(true))
            .child(Popover::new("display-help-popover")
                .trigger(button("display-help", "?", p).size(px(25.)).p(px(0.)).rounded_full()
                    .accessibility_label("Display settings help"))
                .content(move |_, _, _| div().p(px(14.)).max_w(px(300.)).rounded(px(8.))
                    .border_1().border_color(rgb(p.line)).bg(rgb(p.surface)).shadow_lg()
                    .child(caption("Select a display to change its text size. Color profiles, HDR, rotation, Night Shift, and advanced options are not available through this display backend.", p)))))
            .into_any_element()
    }
}
