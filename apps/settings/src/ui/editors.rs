//! Native settings sheets. Drafts stay local until a validated request is queued.
use super::widgets::*;
use super::*;
use crate::system_backend::Job;
use macstill_system::{
    ConsoleKeyboard, IpAddress, IpConfiguration, IpFamily, PowerAction, SystemCommand,
    WifiConnection, WifiSecurity, X11Keyboard,
};
use swiftui_gpui::{settings_divider, settings_group, settings_row};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Security {
    Open,
    Personal,
    Sae,
}
pub(super) fn wifi_security(ap: &macstill_system::AccessPoint) -> Option<Security> {
    let flags = ap.wpa_flags | ap.rsn_flags;
    if flags & 0x100 != 0 {
        Some(Security::Personal)
    } else if flags & 0x400 != 0 {
        Some(Security::Sae)
    } else if ap.flags & 1 == 0 && flags == 0 {
        Some(Security::Open)
    } else {
        None
    }
}
#[derive(Clone)]
pub(super) enum EditorKind {
    Wifi {
        device: String,
        ssid: Option<(String, Vec<u8>)>,
    },
    Ip {
        connection: String,
        family: IpFamily,
    },
    Locale {
        original: Vec<String>,
    },
    X11,
    Console,
    Time,
    AdapterAlias(String),
    DeviceAlias(String),
    DiscoverableTimeout(String),
}
pub(super) struct Field {
    label: &'static str,
    pub input: Entity<InputState>,
}
pub(super) struct Editor {
    pub kind: EditorKind,
    pub fields: Vec<Field>,
    pub choice: usize,
    pub switches: Vec<(&'static str, bool)>,
    pub error: Option<String>,
    pub pending: bool,
    pub revision: u64,
    _subscriptions: Vec<Subscription>,
}
#[derive(Clone)]
pub(super) enum Confirmation {
    ForgetNetwork { path: String, name: String },
    ForgetBluetooth { path: String, name: String },
    Power(PowerAction),
    LocalRtc(bool),
}
impl Confirmation {
    fn text(&self) -> (String, String, &'static str) {
        match self {
            Self::ForgetNetwork { name, .. } => (format!("Forget “{name}”?"), "This removes the saved network and its password. You’ll need to join it again to reconnect.".into(), "Forget"),
            Self::ForgetBluetooth { name, .. } => (format!("Forget “{name}”?"), "This removes the pairing information. Pair this accessory again before using it.".into(), "Forget Device"),
            Self::Power(action) => (format!("{} this computer?", power_label(*action)), "Save your work before continuing. Other users and applications may be using this computer.".into(), power_label(*action)),
            Self::LocalRtc(local) => ("Change the hardware clock?".into(), if *local { "Use local time only if another operating system needs it. Daylight saving changes may affect the clock." } else { "Use UTC for the hardware clock. The displayed time zone will stay the same." }.into(), "Change"),
        }
    }
    fn command(&self) -> SystemCommand {
        match self {
            Self::ForgetNetwork { path, .. } => SystemCommand::ForgetConnection {
                connection: path.clone(),
            },
            Self::ForgetBluetooth { path, .. } => SystemCommand::RemoveBluetoothDevice {
                device: path.clone(),
            },
            Self::Power(action) => SystemCommand::PowerAction(*action),
            Self::LocalRtc(value) => SystemCommand::SetLocalRtc(*value),
        }
    }
}
pub(super) fn power_label(action: PowerAction) -> &'static str {
    match action {
        PowerAction::PowerOff => "Shut Down",
        PowerAction::Reboot => "Restart",
        PowerAction::Suspend => "Sleep",
        PowerAction::Hibernate => "Hibernate",
        PowerAction::HybridSleep => "Hybrid Sleep",
        PowerAction::SuspendThenHibernate => "Sleep, Then Hibernate",
    }
}
impl SettingsView {
    pub(super) fn open_editor(
        &mut self,
        kind: EditorKind,
        fields: Vec<(&'static str, &'static str, String)>,
        choice: usize,
        switches: Vec<(&'static str, bool)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_wifi = matches!(kind, EditorKind::Wifi { .. });
        let mut subscriptions = vec![];
        let fields: Vec<_> = fields
            .into_iter()
            .enumerate()
            .map(|(i, (label, hint, value))| {
                let input = cx.new(|cx| {
                    let mut input = InputState::new(window, cx)
                        .placeholder(hint)
                        .masked(is_wifi && i == 2);
                    input.set_value(value, window, cx);
                    input
                });
                subscriptions.push(cx.observe(&input, |_, _, cx| cx.notify()));
                Field { label, input }
            })
            .collect();
        self.editor = Some(Editor {
            kind,
            fields,
            choice,
            switches,
            error: None,
            pending: false,
            revision: 0,
            _subscriptions: subscriptions,
        });
        self.form = None;
        self.confirmation = None;
        cx.notify();
    }
    pub(super) fn join_network(
        &mut self,
        device: String,
        ssid: Option<(String, Vec<u8>)>,
        security: Security,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = ssid
            .as_ref()
            .map(|(name, _)| name.clone())
            .unwrap_or_default();
        self.open_editor(
            EditorKind::Wifi { device, ssid },
            vec![
                ("Network name", "Wi-Fi network name", name),
                ("Profile name", "A name for this network", String::new()),
                ("Password", "Wi-Fi password", String::new()),
            ],
            match security {
                Security::Open => 0,
                Security::Personal => 1,
                Security::Sae => 2,
            },
            vec![
                ("Hidden network", false),
                ("Automatically join this network", true),
            ],
            window,
            cx,
        );
    }
    pub(super) fn open_connection(&mut self, path: String, cx: &mut Context<Self>) {
        if self.system_state(cx).busy {
            return;
        }
        self.connection_details = Some(path.clone());
        self.wifi_details = None;
        self.editor = None;
        self.confirmation = None;
        if let Err(error) = self.store.read(cx).system.query_connection(path) {
            self.message = Some(error.to_string());
        }
        self.sync_system(cx);
    }
    pub(super) fn sync_system(&mut self, cx: &mut Context<Self>) {
        let next = self.store.read(cx).system.snapshot();
        self.store.update(cx, |s, cx| {
            s.system_snapshot = next;
            cx.notify();
        });
        cx.notify();
    }
    pub(super) fn edit_ip(
        &mut self,
        connection: String,
        family: IpFamily,
        config: IpConfiguration,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let choice = match config.method.as_str() {
            "manual" => 1,
            "link-local" => 2,
            "disabled" => 3,
            "dhcp" => 4,
            _ => 0,
        };
        self.open_editor(
            EditorKind::Ip { connection, family },
            vec![
                (
                    "IP addresses",
                    "192.168.1.20/24, 192.168.1.21/24",
                    config
                        .addresses
                        .iter()
                        .map(|a| format!("{}/{}", a.address, a.prefix))
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
                (
                    "Router",
                    "Optional gateway address",
                    config.gateway.unwrap_or_default(),
                ),
                (
                    "DNS servers",
                    "Separate addresses with commas",
                    config.dns.join(", "),
                ),
                (
                    "Search domains",
                    "example.org, ~internal.example",
                    config.dns_search.join(", "),
                ),
            ],
            choice,
            vec![("Use automatic DNS", !config.ignore_auto_dns)],
            window,
            cx,
        );
    }
    fn editor_job(&self, cx: &App) -> anyhow::Result<Job> {
        let e = self.editor.as_ref().unwrap();
        let value = |i: usize| e.fields[i].input.read(cx).value().trim().to_string();
        let list = |i: usize| {
            value(i)
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        let command = match &e.kind {
            EditorKind::Wifi { device, ssid } => {
                let name = value(0);
                anyhow::ensure!(!name.is_empty(), "Enter a network name.");
                let bytes = ssid
                    .as_ref()
                    .filter(|(original, _)| original == &name)
                    .map(|(_, bytes)| bytes.clone())
                    .unwrap_or_else(|| name.as_bytes().to_vec());
                anyhow::ensure!(
                    bytes.len() <= 32,
                    "The network name must be at most 32 bytes."
                );
                let password = e.fields[2].input.read(cx).value().to_string();
                let security = match e.choice {
                    0 => WifiSecurity::Open {},
                    1 => {
                        anyhow::ensure!(
                            ((8..=63).contains(&password.len())
                                && password.bytes().all(|c| (32..=126).contains(&c)))
                                || (password.len() == 64
                                    && password.bytes().all(|c| c.is_ascii_hexdigit())),
                            "Use 8–63 printable characters, or a 64-digit hexadecimal key."
                        );
                        WifiSecurity::WpaPsk { password }
                    }
                    _ => {
                        anyhow::ensure!(
                            !password.is_empty()
                                && password.len() <= 1024
                                && !password.chars().any(char::is_control),
                            "Enter a valid WPA3 password."
                        );
                        WifiSecurity::Sae { password }
                    }
                };
                let profile = value(1);
                return Ok(Job::JoinWifi {
                    device: device.clone(),
                    config: WifiConnection {
                        name: if profile.is_empty() { name } else { profile },
                        ssid: bytes,
                        hidden: e.switches[0].1,
                        autoconnect: e.switches[1].1,
                        security,
                    },
                });
            }
            EditorKind::Ip { connection, family } => {
                let method = ["auto", "manual", "link-local", "disabled", "dhcp"][e.choice];
                let empty = matches!(method, "disabled" | "link-local");
                let mut addresses = vec![];
                if !empty {
                    for item in list(0) {
                        let (ip, prefix) = item.split_once('/').ok_or_else(|| {
                            anyhow::anyhow!("Give every address a prefix, such as 192.168.1.20/24.")
                        })?;
                        let address: std::net::IpAddr = ip
                            .parse()
                            .map_err(|_| anyhow::anyhow!("Enter a valid IP address."))?;
                        let prefix: u32 = prefix
                            .parse()
                            .map_err(|_| anyhow::anyhow!("Enter a numeric prefix length."))?;
                        anyhow::ensure!(
                            address.is_ipv4() == (*family == IpFamily::Ipv4),
                            "Use addresses from the selected IP version."
                        );
                        anyhow::ensure!(
                            prefix > 0
                                && prefix <= if *family == IpFamily::Ipv4 { 32 } else { 128 },
                            "The prefix length is outside the allowed range."
                        );
                        addresses.push(IpAddress {
                            address: ip.into(),
                            prefix,
                        });
                    }
                }
                anyhow::ensure!(
                    method != "manual" || !addresses.is_empty(),
                    "Manual configuration needs at least one IP address."
                );
                let gateway = if empty || value(1).is_empty() {
                    None
                } else {
                    Some(value(1))
                };
                let dns = if empty { vec![] } else { list(2) };
                for ip in dns.iter().chain(gateway.iter()) {
                    let parsed: std::net::IpAddr = ip
                        .parse()
                        .map_err(|_| anyhow::anyhow!("Enter a valid router and DNS address."))?;
                    anyhow::ensure!(
                        parsed.is_ipv4() == (*family == IpFamily::Ipv4),
                        "The router and DNS servers must use the selected IP version."
                    );
                }
                SystemCommand::SetConnectionIp {
                    connection: connection.clone(),
                    family: *family,
                    config: IpConfiguration {
                        method: method.into(),
                        addresses,
                        gateway,
                        dns,
                        dns_search: if empty { vec![] } else { list(3) },
                        ignore_auto_dns: !empty && !e.switches[0].1,
                    },
                }
            }
            EditorKind::Locale { original } => {
                let keys = [
                    "LANG",
                    "LC_TIME",
                    "LC_NUMERIC",
                    "LC_MONETARY",
                    "LC_MEASUREMENT",
                ];
                let mut assignments: Vec<_> = original
                    .iter()
                    .filter(|entry| !keys.iter().any(|key| entry.starts_with(&format!("{key}="))))
                    .cloned()
                    .collect();
                for (i, key) in keys.into_iter().enumerate() {
                    let locale = value(i);
                    if !locale.is_empty() {
                        assignments.push(format!("{key}={locale}"));
                    }
                }
                SystemCommand::SetLocale(assignments)
            }
            EditorKind::X11 => {
                anyhow::ensure!(!value(0).is_empty(), "Enter at least one keyboard layout.");
                SystemCommand::SetX11Keyboard {
                    keyboard: X11Keyboard {
                        layout: value(0),
                        model: value(1),
                        variant: value(2),
                        options: value(3),
                    },
                    convert: e.switches[0].1,
                }
            }
            EditorKind::Console => SystemCommand::SetConsoleKeyboard {
                keyboard: ConsoleKeyboard {
                    keymap: value(0),
                    keymap_toggle: value(1),
                },
                convert: e.switches[0].1,
            },
            EditorKind::Time => {
                let date = chrono::NaiveDateTime::parse_from_str(&value(0), "%Y-%m-%d %H:%M:%S")
                    .map_err(|_| anyhow::anyhow!("Use YYYY-MM-DD HH:MM:SS, in UTC."))?;
                let unix_microseconds = date.and_utc().timestamp_micros();
                anyhow::ensure!(
                    unix_microseconds >= 0,
                    "Choose a date after January 1, 1970."
                );
                SystemCommand::SetTime { unix_microseconds }
            }
            EditorKind::AdapterAlias(adapter) => SystemCommand::SetBluetoothAdapterAlias {
                adapter: adapter.clone(),
                alias: value(0),
            },
            EditorKind::DeviceAlias(device) => SystemCommand::SetBluetoothDeviceAlias {
                device: device.clone(),
                alias: value(0),
            },
            EditorKind::DiscoverableTimeout(adapter) => {
                SystemCommand::SetBluetoothDiscoverableTimeout {
                    adapter: adapter.clone(),
                    seconds: value(0).parse().map_err(|_| {
                        anyhow::anyhow!("Enter a whole number of seconds. Zero means no timeout.")
                    })?,
                }
            }
        };
        Ok(Job::Apply(command))
    }
    pub(super) fn submit_editor(&mut self, cx: &mut Context<Self>) {
        let job = match self.editor_job(cx) {
            Ok(job) => job,
            Err(error) => {
                self.editor.as_mut().unwrap().error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let result = match job {
            Job::Apply(command) => self.store.read(cx).system.apply(command),
            Job::JoinWifi { config, device } => {
                self.store.read(cx).system.join_wifi(config, device)
            }
            Job::Query(_) => unreachable!(),
            job @ (Job::AirDrop(_) | Job::AirDropDirectory(_)) => {
                self.store.read(cx).system.submit(job)
            }
            job @ (Job::LoginEnabled(..) | Job::AddLogin(_)) => {
                self.store.read(cx).system.submit(job)
            }
            Job::ArrangeDisplays(positions) => {
                self.store.read(cx).system.arrange_displays(positions)
            }
        };
        match result {
            Ok(()) => {
                let revision = self.system_state(cx).revision;
                let e = self.editor.as_mut().unwrap();
                e.pending = true;
                e.revision = revision;
                e.error = None;
                for field in &e.fields {
                    field
                        .input
                        .update(cx, |input, cx| input.set_readonly(true, cx));
                }
            }
            Err(error) => self.editor.as_mut().unwrap().error = Some(error.to_string()),
        }
        self.sync_system(cx);
    }
    pub(super) fn editor_sheet(
        &self,
        p: Palette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let e = self.editor.as_ref().unwrap();
        let (title, detail, label) = match &e.kind {
            EditorKind::Wifi { .. } => (
                "Join Wi-Fi Network",
                "Enter the network’s name and security information.",
                "Join",
            ),
            EditorKind::Ip { family, .. } => (
                if *family == IpFamily::Ipv4 {
                    "Configure IPv4"
                } else {
                    "Configure IPv6"
                },
                "Changes are saved for this network. Reconnect to use the new configuration.",
                "Save",
            ),
            EditorKind::Locale { .. } => (
                "Language & Region",
                "Set the system language and regional formats. Changes apply to newly started applications.",
                "Save",
            ),
            EditorKind::X11 => (
                "Input Sources",
                "Set the default keyboard layouts for new X11 sessions. A running Wayland session manages its own input sources.",
                "Save",
            ),
            EditorKind::Console => (
                "Console Keyboard",
                "Choose the keyboard maps used by Linux virtual consoles.",
                "Save",
            ),
            EditorKind::Time => (
                "Set Date & Time",
                "Enter the date and time in UTC. Automatic time must be turned off first.",
                "Set",
            ),
            EditorKind::AdapterAlias(_) | EditorKind::DeviceAlias(_) => (
                "Rename Bluetooth Device",
                "Choose a name for this device. Leave it empty to restore the original name.",
                "Save",
            ),
            EditorKind::DiscoverableTimeout(_) => (
                "Discoverable Duration",
                "Choose how long this computer stays visible. Zero means no timeout.",
                "Save",
            ),
        };
        let busy = e.pending || self.system_state(cx).busy;
        let mut content = div().flex().flex_col().gap(px(14.));
        let choices: Vec<&str> = match &e.kind {
            EditorKind::Wifi { .. } => vec!["Open", "WPA Personal", "WPA3 Personal"],
            EditorKind::Ip { family, .. } => {
                let mut choices = vec!["Automatic", "Manual", "Link-Local", "Off"];
                if *family == IpFamily::Ipv6 {
                    choices.push("DHCP");
                }
                choices
            }
            _ => vec![],
        };
        if !choices.is_empty() {
            let mut segmented = div().flex().flex_wrap().gap(px(4.));
            for (i, label) in choices.into_iter().enumerate() {
                let selected = e.choice == i;
                segmented = segmented.child(
                    button(("draft-choice", i), label, p)
                        .disabled(busy)
                        .when(selected, |b| {
                            b.bg(rgb(p.accent)).text_color(rgb(p.accent_text))
                        })
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.editor.as_mut().unwrap().choice = i;
                            cx.notify();
                        })),
                );
            }
            content = content.child(segmented);
        }
        for (i, field) in e.fields.iter().enumerate() {
            if matches!(e.kind, EditorKind::Wifi { .. }) && i == 2 && e.choice == 0 {
                continue;
            }
            if matches!(e.kind, EditorKind::Ip { .. }) && matches!(e.choice, 2 | 3) {
                continue;
            }
            content = content.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(5.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(field.label),
                    )
                    .child(text_field(("draft-field", i), &field.input, p, cx).w_full()),
            );
        }
        for (i, (label, value)) in e.switches.iter().enumerate() {
            if matches!(e.kind, EditorKind::Ip { .. }) && matches!(e.choice, 2 | 3) {
                continue;
            }
            content = content.child(
                settings_row(*label, p).px(px(0.)).child(
                    toggle(("draft-toggle", i), *value, p)
                        .disabled(busy)
                        .on_change(cx.listener(move |s, value: &bool, _, cx| {
                            s.editor.as_mut().unwrap().switches[i].1 = *value;
                            cx.notify();
                        })),
                ),
            );
        }
        overlay()
            .when(self.confirmation.is_none(), |d| {
                d.track_focus(&self.sheet_focus)
            })
            .child(
                panel(p)
                    .w(px(440.))
                    .max_h(window.viewport_size().height - px(40.))
                    .rounded(px(12.))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(
                        div()
                            .p(px(22.))
                            .pb(px(14.))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(17.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(caption(detail, p)),
                    )
                    .when_some(e.error.clone(), |body, error| {
                        body.child(
                            div()
                                .id("draft-error")
                                .px(px(22.))
                                .pb(px(10.))
                                .text_size(px(12.))
                                .text_color(rgb(if p.dark { 0xffa697 } else { 0xb53225 }))
                                .child(error),
                        )
                    })
                    .child(
                        div()
                            .id("draft-scroll")
                            .min_h_0()
                            .overflow_y_scroll()
                            .px(px(22.))
                            .pb(px(18.))
                            .child(content),
                    )
                    .child(
                        div()
                            .border_t_1()
                            .border_color(rgb(p.line))
                            .flex_shrink_0()
                            .p(px(14.))
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                button("cancel-draft", "Cancel", p)
                                    .disabled(e.pending)
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        s.editor = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                button(
                                    "save-draft",
                                    if e.pending { "Applying…" } else { label },
                                    p,
                                )
                                .disabled(busy)
                                .bg(rgb(p.accent))
                                .text_color(rgb(p.accent_text))
                                .on_click(cx.listener(|s, _, _, cx| s.submit_editor(cx))),
                            ),
                    ),
            )
            .into_any_element()
    }
    pub(super) fn confirmation_sheet(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let (title, detail, label) = self.confirmation.as_ref().unwrap().text();
        overlay()
            .track_focus(&self.sheet_focus)
            .child(
                panel(p)
                    .w(px(390.))
                    .rounded(px(12.))
                    .p(px(24.))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(div().mx_auto().child(symbol("info", 36., p.muted)))
                    .child(
                        div()
                            .text_center()
                            .text_size(px(17.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(caption(detail, p).text_center())
                    .child(
                        div()
                            .mt(px(6.))
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(button("cancel-confirmation", "Cancel", p).on_click(
                                cx.listener(|s, _, _, cx| {
                                    s.confirmation = None;
                                    cx.notify();
                                }),
                            ))
                            .child(
                                button("confirm-action", label, p)
                                    .bg(rgb(0xc94139))
                                    .text_color(rgb(0xffffff))
                                    .disabled(self.system_state(cx).busy)
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        let command = s.confirmation.take().unwrap().command();
                                        if matches!(command, SystemCommand::ForgetConnection { .. })
                                        {
                                            s.connection_details = None;
                                            s.store.read(cx).system.clear_connection();
                                        }
                                        if matches!(
                                            command,
                                            SystemCommand::RemoveBluetoothDevice { .. }
                                        ) {
                                            s.bluetooth_details = None;
                                        }
                                        s.apply_system(command, cx);
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
    pub(super) fn connection_sheet(
        &mut self,
        p: Palette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.page == Page::Wifi {
            return self.wifi_connection_sheet(p, window, cx);
        }
        let state = self.system_state(cx);
        let path = self.connection_details.as_ref().unwrap();
        let mut content = div().flex().flex_col().gap(px(14.));
        let mut title = "Network Details".to_string();
        if state.connection_path.as_ref() == Some(path)
            && let Some(connection) = &state.connection.value
        {
            title = connection.name.clone();
            let autoconnect_path = path.clone();
            content = content.child(
                settings_group(p).child(
                    settings_row("Automatically connect", p).child(
                        toggle("connection-autoconnect", connection.autoconnect, p)
                            .disabled(state.busy)
                            .on_change(cx.listener(move |s, value: &bool, _, cx| {
                                s.apply_system(
                                    SystemCommand::SetConnectionAutoconnect {
                                        connection: autoconnect_path.clone(),
                                        enabled: *value,
                                    },
                                    cx,
                                )
                            })),
                    ),
                ),
            );
            for (family, label, config) in [
                (IpFamily::Ipv4, "TCP/IP · IPv4", &connection.ipv4),
                (IpFamily::Ipv6, "TCP/IP · IPv6", &connection.ipv6),
            ] {
                let config = config.clone();
                let path = path.clone();
                content = content.child(
                    settings_group(p)
                        .child(
                            settings_row(label, p).child(caption(
                                config
                                    .as_ref()
                                    .map(|c| c.method.clone())
                                    .unwrap_or_else(|| "Not configured".into()),
                                p,
                            )),
                        )
                        .when_some(config.as_ref(), |group, config| {
                            group
                                .child(settings_divider(p))
                                .child(value_row(
                                    "IP addresses",
                                    config
                                        .addresses
                                        .iter()
                                        .map(|a| format!("{}/{}", a.address, a.prefix))
                                        .collect::<Vec<_>>()
                                        .join(", "),
                                    p,
                                ))
                                .child(settings_divider(p))
                                .child(value_row(
                                    "Router",
                                    config.gateway.clone().unwrap_or_else(|| "None".into()),
                                    p,
                                ))
                                .child(settings_divider(p))
                                .child(value_row(
                                    "DNS servers",
                                    if config.dns.is_empty() {
                                        "Automatic".into()
                                    } else {
                                        config.dns.join(", ")
                                    },
                                    p,
                                ))
                        })
                        .child(
                            div().px(px(12.)).pb(px(10.)).flex().justify_end().child(
                                button(eid("edit-ip", label), "Configure…", p)
                                    .disabled(state.busy)
                                    .on_click(cx.listener(move |s, _, w, cx| {
                                        let config = config.clone().unwrap_or(IpConfiguration {
                                            method: "auto".into(),
                                            addresses: vec![],
                                            gateway: None,
                                            dns: vec![],
                                            dns_search: vec![],
                                            ignore_auto_dns: false,
                                        });
                                        s.edit_ip(path.clone(), family, config, w, cx);
                                    })),
                            ),
                        ),
                );
            }
            let path = path.clone();
            let name = connection.name.clone();
            content = content.child(
                button("forget-network", "Forget This Network…", p)
                    .text_color(rgb(0xd23e35))
                    .disabled(state.busy)
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.confirmation = Some(Confirmation::ForgetNetwork {
                            path: path.clone(),
                            name: name.clone(),
                        });
                        cx.notify();
                    })),
            );
        } else {
            content = content.child(unavailable(&state.connection, "Network configuration", p));
        }
        overlay()
            .when(self.editor.is_none() && self.confirmation.is_none(), |d| {
                d.track_focus(&self.sheet_focus)
            })
            .child(
                panel(p)
                    .w(px(490.))
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
                            .id("connection-sheet-scroll")
                            .min_h_0()
                            .overflow_y_scroll()
                            .px(px(20.))
                            .pb(px(20.))
                            .child(content),
                    )
                    .child(
                        div()
                            .p(px(14.))
                            .flex_shrink_0()
                            .border_t_1()
                            .border_color(rgb(p.line))
                            .flex()
                            .justify_end()
                            .child(button("close-connection", "Done", p).on_click(cx.listener(
                                |s, _, _, cx| {
                                    s.connection_details = None;
                                    s.store.read(cx).system.clear_connection();
                                    cx.notify();
                                },
                            ))),
                    ),
            )
            .into_any_element()
    }
}
pub(super) fn overlay() -> Div {
    div()
        .absolute()
        .inset_0()
        .occlude()
        .bg(rgba(0x00000035))
        .flex()
        .items_center()
        .justify_center()
}
