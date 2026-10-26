use super::*;
use gio::{glib::variant::ToVariant, prelude::DBusProxyExt};
use swiftui_gpui::{menu, menu_item, menu_separator};

#[derive(Clone, Copy, PartialEq)]
enum SessionAction {
    Lock,
    Sleep,
    Restart,
    ShutDown,
    LogOut,
}
impl SessionAction {
    fn label(self) -> &'static str {
        match self {
            Self::Lock => "Lock Screen",
            Self::Sleep => "Sleep",
            Self::Restart => "Restart",
            Self::ShutDown => "Shut Down",
            Self::LogOut => "Log Out",
        }
    }
    fn needs_confirmation(self) -> bool {
        matches!(self, Self::Restart | Self::ShutDown | Self::LogOut)
    }
    fn perform(self) -> Result<(), String> {
        let session = matches!(self, Self::Lock | Self::LogOut);
        let proxy = gio::DBusProxy::for_bus_sync(
            gio::BusType::System,
            gio::DBusProxyFlags::DO_NOT_LOAD_PROPERTIES,
            None,
            "org.freedesktop.login1",
            if session {
                "/org/freedesktop/login1/session/auto"
            } else {
                "/org/freedesktop/login1"
            },
            if session {
                "org.freedesktop.login1.Session"
            } else {
                "org.freedesktop.login1.Manager"
            },
            gio::Cancellable::NONE,
        )
        .map_err(|_| "Session controls are unavailable on this system.".to_string())?;
        let method = match self {
            Self::Lock => "Lock",
            Self::Sleep => "Suspend",
            Self::Restart => "Reboot",
            Self::ShutDown => "PowerOff",
            Self::LogOut => "Terminate",
        };
        let parameters = (!session).then(|| (true,).to_variant());
        proxy
            .call_sync(
                method,
                parameters.as_ref(),
                gio::DBusCallFlags::NONE,
                30_000,
                gio::Cancellable::NONE,
            )
            .map(|_| ())
            .map_err(|_| {
                format!(
                    "{} could not be completed. Check your session permissions.",
                    self.label()
                )
            })
    }
}
pub(super) struct AppleMenuState {
    pub focus: FocusHandle,
    confirmation: Option<SessionAction>,
    busy: bool,
}
impl AppleMenuState {
    pub fn new(cx: &mut Context<DesktopView>) -> Self {
        Self {
            focus: cx.focus_handle(),
            confirmation: None,
            busy: false,
        }
    }
}
impl DesktopView {
    fn apple_session_action(
        &mut self,
        action: SessionAction,
        confirmed: bool,
        cx: &mut Context<Self>,
    ) {
        if self.apple_menu.busy {
            return;
        }
        if action.needs_confirmation() && !confirmed {
            self.apple_menu.confirmation = Some(action);
            cx.notify();
            return;
        }
        self.apple_menu.busy = true;
        self.message = None;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { action.perform() })
                .await;
            let _ = this.update_in(cx, |s, w, cx| {
                s.apple_menu.busy = false;
                match result {
                    Ok(()) => w.remove_window(),
                    Err(error) => {
                        s.message = Some(error);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn apple_menu_view(&self, w: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if let Some(bounds) = super::displays::window_bounds(w, cx) {
            if w.bounds().size != bounds.size {
                w.resize(bounds.size);
            }
        }
        let p = Palette::new(self.snapshot(cx).preferences.dark);
        let mut content = menu("apple-menu-content", p).w(px(252.));
        if let Some(action) = self.apple_menu.confirmation {
            content =
                content
                    .child(
                        div()
                            .p(px(9.))
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{}?", action.label())),
                    )
                    .child(
                        div()
                            .px(px(9.))
                            .pb(px(9.))
                            .text_size(px(12.))
                            .child("Save your work before continuing."),
                    )
                    .child(menu_separator(p))
                    .child(
                        menu_item("apple-cancel", "Cancel", p)
                            .disabled(self.apple_menu.busy)
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.apple_menu.confirmation = None;
                                s.message = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        menu_item("apple-confirm", action.label(), p)
                            .disabled(self.apple_menu.busy)
                            .on_click(cx.listener(move |s, _, _, cx| {
                                s.apple_session_action(action, true, cx)
                            })),
                    );
        } else {
            content = content
                .child(
                    menu_item("apple-about", "About Macstill", p).on_click(cx.listener(
                        |s, _, w, cx| match macstill_apps::spawn_companion(
                            "macstill-info",
                            &["--about"],
                        ) {
                            Ok(()) => w.remove_window(),
                            Err(error) => {
                                s.message = Some(error.to_string());
                                cx.notify();
                            }
                        },
                    )),
                )
                .child(menu_separator(p))
                .child(
                    menu_item("apple-settings", "System Settings…", p).on_click(|_, w, cx| {
                        let _ = open_settings(&[], cx);
                        w.remove_window();
                    }),
                )
                .child(
                    menu_item("apple-applications", "Applications…", p).on_click(|_, w, cx| {
                        let _ = open_on(Mode::Launchpad, w.display(cx).map(|d| d.id()), cx);
                        w.remove_window();
                    }),
                )
                .child(menu_separator(p));
            for (index, action) in [
                SessionAction::Sleep,
                SessionAction::Restart,
                SessionAction::ShutDown,
            ]
            .into_iter()
            .enumerate()
            {
                let label = format!(
                    "{}{}",
                    action.label(),
                    if action.needs_confirmation() {
                        "…"
                    } else {
                        ""
                    }
                );
                content = content.child(
                    menu_item(("apple-power", index), label, p)
                        .disabled(self.apple_menu.busy)
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.apple_session_action(action, false, cx)
                        })),
                );
            }
            content = content
                .child(menu_separator(p))
                .child(
                    menu_item("apple-lock", "Lock Screen", p)
                        .disabled(self.apple_menu.busy)
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.apple_session_action(SessionAction::Lock, false, cx)
                        })),
                )
                .child(
                    menu_item("apple-logout", "Log Out…", p)
                        .disabled(self.apple_menu.busy)
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.apple_session_action(SessionAction::LogOut, false, cx)
                        })),
                );
        }
        if let Some(error) = self.message.clone() {
            content = content.child(div().p(px(9.)).text_size(px(11.)).child(error));
        }
        div()
            .relative()
            .size_full()
            .font_family(cx.global::<Fonts>().text.clone())
            .track_focus(&self.apple_menu.focus)
            .on_key_down(cx.listener(|s, e: &KeyDownEvent, w, cx| {
                if e.keystroke.key == "escape" {
                    if s.apple_menu.confirmation.take().is_some() {
                        s.message = None;
                        cx.notify();
                    } else {
                        w.remove_window();
                    }
                }
            }))
            .child(
                div()
                    .id("apple-menu-dismiss")
                    .absolute()
                    .inset_0()
                    .on_mouse_down(MouseButton::Left, |_, w, _| w.remove_window())
                    .on_mouse_down(MouseButton::Right, |_, w, _| w.remove_window()),
            )
            .child(
                div()
                    .absolute()
                    .top(px(32.))
                    .left(px(8.))
                    .w(px(252.))
                    .child(content),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn session_ending_actions_require_confirmation() {
        for action in [
            SessionAction::Restart,
            SessionAction::ShutDown,
            SessionAction::LogOut,
        ] {
            assert!(action.needs_confirmation());
        }
        assert!(!SessionAction::Lock.needs_confirmation());
        assert!(!SessionAction::Sleep.needs_confirmation());
    }
}
