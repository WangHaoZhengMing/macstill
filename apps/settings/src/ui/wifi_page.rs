use super::editors::{Confirmation, overlay};
use super::widgets::{stack, symbol, unavailable};
use super::*;
use gpui_kit::base::Popover;
use macstill_system::{
    AccessPoint, IpConfiguration, IpFamily, NetworkDevice, SavedConnection, SystemCommand,
};
use swiftui_gpui::{Button, settings_divider, settings_group, settings_row, toggle};

pub(super) struct DetailsDraft {
    path: String,
    autoconnect: bool,
    pub(super) pending: bool,
    revision: u64,
    error: Option<String>,
}

fn group(p: Palette) -> Div {
    settings_group(p)
        .rounded(px(7.))
        .bg(rgb(if p.dark { 0x2d2d2f } else { 0xefeff0 }))
}

fn heading(title: &str, p: Palette) -> Div {
    div()
        .px(px(12.))
        .pt(px(9.))
        .text_size(px(12.))
        .text_color(rgb(p.muted))
        .font_weight(FontWeight::SEMIBOLD)
        .child(title.to_owned())
}

fn row_button(id: impl Into<ElementId>, label: String, p: Palette) -> Button {
    Button::new(id)
        .accessibility_label(label)
        .w_full()
        .min_h(px(38.))
        .px(px(12.))
        .py(px(8.))
        .flex()
        .items_center()
        .gap(px(12.))
        .text_color(rgb(p.text))
        .text_size(px(13.))
        .cursor_pointer()
        .hover(move |s| s.bg(rgb(p.hover)))
        .focus_visible(move |s| s.rounded(px(5.)).border_1().border_color(rgb(p.accent)))
        .styles(|s| s.disabled(|s| s.opacity(0.45).cursor_default()))
}

fn secured(ap: &AccessPoint) -> bool {
    ap.flags & 1 != 0 || ap.wpa_flags != 0 || ap.rsn_flags != 0
}

fn signal(strength: Option<u8>, p: Palette) -> AnyElement {
    let strength = strength.unwrap_or(0);
    let color = if strength > 0 { p.text } else { p.muted };
    let markup = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="16" viewBox="0 0 20 16"><g fill="none" stroke="#{color:06x}" stroke-width="2.2" stroke-linecap="round"><path d="M2 4a13 13 0 0 1 16 0" opacity="{}"/><path d="M5 7a8 8 0 0 1 10 0" opacity="{}"/><path d="M8 10a3 3 0 0 1 4 0" opacity="{}"/></g><circle cx="10" cy="13" r="1.3" fill="#{color:06x}"/></svg>"##,
        if strength >= 65 { "1" } else { ".25" },
        if strength >= 35 { "1" } else { ".25" },
        if strength > 0 { "1" } else { ".25" }
    );
    img(std::sync::Arc::new(Image::from_bytes(
        ImageFormat::Svg,
        markup.into_bytes(),
    )))
    .w(px(18.))
    .h(px(16.))
    .into_any_element()
}

impl SettingsView {
    fn wifi_profile_menu(
        &self,
        profile: &SavedConnection,
        radios: &[&NetworkDevice],
        busy: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut menu = div()
            .w(px(195.))
            .p(px(5.))
            .rounded(px(8.))
            .border_1()
            .border_color(rgb(p.line))
            .bg(rgb(p.surface))
            .shadow_lg()
            .flex()
            .flex_col();
        for radio in radios {
            let connection = profile.path.clone();
            let device = radio.path.clone();
            let label = if radios.len() > 1 {
                format!("Connect via {}", radio.interface)
            } else {
                "Connect".into()
            };
            menu = menu.child(
                row_button(
                    eid("wifi-connect", &format!("{}:{}", profile.path, radio.path)),
                    label.clone(),
                    p,
                )
                .disabled(busy)
                .child(label)
                .on_click(cx.listener(move |s, _, _, cx| {
                    s.apply_system(
                        SystemCommand::ActivateConnection {
                            connection: connection.clone(),
                            device: device.clone(),
                        },
                        cx,
                    )
                })),
            );
        }
        let path = profile.path.clone();
        menu = menu.child(
            row_button(
                eid("network-details", &profile.path),
                format!("Details for {}", profile.name),
                p,
            )
            .disabled(busy)
            .child("Network Settings…")
            .on_click(cx.listener(move |s, _, _, cx| s.open_connection(path.clone(), cx))),
        );
        let path = profile.path.clone();
        let name = profile.name.clone();
        menu = menu.child(settings_divider(p)).child(
            row_button(
                eid("wifi-forget", &profile.path),
                format!("Forget {}", profile.name),
                p,
            )
            .disabled(busy)
            .child("Forget This Network…")
            .on_click(cx.listener(move |s, _, _, cx| {
                s.confirmation = Some(Confirmation::ForgetNetwork {
                    path: path.clone(),
                    name: name.clone(),
                });
                cx.notify();
            })),
        );
        Popover::new(eid("wifi-menu", &profile.path))
            .trigger(
                button(eid("wifi-options", &profile.path), "⋯", p)
                    .size(px(22.))
                    .p(px(0.))
                    .rounded_full()
                    .bg(transparent_black())
                    .border_color(rgb(p.muted))
                    .accessibility_label(format!("Options for {}", profile.name))
                    .disabled(busy),
            )
            .content(move |_, _, cx| {
                menu.id("wifi-menu-content")
                    .on_click(cx.listener(|state, _, window, cx| state.dismiss(window, cx)))
            })
            .into_any_element()
    }

    pub(super) fn wifi(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let state = self.system_state(cx);
        let Some(network) = state.system.network.value else {
            return unavailable(&state.system.network, "Wi-Fi is unavailable", p);
        };
        let radios: Vec<_> = network.devices.iter().filter(|d| d.kind == 2).collect();
        let enabled = network.enabled && network.wifi_enabled && network.wifi_hardware_enabled;
        let busy = state.busy || state.loading;
        let mut top = group(p).child(
            div()
                .px(px(12.))
                .py(px(11.))
                .flex()
                .items_center()
                .gap(px(10.))
                .child(super::sidebar::icon(Page::Wifi, 28.))
                .child(div().flex_1().child("Wi-Fi"))
                .child(
                    toggle("wifi-switch", network.wifi_enabled, p)
                        .accessibility_label("Wi-Fi")
                        .disabled(
                            busy || !network.wifi_hardware_enabled
                                || !network.enabled
                                || radios.is_empty(),
                        )
                        .on_change(cx.listener(|s, value: &bool, _, cx| {
                            s.apply_system(SystemCommand::SetWifi(*value), cx)
                        })),
                ),
        );
        if enabled {
            for radio in &radios {
                if radio.state != 100 || radio.active_connection.is_none() {
                    continue;
                }
                let ap = radio
                    .access_points
                    .iter()
                    .find(|ap| radio.active_access_point.as_ref() == Some(&ap.path));
                let profile = network
                    .saved_connections
                    .iter()
                    .find(|c| radio.active_profile.as_ref() == Some(&c.path));
                let title = ap
                    .map(|ap| ap.ssid.clone())
                    .or_else(|| profile.map(|profile| profile.name.clone()))
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| radio.interface.clone());
                let mut row = div()
                    .px(px(12.))
                    .py(px(11.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(title)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.))
                                    .child(div().size(px(6.)).rounded_full().bg(rgb(0x34c759)))
                                    .child(
                                        caption(
                                            match network.connectivity {
                                                1 => "Connected · No Internet",
                                                2 => "Connected · Sign-in required",
                                                3 => "Connected · Limited Internet",
                                                _ => "Connected",
                                            },
                                            p,
                                        )
                                        .text_size(px(11.)),
                                    ),
                            ),
                    );
                if ap.is_some_and(secured) {
                    row = row.child(symbol("lock", 12., p.muted));
                }
                row = row.child(signal(ap.map(|ap| ap.strength), p));
                if let Some(profile) = profile {
                    let path = profile.path.clone();
                    row = row.child(
                        button(eid("wifi-current-details", &radio.path), "Details…", p)
                            .py(px(3.))
                            .disabled(busy)
                            .on_click(
                                cx.listener(move |s, _, _, cx| s.open_connection(path.clone(), cx)),
                            ),
                    );
                }
                let active = radio.active_connection.clone().unwrap();
                let disconnect =
                    row_button(eid("wifi-disconnect", &radio.path), "Disconnect".into(), p)
                        .disabled(busy)
                        .child("Disconnect")
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.apply_system(
                                SystemCommand::DeactivateConnection {
                                    active_connection: active.clone(),
                                },
                                cx,
                            )
                        }));
                row = row.child(
                    Popover::new(eid("wifi-current-menu", &radio.path))
                        .trigger(
                            button(eid("wifi-current-options", &radio.path), "⋯", p)
                                .size(px(22.))
                                .p(px(0.))
                                .rounded_full()
                                .bg(transparent_black())
                                .disabled(busy)
                                .accessibility_label("Connected network options"),
                        )
                        .content(move |_, _, cx| {
                            group(p)
                                .id("wifi-current-menu-content")
                                .w(px(160.))
                                .p(px(4.))
                                .shadow_lg()
                                .child(disconnect)
                                .on_click(
                                    cx.listener(|state, _, window, cx| state.dismiss(window, cx)),
                                )
                        }),
                );
                top = top.child(settings_divider(p)).child(row);
            }
        }
        let mut content = stack().pt(px(8.)).child(top);
        if !network.wifi_hardware_enabled {
            content = content.child(
                caption(
                    "Wi-Fi is disabled by a hardware switch or airplane mode.",
                    p,
                )
                .px(px(12.)),
            );
        } else if !network.enabled {
            content = content
                .child(caption("Turn on networking in Network to use Wi-Fi.", p).px(px(12.)));
        } else if radios.is_empty() {
            content = content.child(
                group(p)
                    .p(px(16.))
                    .child(caption("No Wi-Fi adapter was found.", p)),
            );
        } else if !network.wifi_enabled {
            content =
                content.child(caption("Turn on Wi-Fi to see available networks.", p).px(px(12.)));
        }
        if enabled && !radios.is_empty() {
            let profiles: Vec<_> = network
                .saved_connections
                .iter()
                .filter(|c| c.kind == "802-11-wireless")
                .filter(|c| {
                    !radios
                        .iter()
                        .any(|r| r.state == 100 && r.active_profile.as_ref() == Some(&c.path))
                })
                .collect();
            content = content.child(heading("Known Networks", p));
            let mut known = group(p);
            for (index, profile) in profiles.iter().enumerate() {
                if index > 0 {
                    known = known.child(settings_divider(p));
                }
                let ap = radios
                    .iter()
                    .flat_map(|r| &r.access_points)
                    .find(|ap| ap.ssid == profile.name);
                let mut row = settings_row(profile.name.clone(), p);
                if ap.is_some_and(secured) {
                    row = row.child(symbol("lock", 12., p.muted));
                }
                known = known.child(
                    row.child(signal(ap.map(|ap| ap.strength), p))
                        .child(self.wifi_profile_menu(profile, &radios, busy, p, cx)),
                );
            }
            if profiles.is_empty() {
                known = known.child(caption("No other saved networks.", p).p(px(12.)));
            }
            content = content.child(known);
            let mut other_heading = heading("Other Networks", p)
                .flex()
                .items_center()
                .gap(px(8.))
                .child(div().flex_1());
            for radio in &radios {
                let device = radio.path.clone();
                other_heading = other_heading.child(
                    button(eid("wifi-scan", &radio.path), "↻", p)
                        .accessibility_label(format!("Scan for networks on {}", radio.interface))
                        .border_0()
                        .p(px(0.))
                        .size(px(20.))
                        .bg(transparent_black())
                        .text_size(px(16.))
                        .disabled(busy)
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.apply_system(
                                SystemCommand::ScanWifi {
                                    device: device.clone(),
                                },
                                cx,
                            )
                        })),
                );
            }
            content = content.child(other_heading);
            let mut nearby: Vec<_> = radios
                .iter()
                .flat_map(|r| r.access_points.iter().map(move |ap| (r, ap)))
                .filter(|(_, ap)| !ap.ssid.is_empty())
                .filter(|(_, ap)| {
                    !radios
                        .iter()
                        .any(|r| r.state == 100 && r.active_access_point.as_ref() == Some(&ap.path))
                })
                .filter(|(_, ap)| {
                    !network
                        .saved_connections
                        .iter()
                        .any(|c| c.kind == "802-11-wireless" && c.name == ap.ssid)
                })
                .collect();
            nearby.sort_by_key(|(_, ap)| std::cmp::Reverse(ap.strength));
            let mut seen = std::collections::HashSet::new();
            let mut list = group(p);
            let mut count = 0;
            for (radio, ap) in nearby {
                if !seen.insert(ap.ssid_bytes.clone()) {
                    continue;
                }
                if count > 0 {
                    list = list.child(settings_divider(p));
                }
                count += 1;
                let security = super::editors::wifi_security(ap);
                let device = radio.path.clone();
                let ssid = (ap.ssid.clone(), ap.ssid_bytes.clone());
                list = list.child(
                    row_button(eid("join-wifi", &ap.path), format!("Join {}", ap.ssid), p)
                        .disabled(busy || security.is_none())
                        .child(div().flex_1().child(ap.ssid.clone()))
                        .when(security.is_none(), |row| {
                            row.child(caption("Unsupported security", p).text_size(px(11.)))
                        })
                        .when(secured(ap), |row| row.child(symbol("lock", 12., p.muted)))
                        .child(signal(Some(ap.strength), p))
                        .on_click(cx.listener(move |s, _, w, cx| {
                            if let Some(security) = security {
                                s.join_network(device.clone(), Some(ssid.clone()), security, w, cx);
                            }
                        })),
                );
            }
            if count == 0 {
                list = list.child(caption("No other networks found.", p).p(px(12.)));
            }
            let device = radios[0].path.clone();
            content = content.child(list).child(
                div().flex().justify_end().child(
                    button("join-other-network", "Other…", p)
                        .py(px(3.))
                        .disabled(busy)
                        .on_click(cx.listener(move |s, _, w, cx| {
                            s.join_network(
                                device.clone(),
                                None,
                                super::editors::Security::Personal,
                                w,
                                cx,
                            )
                        })),
                ),
            );
        }
        content = content.child(settings_divider(p)).child(div().flex().flex_col().gap(px(16.))
            .child(settings_row("Ask to join networks", p).child(toggle("wifi-ask-networks", false, p).disabled(true)))
            .child(caption("Known networks are joined automatically according to their saved settings. Join other networks from the list above.", p).px(px(12.)).mt(px(-10.)))
            .child(settings_row("Ask to join hotspots", p).child(toggle("wifi-ask-hotspots", false, p).disabled(true)))
            .child(caption("Hotspot suggestions are not available on this system.", p).px(px(12.)).mt(px(-10.))));
        for warning in network.warnings {
            content = content.child(caption(warning.to_string(), p).px(px(12.)));
        }
        content.into_any_element()
    }

    fn close_wifi_details(&mut self, cx: &mut Context<Self>) {
        self.connection_details = None;
        self.wifi_details = None;
        self.store.read(cx).system.clear_connection();
        cx.notify();
    }

    pub(super) fn wifi_connection_sheet(
        &mut self,
        p: Palette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.system_state(cx);
        let path = self.connection_details.clone().unwrap();
        if self
            .wifi_details
            .as_ref()
            .is_some_and(|draft| draft.pending && !state.busy && state.revision > draft.revision)
        {
            if let Some(error) = &state.error {
                let draft = self.wifi_details.as_mut().unwrap();
                draft.pending = false;
                draft.error = Some(error.clone());
            } else {
                self.close_wifi_details(cx);
                return div().into_any_element();
            }
        }
        let loaded = state.connection_path.as_ref() == Some(&path);
        let connection = state.connection.value.as_ref().filter(|_| loaded);
        if let Some(connection) = connection
            && self
                .wifi_details
                .as_ref()
                .is_none_or(|draft| draft.path != path)
        {
            self.wifi_details = Some(DetailsDraft {
                path: path.clone(),
                autoconnect: connection.autoconnect,
                pending: false,
                revision: state.revision,
                error: None,
            });
        }
        let pending = self.wifi_details.as_ref().is_some_and(|d| d.pending);
        let mut content = div().flex().flex_col().gap(px(12.));
        if let Some(connection) = connection {
            let draft = self.wifi_details.as_ref().unwrap();
            let options = group(p)
                .child(settings_row("Automatically join this network", p)
                    .child(toggle("connection-autoconnect", draft.autoconnect, p).disabled(state.busy || pending)
                        .on_change(cx.listener(|s, value: &bool, _, cx| {
                            if let Some(draft) = &mut s.wifi_details { draft.autoconnect = *value; draft.error = None; } cx.notify();
                        }))))
                .child(settings_divider(p))
                .child(div().px(px(12.)).py(px(9.)).flex().items_start().gap(px(16.))
                    .child(div().flex_1().flex().flex_col().gap(px(3.)).child("Low data mode")
                        .child(caption("Reduce data usage over this Wi-Fi network. Not available on this system.", p).text_size(px(11.))))
                    .child(toggle("wifi-low-data", false, p).disabled(true)))
                .child(settings_divider(p))
                .child(div().px(px(12.)).py(px(9.)).flex().items_start().gap(px(16.))
                    .child(div().flex_1().flex().flex_col().gap(px(3.)).child("Limit IP address tracking")
                        .child(caption("Private Relay is not available on this system.", p).text_size(px(11.))))
                    .child(toggle("wifi-limit-tracking", false, p).disabled(true)));
            let config = connection.ipv4.clone();
            let address = config
                .as_ref()
                .map(|c| {
                    if c.addresses.is_empty() {
                        if c.method == "auto" {
                            "Automatic (DHCP)".into()
                        } else {
                            "Not configured".into()
                        }
                    } else {
                        c.addresses
                            .iter()
                            .map(|a| a.address.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                })
                .unwrap_or_else(|| "Not configured".into());
            let router = config
                .as_ref()
                .and_then(|c| c.gateway.clone())
                .unwrap_or_else(|| {
                    if config.as_ref().is_some_and(|c| c.method == "auto") {
                        "Automatic".into()
                    } else {
                        "Not configured".into()
                    }
                });
            content = content.child(options).child(
                group(p)
                    .child(settings_row("IP address", p).child(caption(address, p)))
                    .child(settings_divider(p))
                    .child(settings_row("Router", p).child(caption(router, p))),
            );
            content = content.child(
                caption(
                    "Saved TCP/IP configuration; live DHCP addresses are not shown.",
                    p,
                )
                .text_size(px(10.))
                .px(px(8.)),
            );
            if let Some(error) = &draft.error {
                content = content.child(caption(error.clone(), p).text_color(rgb(0xd23e35)));
            }
        } else {
            content = content.child(unavailable(&state.connection, "Network configuration", p));
        }
        let mut footer = div().flex().items_center().gap(px(8.));
        if let Some(connection) = connection {
            let name = connection.name.clone();
            let forget_path = path.clone();
            footer = footer.child(
                button("forget-network", "Forget This Network…", p)
                    .py(px(3.))
                    .text_size(px(11.))
                    .disabled(state.busy || pending)
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.confirmation = Some(Confirmation::ForgetNetwork {
                            path: forget_path.clone(),
                            name: name.clone(),
                        });
                        cx.notify();
                    })),
            );
            let config = connection.ipv4.clone().unwrap_or(IpConfiguration {
                method: "auto".into(),
                addresses: vec![],
                gateway: None,
                dns: vec![],
                dns_search: vec![],
                ignore_auto_dns: false,
            });
            let edit_path = path.clone();
            content = content.child(
                div().flex().justify_end().child(
                    button("wifi-tcp-ip", "TCP/IP…", p)
                        .py(px(3.))
                        .text_size(px(11.))
                        .disabled(state.busy || pending)
                        .on_click(cx.listener(move |s, _, w, cx| {
                            s.edit_ip(edit_path.clone(), IpFamily::Ipv4, config.clone(), w, cx)
                        })),
                ),
            );
        }
        footer = footer
            .child(div().flex_1())
            .child(
                button("cancel-wifi-details", "Cancel", p)
                    .py(px(3.))
                    .disabled(pending)
                    .on_click(cx.listener(|s, _, _, cx| s.close_wifi_details(cx))),
            )
            .child(
                button(
                    "close-connection",
                    if pending { "Saving…" } else { "OK" },
                    p,
                )
                .py(px(3.))
                .min_w(px(48.))
                .bg(rgb(p.accent))
                .text_color(rgb(p.accent_text))
                .disabled(state.busy || pending || connection.is_none())
                .on_click(cx.listener(|s, _, _, cx| {
                    let state = s.system_state(cx);
                    let Some(draft) = s.wifi_details.as_ref() else {
                        return;
                    };
                    if state
                        .connection
                        .value
                        .as_ref()
                        .is_some_and(|c| c.autoconnect == draft.autoconnect)
                    {
                        s.close_wifi_details(cx);
                        return;
                    }
                    let command = SystemCommand::SetConnectionAutoconnect {
                        connection: draft.path.clone(),
                        enabled: draft.autoconnect,
                    };
                    let revision = state.revision;
                    let result = s.store.read(cx).system.apply(command);
                    let draft = s.wifi_details.as_mut().unwrap();
                    draft.pending = result.is_ok();
                    draft.revision = revision;
                    draft.error = result.err().map(|error| error.to_string());
                    s.sync_system(cx);
                })),
            );
        overlay()
            .when(self.editor.is_none() && self.confirmation.is_none(), |d| {
                d.track_focus(&self.sheet_focus)
            })
            .child(
                panel(p)
                    .id("wifi-details-sheet")
                    .w(px(480.))
                    .max_w(window.viewport_size().width - px(36.))
                    .max_h(window.viewport_size().height - px(40.))
                    .rounded(px(12.))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(
                        div()
                            .px(px(18.))
                            .pt(px(18.))
                            .pb(px(6.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(super::sidebar::icon(Page::Wifi, 28.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .text_size(px(17.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(
                                        connection
                                            .map(|c| c.name.clone())
                                            .unwrap_or_else(|| "Wi-Fi Details".into()),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("wifi-details-scroll")
                            .min_h_0()
                            .overflow_y_scroll()
                            .p(px(14.))
                            .child(content),
                    )
                    .child(footer.px(px(14.)).pb(px(14.)).flex_shrink_0()),
            )
            .into_any_element()
    }
}
