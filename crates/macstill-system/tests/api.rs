use gio::glib::{
    Variant,
    variant::{ObjectPath, ToVariant},
};
use macstill_system::{
    audio::{Audio, CommandRunner},
    bluetooth::Bluetooth,
    brightness::Brightness,
    dbus::{Bus, Objects, Properties, Transport},
    host::HostSettings,
    network::Network,
    power::Power,
    *,
};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
};
struct Expected {
    method: &'static str,
    signature: &'static str,
    reply: Result<Variant>,
}
struct Mock {
    replies: RefCell<VecDeque<Expected>>,
    calls: RefCell<Vec<(String, String, Variant, bool)>>,
}
impl Mock {
    fn new(replies: Vec<Expected>) -> Self {
        Self {
            replies: RefCell::new(replies.into()),
            calls: RefCell::new(vec![]),
        }
    }
    fn done(&self) {
        assert!(self.replies.borrow().is_empty());
    }
}
impl Transport for Mock {
    fn call(
        &self,
        _: Bus,
        _: &str,
        path: &str,
        _: &str,
        method: &str,
        args: Variant,
        interactive: bool,
    ) -> Result<Variant> {
        self.calls
            .borrow_mut()
            .push((path.into(), method.into(), args.clone(), interactive));
        let e = self
            .replies
            .borrow_mut()
            .pop_front()
            .expect("Unexpected D-Bus call");
        assert_eq!(method, e.method);
        assert_eq!(args.type_().as_str(), e.signature);
        e.reply
    }
}
fn reply(method: &'static str, signature: &'static str, value: Variant) -> Expected {
    Expected {
        method,
        signature,
        reply: Ok(value),
    }
}
fn props(v: &[(&str, Variant)]) -> Properties {
    v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}
#[test]
fn wifi_uses_variant_property_and_daemon_authorization() {
    let t = Mock::new(vec![
        reply("Set", "(ssv)", ().to_variant()),
        reply("Enable", "(b)", ().to_variant()),
    ]);
    Network(&t).set_wifi(true).unwrap();
    Network(&t).set_enabled(false).unwrap();
    t.done();
    let calls = t.calls.borrow();
    let (iface, key, value): (String, String, Variant) = calls[0].2.get().unwrap();
    assert_eq!(
        (iface.as_str(), key.as_str()),
        ("org.freedesktop.NetworkManager", "WirelessEnabled")
    );
    assert_eq!(value.get::<bool>(), Some(true));
    assert!(calls.iter().all(|v| v.3));
}
#[test]
fn activation_uses_object_paths_and_does_not_claim_finished_connectivity() {
    let active =
        ObjectPath::try_from("/org/freedesktop/NetworkManager/ActiveConnection/7").unwrap();
    let t = Mock::new(vec![reply(
        "ActivateConnection",
        "(ooo)",
        (active,).to_variant(),
    )]);
    assert!(
        Network(&t)
            .activate(
                "/org/freedesktop/NetworkManager/Settings/1",
                "/org/freedesktop/NetworkManager/Devices/2"
            )
            .unwrap()
            .ends_with("/7")
    );
    t.done();
    assert!(
        Network(&t)
            .activate(
                "/org/bluez/hci0",
                "/org/freedesktop/NetworkManager/Devices/2"
            )
            .is_err()
    );
    assert_eq!(t.calls.borrow().len(), 1);
}
fn bluez() -> Variant {
    let mut objects = Objects::new();
    objects.insert(
        ObjectPath::try_from("/org/bluez/hci0").unwrap(),
        HashMap::from([(
            "org.bluez.Adapter1".into(),
            props(&[
                ("Alias", "Bluetooth".to_variant()),
                ("Powered", false.to_variant()),
                ("Discovering", false.to_variant()),
                ("Discoverable", false.to_variant()),
            ]),
        )]),
    );
    objects.insert(
        ObjectPath::try_from("/org/bluez/hci0/dev_TEST").unwrap(),
        HashMap::from([(
            "org.bluez.Device1".into(),
            props(&[
                (
                    "Adapter",
                    ObjectPath::try_from("/org/bluez/hci0")
                        .unwrap()
                        .to_variant(),
                ),
                ("Alias", "Headphones".to_variant()),
                ("Paired", false.to_variant()),
                ("Connected", false.to_variant()),
                ("Trusted", false.to_variant()),
            ]),
        )]),
    );
    (objects,).to_variant()
}
#[test]
fn bluetooth_pairing_never_implicitly_trusts_a_device() {
    let t = Mock::new(vec![
        reply("GetManagedObjects", "()", bluez()),
        reply("Pair", "()", ().to_variant()),
    ]);
    Bluetooth(&t).pair("/org/bluez/hci0/dev_TEST").unwrap();
    t.done();
    assert_eq!(t.calls.borrow().len(), 2);
    let state = Mock::new(vec![reply("GetManagedObjects", "()", bluez())]);
    let data = Bluetooth(&state).snapshot().unwrap();
    assert!(!data.devices[0].trusted);
    assert!(data.devices[0].rssi.is_none());
}
#[test]
fn bluetooth_forget_uses_the_devices_adapter_and_an_object_path() {
    let t = Mock::new(vec![
        reply("GetManagedObjects", "()", bluez()),
        reply("RemoveDevice", "(o)", ().to_variant()),
    ]);
    Bluetooth(&t)
        .remove_device("/org/bluez/hci0/dev_TEST")
        .unwrap();
    t.done();
    let calls = t.calls.borrow();
    assert_eq!(calls[1].0, "/org/bluez/hci0");
    assert_eq!(
        calls[1].2.get::<(ObjectPath,)>().unwrap().0.as_str(),
        "/org/bluez/hci0/dev_TEST"
    );
    assert!(calls[1].3);
}
#[test]
fn bluetooth_visibility_pairability_alias_and_blocking_use_typed_properties() {
    let t = Mock::new(
        (0..7)
            .flat_map(|_| {
                vec![
                    reply("GetManagedObjects", "()", bluez()),
                    reply("Set", "(ssv)", ().to_variant()),
                ]
            })
            .collect(),
    );
    let bt = Bluetooth(&t);
    bt.set_discoverable_timeout("/org/bluez/hci0", 120).unwrap();
    bt.set_discoverable("/org/bluez/hci0", true).unwrap();
    bt.set_pairable("/org/bluez/hci0", false).unwrap();
    bt.set_adapter_alias("/org/bluez/hci0", "电脑").unwrap();
    bt.set_device_alias("/org/bluez/hci0/dev_TEST", "Headset")
        .unwrap();
    bt.set_device_alias("/org/bluez/hci0/dev_TEST", "").unwrap();
    bt.set_blocked("/org/bluez/hci0/dev_TEST", true).unwrap();
    t.done();
    let calls = t.calls.borrow();
    let keys = [
        "DiscoverableTimeout",
        "Discoverable",
        "Pairable",
        "Alias",
        "Alias",
        "Alias",
        "Blocked",
    ];
    for (call, key) in calls.iter().skip(1).step_by(2).zip(keys) {
        let (interface, property, value): (String, String, Variant) = call.2.get().unwrap();
        assert_eq!(property, key);
        assert!(call.3);
        if key == "DiscoverableTimeout" {
            assert_eq!(value.get::<u32>(), Some(120));
        }
        assert_eq!(
            interface,
            if call.0.ends_with("dev_TEST") {
                "org.bluez.Device1"
            } else {
                "org.bluez.Adapter1"
            }
        );
    }
}
#[test]
fn bluetooth_alias_and_stale_device_validation_send_no_write() {
    let t = Mock::new(vec![reply("GetManagedObjects", "()", bluez())]);
    assert_eq!(
        Bluetooth(&t)
            .set_device_alias("/org/bluez/hci0/dev_TEST", "bad\nname")
            .unwrap_err()
            .kind,
        ErrorKind::InvalidInput
    );
    assert_eq!(
        Bluetooth(&t)
            .remove_device("/org/bluez/hci0/dev_MISSING")
            .unwrap_err()
            .kind,
        ErrorKind::NotFound
    );
    t.done();
    assert_eq!(t.calls.borrow().len(), 1);
}
#[test]
fn brightness_converts_percentage_but_never_writes_sysfs_directly() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("intel_backlight")).unwrap();
    std::fs::write(dir.path().join("intel_backlight/max_brightness"), "255\n").unwrap();
    std::fs::write(dir.path().join("intel_backlight/brightness"), "100\n").unwrap();
    let t = Mock::new(vec![reply("SetBrightness", "(ssu)", ().to_variant())]);
    let b = Brightness::with_root(&t, dir.path());
    b.set("intel_backlight", 50.).unwrap();
    t.done();
    let values: (String, String, u32) = t.calls.borrow()[0].2.get().unwrap();
    assert_eq!(values, ("backlight".into(), "intel_backlight".into(), 128));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("intel_backlight/brightness")).unwrap(),
        "100\n"
    );
    for n in [f64::NAN, f64::INFINITY, -1., 101.] {
        assert_eq!(
            b.set("intel_backlight", n).unwrap_err().kind,
            ErrorKind::InvalidInput
        );
    }
    assert!(b.set("../intel_backlight", 10.).is_err());
    assert_eq!(t.calls.borrow().len(), 1);
}
#[test]
fn power_profiles_fall_back_to_legacy_name_and_reject_unknown_profiles() {
    let p = props(&[
        ("ActiveProfile", "balanced".to_variant()),
        (
            "Profiles",
            vec![
                props(&[("Profile", "balanced".to_variant())]),
                props(&[("Profile", "power-saver".to_variant())]),
            ]
            .to_variant(),
        ),
    ]);
    let unavailable = || Expected {
        method: "GetAll",
        signature: "(s)",
        reply: Err(SystemError::new(
            ErrorKind::Unavailable,
            "profiles",
            "missing",
        )),
    };
    let t = Mock::new(vec![
        unavailable(),
        reply("GetAll", "(s)", (p.clone(),).to_variant()),
        reply("Set", "(ssv)", ().to_variant()),
        unavailable(),
        reply("GetAll", "(s)", (p,).to_variant()),
    ]);
    Power(&t).set_profile("power-saver").unwrap();
    assert_eq!(
        Power(&t).set_profile("turbo").unwrap_err().kind,
        ErrorKind::InvalidInput
    );
    t.done();
    assert_eq!(t.calls.borrow()[2].0, "/net/hadess/PowerProfiles");
}
#[test]
fn hostname_and_timezone_validation_happens_before_system_calls() {
    let t = Mock::new(vec![]);
    let h = HostSettings(&t);
    for n in [
        "",
        "UPPER",
        "name;reboot",
        "-bad",
        "bad-",
        "bad..name",
        "../../etc",
    ] {
        assert!(h.set_hostname(n).is_err());
    }
    for z in [
        "/etc/passwd",
        "../UTC",
        "Asia/../UTC",
        "Asia//Shanghai",
        "UTC\n",
    ] {
        assert!(h.set_timezone(z).is_err());
    }
    assert!(h.set_pretty_hostname("bad\0name").is_err());
    assert!(t.calls.borrow().is_empty());
    let t = Mock::new(vec![
        reply("SetTimezone", "(sb)", ().to_variant()),
        reply("SetNTP", "(bb)", ().to_variant()),
    ]);
    HostSettings(&t).set_timezone("Asia/Shanghai").unwrap();
    HostSettings(&t).set_ntp(true).unwrap();
    t.done();
    assert!(t.calls.borrow().iter().all(|v| v.3));
}
struct AudioMock {
    calls: RefCell<Vec<Vec<String>>>,
}
impl CommandRunner for AudioMock {
    fn run(&self, args: &[String]) -> Result<Vec<u8>> {
        self.calls.borrow_mut().push(args.to_vec());
        if args.first().is_some_and(|a| a == "--format=json") {
            return Ok(br#"[{"index":7,"name":"speakers","description":"Speakers","channel_map":"front-right,front-left","mute":false,"volume":{"front-left":{"value":32768},"front-right":{"value":65536}}}]"#.to_vec());
        }
        Ok(vec![])
    }
}
#[test]
fn audio_preserves_balance_channel_order_and_uses_argument_arrays() {
    let r = AudioMock {
        calls: RefCell::new(vec![]),
    };
    Audio(&r)
        .set_volume(AudioDirection::Output, 7, 50.)
        .unwrap();
    assert_eq!(
        r.calls.borrow()[1],
        ["set-sink-volume", "7", "50.000000%", "25.000000%"]
    );
    for n in [f64::NAN, f64::INFINITY, 101., -2.] {
        assert_eq!(
            Audio(&r)
                .set_volume(AudioDirection::Input, 7, n)
                .unwrap_err()
                .kind,
            ErrorKind::InvalidInput
        );
    }
    assert_eq!(r.calls.borrow().len(), 2);
    assert_eq!(
        Audio(&r)
            .set_mute(AudioDirection::Output, 99, true)
            .unwrap_err()
            .kind,
        ErrorKind::NotFound
    );
    assert_eq!(r.calls.borrow().len(), 3);
}
#[test]
fn command_json_is_typed_and_read_errors_are_explicit() {
    let c: SystemCommand = serde_json::from_str(
        r#"{"operation":"set_volume","value":{"direction":"output","id":7,"percent":30}}"#,
    )
    .unwrap();
    assert!(matches!(c, SystemCommand::SetVolume { id: 7, .. }));
    let s: State<bool> = Err(SystemError::new(
        ErrorKind::Unavailable,
        "test",
        "no hardware",
    ))
    .into();
    assert!(s.value.is_none());
    assert_eq!(s.error.unwrap().kind, ErrorKind::Unavailable);
}

#[test]
fn battery_report_reads_real_health_and_typed_charge_history_without_authorization() {
    let mouse = ObjectPath::try_from("/org/freedesktop/UPower/devices/mouse_test").unwrap();
    let battery = ObjectPath::try_from("/org/freedesktop/UPower/devices/battery_BAT0").unwrap();
    let t = Mock::new(vec![
        reply(
            "EnumerateDevices",
            "()",
            (vec![mouse, battery],).to_variant(),
        ),
        reply(
            "GetAll",
            "(s)",
            (props(&[
                ("Type", 2u32.to_variant()),
                ("PowerSupply", true.to_variant()),
                ("IsPresent", true.to_variant()),
                ("HasHistory", true.to_variant()),
                ("Capacity", 93.5f64.to_variant()),
                ("ChargeCycles", 132i32.to_variant()),
                ("EnergyFull", 51.2f64.to_variant()),
                ("EnergyFullDesign", 54.4f64.to_variant()),
            ]),)
                .to_variant(),
        ),
        reply(
            "GetAll",
            "(s)",
            (props(&[
                ("Type", 5u32.to_variant()),
                ("PowerSupply", false.to_variant()),
                ("IsPresent", true.to_variant()),
            ]),)
                .to_variant(),
        ),
        reply(
            "GetHistory",
            "(suu)",
            (vec![
                (200u32, 74f64, 2u32),
                (100, 80., 1),
                (100, 80., 1),
                (300, f64::NAN, 2),
                (400, 120., 2),
                (500, 40., 0),
            ],)
                .to_variant(),
        ),
    ]);
    let report = Power(&t).battery_report().unwrap();
    t.done();
    assert_eq!(report.device, "BAT0");
    assert_eq!(report.capacity_percent, Some(93.5));
    assert_eq!(report.charge_cycles, Some(132));
    assert_eq!(report.battery_count, 1);
    assert_eq!(report.history.len(), 2);
    assert_eq!(report.history[0].timestamp, 100);
    assert_eq!(report.history[1].percentage, 74.);
    assert_eq!(
        t.calls.borrow()[3].2.get::<(String, u32, u32)>(),
        Some(("charge".into(), 864000, 2400))
    );
    assert!(t.calls.borrow().iter().all(|call| !call.3));
}
#[test]
fn missing_history_and_health_are_not_reported_as_normal_or_as_zero() {
    let battery = ObjectPath::try_from("/org/freedesktop/UPower/devices/battery_BAT0").unwrap();
    let t = Mock::new(vec![
        reply("EnumerateDevices", "()", (vec![battery],).to_variant()),
        reply(
            "GetAll",
            "(s)",
            (props(&[
                ("Type", 2u32.to_variant()),
                ("PowerSupply", true.to_variant()),
                ("IsPresent", true.to_variant()),
                ("Capacity", 0f64.to_variant()),
                ("ChargeCycles", (-1i32).to_variant()),
                ("HasHistory", false.to_variant()),
            ]),)
                .to_variant(),
        ),
    ]);
    let report = Power(&t).battery_report().unwrap();
    t.done();
    assert!(report.capacity_percent.is_none());
    assert!(report.charge_cycles.is_none());
    assert!(report.history.is_empty());
    assert!(report.history_error.is_some());
}
#[test]
fn history_protocol_failure_does_not_erase_available_battery_health() {
    let battery = ObjectPath::try_from("/org/freedesktop/UPower/devices/battery_BAT0").unwrap();
    let t = Mock::new(vec![
        reply("EnumerateDevices", "()", (vec![battery],).to_variant()),
        reply(
            "GetAll",
            "(s)",
            (props(&[
                ("Type", 2u32.to_variant()),
                ("PowerSupply", true.to_variant()),
                ("IsPresent", true.to_variant()),
                ("Capacity", 92f64.to_variant()),
                ("HasHistory", true.to_variant()),
            ]),)
                .to_variant(),
        ),
        reply("GetHistory", "(suu)", ("invalid",).to_variant()),
    ]);
    let report = Power(&t).battery_report().unwrap();
    t.done();
    assert_eq!(report.capacity_percent, Some(92.));
    assert!(report.history_error.is_some());
    assert!(report.history.is_empty());
}

#[test]
fn wifi_snapshot_reads_active_access_point_and_profile_without_secrets() {
    let object = |path: &str| ObjectPath::try_from(path).unwrap();
    let device = object("/org/freedesktop/NetworkManager/Devices/1");
    let active = object("/org/freedesktop/NetworkManager/ActiveConnection/7");
    let ap = object("/org/freedesktop/NetworkManager/AccessPoint/2");
    let profile = object("/org/freedesktop/NetworkManager/Settings/3");
    let t = Mock::new(vec![
        reply(
            "GetAll",
            "(s)",
            (props(&[
                ("Devices", vec![device.clone()].to_variant()),
                ("NetworkingEnabled", true.to_variant()),
                ("WirelessEnabled", true.to_variant()),
                ("WirelessHardwareEnabled", true.to_variant()),
                ("Connectivity", 4u32.to_variant()),
                ("State", 70u32.to_variant()),
            ]),)
                .to_variant(),
        ),
        reply(
            "GetAll",
            "(s)",
            (props(&[
                ("DeviceType", 2u32.to_variant()),
                ("ActiveConnection", active.to_variant()),
                ("Interface", "wlan0".to_variant()),
                ("State", 100u32.to_variant()),
            ]),)
                .to_variant(),
        ),
        reply("GetAllAccessPoints", "()", (vec![ap.clone()],).to_variant()),
        reply(
            "GetAll",
            "(s)",
            (props(&[
                ("Ssid", b"Tahoe".to_vec().to_variant()),
                ("Strength", 88u8.to_variant()),
                ("Flags", 1u32.to_variant()),
                ("WpaFlags", 0u32.to_variant()),
                ("RsnFlags", 256u32.to_variant()),
            ]),)
                .to_variant(),
        ),
        reply(
            "GetAll",
            "(s)",
            (props(&[("ActiveAccessPoint", ap.to_variant())]),).to_variant(),
        ),
        reply(
            "GetAll",
            "(s)",
            (props(&[("Connection", profile.to_variant())]),).to_variant(),
        ),
        reply(
            "ListConnections",
            "()",
            (Vec::<ObjectPath>::new(),).to_variant(),
        ),
        reply(
            "GetPermissions",
            "()",
            (HashMap::<String, String>::new(),).to_variant(),
        ),
    ]);
    let data = Network(&t).snapshot().unwrap();
    t.done();
    assert_eq!(
        data.devices[0].active_profile.as_deref(),
        Some("/org/freedesktop/NetworkManager/Settings/3")
    );
    assert_eq!(
        data.devices[0].active_access_point.as_deref(),
        Some("/org/freedesktop/NetworkManager/AccessPoint/2")
    );
    assert!(data.warnings.is_empty());
    assert!(
        t.calls
            .borrow()
            .iter()
            .all(|call| !call.3 && call.1 != "GetSecrets")
    );
}
