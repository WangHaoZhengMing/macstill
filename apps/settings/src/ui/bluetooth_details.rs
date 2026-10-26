use super::*;
use super::{
    editors::{Confirmation, EditorKind},
    widgets::*,
};
use macstill_system::SystemCommand;
use swiftui_gpui::{settings_divider, settings_group, settings_row};
#[derive(Clone)]
pub(super) enum BluetoothTarget {
    Adapter(String),
    Device(String),
}
impl SettingsView {
    pub(super) fn bluetooth_sheet(
        &self,
        p: Palette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.system_state(cx);
        let mut title = "Bluetooth Details".to_string();
        let mut content = div().flex().flex_col().gap(px(14.));
        if let Some(bluetooth) = state.system.bluetooth.value {
            match self.bluetooth_details.as_ref().unwrap() {
                BluetoothTarget::Adapter(path) => {
                    if let Some(adapter) = bluetooth.adapters.iter().find(|a| &a.path == path) {
                        title = adapter.name.clone();
                        let path = path.clone();
                        let name = adapter.name.clone();
                        let visible_path = path.clone();
                        let pair_path = path.clone();
                        let timeout_path = path.clone();
                        let timeout = adapter
                            .discoverable_timeout_seconds
                            .unwrap_or(180)
                            .to_string();
                        content = content.child(
                            settings_group(p)
                                .child(
                                    settings_row("Name", p)
                                        .child(caption(name.clone(), p))
                                        .child(
                                            button("rename-adapter", "Rename…", p)
                                                .disabled(state.busy)
                                                .on_click(cx.listener(move |s, _, w, cx| {
                                                    s.open_editor(
                                                        EditorKind::AdapterAlias(path.clone()),
                                                        vec![(
                                                            "Name",
                                                            "Bluetooth name",
                                                            name.clone(),
                                                        )],
                                                        0,
                                                        vec![],
                                                        w,
                                                        cx,
                                                    )
                                                })),
                                        ),
                                )
                                .child(settings_divider(p))
                                .child(
                                    settings_row("Discoverable", p).child(
                                        toggle("bluetooth-discoverable", adapter.discoverable, p)
                                            .disabled(state.busy || !adapter.powered)
                                            .on_change(cx.listener(
                                                move |s, value: &bool, _, cx| {
                                                    s.apply_system(
                                                        SystemCommand::SetBluetoothDiscoverable {
                                                            adapter: visible_path.clone(),
                                                            discoverable: *value,
                                                        },
                                                        cx,
                                                    )
                                                },
                                            )),
                                    ),
                                )
                                .when_some(adapter.pairable, |group, pairable| {
                                    group.child(settings_divider(p)).child(
                                        settings_row("Allow new pairing", p).child(
                                            toggle("bluetooth-pairable", pairable, p)
                                                .disabled(state.busy || !adapter.powered)
                                                .on_change(cx.listener(
                                                    move |s, value: &bool, _, cx| {
                                                        s.apply_system(
                                                            SystemCommand::SetBluetoothPairable {
                                                                adapter: pair_path.clone(),
                                                                pairable: *value,
                                                            },
                                                            cx,
                                                        )
                                                    },
                                                )),
                                        ),
                                    )
                                })
                                .when_some(
                                    adapter.discoverable_timeout_seconds,
                                    |group, seconds| {
                                        group.child(settings_divider(p)).child(
                                            settings_row("Discoverable duration", p)
                                                .child(caption(
                                                    if seconds == 0 {
                                                        "No timeout".into()
                                                    } else {
                                                        format!("{seconds} seconds")
                                                    },
                                                    p,
                                                ))
                                                .child(
                                                    button("edit-discovery-timeout", "Set…", p)
                                                        .disabled(state.busy)
                                                        .on_click(cx.listener(
                                                            move |s, _, w, cx| {
                                                                s.open_editor(
                                                                    EditorKind::DiscoverableTimeout(
                                                                        timeout_path.clone(),
                                                                    ),
                                                                    vec![(
                                                                        "Seconds",
                                                                        "120",
                                                                        timeout.clone(),
                                                                    )],
                                                                    0,
                                                                    vec![],
                                                                    w,
                                                                    cx,
                                                                )
                                                            },
                                                        )),
                                                ),
                                        )
                                    },
                                ),
                        );
                        content=content.child(caption("Discoverable lets nearby devices find this computer. Pairing still requires confirmation from your desktop’s pairing dialog.",p));
                    }
                }
                BluetoothTarget::Device(path) => {
                    if let Some(device) = bluetooth.devices.iter().find(|d| &d.path == path) {
                        title = device.name.clone();
                        let name = device.name.clone();
                        let path = path.clone();
                        let trust_path = path.clone();
                        let block_path = path.clone();
                        let forget_path = path.clone();
                        let forget_name = name.clone();
                        content = content.child(
                            settings_group(p)
                                .child(
                                    settings_row("Name", p)
                                        .child(caption(name.clone(), p))
                                        .child(
                                            button("rename-bluetooth-device", "Rename…", p)
                                                .disabled(state.busy)
                                                .on_click(cx.listener(move |s, _, w, cx| {
                                                    s.open_editor(
                                                        EditorKind::DeviceAlias(path.clone()),
                                                        vec![("Name", "Device name", name.clone())],
                                                        0,
                                                        vec![],
                                                        w,
                                                        cx,
                                                    )
                                                })),
                                        ),
                                )
                                .child(settings_divider(p))
                                .child(value_row(
                                    "Status",
                                    if device.connected {
                                        "Connected"
                                    } else {
                                        "Not connected"
                                    },
                                    p,
                                ))
                                .child(settings_divider(p))
                                .child(
                                    settings_row("Trusted accessory", p).child(
                                        toggle("details-bluetooth-trusted", device.trusted, p)
                                            .disabled(state.busy)
                                            .on_change(cx.listener(
                                                move |s, value: &bool, _, cx| {
                                                    s.apply_system(
                                                        SystemCommand::SetBluetoothTrusted {
                                                            device: trust_path.clone(),
                                                            trusted: *value,
                                                        },
                                                        cx,
                                                    )
                                                },
                                            )),
                                    ),
                                )
                                .when_some(device.blocked, |group, blocked| {
                                    group.child(settings_divider(p)).child(
                                        settings_row("Block this device", p).child(
                                            toggle("bluetooth-blocked", blocked, p)
                                                .disabled(state.busy)
                                                .on_change(cx.listener(
                                                    move |s, value: &bool, _, cx| {
                                                        s.apply_system(
                                                            SystemCommand::SetBluetoothBlocked {
                                                                device: block_path.clone(),
                                                                blocked: *value,
                                                            },
                                                            cx,
                                                        )
                                                    },
                                                )),
                                        ),
                                    )
                                }),
                        );
                        content = content.child(
                            button("forget-bluetooth-device", "Forget This Device…", p)
                                .text_color(rgb(0xd23e35))
                                .disabled(state.busy)
                                .on_click(cx.listener(move |s, _, _, cx| {
                                    s.confirmation = Some(Confirmation::ForgetBluetooth {
                                        path: forget_path.clone(),
                                        name: forget_name.clone(),
                                    });
                                    cx.notify();
                                })),
                        );
                    }
                }
            }
        }
        super::editors::overlay()
            .when(self.editor.is_none() && self.confirmation.is_none(), |d| {
                d.track_focus(&self.sheet_focus)
            })
            .child(
                panel(p)
                    .w(px(460.))
                    .max_h(window.viewport_size().height - px(40.))
                    .rounded(px(12.))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(
                        div()
                            .p(px(20.))
                            .text_size(px(17.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .id("bluetooth-details-scroll")
                            .min_h_0()
                            .overflow_y_scroll()
                            .px(px(20.))
                            .pb(px(20.))
                            .child(content),
                    )
                    .child(
                        div()
                            .border_t_1()
                            .border_color(rgb(p.line))
                            .p(px(14.))
                            .flex_shrink_0()
                            .flex()
                            .justify_end()
                            .child(button("close-bluetooth-details", "Done", p).on_click(
                                cx.listener(|s, _, _, cx| {
                                    s.bluetooth_details = None;
                                    cx.notify();
                                }),
                            )),
                    ),
            )
            .into_any_element()
    }
}
