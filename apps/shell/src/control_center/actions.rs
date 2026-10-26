use super::*;
impl DesktopView {
    pub(super) fn toggle_airdrop(&mut self, cx: &mut Context<Self>) {
        if self.control_center.busy {
            return;
        }
        self.control_center.busy = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async {
                    macstill_airdrop::ensure_service()?;
                    let state = macstill_airdrop::snapshot()?;
                    if !state.available || (!state.enabled() && state.mode != "shared") {
                        macstill_airdrop::open_settings()?;
                    } else {
                        macstill_airdrop::send(if state.enabled() {
                            macstill_airdrop::Command::Disable
                        } else {
                            macstill_airdrop::Command::Enable { seconds: 600 }
                        })?;
                    }
                    Ok::<_, anyhow::Error>(())
                })
                .await;
            let _ = this.update_in(cx, |s, w, cx| {
                s.control_center.busy = false;
                if let Err(e) = result {
                    s.message = Some(e.to_string())
                }
                s.refresh_control_center(w, cx);
                cx.notify();
            });
        })
        .detach();
    }
}

#[derive(Clone, Copy)]
pub(super) enum ControlAction {
    Wifi(bool),
    Bluetooth(bool),
    Volume(f64),
    Brightness(f64),
}
impl Status {
    pub(super) fn command(&self, action: ControlAction) -> Option<SystemCommand> {
        Some(match action {
            ControlAction::Wifi(enabled) => {
                self.wifi?;
                SystemCommand::SetWifi(enabled)
            }
            ControlAction::Bluetooth(powered) => SystemCommand::SetBluetoothPowered {
                adapter: self.bluetooth_adapter.clone()?,
                powered,
            },
            ControlAction::Volume(percent) => SystemCommand::SetVolume {
                direction: macstill_system::AudioDirection::Output,
                id: self.audio_output?,
                percent,
            },
            ControlAction::Brightness(percent) => SystemCommand::SetBrightness {
                name: self.backlight.clone()?,
                percent,
            },
        })
    }
}
impl DesktopView {
    pub(super) fn cc_action(&mut self, action: ControlAction, cx: &mut Context<Self>) {
        if self.control_center.busy {
            return;
        }
        let Some(command) = self.control_center.status.command(action) else {
            self.message = Some("This control is unavailable on this system.".into());
            cx.notify();
            return;
        };
        self.control_center.busy = true;
        self.message = None;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { SystemApi::new().apply(command) })
                .await;
            let _ = this.update_in(cx, |s, w, cx| {
                s.control_center.busy = false;
                if let Err(error) = result {
                    s.message = Some(error.to_string());
                }
                s.refresh_control_center(w, cx);
                cx.notify();
            });
        })
        .detach();
    }
}
