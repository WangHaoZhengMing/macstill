use super::*;
impl DesktopView {
    pub(super) fn cc_icon(name: &str, size: f32, color: u32, _dark: bool) -> AnyElement {
        let png: Option<&[u8]> = match name {
            "bluetooth" => Some(include_bytes!(
                "../../../../assets/icons/control-center/bluetooth.png"
            )),
            "airdrop" => Some(include_bytes!(
                "../../../../assets/icons/control-center/airdrop.png"
            )),
            "stage" => Some(include_bytes!(
                "../../../../assets/icons/control-center/stage.png"
            )),
            _ => None,
        };
        if let Some(bytes) = png {
            let mut pixels = image::load_from_memory(bytes)
                .expect("bundled control center PNG must be valid")
                .into_rgba8();
            // Use the PNG's alpha as a mask so theme and active-state tints retain smooth edges.
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
                .expect("control center PNG encoding must succeed");
            return img(std::sync::Arc::new(Image::from_bytes(
                ImageFormat::Png,
                png.into_inner(),
            )))
            .size(px(size))
            .object_fit(ObjectFit::Contain)
            .into_any_element();
        }
        let source = match name {
            "sun" => include_str!("../../../../assets/icons/control-center/cc_sun.svg"),
            "speaker" => include_str!("../../../../assets/icons/control-center/cc_speaker.svg"),
            "muted" => include_str!("../../../../assets/icons/control-center/cc_muted.svg"),
            "output" => include_str!("../../../../assets/icons/control-center/cc_output.svg"),
            "wifi" => include_str!("../../../../assets/icons/control-center/cc_wifi.svg"),
            "focus" => include_str!("../../../../assets/icons/control-center/cc_focus.svg"),
            "mirror" => include_str!("../../../../assets/icons/control-center/cc_mirror.svg"),
            "music" => include_str!("../../../../assets/icons/control-center/cc_music.svg"),
            "previous" => include_str!("../../../../assets/icons/control-center/cc_previous.svg"),
            "play" => include_str!("../../../../assets/icons/control-center/cc_play.svg"),
            "pause" => include_str!("../../../../assets/icons/control-center/cc_pause.svg"),
            "next" => include_str!("../../../../assets/icons/control-center/cc_next.svg"),
            _ => unreachable!("unknown control center symbol"),
        };
        let svg = source.replace("#202124", &format!("#{color:06x}"));
        img(std::sync::Arc::new(Image::from_bytes(
            ImageFormat::Svg,
            svg.into_bytes(),
        )))
        .size(px(size))
        .object_fit(ObjectFit::Contain)
        .into_any_element()
    }
    pub(in crate::ui) fn control_center_view(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(bounds) = super::displays::window_bounds(window, cx) {
            let wanted = bounds.size;
            if window.bounds().size != wanted {
                window.resize(wanted);
            }
        }
        let p = Palette::new(self.snapshot(cx).preferences.dark);
        let state = &self.control_center.status;
        let wifi = state.wifi;
        let bluetooth = state.bluetooth;
        let notifications = &self.store.read(cx).notification_snapshot;
        let focus = notifications.available.then_some(notifications.focus);
        let connection = |id, icon, title: &'static str, detail: String, active: Option<bool>| {
            gpui_kit::base::Button::new(id)
                .w_full()
                .h(px(40.))
                .px(px(7.))
                .flex()
                .justify_start()
                .items_center()
                .gap(px(7.))
                .disabled(active.is_none() || self.control_center.busy)
                .cursor_pointer()
                .child(
                    div()
                        .size(px(28.))
                        .rounded_full()
                        .bg(rgb(if active == Some(true) {
                            if icon == "focus" { 0x5856d6 } else { 0x007aff }
                        } else {
                            p.hover
                        }))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Self::cc_icon(
                            icon,
                            18.,
                            if active == Some(true) {
                                0xffffff
                            } else {
                                p.muted
                            },
                            p.dark,
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .text_size(px(11.))
                        .child(title)
                        .child(
                            div()
                                .text_size(px(9.))
                                .text_color(rgb(p.muted))
                                .child(detail),
                        ),
                )
        };
        let on_off = |s: Option<bool>| {
            match s {
                Some(true) => "On",
                Some(false) => "Off",
                None if self.control_center.loading => "Checking…",
                None => "Unavailable",
            }
            .to_string()
        };
        let connectivity = control_center_tile(p)
            .flex_1()
            .min_w_0()
            .h(px(132.))
            .py(px(6.))
            .child(
                connection(
                    "cc-wifi",
                    "wifi",
                    "Wi-Fi",
                    if wifi == Some(true) && !state.ssid.is_empty() {
                        state.ssid.clone()
                    } else {
                        on_off(wifi)
                    },
                    wifi,
                )
                .on_click(cx.listener(move |s, _, _, cx| {
                    s.cc_action(ControlAction::Wifi(wifi != Some(true)), cx)
                })),
            )
            .child(
                connection(
                    "cc-bluetooth",
                    "bluetooth",
                    "Bluetooth",
                    on_off(bluetooth),
                    bluetooth,
                )
                .on_click(cx.listener(move |s, _, _, cx| {
                    s.cc_action(ControlAction::Bluetooth(bluetooth != Some(true)), cx)
                })),
            )
            .child(
                connection(
                    "cc-airdrop",
                    "airdrop",
                    "AirDrop",
                    state.airdrop.detail(),
                    Some(state.airdrop.enabled()),
                )
                .disabled(state.airdrop.busy() || self.control_center.busy)
                .on_click(cx.listener(|s, _, _, cx| s.toggle_airdrop(cx)))
                .on_mouse_down(MouseButton::Right, |_, _, _| {
                    let _ = macstill_airdrop::open_app();
                }),
            );
        let shortcuts = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                control_center_tile(p).child(
                    connection("cc-focus", "focus", "Focus", on_off(focus), focus)
                        .disabled(focus.is_none())
                        .h(px(64.))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.store
                                .read(cx)
                                .notifications
                                .send(crate::notifications::Command::Focus(focus != Some(true)));
                        })),
                ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        control_center_tile(p).flex_1().child(
                            gpui_kit::base::Button::new("cc-stage")
                                .accessibility_label("Stage Manager")
                                .disabled(
                                    !self.snapshot(cx).stage.available
                                        || self.snapshot(cx).stage.busy,
                                )
                                .w_full()
                                .h(px(60.))
                                .flex()
                                .flex_col()
                                .gap(px(4.))
                                .rounded(px(10.))
                                .when(self.snapshot(cx).stage.enabled, |button| {
                                    button.bg(rgb(p.accent)).text_color(rgb(p.accent_text))
                                })
                                .child(Self::cc_icon(
                                    "stage",
                                    20.,
                                    if self.snapshot(cx).stage.enabled {
                                        p.accent_text
                                    } else {
                                        p.text
                                    },
                                    p.dark,
                                ))
                                .child(div().text_size(px(9.)).text_center().child("Stage Manager"))
                                .on_click(cx.listener(|s, _, _, cx| {
                                    s.stage_command(crate::hyprland::stage::Command::Toggle, cx)
                                })),
                        ),
                    )
                    .child(
                        control_center_tile(p).flex_1().child(
                            gpui_kit::base::Button::new("cc-mirror")
                                .w_full()
                                .h(px(60.))
                                .flex()
                                .flex_col()
                                .gap(px(4.))
                                .cursor_pointer()
                                .child(Self::cc_icon("mirror", 20., p.text, p.dark))
                                .child(
                                    div()
                                        .text_size(px(9.))
                                        .text_center()
                                        .child("Screen Mirroring"),
                                )
                                .on_click(cx.listener(|s, _, _, cx| {
                                    if let Err(error) = open_settings(&["--page", "displays"], cx) {
                                        s.message = Some(error.to_string());
                                    }
                                })),
                        ),
                    ),
            );
        let panel = control_center_panel(p)
            .id("cc-panel")
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(connectivity)
                    .child(shortcuts),
            )
            .child(control_center_slider_content(
                "Display",
                &self.control_center.brightness,
                state.brightness.is_some(),
                p,
                cx,
                Some(Self::cc_icon("sun", 13., 0x8e8e98, p.dark)),
                None,
            ))
            .child(control_center_slider_content(
                "Sound",
                &self.control_center.volume,
                state.volume.is_some(),
                p,
                cx,
                Some(Self::cc_icon(
                    if state.volume.unwrap_or(0.) < 1. {
                        "muted"
                    } else {
                        "speaker"
                    },
                    13.,
                    0x8e8e98,
                    p.dark,
                )),
                Some(
                    gpui_kit::base::Button::new("cc-audio-output")
                        .accessibility_label("Audio output settings")
                        .size(px(28.))
                        .rounded_full()
                        .bg(if p.dark {
                            rgba(0xffffff18)
                        } else {
                            rgba(0x0000000c)
                        })
                        .child(Self::cc_icon("output", 18., p.text, p.dark))
                        .on_click(|_, w, cx| {
                            let _ = open_settings(&["--page", "sound"], cx);
                            w.remove_window();
                        })
                        .into_any_element(),
                ),
            ))
            .when(
                self.snapshot(cx)
                    .preferences
                    .control_center
                    .battery_in_control_center,
                |panel| {
                    let level = self.snapshot(cx).battery;
                    panel.child(
                        control_center_tile(p).child(
                            gpui_kit::base::Button::new("cc-battery")
                                .accessibility_label("Battery settings")
                                .w_full()
                                .h(px(42.))
                                .px(px(10.))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(div().flex_1().child("Battery"))
                                .when_some(level, |button, level| {
                                    button
                                        .child(Self::battery_icon(
                                            level,
                                            self.snapshot(cx).battery_charging,
                                            p.text,
                                        ))
                                        .child(format!("{level}%"))
                                })
                                .when(level.is_none(), |button| button.child("No battery"))
                                .on_click(|_, window, cx| {
                                    let _ = open_settings(&["--page", "battery"], cx);
                                    window.remove_window();
                                }),
                        ),
                    )
                },
            )
            .child(self.media_card(p, cx))
            .when_some(self.message.clone(), |d, message| {
                d.child(div().text_size(px(10.)).child(message))
            })
            .when_some(self.snapshot(cx).stage.error, |d, message| {
                d.child(div().text_size(px(10.)).child(message))
            })
            .child(
                gpui_kit::base::Button::new("cc-settings")
                    .w_full()
                    .h(px(24.))
                    .text_size(px(11.))
                    .cursor_pointer()
                    .child("System Settings…")
                    .on_click(|_, w, cx| {
                        let _ = open_settings(&[], cx);
                        w.remove_window();
                    }),
            );
        div()
            .relative()
            .size_full()
            .font_family(cx.global::<Fonts>().text.clone())
            .track_focus(&self.control_center.focus)
            .on_key_down(|event, w, _| {
                if event.keystroke.key == "escape" {
                    w.remove_window();
                }
            })
            .child(
                div()
                    .id("cc-dismiss")
                    .absolute()
                    .inset_0()
                    .on_mouse_down(MouseButton::Left, |_, w, _| w.remove_window()),
            )
            .child(
                div()
                    .absolute()
                    .top(px(34.))
                    .right(px(12.))
                    .w(px(315.))
                    .child(panel),
            )
            .into_any_element()
    }
}
