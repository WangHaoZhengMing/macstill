//! AirDrop is a virtual Finder destination. All blocking IPC stays off the UI thread.
use super::*;
use gpui_kit::{ExternalPaths, PathPromptOptions, relative, rgb};
use macstill_airdrop::{Command, Snapshot, Visibility};
#[derive(Default)]
pub(super) struct State {
    snapshot: Snapshot,
    poll: Option<gpui_kit::Task<()>>,
    busy: bool,
    error: Option<String>,
    menu: bool,
    pub(super) files: Vec<PathBuf>,
}
fn button(
    id: impl Into<gpui_kit::ElementId>,
    label: impl Into<SharedString>,
) -> gpui_kit::base::Button {
    gpui_kit::base::Button::new(id)
        .px(px(12.))
        .py(px(6.))
        .rounded(px(6.))
        .bg(t::bar_bg())
        .child(label.into())
}
impl Browser {
    pub(super) fn in_airdrop(&self) -> bool {
        self.navigation.location == Location::AirDrop
    }
    pub(super) fn start_airdrop_monitor(&mut self, cx: &mut Context<Self>) {
        if cfg!(test) {
            return;
        }
        self.airdrop.poll = Some(cx.spawn(async |this, cx| {
            loop {
                let visible = this.update(cx, |s, _| s.in_airdrop());
                let Ok(visible) = visible else {
                    break;
                };
                if visible {
                    let result = cx
                        .background_spawn(async {
                            macstill_airdrop::ensure_service()
                                .and_then(|_| macstill_airdrop::snapshot())
                        })
                        .await;
                    if this
                        .update(cx, |s, cx| {
                            match result {
                                Ok(snapshot) => {
                                    s.airdrop.snapshot = snapshot;
                                    s.airdrop.error = None;
                                }
                                Err(e) => s.airdrop.error = Some(e.to_string()),
                            }
                            cx.notify();
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                cx.background_executor()
                    .timer(Duration::from_millis(700))
                    .await;
            }
        }));
    }
    fn airdrop_command(&mut self, command: Command, cx: &mut Context<Self>) {
        if self.airdrop.busy {
            return;
        }
        self.airdrop.busy = true;
        self.airdrop.error = None;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { macstill_airdrop::send(command) })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.airdrop.busy = false;
                s.airdrop.error = result.err().map(|e| e.to_string());
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn send_airdrop(&mut self, peer: String, files: Vec<PathBuf>, cx: &mut Context<Self>) {
        if !self.airdrop.snapshot.enabled() || self.airdrop.busy {
            return;
        }
        if !files.is_empty() {
            self.airdrop_command(Command::Send { peer, files }, cx);
            return;
        }
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: true,
            prompt: Some("Choose files to AirDrop".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(files))) = paths.await {
                let _ = this.update(cx, |s, cx| {
                    s.airdrop_command(Command::Send { peer, files }, cx)
                });
            }
        })
        .detach();
    }
    pub(super) fn airdrop_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let snapshot = &self.airdrop.snapshot;
        let busy = self.airdrop.busy || snapshot.busy();
        let mut peers = div()
            .flex()
            .flex_wrap()
            .justify_center()
            .gap(px(24.))
            .p(px(24.));
        for peer in &snapshot.peers {
            let click_id = peer.id.clone();
            let drop_id = peer.id.clone();
            let external_id = peer.id.clone();
            peers = peers.child(
                button(SharedString::from(format!("airdrop-peer-{}", peer.id)), "")
                    .w(px(130.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .items_center()
                    .bg(t::content_bg())
                    .disabled(busy || !snapshot.enabled())
                    .child(
                        div()
                            .size(px(58.))
                            .rounded_full()
                            .bg(t::selection())
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(gpui_kit::white())
                            .text_size(px(26.))
                            .child(peer.name.chars().next().unwrap_or('?').to_string()),
                    )
                    .child(peer.name.clone())
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(t::text_secondary())
                            .child(peer.model.clone()),
                    )
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.send_airdrop(click_id.clone(), s.airdrop.files.clone(), cx)
                    }))
                    .on_drop(cx.listener(move |s, drop: &dnd::Dragged, _, cx| {
                        cx.stop_propagation();
                        s.send_airdrop(drop_id.clone(), drop.paths.clone(), cx);
                    }))
                    .on_drop(cx.listener(move |s, drop: &ExternalPaths, _, cx| {
                        cx.stop_propagation();
                        s.send_airdrop(external_id.clone(), drop.paths().to_vec(), cx);
                    })),
            );
        }
        let mut policy = div().flex().flex_col().items_center().gap(px(6.)).child(
            button(
                "airdrop-discoverability",
                format!(
                    "Allow me to be discovered by: {} ▾",
                    snapshot.visibility.label()
                ),
            )
            .disabled(busy)
            .on_click(cx.listener(|s, _, _, cx| {
                s.airdrop.menu = !s.airdrop.menu;
                cx.notify();
            })),
        );
        if self.airdrop.menu {
            for visibility in [
                Visibility::Off,
                Visibility::EveryoneTenMinutes,
                Visibility::Everyone,
            ] {
                policy = policy.child(
                    button(
                        SharedString::from(format!("visibility-{visibility:?}")),
                        visibility.label(),
                    )
                    .disabled(busy || (!snapshot.available && visibility != Visibility::Off))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.airdrop.menu = false;
                        s.airdrop_command(Command::SetVisibility { visibility }, cx);
                    })),
                );
            }
        }
        let mut transfers = div().w_full().flex().flex_col().gap(px(8.));
        for transfer in snapshot.transfers.iter().rev().take(10) {
            let id = transfer.id.clone();
            let mut row = div()
                .w_full()
                .rounded(px(8.))
                .p(px(10.))
                .bg(t::bar_bg())
                .flex()
                .items_center()
                .gap(px(12.))
                .child(
                    div()
                        .flex_1()
                        .child(format!("{} · {}", transfer.peer, transfer.state))
                        .when(transfer.total > 0, |d| {
                            d.child(
                                div()
                                    .text_size(px(11.))
                                    .child(format!("{} / {} bytes", transfer.sent, transfer.total)),
                            )
                        }),
                );
            if !transfer.incoming && ["preparing", "sending"].contains(&transfer.state.as_str()) {
                row = row.child(
                    button(SharedString::from(format!("cancel-{id}")), "Cancel").on_click(
                        cx.listener(move |s, _, _, cx| {
                            s.airdrop_command(Command::Cancel { id: id.clone() }, cx)
                        }),
                    ),
                );
            }
            if transfer.incoming && transfer.state == "pending" {
                row = row.child(
                    div()
                        .text_size(px(12.))
                        .child("Respond in Notification Center"),
                );
            }
            if let Some(error) = &transfer.error {
                row = row.child(div().text_size(px(12.)).child(error.clone()));
            }
            transfers = transfers.child(row);
        }
        let mut scene = div().relative().flex_1().min_h_0().overflow_hidden();
        for diameter in [240., 400., 560., 720., 880.] {
            scene = scene.child(
                div()
                    .absolute()
                    .left(relative(0.5))
                    .top(px(240. - diameter / 2.))
                    .ml(px(-diameter / 2.))
                    .size(px(diameter))
                    .rounded_full()
                    .border_1()
                    .border_color(t::separator()),
            );
        }
        scene =
            scene.child(
                div()
                    .id("airdrop-content")
                    .relative()
                    .size_full()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(18.))
                    .p(px(24.))
                    .child(peers)
                    .when(snapshot.peers.is_empty(), |d| {
                        d.child(div().h(px(95.)).flex().items_center().child(
                            if snapshot.enabled() {
                                "Looking for nearby devices…"
                            } else {
                                "Turn on AirDrop to discover nearby devices."
                            },
                        ))
                    })
                    .child(sym("airdrop", 66., rgb(0x007aff)))
                    .child("Share with nearby devices. Drop files onto a device to send.")
                    .when(!self.airdrop.files.is_empty(), |d| {
                        d.child(format!(
                            "{} items ready to send — choose a device",
                            self.airdrop.files.len()
                        ))
                    })
                    .when(!self.airdrop.files.is_empty(), |d| {
                        d.child(
                            button("airdrop-clear-selection", "Clear Selection").on_click(
                                cx.listener(|s, _, _, cx| {
                                    s.airdrop.files.clear();
                                    cx.notify();
                                }),
                            ),
                        )
                    })
                    .child(policy)
                    .when_some(
                        self.airdrop
                            .error
                            .clone()
                            .or(snapshot.error.clone())
                            .or(snapshot.warning.clone()),
                        |d, error| d.child(div().text_size(px(12.)).child(error)),
                    )
                    .child(
                        button("airdrop-settings", "AirDrop Settings…").on_click(|_, _, _| {
                            let _ = macstill_airdrop::open_settings();
                        }),
                    )
                    .child(transfers),
            );
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(t::content_bg())
            .child(
                div()
                    .h(px(t::TOOLBAR_H))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(18.))
                    .window_control_area(WindowControlArea::Drag)
                    .child(
                        button("airdrop-back", "‹")
                            .disabled(self.navigation.back.is_empty())
                            .on_click(cx.listener(|s, _, w, cx| s.go_back(&GoBack, w, cx))),
                    )
                    .child(
                        button("airdrop-forward", "›")
                            .disabled(self.navigation.forward.is_empty())
                            .on_click(cx.listener(|s, _, w, cx| s.go_forward(&GoForward, w, cx))),
                    )
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .child("AirDrop"),
                    ),
            )
            .child(scene)
            .into_any_element()
    }
}
