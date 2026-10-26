//! Full D-Bus envelopes, validation, and dispatch for the extended settings API.
//! All transports and audio runners are isolated: no real system writes.
use gio::glib::{
    Variant,
    variant::{ObjectPath, ToVariant},
};
use macstill_system::{
    audio::CommandRunner,
    dbus::{Bus, Properties, Transport},
    locale::LocaleSettings,
    network::Network,
    session::SessionSettings,
    *,
};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    rc::Rc,
};

#[derive(Clone)]
struct Mock(Rc<RefCell<MockState>>);
#[derive(Default)]
struct MockState {
    expected: VecDeque<Expected>,
    calls: Vec<Call>,
}
struct Expected {
    method: &'static str,
    signature: &'static str,
    result: Result<Variant>,
}
struct Call {
    service: String,
    path: String,
    interface: String,
    method: String,
    args: Variant,
    interactive: bool,
}
impl Mock {
    fn new(expected: Vec<Expected>) -> Self {
        Self(Rc::new(RefCell::new(MockState {
            expected: expected.into(),
            calls: vec![],
        })))
    }
    fn done(&self) {
        assert!(self.0.borrow().expected.is_empty());
    }
}
impl Transport for Mock {
    fn call(
        &self,
        bus: Bus,
        service: &str,
        path: &str,
        interface: &str,
        method: &str,
        args: Variant,
        interactive: bool,
    ) -> Result<Variant> {
        assert_eq!(bus, Bus::System);
        let mut state = self.0.borrow_mut();
        let expected = state.expected.pop_front().expect("unexpected D-Bus call");
        assert_eq!(method, expected.method);
        assert_eq!(args.type_().as_str(), expected.signature);
        state.calls.push(Call {
            service: service.into(),
            path: path.into(),
            interface: interface.into(),
            method: method.into(),
            args,
            interactive,
        });
        expected.result
    }
}
fn response(method: &'static str, signature: &'static str, value: impl ToVariant) -> Expected {
    Expected {
        method,
        signature,
        result: Ok(value.to_variant()),
    }
}
fn failure(method: &'static str, kind: ErrorKind) -> Expected {
    Expected {
        method,
        signature: "()",
        result: Err(SystemError::new(kind, method, "isolated failure")),
    }
}
fn props(items: &[(&str, Variant)]) -> Properties {
    items
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}
fn obj(path: &str) -> ObjectPath {
    ObjectPath::try_from(path).unwrap()
}
struct NoAudio;
impl CommandRunner for NoAudio {
    fn run(&self, _: &[String]) -> Result<Vec<u8>> {
        panic!("unexpected audio command")
    }
}
const PROFILE: &str = "/org/freedesktop/NetworkManager/Settings/3";
const NM: &str = "org.freedesktop.NetworkManager";
const NM_CONNECTION: &str = "org.freedesktop.NetworkManager.Settings.Connection";
type Settings = HashMap<String, Properties>;
fn settings() -> Settings {
    HashMap::from([
        (
            "connection".into(),
            props(&[
                ("id", "Office".to_variant()),
                ("uuid", "test-uuid".to_variant()),
                ("type", "802-11-wireless".to_variant()),
            ]),
        ),
        (
            "802-11-wireless-security".into(),
            props(&[
                ("key-mgmt", "wpa-psk".to_variant()),
                ("psk-flags", 0u32.to_variant()),
            ]),
        ),
        (
            "ipv4".into(),
            props(&[
                ("method", "auto".to_variant()),
                (
                    "address-data",
                    vec![props(&[
                        ("address", "192.0.2.2".to_variant()),
                        ("prefix", 24u32.to_variant()),
                    ])]
                    .to_variant(),
                ),
                ("gateway", "192.0.2.1".to_variant()),
                ("dns", vec![u32::from_ne_bytes([1, 1, 1, 1])].to_variant()),
                ("dns-data", vec!["8.8.8.8"].to_variant()),
                ("addresses", vec![vec![0u32, 24, 0]].to_variant()),
                ("route-metric", 77i64.to_variant()),
                (
                    "route-data",
                    vec![props(&[
                        ("dest", "198.51.100.0".to_variant()),
                        ("prefix", 24u32.to_variant()),
                    ])]
                    .to_variant(),
                ),
            ]),
        ),
        (
            "ipv6".into(),
            props(&[
                ("method", "auto".to_variant()),
                ("dns", vec![vec![0u8; 16]].to_variant()),
            ]),
        ),
        ("proxy".into(), props(&[("method", 0i32.to_variant())])),
    ])
}
fn ip_config() -> IpConfiguration {
    IpConfiguration {
        method: "manual".into(),
        addresses: vec![IpAddress {
            address: "192.0.2.40".into(),
            prefix: 24,
        }],
        gateway: Some("192.0.2.1".into()),
        dns: vec!["9.9.9.9".into()],
        dns_search: vec!["example.org".into(), "~.".into()],
        ignore_auto_dns: true,
    }
}
#[test]
fn profile_edit_preserves_unrelated_settings_and_excludes_secrets() {
    let original = settings();
    let t = Mock::new(vec![
        response("GetSettings", "()", (original.clone(),)),
        response("Update", "(a{sa{sv}})", ()),
    ]);
    let api = SystemApi::with_backends(t.clone(), NoAudio);
    api.apply(SystemCommand::SetConnectionIp {
        connection: PROFILE.into(),
        family: IpFamily::Ipv4,
        config: ip_config(),
    })
    .unwrap();
    t.done();
    let state = t.0.borrow();
    assert!(
        state
            .calls
            .iter()
            .all(|c| c.service == NM && c.path == PROFILE && c.interface == NM_CONNECTION)
    );
    assert!(!state.calls[0].interactive);
    assert!(state.calls[1].interactive);
    let (updated,): (Settings,) = state.calls[1].args.get().unwrap();
    for section in ["connection", "proxy", "ipv6", "802-11-wireless-security"] {
        assert_eq!(updated[section], original[section]);
    }
    let v4 = &updated["ipv4"];
    assert!(!v4.contains_key("addresses"));
    assert!(!v4.contains_key("dns-data"));
    assert_eq!(
        v4["dns"].get::<Vec<u32>>().unwrap()[0].to_ne_bytes(),
        [9, 9, 9, 9]
    );
    assert_eq!(v4["route-metric"], original["ipv4"]["route-metric"]);
    assert_eq!(v4["route-data"], original["ipv4"]["route-data"]);
    let addresses: Vec<Properties> = v4["address-data"].get().unwrap();
    assert_eq!(addresses[0]["address"].str(), Some("192.0.2.40"));
    assert_eq!(addresses[0]["prefix"].get::<u32>(), Some(24));
    assert_eq!(v4["ignore-auto-dns"].get::<bool>(), Some(true));
    assert!(!updated["802-11-wireless-security"].contains_key("psk"));
}
#[test]
fn ipv6_dns_has_byte_array_wire_type_and_readback_handles_both_dns_formats() {
    let t = Mock::new(vec![
        response("GetSettings", "()", (settings(),)),
        response("Update", "(a{sa{sv}})", ()),
        response("GetSettings", "()", (settings(),)),
    ]);
    let mut config = ip_config();
    config.addresses[0].address = "2001:db8::40".into();
    config.addresses[0].prefix = 64;
    config.gateway = Some("2001:db8::1".into());
    config.dns = vec!["2606:4700:4700::1111".into()];
    Network(&t)
        .set_ip(PROFILE, IpFamily::Ipv6, &config)
        .unwrap();
    let details = Network(&t).connection(PROFILE).unwrap();
    assert!(details.autoconnect);
    assert_eq!(details.ipv4.unwrap().dns, ["8.8.8.8"]);
    assert_eq!(details.ipv6.unwrap().dns, ["::"]);
    t.done();
    let state = t.0.borrow();
    let (updated,): (Settings,) = state.calls[1].args.get().unwrap();
    assert_eq!(updated["ipv6"]["dns"].type_().as_str(), "aay");
    let dns: Vec<Vec<u8>> = updated["ipv6"]["dns"].get().unwrap();
    assert_eq!(
        dns[0],
        "2606:4700:4700::1111"
            .parse::<std::net::Ipv6Addr>()
            .unwrap()
            .octets()
    );
}
#[test]
fn network_rejects_invalid_ip_families_methods_domains_and_paths_before_io() {
    let t = Mock::new(vec![]);
    let invalid = [
        IpConfiguration {
            method: "execute".into(),
            ..ip_config()
        },
        IpConfiguration {
            addresses: vec![],
            ..ip_config()
        },
        IpConfiguration {
            gateway: Some("::1".into()),
            ..ip_config()
        },
        IpConfiguration {
            dns: vec!["bad;command".into()],
            ..ip_config()
        },
        IpConfiguration {
            dns_search: vec!["~..".into()],
            ..ip_config()
        },
        IpConfiguration {
            dns_search: vec!["bad..domain".into()],
            ..ip_config()
        },
        IpConfiguration {
            method: "disabled".into(),
            ..ip_config()
        },
        IpConfiguration {
            addresses: vec![IpAddress {
                address: "192.0.2.1".into(),
                prefix: 33,
            }],
            ..ip_config()
        },
    ];
    for config in invalid {
        assert_eq!(
            Network(&t)
                .set_ip(PROFILE, IpFamily::Ipv4, &config)
                .unwrap_err()
                .kind,
            ErrorKind::InvalidInput
        );
    }
    assert!(
        Network(&t)
            .set_autoconnect("/org/bluez/hci0", true)
            .is_err()
    );
    assert!(
        Network(&t)
            .forget_connection("/org/freedesktop/NetworkManager/Settings/")
            .is_err()
    );
    t.done();
    assert!(t.0.borrow().calls.is_empty());
}
#[test]
fn autoconnect_and_forget_target_only_the_selected_profile_and_propagate_denials() {
    let t = Mock::new(vec![
        response("GetSettings", "()", (settings(),)),
        response("Update", "(a{sa{sv}})", ()),
        failure("Delete", ErrorKind::PermissionDenied),
    ]);
    Network(&t).set_autoconnect(PROFILE, false).unwrap();
    assert_eq!(
        Network(&t).forget_connection(PROFILE).unwrap_err().kind,
        ErrorKind::PermissionDenied
    );
    t.done();
    let state = t.0.borrow();
    assert!(
        state
            .calls
            .iter()
            .all(|c| c.path == PROFILE && c.service == NM)
    );
    let (updated,): (Settings,) = state.calls[1].args.get().unwrap();
    assert_eq!(
        updated["connection"]["autoconnect"].get::<bool>(),
        Some(false)
    );
}
fn wifi(security: WifiSecurity) -> WifiConnection {
    WifiConnection {
        name: "Home".into(),
        ssid: vec![0xff, b'x'],
        hidden: true,
        autoconnect: false,
        security,
    }
}
#[test]
fn wifi_creation_uses_binary_ssids_and_explicit_security_without_logging_passwords() {
    for security in [
        WifiSecurity::Open {},
        WifiSecurity::WpaPsk {
            password: "example-passphrase".into(),
        },
        WifiSecurity::Sae {
            password: "测试密码".into(),
        },
    ] {
        let t = Mock::new(vec![response(
            "AddConnection",
            "(a{sa{sv}})",
            (obj(PROFILE),),
        )]);
        let config = wifi(security.clone());
        assert!(!format!("{config:?}").contains("example-passphrase"));
        assert!(!format!("{config:?}").contains("测试密码"));
        assert_eq!(Network(&t).add_wifi(&config).unwrap(), PROFILE);
        t.done();
        let state = t.0.borrow();
        let c = &state.calls[0];
        assert_eq!(
            (&*c.service, &*c.path, &*c.interface),
            (
                NM,
                "/org/freedesktop/NetworkManager/Settings",
                "org.freedesktop.NetworkManager.Settings"
            )
        );
        assert!(c.interactive);
        let (settings,): (Settings,) = c.args.get().unwrap();
        assert_eq!(
            settings["802-11-wireless"]["ssid"].get::<Vec<u8>>(),
            Some(vec![0xff, b'x'])
        );
        assert_eq!(
            settings["connection"]["autoconnect"].get::<bool>(),
            Some(false)
        );
        match security {
            WifiSecurity::Open {} => assert!(!settings.contains_key("802-11-wireless-security")),
            WifiSecurity::WpaPsk { password } | WifiSecurity::Sae { password } => {
                let s = &settings["802-11-wireless-security"];
                assert_eq!(s["psk"].str(), Some(password.as_str()));
                assert_eq!(s["psk-flags"].get::<u32>(), Some(0));
            }
        }
    }
}
#[test]
fn invalid_wifi_input_never_calls_the_bus_and_json_rejects_typos() {
    let t = Mock::new(vec![]);
    for config in [
        WifiConnection {
            ssid: vec![],
            ..wifi(WifiSecurity::Open {})
        },
        WifiConnection {
            ssid: vec![b'x'; 33],
            ..wifi(WifiSecurity::Open {})
        },
        WifiConnection {
            name: "bad\nname".into(),
            ..wifi(WifiSecurity::Open {})
        },
        wifi(WifiSecurity::WpaPsk {
            password: "short".into(),
        }),
        wifi(WifiSecurity::WpaPsk {
            password: "f".repeat(63) + "z",
        }),
        wifi(WifiSecurity::Sae {
            password: "".into(),
        }),
    ] {
        assert_eq!(
            Network(&t).add_wifi(&config).unwrap_err().kind,
            ErrorKind::InvalidInput
        );
    }
    assert!(serde_json::from_str::<SystemCommand>(r#"{"operation":"add_wifi_connection","value":{"name":"test","ssid":[65],"hidden":false,"autoconnect":false,"security":{"mode":"open","password":"typo"}}}"#).is_err());
    assert!(serde_json::from_str::<SystemCommand>(r#"{"operation":"set_connection_ip","value":{"connection":"x","family":"ipv4","config":{"method":"auto","addresses":[],"gateway":null,"dns":[],"dns_search":[],"ignore_auto_dns":false,"typo":1}}}"#).is_err());
    t.done();
}
#[test]
fn locale_and_keyboard_preserve_exact_method_signatures_and_conversion_choice() {
    let t = Mock::new(vec![
        response("SetLocale", "(asb)", ()),
        response("SetX11Keyboard", "(ssssbb)", ()),
        response("SetVConsoleKeyboard", "(ssbb)", ()),
    ]);
    let api = SystemApi::with_backends(t.clone(), NoAudio);
    api.apply(SystemCommand::SetLocale(vec![
        "LANG=en_US.UTF-8".into(),
        "LC_TIME=zh_CN.UTF-8".into(),
    ]))
    .unwrap();
    api.apply(SystemCommand::SetX11Keyboard {
        keyboard: X11Keyboard {
            layout: "us,de".into(),
            model: "pc105".into(),
            variant: ",nodeadkeys".into(),
            options: "grp:alt_shift_toggle".into(),
        },
        convert: false,
    })
    .unwrap();
    api.apply(SystemCommand::SetConsoleKeyboard {
        keyboard: ConsoleKeyboard {
            keymap: "us".into(),
            keymap_toggle: "".into(),
        },
        convert: true,
    })
    .unwrap();
    t.done();
    let state = t.0.borrow();
    assert!(
        state
            .calls
            .iter()
            .all(|c| c.service == "org.freedesktop.locale1"
                && c.path == "/org/freedesktop/locale1"
                && c.interface == c.service
                && c.interactive)
    );
    let args: (String, String, String, String, bool, bool) = state.calls[1].args.get().unwrap();
    assert_eq!(
        args,
        (
            "us,de".into(),
            "pc105".into(),
            ",nodeadkeys".into(),
            "grp:alt_shift_toggle".into(),
            false,
            true
        )
    );
}
#[test]
fn locale_and_keyboard_validation_prevents_unrelated_environment_or_path_writes() {
    let t = Mock::new(vec![]);
    let locale = LocaleSettings(&t);
    for assignments in [
        vec!["PATH=/tmp".into()],
        vec!["LANG".into()],
        vec!["LANG=C".into(), "LANG=en_US.UTF-8".into()],
        vec!["LC_ALL=C".into()],
        vec!["LANG=C\n".into()],
    ] {
        assert_eq!(
            locale.set_locale(&assignments).unwrap_err().kind,
            ErrorKind::InvalidInput
        );
    }
    assert!(
        locale
            .set_console_keyboard(
                &ConsoleKeyboard {
                    keymap: "../us".into(),
                    keymap_toggle: "".into()
                },
                false
            )
            .is_err()
    );
    assert!(
        locale
            .set_x11_keyboard(
                &X11Keyboard {
                    layout: "us,,de".into(),
                    model: "pc105".into(),
                    variant: "".into(),
                    options: "".into()
                },
                false
            )
            .is_err()
    );
    assert!(
        locale
            .set_x11_keyboard(
                &X11Keyboard {
                    layout: "us,de".into(),
                    model: "pc105".into(),
                    variant: "dvorak".into(),
                    options: "".into()
                },
                false
            )
            .is_err()
    );
    t.done();
}
#[test]
fn locale_snapshot_reads_system_defaults_without_requesting_authorization() {
    let p = props(&[
        ("Locale", vec!["LANG=C.UTF-8"].to_variant()),
        ("X11Layout", "us".to_variant()),
        ("X11Model", "pc105".to_variant()),
        ("X11Variant", "".to_variant()),
        ("X11Options", "".to_variant()),
        ("VConsoleKeymap", "us".to_variant()),
        ("VConsoleKeymapToggle", "".to_variant()),
    ]);
    let t = Mock::new(vec![response("GetAll", "(s)", (p,))]);
    let value = LocaleSettings(&t).snapshot().unwrap();
    assert_eq!(value.locale, ["LANG=C.UTF-8"]);
    assert_eq!(value.x11.layout, "us");
    assert!(!t.0.borrow().calls[0].interactive);
    t.done();
}
#[test]
fn time_and_rtc_use_absolute_utc_and_keep_the_system_clock_authoritative() {
    let t = Mock::new(vec![
        response("SetTime", "(xbb)", ()),
        response("SetLocalRTC", "(bbb)", ()),
        response("ListTimezones", "()", (vec!["UTC", "Asia/Shanghai"],)),
    ]);
    let api = SystemApi::with_backends(t.clone(), NoAudio);
    api.apply(SystemCommand::SetTime {
        unix_microseconds: 1_800_000_000_000_000,
    })
    .unwrap();
    api.apply(SystemCommand::SetLocalRtc(true)).unwrap();
    assert!(
        matches!(api.query(SystemQuery::Timezones).unwrap(), SystemQueryResult::Timezones(v) if v.len()==2)
    );
    assert_eq!(
        api.host().set_time(-1).unwrap_err().kind,
        ErrorKind::InvalidInput
    );
    t.done();
    let state = t.0.borrow();
    assert_eq!(
        state.calls[0].args.get::<(i64, bool, bool)>(),
        Some((1_800_000_000_000_000, false, true))
    );
    assert_eq!(
        state.calls[1].args.get::<(bool, bool, bool)>(),
        Some((true, false, true))
    );
    assert!(state.calls[..2].iter().all(|c| c.interactive));
    assert!(!state.calls[2].interactive);
    assert!(
        state
            .calls
            .iter()
            .all(|c| c.service == "org.freedesktop.timedate1"
                && c.path == "/org/freedesktop/timedate1"
                && c.interface == c.service)
    );
}
#[test]
fn power_capabilities_keep_authentication_and_inhibition_states_and_partial_errors() {
    let t = Mock::new(vec![
        response("CanPowerOff", "()", ("challenge",)),
        response("CanReboot", "()", ("inhibitor-blocked",)),
        response("CanSuspend", "()", ("yes",)),
        response("CanHibernate", "()", ("na",)),
        failure("CanHybridSleep", ErrorKind::Unsupported),
        response("CanSuspendThenHibernate", "()", ("no",)),
        response(
            "ListInhibitors",
            "()",
            (vec![(
                "sleep",
                "Editor",
                "Saving work",
                "delay",
                1000u32,
                42u32,
            )],),
        ),
    ]);
    let snapshot = SessionSettings(&t).power_actions().unwrap();
    t.done();
    assert_eq!(
        snapshot.capabilities[0].availability.value.as_deref(),
        Some("challenge")
    );
    assert_eq!(
        snapshot.capabilities[1].availability.value.as_deref(),
        Some("inhibitor-blocked")
    );
    assert_eq!(
        snapshot.capabilities[4]
            .availability
            .error
            .as_ref()
            .unwrap()
            .kind,
        ErrorKind::Unsupported
    );
    assert_eq!(snapshot.inhibitors.value.unwrap()[0].why, "Saving work");
    assert!(t.0.borrow().calls.iter().all(|c| !c.interactive));
}
#[test]
fn power_writes_use_logind_policy_without_force_flags_or_inhibitor_overrides() {
    let methods = [
        "PowerOff",
        "Reboot",
        "Suspend",
        "Hibernate",
        "HybridSleep",
        "SuspendThenHibernate",
    ];
    let t = Mock::new(
        methods
            .iter()
            .map(|method| response(method, "(b)", ()))
            .collect(),
    );
    let api = SystemApi::with_backends(t.clone(), NoAudio);
    for action in PowerAction::ALL {
        api.apply(SystemCommand::PowerAction(action)).unwrap();
    }
    t.done();
    let state = t.0.borrow();
    for (c, method) in state.calls.iter().zip(methods) {
        assert_eq!(c.method, method);
        assert_eq!(
            (&*c.service, &*c.path, &*c.interface),
            (
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager"
            )
        );
        assert_eq!(c.args.get::<(bool,)>(), Some((true,)));
        assert!(c.interactive);
    }
}
#[test]
fn lock_targets_the_callers_session_and_reports_no_session_errors() {
    let t = Mock::new(vec![
        response(
            "GetSessionByPID",
            "(u)",
            (obj("/org/freedesktop/login1/session/_32"),),
        ),
        response("Lock", "()", ()),
    ]);
    let api = SystemApi::with_backends(t.clone(), NoAudio);
    api.apply(serde_json::from_str(r#"{"operation":"lock_session"}"#).unwrap())
        .unwrap();
    t.done();
    let state = t.0.borrow();
    assert_eq!(
        state.calls[0].args.get::<(u32,)>(),
        Some((std::process::id(),))
    );
    assert!(!state.calls[0].interactive);
    assert_eq!(state.calls[1].path, "/org/freedesktop/login1/session/_32");
    assert_eq!(state.calls[1].interface, "org.freedesktop.login1.Session");
    let unavailable = Mock::new(vec![Expected {
        method: "GetSessionByPID",
        signature: "(u)",
        result: Err(SystemError::new(
            ErrorKind::NotFound,
            "session",
            "no login session",
        )),
    }]);
    assert_eq!(
        SessionSettings(&unavailable).lock().unwrap_err().kind,
        ErrorKind::NotFound
    );
    unavailable.done();
}
#[test]
fn connection_query_is_typed_and_never_contains_credentials() {
    let t = Mock::new(vec![response("GetSettings", "()", (settings(),))]);
    let api = SystemApi::with_backends(t.clone(), NoAudio);
    let result = api
        .query(SystemQuery::Connection {
            connection: PROFILE.into(),
        })
        .unwrap();
    let json = serde_json::to_value(result).unwrap();
    assert_eq!(json["operation"], "connection");
    assert_eq!(json["value"]["name"], "Office");
    assert!(json["value"].get("security").is_none());
    t.done();
}
#[test]
fn session_snapshot_reads_the_resolved_session_and_preserves_reported_hints() {
    let t = Mock::new(vec![
        response(
            "GetSessionByPID",
            "(u)",
            (obj("/org/freedesktop/login1/session/_32"),),
        ),
        response(
            "GetAll",
            "(s)",
            (props(&[
                ("Id", "2".to_variant()),
                (
                    "User",
                    (1000u32, obj("/org/freedesktop/login1/user/_1000")).to_variant(),
                ),
                ("Name", "test-user".to_variant()),
                ("Type", "wayland".to_variant()),
                ("Desktop", "Hyprland".to_variant()),
                ("State", "active".to_variant()),
                ("Active", true.to_variant()),
                ("LockedHint", false.to_variant()),
                ("IdleHint", true.to_variant()),
            ]),),
        ),
    ]);
    let state = SessionSettings(&t).snapshot().unwrap();
    assert_eq!(state.user_id, 1000);
    assert_eq!(state.kind, "wayland");
    assert!(state.active && state.idle && !state.locked);
    t.done();
    let recorded = t.0.borrow();
    assert_eq!(
        recorded.calls[1].path,
        "/org/freedesktop/login1/session/_32"
    );
    assert_eq!(
        recorded.calls[1].args.get::<(String,)>(),
        Some(("org.freedesktop.login1.Session".into(),))
    );
    assert!(recorded.calls.iter().all(|c| !c.interactive));
}
