use super::widgets::*;
use super::*;
use macstill_system::{
    SystemCommand,
    pointer::{PointerCommand, PointerSetting as Setting, PointerSnapshot, PointerValue},
};
use swiftui_gpui::{settings_divider, settings_group, settings_row};

impl SettingsView {
    pub(super) fn pointer_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let state = self.system_state(cx);
        let trackpad = self.page == Page::Trackpad;
        let pointer = state.system.pointer.value.as_ref();
        let tabs: &[&str] = if trackpad {
            &["Point & Click", "Scroll & Zoom", "More Gestures"]
        } else {
            &["Point & Click", "Scrolling"]
        };
        self.pointer_tab = self.pointer_tab.min(tabs.len() - 1);
        let tab = self.pointer_tab;
        let mut content = stack().child(pointer_preview(trackpad, tab, p));
        let mut segmented = div()
            .w_full()
            .flex()
            .p(px(2.))
            .rounded(px(8.))
            .bg(rgb(if p.dark { 0x38383b } else { 0xe6e4e4 }))
            .border_1()
            .border_color(rgb(p.line));
        for (i, label) in tabs.iter().enumerate() {
            let selected = i == tab;
            segmented = segmented.child(
                button(("pointer-tab", i), *label, p)
                    .flex_1()
                    .px(px(4.))
                    .py(px(4.))
                    .border_color(if selected { rgb(p.line) } else { rgba(0) })
                    .rounded(px(6.))
                    .bg(if selected {
                        rgb(p.surface).into()
                    } else {
                        transparent_black()
                    })
                    .text_size(px(12.))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.pointer_tab = i;
                        cx.notify();
                    })),
            );
        }
        content = content.child(segmented);
        if let Some(pointer) = pointer {
            let names = pointer
                .devices
                .iter()
                .filter(|d| d.trackpad == trackpad)
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>();
            if !names.is_empty() {
                content =
                    content.child(caption(names.join(" · "), p).text_size(px(11.)).px(px(8.)));
            } else if pointer.devices_error.is_none() {
                content=content.child(caption(if trackpad{"No connected trackpad detected. These settings apply when one is connected."}else{"No connected mouse detected. These settings apply when one is connected."},p).px(px(8.)));
            }
        } else {
            content = content.child(unavailable(
                &state.system.pointer,
                "Input settings are unavailable in this session",
                p,
            ));
        }
        let mut group = settings_group(p);
        if tab == 0 {
            let speed = if trackpad {
                Setting::TrackpadSpeed
            } else {
                Setting::MouseSpeed
            };
            group =
                group.child(self.pointer_slider_row("Tracking speed", speed, pointer, window, cx));
            if trackpad {
                group = group.child(settings_divider(p)).child(value_row(
                    "Click pressure",
                    "Not available on this Linux backend",
                    p,
                ));
                group = group.child(settings_divider(p)).child(self.pointer_toggle(
                    "Force Click and haptic feedback",
                    "Requires pressure-sensitive hardware and a compatible driver.",
                    None,
                    pointer,
                    cx,
                ));
                group = group.child(settings_divider(p)).child(self.pointer_choices(
                    "Secondary click",
                    Setting::TrackpadSecondaryClick,
                    pointer,
                    cx,
                ));
                group = group.child(settings_divider(p)).child(self.pointer_toggle(
                    "Tap to click",
                    "Tap with one finger.",
                    Some(Setting::TrackpadTapToClick),
                    pointer,
                    cx,
                ));
            } else {
                group = group
                    .child(settings_divider(p))
                    .child(self.pointer_slider_row(
                        "Pointer size",
                        Setting::MouseCursorSize,
                        pointer,
                        window,
                        cx,
                    ));
                group = group.child(settings_divider(p)).child(self.pointer_toggle(
                    "Primary button on the right",
                    "Swap left and right buttons for left-handed use.",
                    Some(Setting::MouseLeftHanded),
                    pointer,
                    cx,
                ));
                group = group.child(settings_divider(p)).child(self.pointer_choices(
                    "Pointer acceleration",
                    Setting::MouseAcceleration,
                    pointer,
                    cx,
                ));
                if pointer
                    .and_then(|p| p.number(Setting::MouseDoubleClick))
                    .is_some()
                {
                    group = group
                        .child(settings_divider(p))
                        .child(self.pointer_slider_row(
                            "Double-click speed",
                            Setting::MouseDoubleClick,
                            pointer,
                            window,
                            cx,
                        ));
                }
            }
        } else if tab == 1 {
            let natural = if trackpad {
                Setting::TrackpadNaturalScroll
            } else {
                Setting::MouseNaturalScroll
            };
            group = group.child(self.pointer_toggle(
                "Natural scrolling",
                "Move content in the same direction as your fingers.",
                Some(natural),
                pointer,
                cx,
            ));
            if trackpad {
                group = group.child(settings_divider(p)).child(self.pointer_toggle(
                    "Two-finger scrolling",
                    "Scroll vertically or horizontally with two fingers.",
                    Some(Setting::TrackpadTwoFingerScroll),
                    pointer,
                    cx,
                ));
            }
            let setting = if trackpad {
                Setting::TrackpadScrollSpeed
            } else {
                Setting::MouseScrollSpeed
            };
            if pointer.and_then(|p| p.number(setting)).is_some() {
                group = group
                    .child(settings_divider(p))
                    .child(self.pointer_slider_row(
                        "Scrolling speed",
                        setting,
                        pointer,
                        window,
                        cx,
                    ));
            }
            if trackpad {
                group = group.child(settings_divider(p)).child(value_row(
                    "Zoom in or out",
                    "Pinch gestures are handled by applications",
                    p,
                ));
                group = group.child(settings_divider(p)).child(value_row(
                    "Smart zoom",
                    "Not provided by this desktop backend",
                    p,
                ));
            }
        } else {
            group = group.child(self.pointer_toggle(
                "Tap and drag",
                "Tap, then touch and move to drag an item.",
                Some(Setting::TrackpadTapAndDrag),
                pointer,
                cx,
            ));
            group = group.child(settings_divider(p)).child(self.pointer_toggle(
                "Drag lock",
                "Keep dragging briefly after lifting your finger.",
                Some(Setting::TrackpadDragLock),
                pointer,
                cx,
            ));
            group = group.child(settings_divider(p)).child(self.pointer_toggle(
                "Ignore accidental input while typing",
                "Pause the trackpad while you type.",
                Some(Setting::TrackpadDisableWhileTyping),
                pointer,
                cx,
            ));
            group = group.child(settings_divider(p)).child(self.pointer_toggle(
                "Emulate middle click",
                "Press the left and right buttons together.",
                Some(Setting::TrackpadMiddleClick),
                pointer,
                cx,
            ));
            group = group.child(settings_divider(p)).child(value_row(
                "Workspace and desktop gestures",
                "Three fingers: up for Mission Control, down to return, left/right for desktops",
                p,
            ));
        }
        content = content.child(group);
        if let Some(pointer) = pointer
            && pointer.shared_tracking_speed
            && tab == 0
        {
            content=content.child(caption("Tracking speed is shared by mice and trackpads in this Hyprland session. Per-device overrides in your compositor configuration can take precedence.",p).px(px(8.)));
        }
        content
            .child(
                div().flex().justify_end().child(
                    button(
                        "pointer-bluetooth",
                        if trackpad {
                            "Set Up Bluetooth Trackpad…"
                        } else {
                            "Set Up Bluetooth Mouse…"
                        },
                        p,
                    )
                    .on_click(cx.listener(|s, _, w, cx| s.set_page(Page::Bluetooth, w, cx))),
                ),
            )
            .into_any_element()
    }
    fn pointer_slider_row(
        &mut self,
        label: &str,
        setting: Setting,
        pointer: Option<&PointerSnapshot>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let value = pointer.and_then(|s| s.number(setting));
        let writable = pointer.is_some_and(|s| s.writable(setting));
        let percent = value
            .map(|v| match setting {
                Setting::MouseSpeed | Setting::TrackpadSpeed => (v + 1.) * 50.,
                Setting::MouseDoubleClick => (1000. - v) / 9.,
                Setting::MouseCursorSize => (v - 24.) / 72. * 100.,
                _ => (v - 0.1) / 4.9 * 100.,
            })
            .unwrap_or(50.)
            .clamp(0., 100.);
        let slider = self.slider(
            format!("pointer-{setting:?}"),
            percent,
            SliderTarget::Pointer(setting, writable),
            window,
            cx,
        );
        settings_row(label.to_owned(), p).child(
            div()
                .w(px(236.))
                .flex_shrink_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(slider)
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .text_size(px(11.))
                        .text_color(rgb(p.muted))
                        .child(if setting == Setting::MouseCursorSize {
                            "Small"
                        } else {
                            "Slow"
                        })
                        .child(if setting == Setting::MouseCursorSize {
                            "Large"
                        } else {
                            "Fast"
                        }),
                ),
        )
    }
    fn pointer_toggle(
        &self,
        label: &str,
        detail: &str,
        setting: Option<Setting>,
        pointer: Option<&PointerSnapshot>,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let value = setting.and_then(|s| pointer.and_then(|p| p.boolean(s)));
        let writable = setting.is_some_and(|s| pointer.is_some_and(|p| p.writable(s)));
        let id = setting
            .map(|s| format!("pointer-toggle-{s:?}"))
            .unwrap_or_else(|| "pointer-force-click".into());
        div()
            .px(px(12.))
            .py(px(10.))
            .flex()
            .items_center()
            .gap(px(18.))
            .child(Self::row(
                label.to_owned(),
                if value.is_none() && setting.is_some() {
                    "Not configurable by this desktop version."
                } else {
                    detail
                },
                p,
            ))
            .child(
                toggle(id, value.unwrap_or(false), p)
                    .accessibility_label(label.to_owned())
                    .disabled(!writable || self.system_state(cx).busy)
                    .on_change(cx.listener(move |s, value: &bool, _, cx| {
                        if let Some(setting) = setting {
                            s.apply_system(
                                SystemCommand::SetPointer(PointerCommand {
                                    setting,
                                    value: PointerValue::Bool(*value),
                                }),
                                cx,
                            );
                        }
                    })),
            )
    }
    fn pointer_choices(
        &self,
        label: &str,
        setting: Setting,
        pointer: Option<&PointerSnapshot>,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let value = pointer.and_then(|p| p.text(setting));
        let choices = pointer
            .and_then(|p| p.control(setting))
            .map(|c| c.choices.clone())
            .unwrap_or_default();
        let mut buttons = div().flex().justify_end().flex_wrap().gap(px(4.));
        for choice in choices {
            let selected = value == Some(&choice);
            let title = match choice.as_str() {
                "default" => "Default",
                "fingers" => "Two Fingers",
                "areas" => "Bottom Corners",
                "none" => "Off",
                "adaptive" => "Adaptive",
                "flat" => "Off",
                _ => &choice,
            }
            .to_string();
            buttons = buttons.child(
                button(format!("pointer-choice-{setting:?}-{choice}"), title, p)
                    .px(px(7.))
                    .disabled(
                        self.system_state(cx).busy || !pointer.is_some_and(|p| p.writable(setting)),
                    )
                    .when(selected, |b| {
                        b.bg(rgb(p.hover))
                            .text_color(rgb(p.text))
                            .border_color(rgb(p.accent))
                    })
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.apply_system(
                            SystemCommand::SetPointer(PointerCommand {
                                setting,
                                value: PointerValue::Text(choice.clone()),
                            }),
                            cx,
                        )
                    })),
            );
        }
        settings_row(label.to_owned(), p).child(buttons)
    }
}
fn pointer_preview(trackpad: bool, tab: usize, p: Palette) -> Div {
    let color = if p.dark { "#4e4e52" } else { "#dedbd9" };
    let border = if p.dark { "#8e8e94" } else { "#afadab" };
    let device = if trackpad {
        format!(
            r##"<rect x="82" y="36" width="156" height="118" rx="16" fill="none" stroke="{border}" stroke-width="3"/><circle cx="153" cy="76" r="9" fill="#c8deff" stroke="#4594f9"/><circle cx="172" cy="76" r="9" fill="#c8deff" stroke="#4594f9" opacity="{}"/><path d="M160 101v22m-5-5 5 5 5-5" stroke="#4594f9" stroke-width="2" fill="none" opacity="{}"/>"##,
            if tab > 0 { "1" } else { "0" },
            if tab > 0 { "1" } else { "0" }
        )
    } else {
        format!(
            r##"<rect x="117" y="22" width="86" height="145" rx="43" fill="none" stroke="{border}" stroke-width="3"/><path d="M160 22v59M119 81h82" fill="none" stroke="{border}" stroke-width="2"/><rect x="155" y="44" width="10" height="25" rx="5" fill="#4594f9"/><circle cx="139" cy="54" r="8" fill="#c8deff" opacity=".8"/>"##
        )
    };
    let left = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="190"><rect width="320" height="190" rx="8" fill="{color}" fill-opacity=".22"/>{device}</svg>"##
    );
    let right = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="190"><defs><linearGradient id="s" x2=".8" y2="1"><stop stop-color="#248bc3"/><stop offset="1" stop-color="#77c7ee"/></linearGradient></defs><rect width="320" height="190" rx="8" fill="url(#s)"/><rect x="38" y="19" width="238" height="138" rx="5" fill="#eaf7fd"/><path d="M38 34h238" stroke="#c6e8f7"/><rect x="64" y="48" width="110" height="5" rx="2" fill="#adc5cd"/><path d="M64 63h160M64 71h160M64 79h160M64 87h124" stroke="#bacdd4" stroke-width="3"/><circle cx="78" cy="120" r="16" fill="#a98af6"/><rect x="108" y="102" width="53" height="36" rx="4" fill="#55c4e9"/><rect x="181" y="{}" width="104" height="71" rx="4" fill="white" stroke="#b6d7e6"/><path d="M194 100h72M194 110h55M194 120h65M194 130h42" stroke="#c2d3db" stroke-width="3"/><path d="m224 62 6 17 3-6 7 3Z" fill="#414951" stroke="white"/><rect x="83" y="170" width="159" height="15" rx="5" fill="white" fill-opacity=".3"/><g fill="#c1e5fc"><rect x="88" y="173" width="10" height="9" rx="2"/><rect x="103" y="173" width="10" height="9" rx="2"/><rect x="118" y="173" width="10" height="9" rx="2"/><rect x="133" y="173" width="10" height="9" rx="2"/></g></svg>"##,
        if tab == 2 { "72" } else { "85" }
    );
    let image = |svg: String| {
        img(std::sync::Arc::new(Image::from_bytes(
            ImageFormat::Svg,
            svg.into_bytes(),
        )))
        .w_full()
        .h_full()
        .object_fit(ObjectFit::Contain)
    };
    div()
        .flex()
        .gap(px(12.))
        .h(px(174.))
        .mb(px(3.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .rounded(px(8.))
                .border_1()
                .border_color(rgb(p.line))
                .overflow_hidden()
                .child(image(left)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .rounded(px(8.))
                .border_1()
                .border_color(rgb(p.line))
                .overflow_hidden()
                .child(image(right)),
        )
}
