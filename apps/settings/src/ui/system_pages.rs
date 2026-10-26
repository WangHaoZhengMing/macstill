use super::widgets::*;
use super::*;
use macstill_system::{AudioDirection, SystemCommand};
use swiftui_gpui::{settings_divider, settings_group, settings_row};

impl SettingsView {
    pub(super) fn general(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let mut content = stack();
        // Keep the macOS General grouping while making unavailable destinations explicit.
        for entries in [
            vec![
                ("About", "display", Some(Page::About)),
                ("Software Update", "gear", None),
                ("Storage", "apps", Some(Page::Storage)),
                ("AirDrop", "network", Some(Page::AirDrop)),
                ("Login Items", "shortcut", Some(Page::LoginItems)),
            ],
            vec![("AppleCare & Warranty", "info", None)],
            vec![
                ("Language & Region", "network", Some(Page::LanguageRegion)),
                ("Date & Time", "clock", Some(Page::DateTime)),
            ],
            vec![
                ("Sharing", "shortcut", None),
                ("Time Machine", "clock", None),
                ("Transfer or Reset", "gear", None),
                ("Startup Disk", "apps", None),
            ],
        ] {
            let mut group = settings_group(p).overflow_hidden();
            for (index, (label, kind, destination)) in entries.into_iter().enumerate() {
                if index > 0 {
                    group = group.child(settings_divider(p));
                }
                let available = destination.is_some();
                let icon_color = if matches!(kind, "network" | "clock") && available {
                    0x007aff
                } else {
                    0x929498
                };
                let id = destination.map_or(label, Page::title);
                group = group.child(
                    button(eid("general-open", id), "", p)
                        .w_full()
                        .min_h(px(42.))
                        .px(px(12.))
                        .py(px(9.))
                        .border_0()
                        .rounded(px(0.))
                        .bg(transparent_black())
                        .flex()
                        .gap(px(12.))
                        .items_center()
                        .justify_start()
                        .disabled(!available)
                        .accessibility_label(if available {
                            format!("Open {label}")
                        } else {
                            format!("{label}, unavailable on Macstill")
                        })
                        .child(if let Some(icon) = super::reference::general(label) {
                            icon.size(px(24.))
                                .object_fit(ObjectFit::Contain)
                                .into_any_element()
                        } else {
                            div()
                                .size(px(24.))
                                .flex_shrink_0()
                                .rounded(px(6.))
                                .bg(rgb(icon_color))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(symbol(kind, 18., 0xffffff))
                                .into_any_element()
                        })
                        .child(div().flex_1().text_size(px(13.)).text_left().child(label))
                        .when(!available, |row| row.child(caption("Unavailable", p)))
                        .child(div().text_size(px(20.)).text_color(rgb(p.muted)).child("›"))
                        .on_click(cx.listener(move |s, _, w, cx| {
                            if let Some(page) = destination {
                                s.set_page(page, w, cx);
                            }
                        })),
                );
            }
            content = content.child(group);
        }
        content.into_any_element()
    }
    pub(super) fn network(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let state = self.system_state(cx);
        let Some(network) = state.system.network.value else {
            return unavailable(&state.system.network, "Network is unavailable", p);
        };
        let mut content = stack()
            .child(hero(Page::Network, "Manage your network connections.", p))
            .child(
                settings_group(p).child(
                    settings_row("Networking", p).child(
                        toggle("network-enabled", network.enabled, p)
                            .accessibility_label("Networking")
                            .disabled(state.busy)
                            .on_change(cx.listener(|s, v: &bool, _, cx| {
                                s.apply_system(SystemCommand::SetNetworking(*v), cx)
                            })),
                    ),
                ),
            )
            .child(section("Connections", p));
        let mut list = settings_group(p);
        for device in &network.devices {
            if device.kind == 32 {
                continue;
            } // Loopback is not a user network service.
            let title = format!(
                "{} · {}",
                match device.kind {
                    1 => "Ethernet",
                    2 => "Wi-Fi",
                    8 => "Mobile",
                    13 => "Bridge",
                    _ => "Interface",
                },
                device.interface
            );
            let mut row = settings_row(title, p).child(status(
                connection_status(device.state),
                device.state == 100,
                p,
            ));
            if let Some(active) = &device.active_connection {
                let active = active.clone();
                row = row.child(
                    button(eid("network-disconnect", &device.path), "Disconnect", p)
                        .disabled(state.busy)
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.apply_system(
                                SystemCommand::DeactivateConnection {
                                    active_connection: active.clone(),
                                },
                                cx,
                            )
                        })),
                );
            }
            list = list.child(row).child(settings_divider(p));
        }
        content = content.child(list).child(
            caption(
                match network.connectivity {
                    4 => "Connected to the internet.",
                    3 => "Connected to a local network with limited internet access.",
                    2 => "This network may require signing in.",
                    _ => "Internet connectivity is not confirmed.",
                },
                p,
            )
            .px(px(12.)),
        );
        if !network.saved_connections.is_empty() {
            let mut profiles = settings_group(p);
            for profile in network.saved_connections {
                let path = profile.path.clone();
                profiles = profiles
                    .child(
                        settings_row(profile.name, p).child(
                            button(eid("network-details", &profile.path), "Details…", p)
                                .disabled(state.busy)
                                .on_click(cx.listener(move |s, _, _, cx| {
                                    s.open_connection(path.clone(), cx)
                                })),
                        ),
                    )
                    .child(settings_divider(p));
            }
            content = content
                .child(section("Saved Connections", p))
                .child(profiles);
        }
        content.into_any_element()
    }
    pub(super) fn bluetooth(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let state = self.system_state(cx);
        let Some(bluetooth) = state.system.bluetooth.value else {
            return unavailable(&state.system.bluetooth, "Bluetooth is unavailable", p);
        };
        let mut content = stack().child(hero(
            Page::Bluetooth,
            "Connect a keyboard, headphones and other accessories.",
            p,
        ));
        if bluetooth.adapters.is_empty() {
            return content
                .child(
                    settings_group(p)
                        .p(px(20.))
                        .child(caption("No Bluetooth adapter was found.", p)),
                )
                .into_any_element();
        }
        for adapter in &bluetooth.adapters {
            let path = adapter.path.clone();
            let scan_path = path.clone();
            let scanning = state.discovery.contains(&path);
            let details_path = path.clone();
            content = content.child(
                settings_group(p)
                    .child(
                        settings_row(
                            if bluetooth.adapters.len() == 1 {
                                "Bluetooth".to_string()
                            } else {
                                adapter.name.clone()
                            },
                            p,
                        )
                        .child(
                            button(
                                eid("bluetooth-adapter-details", &details_path),
                                "Details…",
                                p,
                            )
                            .disabled(state.busy)
                            .on_click(cx.listener(
                                move |s, _, _, cx| {
                                    s.bluetooth_details =
                                        Some(super::bluetooth_details::BluetoothTarget::Adapter(
                                            details_path.clone(),
                                        ));
                                    cx.notify();
                                },
                            )),
                        )
                        .child(caption(if adapter.powered { "On" } else { "Off" }, p))
                        .child(
                            toggle(eid("bluetooth-powered", &path), adapter.powered, p)
                                .accessibility_label(format!("Bluetooth {}", adapter.name))
                                .disabled(state.busy)
                                .on_change(cx.listener(move |s, v: &bool, _, cx| {
                                    s.apply_system(
                                        SystemCommand::SetBluetoothPowered {
                                            adapter: path.clone(),
                                            powered: *v,
                                        },
                                        cx,
                                    )
                                })),
                        ),
                    )
                    .child(settings_divider(p))
                    .child(
                        settings_row("Nearby devices", p)
                            .child(caption(if scanning { "Searching…" } else { "" }, p))
                            .child(
                                button(
                                    eid("bluetooth-scan", &scan_path),
                                    if scanning {
                                        "Stop searching"
                                    } else {
                                        "Find devices"
                                    },
                                    p,
                                )
                                .disabled(state.busy || !adapter.powered)
                                .on_click(cx.listener(
                                    move |s, _, _, cx| {
                                        s.apply_system(
                                            if scanning {
                                                SystemCommand::StopBluetoothDiscovery {
                                                    adapter: scan_path.clone(),
                                                }
                                            } else {
                                                SystemCommand::StartBluetoothDiscovery {
                                                    adapter: scan_path.clone(),
                                                }
                                            },
                                            cx,
                                        )
                                    },
                                )),
                            ),
                    ),
            );
        }
        for (paired, title) in [(true, "My Devices"), (false, "Nearby Devices")] {
            content = content.child(section(title, p));
            let mut list = settings_group(p);
            let mut devices: Vec<_> = bluetooth
                .devices
                .iter()
                .filter(|d| d.paired == paired)
                .collect();
            devices.sort_by_key(|d| (!d.connected, d.name.clone()));
            if devices.is_empty() {
                list = list.p(px(16.)).child(caption(
                    if paired {
                        "No paired devices."
                    } else {
                        "Use Find devices to discover Bluetooth accessories."
                    },
                    p,
                ));
            }
            for device in devices {
                let path = device.path.clone();
                let details_path = path.clone();
                let trusted_path = path.clone();
                let connected = device.connected;
                let powered = bluetooth
                    .adapters
                    .iter()
                    .any(|a| a.path == device.adapter && a.powered);
                let label = if connected {
                    "Disconnect"
                } else if paired {
                    "Connect"
                } else {
                    "Pair…"
                };
                list = list
                    .child(
                        div()
                            .px(px(12.))
                            .py(px(10.))
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(symbol("bluetooth", 22., p.muted))
                            .child(Self::row(
                                device.name.clone(),
                                if connected {
                                    "Connected"
                                } else if paired {
                                    "Not connected"
                                } else {
                                    "Available"
                                },
                                p,
                            ))
                            .child(
                                button(eid("bluetooth-device-details", &details_path), "ⓘ", p)
                                    .accessibility_label(format!("Details for {}", device.name))
                                    .disabled(state.busy)
                                    .on_click(cx.listener(move |s, _, _, cx| {
                                        s.bluetooth_details = Some(
                                            super::bluetooth_details::BluetoothTarget::Device(
                                                details_path.clone(),
                                            ),
                                        );
                                        cx.notify();
                                    })),
                            )
                            .child(
                                button(eid("bluetooth-device", &path), label, p)
                                    .disabled(state.busy || !powered)
                                    .on_click(cx.listener(move |s, _, _, cx| {
                                        s.apply_system(
                                            if connected {
                                                SystemCommand::DisconnectBluetooth {
                                                    device: path.clone(),
                                                }
                                            } else if paired {
                                                SystemCommand::ConnectBluetooth {
                                                    device: path.clone(),
                                                }
                                            } else {
                                                SystemCommand::PairBluetooth {
                                                    device: path.clone(),
                                                }
                                            },
                                            cx,
                                        )
                                    })),
                            ),
                    )
                    .when(paired, |list| {
                        list.child(
                            settings_row("Trusted accessory", p).child(
                                toggle(eid("bluetooth-trusted", &trusted_path), device.trusted, p)
                                    .disabled(state.busy)
                                    .accessibility_label(format!("Trust {}", device.name))
                                    .on_change(cx.listener(move |s, v: &bool, _, cx| {
                                        s.apply_system(
                                            SystemCommand::SetBluetoothTrusted {
                                                device: trusted_path.clone(),
                                                trusted: *v,
                                            },
                                            cx,
                                        )
                                    })),
                            ),
                        )
                    })
                    .child(settings_divider(p));
            }
            content = content.child(list);
        }
        if state.pairing.is_some() {
            content = content.child(
                button(
                    "cancel-pairing",
                    if state.cancelling_pairing {
                        "Cancelling…"
                    } else {
                        "Cancel Pairing"
                    },
                    p,
                )
                .disabled(state.cancelling_pairing)
                .on_click(cx.listener(|s, _, _, cx| {
                    if let Err(error) = s.store.read(cx).system.cancel_pairing() {
                        s.message = Some(error.to_string());
                    }
                    s.sync_system(cx);
                })),
            );
        }
        content.child(caption("If pairing needs a PIN or confirmation, use the system pairing dialog. Searching stops when you leave this page.", p).px(px(12.))).into_any_element()
    }
    pub(super) fn sound(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let state = self.system_state(cx);
        let Some(audio) = state.system.audio.value else {
            return unavailable(&state.system.audio, "Sound is unavailable", p);
        };
        let mut effects = settings_group(p)
            .child(value_row("Alert sound", "Unavailable", p))
            .child(settings_divider(p))
            .child(value_row("Play sound effects through", "System default", p))
            .child(settings_divider(p))
            .child(value_row("Alert volume", "Unavailable", p));
        for (id, label) in [
            ("sound-startup", "Play sound on startup"),
            ("sound-interface", "Play user interface sound effects"),
            ("sound-feedback", "Play feedback when volume is changed"),
        ] {
            effects = effects.child(settings_divider(p)).child(
                settings_row(label, p).child(
                    toggle(id, false, p)
                        .disabled(true)
                        .accessibility_label(format!("{label}, unavailable")),
                ),
            );
        }
        let mut content = stack()
            .child(section("Sound Effects", p))
            .child(effects)
            .child(
                caption(
                    "Sound effect preferences are unavailable on this system.",
                    p,
                )
                .px(px(12.)),
            )
            .child(section("Output & Input", p));
        let mut tabs = div()
            .m(px(10.))
            .p(px(2.))
            .flex()
            .rounded(px(7.))
            .bg(rgb(if p.dark { 0x38383b } else { 0xe6e4e4 }));
        for (input, label) in [(false, "Output"), (true, "Input")] {
            let selected = self.sound_input == input;
            tabs = tabs.child(
                button(if input { "sound-input" } else { "sound-output" }, label, p)
                    .flex_1()
                    .py(px(4.))
                    .rounded(px(6.))
                    .border_0()
                    .bg(if selected {
                        rgb(p.surface).into()
                    } else {
                        transparent_black()
                    })
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.sound_input = input;
                        cx.notify();
                    })),
            );
        }
        let (direction, title, devices) = if self.sound_input {
            (AudioDirection::Input, "Input", audio.inputs)
        } else {
            (AudioDirection::Output, "Output", audio.outputs)
        };
        {
            let mut list = settings_group(p)
                .overflow_hidden()
                .child(tabs)
                .child(settings_divider(p))
                .child(
                    div()
                        .px(px(12.))
                        .py(px(6.))
                        .flex()
                        .justify_between()
                        .bg(rgb(if p.dark { 0x333335 } else { 0xf0f0f2 }))
                        .child(caption("Name", p))
                        .child(caption("Status", p)),
                );
            if devices.is_empty() {
                list = list.child(div().p(px(16.)).child(caption(
                    if self.sound_input {
                        "No input devices found."
                    } else {
                        "No output devices found."
                    },
                    p,
                )));
            }
            for device in &devices {
                let id = device.id;
                let selected = device.is_default;
                list = list.child(
                    button(eid("audio-select", &format!("{direction:?}:{id}")), "", p)
                        .w_full()
                        .px(px(12.))
                        .py(px(8.))
                        .border_0()
                        .rounded(px(0.))
                        .bg(if device.is_default {
                            rgb(if p.dark { 0x454548 } else { 0xdcdce0 }).into()
                        } else {
                            transparent_black()
                        })
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .disabled(state.busy)
                        .accessibility_label(format!("Select {} for {title}", device.description))
                        .child(
                            div()
                                .flex_1()
                                .text_left()
                                .text_size(px(13.))
                                .child(device.description.clone()),
                        )
                        .child(caption(
                            if device.is_default {
                                "Selected"
                            } else {
                                "Available"
                            },
                            p,
                        ))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            if !selected && !s.system_state(cx).busy {
                                s.apply_system(SystemCommand::SetDefaultAudio { direction, id }, cx)
                            }
                        })),
                );
            }
            content = content.child(list);
            if let Some(device) = devices.iter().find(|d| d.is_default) {
                let id = device.id;
                let slider = self.slider(
                    format!("{direction:?}-volume-{id}"),
                    device.percent,
                    SliderTarget::Volume(direction, id),
                    window,
                    cx,
                );
                content = content.child(
                    settings_group(p)
                        .child(
                            settings_row(
                                if direction == AudioDirection::Output {
                                    "Output volume"
                                } else {
                                    "Input volume"
                                },
                                p,
                            )
                            .child(slider),
                        )
                        .child(settings_divider(p))
                        .child(
                            settings_row("Mute", p).child(
                                toggle(
                                    eid("audio-mute", &format!("{direction:?}:{id}")),
                                    device.muted,
                                    p,
                                )
                                .accessibility_label(format!("Mute {title}"))
                                .disabled(state.busy)
                                .on_change(cx.listener(
                                    move |s, v: &bool, _, cx| {
                                        s.apply_system(
                                            SystemCommand::SetMute {
                                                direction,
                                                id,
                                                muted: *v,
                                            },
                                            cx,
                                        )
                                    },
                                )),
                            ),
                        ),
                );
            }
        }
        content.into_any_element()
    }
    pub(super) fn date_time(&self, cx: &mut Context<Self>) -> AnyElement {
        use super::editors::{Confirmation, EditorKind};
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let state = self.system_state(cx);
        let Some(time) = state.system.date_time.value else {
            return unavailable(&state.system.date_time, "Date & Time is unavailable", p);
        };
        let zone = time.timezone.clone();
        let date = time
            .unix_microseconds
            .and_then(|value| i64::try_from(value).ok())
            .and_then(chrono::DateTime::from_timestamp_micros)
            .unwrap_or_else(chrono::Utc::now)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        let manual = date.clone();
        let mut content = stack()
            .child(hero(
                Page::DateTime,
                "Keep your computer’s time accurate.",
                p,
            ))
            .child(
                settings_group(p)
                    .child(
                        settings_row("Set time and date automatically", p).child(
                            toggle("automatic-time", time.ntp, p)
                                .accessibility_label("Set time and date automatically")
                                .disabled(state.busy || !time.can_ntp)
                                .on_change(cx.listener(|s, v: &bool, _, cx| {
                                    s.apply_system(SystemCommand::SetNtp(*v), cx)
                                })),
                        ),
                    )
                    .child(settings_divider(p))
                    .child(value_row(
                        "Time synchronization",
                        if time.synchronized {
                            "Synchronized"
                        } else {
                            "Not synchronized"
                        },
                        p,
                    ))
                    .child(settings_divider(p))
                    .child(
                        settings_row("Date & time (UTC)", p)
                            .child(caption(date, p))
                            .child(
                                button("edit-manual-time", "Set…", p)
                                    .disabled(state.busy || time.ntp)
                                    .on_click(cx.listener(move |s, _, w, cx| {
                                        s.open_editor(
                                            EditorKind::Time,
                                            vec![(
                                                "Date & time (UTC)",
                                                "YYYY-MM-DD HH:MM:SS",
                                                manual.clone(),
                                            )],
                                            0,
                                            vec![],
                                            w,
                                            cx,
                                        )
                                    })),
                            ),
                    ),
            )
            .child(
                settings_group(p).child(
                    settings_row("Time zone", p)
                        .child(caption(time.timezone, p))
                        .child(
                            button("edit-timezone", "Set…", p)
                                .disabled(state.busy)
                                .on_click(cx.listener(move |s, _, w, cx| {
                                    s.edit_system(EditField::Timezone, &zone, w, cx)
                                })),
                        ),
                ),
            );
        if let Some(local) = time.local_rtc {
            content = content.child(
                settings_group(p).child(
                    settings_row("Hardware clock uses local time", p).child(
                        toggle("local-rtc", local, p)
                            .disabled(state.busy)
                            .on_change(cx.listener(|s, value: &bool, _, cx| {
                                s.confirmation = Some(Confirmation::LocalRtc(*value));
                                cx.notify();
                            })),
                    ),
                ),
            );
        }
        content.child(caption("Turn off automatic time before setting the date manually. The time zone controls how time is displayed.",p).px(px(12.))).into_any_element()
    }
    pub(super) fn system_about(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let state = self.system_state(cx);
        let mut content = stack().child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .pt(px(20.))
                .pb(px(20.))
                .gap(px(10.))
                .child(
                    super::reference::computer()
                        .size(px(88.))
                        .object_fit(ObjectFit::Contain),
                )
                .child(
                    div()
                        .text_size(px(28.))
                        .font_weight(FontWeight::BOLD)
                        .child("Macstill"),
                )
                .child(caption(format!("Version {}", env!("CARGO_PKG_VERSION")), p)),
        );
        if let Some(host) = state.system.host.value {
            content = content.child(
                settings_group(p)
                    .child(value_row(
                        "Name",
                        if host.pretty_hostname.is_empty() {
                            host.hostname
                        } else {
                            host.pretty_hostname
                        },
                        p,
                    ))
                    .child(settings_divider(p))
                    .child(value_row("Operating system", host.operating_system, p))
                    .child(settings_divider(p))
                    .child(value_row("Kernel", host.kernel, p))
                    .child(settings_divider(p))
                    .child(value_row("Desktop", "Macstill · Hyprland", p)),
            );
        } else {
            content = content.child(unavailable(&state.system.host, "Computer information", p));
        }
        content
            .child(caption("A familiar desktop. A more personal space.", p).text_center())
            .into_any_element()
    }
}
fn connection_status(state: u32) -> &'static str {
    match state {
        100 => "Connected",
        40..=90 => "Connecting…",
        110 => "Disconnecting…",
        120 => "Connection failed",
        10 => "Unmanaged",
        20 => "Unavailable",
        _ => "Not connected",
    }
}
