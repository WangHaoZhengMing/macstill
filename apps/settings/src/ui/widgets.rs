use super::*;
use gpui_kit::base::slider::{
    Slider, SliderEvent, SliderIndicator, SliderState, SliderThumb, SliderTrack,
};
use macstill_system::{AudioDirection, State, SystemCommand};
use swiftui_gpui::{settings_group, settings_row};

#[derive(Clone, Copy)]
pub(super) enum EditField {
    Timezone,
}
#[derive(Clone)]
pub(super) enum SliderTarget {
    Pointer(macstill_system::pointer::PointerSetting, bool),
    Volume(AudioDirection, u32),
    Brightness(String),
    DockSize,
}
pub(super) struct SliderBinding {
    state: Entity<SliderState>,
    dragging: bool,
    _subscription: Subscription,
}

pub(super) fn page_color(page: Page) -> u32 {
    match page {
        Page::Wifi | Page::Bluetooth | Page::Network => 0x007aff,
        Page::Sound => 0xfa3c60,
        Page::Appearance => 0x52515b,
        Page::Displays => 0x2584ed,
        Page::Wallpaper => 0x61c6e6,
        Page::Battery => 0x31b453,
        Page::DateTime => 0xef4841,
        Page::Shortcuts => 0x9864d8,
        Page::Applications => 0x007aff,
        _ => 0x8a8b90,
    }
}
pub(super) fn symbol(kind: &str, size: f32, color: u32) -> AnyElement {
    let paths = match kind {
        "chevron-left" => r#"<path d="m15 4-8 8 8 8"/>"#,
        "chevron-right" => r#"<path d="m9 4 8 8-8 8"/>"#,
        "refresh" => r#"<path d="M19 8a8 8 0 1 0 1 8M19 3v6h-6"/>"#,
        "mouse" => r#"<rect x="6" y="2" width="12" height="20" rx="6"/><path d="M12 2v7M6 9h12"/>"#,
        "trackpad" => {
            r#"<rect x="2" y="4" width="20" height="16" rx="3"/><path d="M4 16h16M12 9v3"/>"#
        }
        "wifi" => {
            r#"<path d="M2 8a16 16 0 0 1 20 0M5.5 11.5a10.5 10.5 0 0 1 13 0M9 15a5 5 0 0 1 6 0"/><circle cx="12" cy="19" r="1"/>"#
        }
        "bluetooth" => r#"<path d="m7 7 10 10-5 5V2l5 5L7 17"/>"#,
        "network" => {
            r#"<circle cx="12" cy="12" r="9"/><ellipse cx="12" cy="12" rx="4" ry="9"/><path d="M3 12h18M5 6.5h14M5 17.5h14"/>"#
        }
        "battery" => {
            r#"<rect x="2" y="6" width="18" height="12" rx="3"/><path d="M23 10v4M6 9v6M9 9v6M12 9v6M15 9v6"/>"#
        }
        "clock" => r#"<circle cx="12" cy="12" r="9"/><path d="M12 6v6l4 2"/>"#,
        "sound" => r#"<path d="M3 9h4l5-5v16l-5-5H3ZM16 8a6 6 0 0 1 0 8M19 5a10 10 0 0 1 0 14"/>"#,
        "display" => {
            r#"<rect x="2" y="3" width="20" height="15" rx="2"/><path d="M8 22h8M12 18v4"/>"#
        }
        "appearance" => {
            r#"<circle cx="12" cy="12" r="9"/><path d="M12 3a9 9 0 0 1 0 18Z" fill="currentColor" stroke="none"/>"#
        }
        "apps" => {
            r#"<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>"#
        }
        "shortcut" => {
            r#"<rect x="3" y="10" width="11" height="11" rx="3"/><path d="M11 3h10v10M21 3 10 14"/>"#
        }
        "check" => r#"<path d="m5 12 4 4L19 6"/>"#,
        "lock" => {
            r#"<rect x="5" y="10" width="14" height="11" rx="3"/><path d="M8 10V7a4 4 0 0 1 8 0v3"/>"#
        }
        "keyboard" => {
            r#"<rect x="2" y="5" width="20" height="14" rx="2"/><path d="M5 9h1m3 0h1m3 0h1m3 0h1M5 12h1m3 0h1m3 0h1m3 0h1M7 16h10"/>"#
        }
        "search" => r#"<circle cx="10" cy="10" r="6"/><path d="m15 15 6 6"/>"#,
        "sun" => {
            r#"<circle cx="12" cy="12" r="4"/><path d="M12 1v3M12 20v3M1 12h3M20 12h3M4 4l2 2M18 18l2 2M4 20l2-2M18 6l2-2"/>"#
        }
        "info" => r#"<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7v.2"/>"#,
        _ => {
            r#"<circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="3"/><path d="M12 1v3M12 20v3M1 12h3M20 12h3M4 4l2 2M18 18l2 2M4 20l2-2M18 6l2-2"/>"#
        }
    };
    let source = match kind {
        "wifi" => Some(include_str!(
            "../../../../assets/icons/settings/sf-wifi.svg"
        )),
        "network" => Some(include_str!(
            "../../../../assets/icons/settings/sf-network.svg"
        )),
        "battery" => Some(include_str!(
            "../../../../assets/icons/settings/sf-battery.svg"
        )),
        "clock" => Some(include_str!(
            "../../../../assets/icons/settings/sf-clock.svg"
        )),
        "gear" => Some(include_str!(
            "../../../../assets/icons/settings/sf-gear.svg"
        )),
        "sound" => Some(include_str!(
            "../../../../assets/icons/settings/sf-sound.svg"
        )),
        "display" => Some(include_str!(
            "../../../../assets/icons/settings/sf-display.svg"
        )),
        "appearance" => Some(include_str!(
            "../../../../assets/icons/settings/sf-appearance.svg"
        )),
        "apps" => Some(include_str!(
            "../../../../assets/icons/settings/sf-apps.svg"
        )),
        "shortcut" => Some(include_str!(
            "../../../../assets/icons/settings/sf-shortcut.svg"
        )),
        "check" => Some(include_str!(
            "../../../../assets/icons/settings/sf-check.svg"
        )),
        "lock" => Some(include_str!(
            "../../../../assets/icons/settings/sf-lock.svg"
        )),
        "search" => Some(include_str!(
            "../../../../assets/icons/settings/sf-search.svg"
        )),
        "sun" => Some(include_str!("../../../../assets/icons/settings/sf-sun.svg")),
        "info" => Some(include_str!(
            "../../../../assets/icons/settings/sf-info.svg"
        )),
        "dock" => Some(include_str!(
            "../../../../assets/icons/settings/sf-dock.svg"
        )),
        "mic" => Some(include_str!("../../../../assets/icons/settings/sf-mic.svg")),
        _ => None,
    };
    let markup = if let Some(source) = source {
        source.replace("#ffffff", &format!("#{color:06x}"))
    } else {
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" color="#{color:06x}"><g fill="none" stroke="#{color:06x}" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">{paths}</g></svg>"##
        )
    };
    type IconCache = std::collections::HashMap<(String, u32), std::sync::Arc<Image>>;
    static CACHE: std::sync::OnceLock<std::sync::Mutex<IconCache>> = std::sync::OnceLock::new();
    let image = CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .entry((kind.to_string(), color))
        .or_insert_with(|| {
            std::sync::Arc::new(Image::from_bytes(ImageFormat::Svg, markup.into_bytes()))
        })
        .clone();
    img(image).size(px(size)).into_any_element()
}
pub(super) fn appearance_choice(
    id: impl Into<ElementId>,
    dark: bool,
    selected: bool,
    p: Palette,
) -> swiftui_gpui::Button {
    swiftui_gpui::Button::new(id)
        .cursor_pointer()
        .text_color(rgb(p.text))
        .focus_visible(move |s| s.rounded(px(8.)).bg(rgb(p.hover)))
        .w(px(100.))
        .p(px(0.))
        .border_0()
        .bg(transparent_black())
        .flex()
        .flex_col()
        .gap(px(6.))
        .accessibility_label(if dark {
            "Dark appearance"
        } else {
            "Light appearance"
        })
        .child(
            div()
                .w_full()
                .h(px(70.))
                .p(px(3.))
                .rounded(px(8.))
                .border_2()
                .border_color(if selected {
                    rgb(p.accent).into()
                } else {
                    transparent_black()
                })
                .child(
                    super::reference::appearance(dark)
                        .size_full()
                        .object_fit(ObjectFit::Contain),
                ),
        )
        .child(
            div()
                .text_size(px(12.))
                .font_weight(if selected {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                })
                .child(if dark { "Dark" } else { "Light" }),
        )
}
pub(super) fn section(title: &str, p: Palette) -> Div {
    div()
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(p.muted))
        .px(px(12.))
        .mt(px(9.))
        .mb(px(2.))
        .child(title.to_owned())
}
pub(super) fn stack() -> Div {
    div().flex().flex_col().gap(px(18.))
}
pub(super) fn hero(page: Page, detail: &str, p: Palette) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .pt(px(3.))
        .pb(px(6.))
        .child(super::sidebar::icon(page, 34.))
        .child(caption(detail.to_owned(), p).max_w(px(470.)))
}
pub(super) fn status(label: &str, good: bool, p: Palette) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .child(
            div()
                .size(px(7.))
                .rounded_full()
                .bg(rgb(if good { 0x32b957 } else { p.muted })),
        )
        .child(caption(label.to_owned(), p))
}
pub(super) fn value_row(title: &str, value: impl Into<SharedString>, p: Palette) -> Div {
    settings_row(title.to_owned(), p).child(caption(value, p).max_w(px(330.)))
}
pub(super) fn unavailable<T>(state: &State<T>, title: &str, p: Palette) -> AnyElement {
    settings_group(p)
        .p(px(24.))
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_owned()),
        )
        .child(caption(
            state
                .error
                .as_ref()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Reading system settings…".into()),
            p,
        ))
        .into_any_element()
}
impl SettingsView {
    pub(super) fn system_state(&self, cx: &App) -> crate::system_backend::Snapshot {
        self.store.read(cx).system_snapshot.clone()
    }
    pub(super) fn apply_system(&mut self, command: SystemCommand, cx: &mut Context<Self>) {
        if let Err(e) = self.store.read(cx).system.apply(command) {
            self.message = Some(e.to_string());
        }
        let next = self.store.read(cx).system.snapshot();
        self.store.update(cx, |s, cx| {
            s.system_snapshot = next;
            cx.notify();
        });
        cx.notify();
    }
    pub(super) fn edit_system(
        &mut self,
        field: EditField,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.form = Some(field);
        self.form_input.update(cx, |s, cx| {
            s.set_value(value, window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }
    pub(super) fn form_sheet(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let field = self.form.unwrap();
        let (title, detail) = match field {
            EditField::Timezone => (
                "Time zone",
                "Enter an IANA time zone, such as Asia/Shanghai or Europe/Berlin.",
            ),
        };
        let valid = !self.form_input.read(cx).value().trim().is_empty();
        div()
            .track_focus(&self.sheet_focus)
            .absolute()
            .inset_0()
            .bg(rgba(0x00000045))
            .flex()
            .items_center()
            .justify_center()
            .child(
                panel(p)
                    .w(px(370.))
                    .p(px(22.))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(caption(detail, p))
                    .child(text_field("system-edit-field", &self.form_input, p, cx))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                button("cancel-system-edit", "Cancel", p).on_click(cx.listener(
                                    |s, _, _, cx| {
                                        s.form = None;
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                button("save-system-edit", "Save", p)
                                    .bg(rgb(p.accent))
                                    .text_color(rgb(p.accent_text))
                                    .disabled(!valid || self.system_state(cx).busy)
                                    .on_click(cx.listener(move |s, _, _, cx| {
                                        let value =
                                            s.form_input.read(cx).value().trim().to_string();
                                        let command = match field {
                                            EditField::Timezone => {
                                                SystemCommand::SetTimezone(value)
                                            }
                                        };
                                        s.form = None;
                                        s.apply_system(command, cx);
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
    pub(super) fn slider(
        &mut self,
        key: String,
        value: f64,
        target: SliderTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let busy = self.system_state(cx).busy;
        let dock = matches!(target, SliderTarget::DockSize);
        let pointer = matches!(target, SliderTarget::Pointer(_, _));
        let unsupported = matches!(target, SliderTarget::Pointer(_, false));
        let accessible_label = match &target {
            SliderTarget::Pointer(setting, _) => match setting {
                macstill_system::pointer::PointerSetting::MouseSpeed
                | macstill_system::pointer::PointerSetting::TrackpadSpeed => {
                    "Tracking speed".to_string()
                }
                macstill_system::pointer::PointerSetting::MouseDoubleClick => {
                    "Double-click speed".to_string()
                }
                macstill_system::pointer::PointerSetting::MouseCursorSize => {
                    "Pointer size".to_string()
                }
                _ => "Scrolling speed".to_string(),
            },
            _ => key.clone(),
        };
        let min = if dock { 36. } else { 0. };
        let max = if dock { 72. } else { 100. };
        if !self.sliders.contains_key(&key) {
            let state = cx.new(|_| {
                SliderState::new()
                    .min(min)
                    .max(max)
                    .default_value(value as f32)
            });
            let id = key.clone();
            let sub = cx.subscribe(&state, move |s, _, event: &SliderEvent, cx| {
                match event {
                    SliderEvent::Change(_) => {
                        if let Some(b) = s.sliders.get_mut(&id) {
                            b.dragging = true;
                        }
                    }
                    SliderEvent::Release(v) => {
                        if let Some(b) = s.sliders.get_mut(&id) {
                            b.dragging = false;
                        }
                        let percent = v.end() as f64;
                        match &target {
                            SliderTarget::Pointer(setting, true) => {
                                use macstill_system::pointer::{
                                    PointerCommand, PointerSetting, PointerValue,
                                };
                                let value = match setting {
                                    PointerSetting::MouseSpeed | PointerSetting::TrackpadSpeed => {
                                        percent / 50. - 1.
                                    }
                                    PointerSetting::MouseCursorSize => {
                                        (24. + percent * 72. / 100.).round()
                                    }
                                    PointerSetting::MouseDoubleClick => {
                                        (1000. - percent * 9.).round()
                                    }
                                    _ => 0.1 + percent * 4.9 / 100.,
                                };
                                s.apply_system(
                                    SystemCommand::SetPointer(PointerCommand {
                                        setting: *setting,
                                        value: PointerValue::Number(value),
                                    }),
                                    cx,
                                );
                            }
                            SliderTarget::Pointer(_, false) => {}
                            SliderTarget::Volume(direction, device) => s.apply_system(
                                SystemCommand::SetVolume {
                                    direction: *direction,
                                    id: *device,
                                    percent,
                                },
                                cx,
                            ),
                            SliderTarget::Brightness(name) => s.apply_system(
                                SystemCommand::SetBrightness {
                                    name: name.clone(),
                                    percent,
                                },
                                cx,
                            ),
                            SliderTarget::DockSize => {
                                s.edit(cx, |p| p.dock_size = percent.round() as u16)
                            }
                        }
                    }
                }
                cx.notify();
            });
            self.sliders.insert(
                key.clone(),
                SliderBinding {
                    state,
                    dragging: false,
                    _subscription: sub,
                },
            );
        }
        let binding = &self.sliders[&key];
        let state = binding.state.clone();
        if !binding.dragging && !busy && (state.read(cx).value().end() - value as f32).abs() > 0.5 {
            state.update(cx, |s, cx| s.set_value(value as f32, window, cx));
        }
        let v = state.read(cx).value().end();
        let fraction = ((v - min) / (max - min)).clamp(0., 1.);
        let disabled = (busy && !dock) || unsupported;
        div()
            .id(eid("slider-control", &key))
            .flex()
            .items_center()
            .gap(px(12.))
            .when(dock, |d| d.w_full())
            .child(
                Slider::new(&state)
                    .accessibility_label(accessible_label)
                    .disabled(disabled)
                    .when(dock, |s| s.w_full())
                    .when(!dock, |s| s.w(px(if pointer { 236. } else { 174. })))
                    .h(px(24.))
                    .opacity(if disabled { 0.5 } else { 1. })
                    .child(
                        SliderTrack::new(&state)
                            .disabled(disabled)
                            .relative()
                            .w_full()
                            .h_full()
                            .flex()
                            .items_center()
                            .child(
                                SliderIndicator::new(&state)
                                    .relative()
                                    .w_full()
                                    .h(px(4.))
                                    .rounded_full()
                                    .bg(rgb(p.line))
                                    .child(
                                        div()
                                            .absolute()
                                            .left_0()
                                            .top_0()
                                            .h_full()
                                            .w(relative(fraction))
                                            .rounded_full()
                                            .bg(rgb(p.accent)),
                                    ),
                            )
                            .child(
                                SliderThumb::new(&state)
                                    .disabled(disabled)
                                    .absolute()
                                    .left(relative(fraction))
                                    .ml(px(-8.))
                                    .top(px(4.))
                                    .size(px(16.))
                                    .rounded_full()
                                    .bg(rgb(0xffffff))
                                    .border_1()
                                    .border_color(rgb(0xd5d5da))
                                    .shadow_sm(),
                            ),
                    ),
            )
            .when(!pointer && !dock, |d| {
                d.child(
                    caption(format!("{v:.0}{}", if dock { " px" } else { "%" }), p)
                        .w(px(42.))
                        .text_right(),
                )
            })
            .into_any_element()
    }
}
