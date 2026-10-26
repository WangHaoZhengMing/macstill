use super::*;
use super::{
    editors::{Confirmation, EditorKind, power_label},
    widgets::*,
};
use macstill_system::SystemCommand;
use swiftui_gpui::{settings_divider, settings_group, settings_row};

const LOCALES: [(&str, &str); 5] = [
    ("LANG", "System language"),
    ("LC_TIME", "Date & time format"),
    ("LC_NUMERIC", "Number format"),
    ("LC_MONETARY", "Currency format"),
    ("LC_MEASUREMENT", "Measurement format"),
];
fn locale_value(assignments: &[String], key: &str) -> String {
    assignments
        .iter()
        .find_map(|v| v.strip_prefix(&format!("{key}=")).map(str::to_string))
        .unwrap_or_default()
}
impl SettingsView {
    pub(super) fn language_region(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let state = self.system_state(cx);
        let Some(locale) = state.system.locale.value else {
            return unavailable(&state.system.locale, "Language & Region is unavailable", p);
        };
        let mut content = stack().child(hero(
            Page::LanguageRegion,
            "Choose the language and regional formats for this computer.",
            p,
        ));
        let mut group = settings_group(p);
        let primary = locale_value(&locale.locale, "LANG");
        for (i, (key, label)) in LOCALES.iter().enumerate() {
            let value = locale_value(&locale.locale, key);
            if i > 0 {
                group = group.child(settings_divider(p));
            }
            group = group.child(value_row(
                label,
                if value.is_empty() {
                    if primary.is_empty() {
                        "System default".into()
                    } else {
                        primary.clone()
                    }
                } else {
                    value
                },
                p,
            ));
        }
        let original = locale.locale;
        content=content.child(group).child(div().flex().justify_end().child(button("edit-language-region","Language & Region…",p).disabled(state.busy).on_click(cx.listener(move|s,_,w,cx|{
            let fields=LOCALES.iter().map(|(key,label)|(*label,if *key=="LANG"{"en_US.UTF-8"}else{"Inherit system language"},locale_value(&original,key))).collect();
            s.open_editor(EditorKind::Locale {original:original.clone()},fields,0,vec![],w,cx);
        })))).child(caption("These defaults apply to new applications and login sessions. Your desktop may also have its own language preference.",p).px(px(12.)));
        content.into_any_element()
    }
    pub(super) fn keyboard(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let state = self.system_state(cx);
        let Some(locale) = state.system.locale.value else {
            return unavailable(&state.system.locale, "Keyboard settings are unavailable", p);
        };
        let keyboard = locale.x11;
        let console = locale.console;
        let x11 = keyboard.clone();
        let vc = console.clone();
        stack().child(hero(Page::Keyboard,"Choose the keyboard layouts used by this computer.",p))
            .child(section("Text Input",p))
            .child(settings_group(p).child(settings_row("Input Sources",p).child(caption(if keyboard.layout.is_empty(){"Not configured".into()}else{keyboard.layout.clone()},p)).child(button("edit-input-sources","Edit…",p).disabled(state.busy).on_click(cx.listener(move|s,_,w,cx|s.open_editor(EditorKind::X11,vec![("Layouts","us,de",x11.layout.clone()),("Keyboard model","pc105",x11.model.clone()),("Variants","For example: ,nodeadkeys",x11.variant.clone()),("Options","For example: grp:alt_shift_toggle",x11.options.clone())],0,vec![("Also update console keyboard",false)],w,cx)))))
                .child(settings_divider(p)).child(value_row("Keyboard model",if keyboard.model.is_empty(){"Default".into()}else{keyboard.model},p))
                .child(settings_divider(p)).child(value_row("Layout variants",if keyboard.variant.is_empty(){"Default".into()}else{keyboard.variant},p)))
            .child(caption("Sets keyboard defaults for new X11 sessions. Input sources in a running Wayland desktop are managed by that desktop.",p).px(px(12.)))
            .child(section("Console Keyboard",p))
            .child(settings_group(p).child(settings_row("Keymap",p).child(caption(if console.keymap.is_empty(){"Default".into()}else{console.keymap},p)).child(button("edit-console-keyboard","Edit…",p).disabled(state.busy).on_click(cx.listener(move|s,_,w,cx|s.open_editor(EditorKind::Console,vec![("Keymap","us",vc.keymap.clone()),("Toggle keymap","Optional alternate keymap",vc.keymap_toggle.clone())],0,vec![("Also update X11 keyboard",false)],w,cx))))))
            .into_any_element()
    }
    pub(super) fn lock_screen(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let state = self.system_state(cx);
        let mut content = stack().child(hero(
            Page::LockScreen,
            "Lock your session or put this computer to sleep.",
            p,
        ));
        if let Some(session) = &state.system.session.value {
            content = content.child(
                settings_group(p)
                    .child(value_row(
                        "Current session",
                        format!("{} · {}", session.user_name, session.kind),
                        p,
                    ))
                    .child(settings_divider(p))
                    .child(
                        settings_row("Screen lock", p)
                            .child(caption(
                                if session.locked { "Locked" } else { "Unlocked" },
                                p,
                            ))
                            .child(
                                button("lock-session", "Lock Screen", p)
                                    .disabled(state.busy || session.locked)
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        s.apply_system(SystemCommand::LockSession, cx)
                                    })),
                            ),
                    ),
            );
        } else {
            content = content.child(unavailable(
                &state.system.session,
                "Current login session",
                p,
            ));
        }
        content = content
            .child(
                caption(
                    "Lock Screen uses the screen locker registered with your desktop session.",
                    p,
                )
                .px(px(12.)),
            )
            .child(section("Sleep & Power", p));
        if let Some(actions) = &state.system.power_actions.value {
            let mut group = settings_group(p);
            for (i, capability) in actions.capabilities.iter().enumerate() {
                let action = capability.action;
                let availability = capability.availability.value.as_deref();
                let enabled = matches!(availability, Some("yes" | "challenge" | "inhibited"));
                let description = match availability {
                    Some("yes") => "Available",
                    Some("challenge") => "Requires authentication",
                    Some("inhibited") => "An application is preventing this action",
                    Some("no" | "inhibitor-blocked" | "challenge-inhibitor-blocked") => {
                        "Not currently allowed"
                    }
                    Some("na") => "Not supported",
                    _ => "Unavailable",
                };
                if i > 0 {
                    group = group.child(settings_divider(p));
                }
                let label = power_label(action);
                group = group.child(
                    settings_row(label, p).child(caption(description, p)).child(
                        button(
                            eid("power-action", &format!("{action:?}")),
                            format!("{label}…"),
                            p,
                        )
                        .disabled(state.busy || !enabled)
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.confirmation = Some(Confirmation::Power(action));
                            cx.notify();
                        })),
                    ),
                );
            }
            content = content.child(group);
            if let Some(inhibitors) = &actions.inhibitors.value
                && !inhibitors.is_empty()
            {
                content = content.child(section("Apps Keeping Your Computer Awake", p));
                let mut group = settings_group(p);
                for inhibitor in inhibitors {
                    group = group
                        .child(
                            settings_row(inhibitor.who.clone(), p)
                                .child(caption(inhibitor.why.clone(), p)),
                        )
                        .child(settings_divider(p));
                }
                content = content.child(group);
            }
            if let Some(error) = &actions.inhibitors.error {
                content = content.child(caption(
                    format!(
                        "Couldn’t check which apps are preventing sleep: {}",
                        error.message
                    ),
                    p,
                ));
            }
        } else {
            content = content.child(unavailable(
                &state.system.power_actions,
                "Power actions are unavailable",
                p,
            ));
        }
        content.into_any_element()
    }
}
