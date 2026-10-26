use super::*;
impl DesktopView {
    pub(in crate::ui) fn control_center_menu_bar(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        use macstill_config::ControlCenterModule as Module;
        let snapshot = self.snapshot(cx);
        let p = Palette::new(snapshot.preferences.dark);
        let cache = self.store.read(cx).control_status.clone();
        let status = cache.read(cx).status.clone().unwrap_or_default();
        let focus_active = {
            let notification = &self.store.read(cx).notification_snapshot;
            notification.available && notification.focus
        };
        let mut icons = div().h(px(30.)).flex().items_center();
        let mut visible = false;
        for (module, glyph, active) in [
            (
                Module::Wifi,
                "wifi",
                status.wifi == Some(true) && !status.ssid.is_empty(),
            ),
            (
                Module::Bluetooth,
                "bluetooth",
                status.bluetooth == Some(true),
            ),
            (Module::AirDrop, "airdrop", status.airdrop.enabled()),
            (Module::Focus, "focus", focus_active),
            (Module::StageManager, "stage", snapshot.stage.enabled),
            (Module::Display, "sun", status.brightness.is_some()),
            (Module::Sound, "speaker", status.volume.is_some()),
            (
                Module::NowPlaying,
                "play",
                status.media.as_ref().is_some_and(|player| player.playing()),
            ),
        ] {
            if snapshot.preferences.control_center.visible(module, active) {
                visible = true;
                icons = icons.child(
                    gpui_kit::base::Button::new(eid("menu-bar-module", module.key()))
                        .accessibility_label(module.title())
                        .w(px(32.))
                        .h(px(30.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor(CursorStyle::Arrow)
                        .child(Self::cc_icon(glyph, 16., p.text, p.dark))
                        .on_click(|_, window, cx| toggle_popover(Mode::ControlCenter, window, cx)),
                );
            }
        }
        visible.then(|| icons.into_any_element())
    }
}
