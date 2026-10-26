use super::widgets::*;
use super::*;
use crate::system_backend::Job;
use gpui_kit::base::Popover;
use macstill_airdrop::Visibility;
use swiftui_gpui::{settings_group, settings_row};
impl SettingsView {
    pub(super) fn airdrop_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let state = self.system_state(cx);
        let Some(airdrop) = state.airdrop.value else {
            return unavailable(&state.airdrop, "AirDrop", p);
        };
        let selected = airdrop.visibility;
        let busy = state.busy || airdrop.busy();
        let view = cx.entity().downgrade();
        let picker = Popover::new("airdrop-policy-menu")
            .anchor(Anchor::TopRight)
            .offset(px(3.))
            .when(busy, |popover| popover.open(false))
            .trigger(
                button("airdrop-policy", selected.label(), p)
                    .disabled(busy)
                    .border_0()
                    .bg(transparent_black())
                    .text_size(px(12.))
                    .child(
                        div()
                            .ml(px(7.))
                            .rounded(px(4.))
                            .bg(rgb(p.line))
                            .child(super::control_center_page::chevrons(p)),
                    ),
            )
            .content(move |_, _, cx| {
                let mut menu = div()
                    .w(px(255.))
                    .p(px(5.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(rgb(p.line))
                    .bg(rgb(p.surface))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .gap(px(2.));
                for policy in [
                    Visibility::Off,
                    Visibility::EveryoneTenMinutes,
                    Visibility::Everyone,
                ] {
                    let view = view.clone();
                    menu = menu.child(
                        button(
                            eid("airdrop-visibility", policy.label()),
                            format!(
                                "{}  {}",
                                if selected == policy { "✓" } else { "  " },
                                policy.label()
                            ),
                            p,
                        )
                        .w_full()
                        .justify_start()
                        .border_0()
                        .bg(transparent_black())
                        .text_size(px(12.))
                        .disabled(busy)
                        .on_click(cx.listener(
                            move |popover, _, window, cx| {
                                let _ = view.update(cx, |s, cx| {
                                    if let Err(error) =
                                        s.store.read(cx).system.submit(Job::AirDrop(policy))
                                    {
                                        s.message = Some(error.to_string());
                                    }
                                    cx.notify();
                                });
                                popover.dismiss(window, cx);
                            },
                        )),
                    );
                }
                menu
            });
        let group = settings_group(p).child(div().px(px(14.)).py(px(14.)).flex().items_center().gap(px(24.))
            .child(div().flex_1().min_w_0().flex().flex_col().gap(px(5.))
                .child(div().text_size(px(13.)).child("AirDrop"))
                .child(caption("Choose who can discover you and send you files. You’ll be asked before receiving anything.", p).text_size(px(12.))))
            .child(picker));
        stack()
            .child(group)
            .when(airdrop.expires_at > 0, |d| {
                d.child(caption(
                    format!(
                        "Discoverable for {} more minutes",
                        airdrop
                            .expires_at
                            .saturating_sub(macstill_airdrop::now())
                            .div_ceil(60)
                    ),
                    p,
                ))
            })
            .when_some(airdrop.error.clone(), |d, message| {
                d.child(
                    settings_group(p)
                        .p(px(14.))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(div().text_size(px(13.)).child("AirDrop is unavailable"))
                        .child(caption(message, p)),
                )
            })
            .when_some(airdrop.warning.clone(), |d, message| {
                d.child(caption(message, p))
            })
            .when(!airdrop.available && airdrop.error.is_none(), |d| {
                d.child(caption(
                    "AirDrop is not ready. Choose a discovery option to try connecting again.",
                    p,
                ))
            })
            .child(
                settings_group(p).child(
                    settings_row("Save received files to", p)
                        .child(caption(airdrop.receive_directory.display().to_string(), p))
                        .child(
                            button("airdrop-download-folder", "Choose…", p)
                                .disabled(busy_or_enabled(&airdrop, state.busy))
                                .on_click(cx.listener(|_, _, _, cx| {
                                    let prompt = cx.prompt_for_paths(PathPromptOptions {
                                        files: false,
                                        directories: true,
                                        multiple: false,
                                        prompt: Some("Choose the AirDrop download folder".into()),
                                    });
                                    cx.spawn(async move |this, cx| {
                                        if let Ok(Ok(Some(paths))) = prompt.await {
                                            if let Some(path) = paths.into_iter().next() {
                                                let _ = this.update(cx, |s, cx| {
                                                    if let Err(error) = s
                                                        .store
                                                        .read(cx)
                                                        .system
                                                        .submit(Job::AirDropDirectory(path))
                                                    {
                                                        s.message = Some(error.to_string());
                                                    }
                                                    cx.notify();
                                                });
                                            }
                                        }
                                    })
                                    .detach();
                                })),
                        ),
                ),
            )
            .when(airdrop.enabled(), |d| {
                d.child(caption(
                    "Turn off AirDrop to change the download folder.",
                    p,
                ))
            })
            .child(
                button("airdrop-open-finder", "Open AirDrop in Finder…", p).on_click(cx.listener(
                    |s, _, _, cx| {
                        if let Err(error) = macstill_airdrop::open_app() {
                            s.message = Some(error.to_string());
                            cx.notify();
                        }
                    },
                )),
            )
            .into_any_element()
    }
}

fn busy_or_enabled(snapshot: &macstill_airdrop::Snapshot, busy: bool) -> bool {
    busy || snapshot.busy() || snapshot.enabled()
}
