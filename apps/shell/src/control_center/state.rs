use super::*;
#[derive(Clone, Default, PartialEq)]
pub(super) struct Status {
    pub(super) airdrop: macstill_airdrop::Snapshot,
    pub(super) wifi: Option<bool>,
    pub(super) ssid: String,
    pub(super) bluetooth: Option<bool>,
    pub(super) volume: Option<f32>,
    pub(super) audio_output: Option<u32>,
    pub(super) backlight: Option<String>,
    pub(super) bluetooth_adapter: Option<String>,
    pub(super) brightness: Option<f32>,
    pub(super) media: Option<macstill_system::media::Player>,
    pub(super) cover: Option<Vec<u8>>,
}
pub(in crate::ui) struct StatusCache {
    pub(super) status: Option<Status>,
    pub(super) _poll: Task<()>,
}
pub(in crate::ui) struct ControlCenter {
    pub(super) status: Status,
    pub(super) volume: Entity<SliderState>,
    pub(super) brightness: Entity<SliderState>,
    pub(in crate::ui) focus: FocusHandle,
    pub(super) _subscriptions: Vec<Subscription>,
    pub(super) busy: bool,
    pub(super) loading: bool,
    pub(super) adjusting_volume: bool,
    pub(super) adjusting_brightness: bool,
}
impl StatusCache {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            let _ = cx
                .background_executor()
                .spawn(async { macstill_airdrop::ensure_service() })
                .await;
            loop {
                let next = cx.background_executor().spawn(async { status() }).await;
                if this
                    .update(cx, |cache, cx| {
                        if cache.status.as_ref() != Some(&next) {
                            cache.status = Some(next);
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
                cx.background_executor().timer(Duration::from_secs(3)).await;
            }
        });
        Self {
            status: None,
            _poll: poll,
        }
    }
}
impl ControlCenter {
    pub fn new(window: &mut Window, cx: &mut Context<DesktopView>) -> Self {
        let cache = cx.global::<SharedStore>().0.read(cx).control_status.clone();
        let cached = cache.read(cx).status.clone();
        let initial = cached.clone().unwrap_or_default();
        let volume = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(initial.volume.unwrap_or(0.))
        });
        let brightness = cx.new(|_| {
            SliderState::new()
                .min(1.)
                .max(100.)
                .step(1.)
                .default_value(initial.brightness.unwrap_or(1.))
        });
        let mut subscriptions = Vec::new();
        for (slider, sound) in [(&volume, true), (&brightness, false)] {
            subscriptions.push(cx.observe(slider, |_, _, cx| cx.notify()));
            subscriptions.push(cx.subscribe(slider, move |s, _, event, cx| {
                if matches!(event, SliderEvent::Change(_)) {
                    if sound {
                        s.control_center.adjusting_volume = true;
                    } else {
                        s.control_center.adjusting_brightness = true;
                    }
                }
                if let SliderEvent::Release(value) = event {
                    if sound {
                        s.control_center.adjusting_volume = false;
                    } else {
                        s.control_center.adjusting_brightness = false;
                    }
                    let value = f64::from(value.end().round());
                    s.cc_action(
                        if sound {
                            ControlAction::Volume(value)
                        } else {
                            ControlAction::Brightness(value)
                        },
                        cx,
                    );
                }
            }));
        }
        subscriptions.push(cx.observe_in(&cache, window, |s, cache, w, cx| {
            if s.mode != Mode::ControlCenter {
                return;
            }
            if let Some(status) = cache.read(cx).status.clone() {
                s.apply_control_status(status, w, cx);
            }
        }));
        Self {
            status: initial,
            volume,
            brightness,
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
            busy: false,
            loading: cached.is_none(),
            adjusting_volume: false,
            adjusting_brightness: false,
        }
    }
}
impl DesktopView {
    pub(super) fn apply_control_status(
        &mut self,
        next: Status,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.control_center.adjusting_volume && !self.control_center.busy {
            if let Some(v) = next.volume {
                self.control_center
                    .volume
                    .update(cx, |state, cx| state.set_value(v, window, cx));
            }
        }
        if !self.control_center.adjusting_brightness && !self.control_center.busy {
            if let Some(v) = next.brightness {
                self.control_center
                    .brightness
                    .update(cx, |state, cx| state.set_value(v, window, cx));
            }
        }
        self.control_center.status = next;
        self.control_center.loading = false;
        cx.notify();
    }
    pub(in crate::ui) fn refresh_control_center(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cache = self.store.read(cx).control_status.clone();
        cx.spawn(async move |this, cx| {
            let next = cx.background_executor().spawn(async { status() }).await;
            let _ = this.update_in(cx, |s, w, cx| {
                cache.update(cx, |cache, cx| {
                    cache.status = Some(next.clone());
                    cx.notify();
                });
                s.apply_control_status(next, w, cx);
            });
        })
        .detach();
    }
}

// Only the requested domains are queried; unrelated services cannot block availability.
pub(super) fn status() -> Status {
    let api = SystemApi::new();
    let network = api.network().snapshot().ok();
    let bluetooth = api.bluetooth().snapshot().ok();
    let audio = api.audio().snapshot().ok();
    let backlights = api.brightness().snapshot().ok();
    let mut result = Status::from_system(
        network.as_ref(),
        bluetooth.as_ref(),
        audio.as_ref(),
        backlights.as_deref(),
    );
    result.airdrop = macstill_airdrop::snapshot().unwrap_or_default();
    result.media = api.media().current(None).ok().flatten();
    result.cover = result
        .media
        .as_ref()
        .and_then(|p| p.art_url.as_deref())
        .and_then(super::media::media_cover);
    result
}
impl Status {
    pub(super) fn from_system(
        network: Option<&macstill_system::NetworkState>,
        bluetooth: Option<&macstill_system::BluetoothState>,
        audio: Option<&macstill_system::AudioState>,
        backlights: Option<&[macstill_system::Backlight]>,
    ) -> Self {
        let adapter =
            bluetooth.and_then(|s| s.adapters.iter().min_by_key(|a| (!a.powered, &a.path)));
        let output = audio.and_then(|s| s.outputs.iter().find(|d| d.is_default));
        let light = backlights.and_then(|s| s.iter().min_by_key(|b| &b.name));
        Self {
            wifi: network
                .filter(|s| s.devices.iter().any(|d| d.kind == 2))
                .map(|s| s.wifi_enabled),
            ssid: network
                .filter(|s| s.wifi_enabled)
                .and_then(|s| {
                    s.devices.iter().filter(|d| d.kind == 2).find_map(|d| {
                        d.access_points
                            .iter()
                            .find(|a| Some(&a.path) == d.active_access_point.as_ref())
                            .map(|a| a.ssid.clone())
                    })
                })
                .unwrap_or_default(),
            bluetooth: adapter.map(|a| a.powered),
            bluetooth_adapter: adapter.map(|a| a.path.clone()),
            volume: output.map(|d| d.percent.clamp(0., 100.) as f32),
            audio_output: output.map(|d| d.id),
            brightness: light.map(|d| d.percent.clamp(1., 100.) as f32),
            backlight: light.map(|d| d.name.clone()),
            ..Self::default()
        }
    }
}
#[cfg(test)]
impl StatusCache {
    pub(in crate::ui) fn fixture() -> Self {
        Self {
            status: Some(Status::default()),
            _poll: Task::ready(()),
        }
    }
}
