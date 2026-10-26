use super::widgets::*;
use super::*;
use crate::system_backend::Job;
use swiftui_gpui::{settings_divider, settings_group, settings_row};
fn capacity(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1_000_000_000.)
}
impl SettingsView {
    pub(super) fn storage(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let state = self.system_state(cx);
        let Some(volumes) = state.storage.value else {
            return unavailable(&state.storage, "Reading storage…", p);
        };
        let mut content = stack();
        if volumes.is_empty() {
            return content
                .child(caption("No local disks found.", p))
                .into_any_element();
        }
        for volume in volumes {
            let path = volume.mount.clone();
            let reserved = volume
                .total
                .saturating_sub(volume.used)
                .saturating_sub(volume.available);
            let fraction = (volume.used as f64 / volume.total.max(1) as f64).clamp(0., 1.) as f32;
            content = content.child(
                settings_group(p)
                    .child(
                        settings_row(
                            if volume.mount == "/" {
                                "Startup disk".to_owned()
                            } else {
                                volume.mount.clone()
                            },
                            p,
                        )
                        .child(caption(capacity(volume.total), p)),
                    )
                    .child(
                        div().px(px(14.)).py(px(10.)).child(
                            div()
                                .h(px(16.))
                                .w_full()
                                .rounded(px(5.))
                                .overflow_hidden()
                                .bg(rgb(p.line))
                                .child(div().h_full().w(relative(fraction)).bg(rgb(0x007aff))),
                        ),
                    )
                    .child(value_row("Used", capacity(volume.used), p))
                    .child(value_row("Available", capacity(volume.available), p))
                    .when(reserved > 0, |group| {
                        group.child(value_row("Reserved by system", capacity(reserved), p))
                    })
                    .child(settings_divider(p))
                    .child(
                        settings_row("Location", p)
                            .child(caption(volume.mount, p))
                            .child(button(eid("storage-open", &path), "Open…", p).on_click(
                                cx.listener(move |_, _, _, cx| {
                                    cx.open_url(&macstill_system::storage::location_uri(&path))
                                }),
                            )),
                    )
                    .child(value_row("Device", volume.device, p)),
            );
        }
        content.into_any_element()
    }
    pub(super) fn login_items(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let state = self.system_state(cx);
        let Some(login) = state.login_items.value else {
            return unavailable(&state.login_items, "Reading login items…", p);
        };
        let mut group = settings_group(p);
        if login.items.is_empty() {
            group = group.child(settings_row("No login items", p));
        }
        for (index, item) in login.items.into_iter().enumerate() {
            if index > 0 {
                group = group.child(settings_divider(p));
            }
            let id = item.id.clone();
            group = group.child(
                settings_row(item.name, p)
                    .when(!item.applicable, |row| {
                        row.child(caption("Unavailable in this session", p))
                    })
                    .child(
                        toggle(eid("login-enabled", &item.id), item.enabled, p)
                            .disabled(state.busy || !item.applicable)
                            .on_change(cx.listener(move |s, enabled: &bool, _, cx| {
                                if let Err(error) = s
                                    .store
                                    .read(cx)
                                    .system
                                    .submit(Job::LoginEnabled(id.clone(), *enabled))
                                {
                                    s.message = Some(error.to_string());
                                }
                                cx.notify();
                            })),
                    ),
            );
        }
        let mut available = settings_group(p)
            .id("login-available-apps")
            .max_h(px(320.))
            .overflow_y_scroll();
        for (index, app) in login.applications.into_iter().enumerate() {
            if index > 0 {
                available = available.child(settings_divider(p));
            }
            let id = app.id.clone();
            available = available.child(
                settings_row(app.name, p).child(
                    button(eid("login-add", &app.id), "Add", p)
                        .disabled(state.busy)
                        .on_click(cx.listener(move |s, _, _, cx| {
                            if let Err(error) =
                                s.store.read(cx).system.submit(Job::AddLogin(id.clone()))
                            {
                                s.message = Some(error.to_string());
                            }
                            cx.notify();
                        })),
                ),
            );
        }
        stack().child(section("Open at Login", p))
            .child(caption("These applications open automatically when you log in. Changes take effect at your next login.", p))
            .child(group)
            .child(button("login-show-apps", if self.show_login_apps { "Done" } else { "Add Application…" }, p)
                .on_click(cx.listener(|s, _, _, cx| { s.show_login_apps = !s.show_login_apps; cx.notify(); })))
            .when(self.show_login_apps, |content| content.child(section("Choose an Application", p)).child(available))
            .into_any_element()
    }
}
