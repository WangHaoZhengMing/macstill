use super::{Action, Backend, Page, SettingsView, SharedStore, Store};
use crate::system_backend::Job;
use gpui_test::{self as gpui, TestAppContext, VisualTestContext};
use gpui_test::{ElementId, Entity, Modifiers, Task, point, prelude::*, px, size};
use macstill_system::{SystemCommand, SystemSnapshot};
use std::sync::mpsc::Receiver;

fn fixture() -> SystemSnapshot {
    let state = |v: serde_json::Value| serde_json::json!({"value":v,"error":null});
    let mut system:SystemSnapshot=serde_json::from_value(serde_json::json!({
        "network":state(serde_json::json!({"enabled":true,"wifi_enabled":true,"wifi_hardware_enabled":true,"connectivity":4,"state":70,"devices":[{"path":"/org/freedesktop/NetworkManager/Devices/1","interface":"wlan0","kind":2,"state":100,"active_connection":null,"access_points":[{"path":"/org/freedesktop/NetworkManager/AccessPoint/1","ssid":"Studio Wi-Fi","ssid_bytes":[83,116,117,100,105,111],"strength":88,"flags":1,"wpa_flags":256,"rsn_flags":256}]}],"saved_connections":[{"path":"/org/freedesktop/NetworkManager/Settings/3","name":"Home","kind":"802-11-wireless","uuid":"test-uuid"}],"permissions":{},"warnings":[]})),
        "bluetooth":state(serde_json::json!({"adapters":[{"path":"/org/bluez/hci0","name":"Bluetooth","powered":true,"discovering":false,"discoverable":false,"pairable":true,"discoverable_timeout_seconds":120}],"devices":[{"path":"/org/bluez/hci0/dev_TEST","adapter":"/org/bluez/hci0","name":"Studio Headphones","paired":true,"connected":true,"trusted":true,"rssi":null,"blocked":false}]})),
        "audio":state(serde_json::json!({"server":"Test server","outputs":[{"id":7,"name":"speaker","description":"Built-in Speakers","direction":"output","percent":50.0,"muted":false,"is_default":true,"channels":[{"name":"front-left","volume":32768},{"name":"front-right","volume":32768}]}],"inputs":[]})),
        "backlights":state(serde_json::json!([{"name":"test_backlight","current":50,"maximum":100,"percent":50.0}])),
        "power":state(serde_json::json!({"on_battery":true,"battery":{"percentage":73.0,"state":2,"time_to_empty_seconds":7200,"time_to_full_seconds":0}})),
        "power_profiles":state(serde_json::json!({"active":"balanced","available":["power-saver","balanced"],"performance_degraded":""})),
        "date_time":state(serde_json::json!({"timezone":"Asia/Shanghai","ntp":true,"can_ntp":true,"synchronized":true,"local_rtc":false,"unix_microseconds":1792800000000000i64})),
        "locale":state(serde_json::json!({"locale":["LANG=en_US.UTF-8","LC_TIME=en_GB.UTF-8","LC_PAPER=de_DE.UTF-8"],"x11":{"layout":"us,de","model":"pc105","variant":",nodeadkeys","options":"grp:alt_shift_toggle"},"console":{"keymap":"us","keymap_toggle":""}})),
        "session":state(serde_json::json!({"id":"2","user_id":1000,"user_name":"Taylor","kind":"wayland","desktop":"Hyprland","state":"active","active":true,"locked":false,"idle":false})),
        "power_actions":state(serde_json::json!({"capabilities":[{"action":"power_off","availability":{"value":"yes","error":null}},{"action":"reboot","availability":{"value":"challenge","error":null}},{"action":"suspend","availability":{"value":"yes","error":null}},{"action":"hibernate","availability":{"value":"na","error":null}}],"inhibitors":{"value":[],"error":null}})),
        "host":state(serde_json::json!({"hostname":"test-computer","static_hostname":"test-computer","pretty_hostname":"My Computer","operating_system":"Linux","kernel":"6.x"}))
    })).unwrap();
    use macstill_system::pointer::*;
    let controls = PointerSetting::ALL
        .into_iter()
        .map(|setting| {
            let value = match setting {
                PointerSetting::MouseSpeed | PointerSetting::TrackpadSpeed => {
                    PointerValue::Number(0.2)
                }
                PointerSetting::MouseScrollSpeed | PointerSetting::TrackpadScrollSpeed => {
                    PointerValue::Number(1.0)
                }
                PointerSetting::MouseDoubleClick => PointerValue::Number(400.0),
                PointerSetting::MouseCursorSize => PointerValue::Number(32.0),
                PointerSetting::MouseAcceleration => PointerValue::Text("adaptive".into()),
                PointerSetting::TrackpadSecondaryClick => PointerValue::Text("fingers".into()),
                _ => PointerValue::Bool(setting != PointerSetting::MouseLeftHanded),
            };
            PointerControl {
                setting,
                value: Some(value),
                writable: true,
                choices: setting.choices(),
                error: None,
            }
        })
        .collect();
    system.pointer = Ok(PointerSnapshot {
        backend: "GNOME".into(),
        controls,
        devices: vec![
            PointerDevice {
                name: "Built-in Trackpad".into(),
                trackpad: true,
            },
            PointerDevice {
                name: "USB Mouse".into(),
                trackpad: false,
            },
        ],
        devices_error: None,
        shared_tracking_speed: false,
    })
    .into();
    system
}
fn battery_fixture() -> macstill_system::BatteryReport {
    use macstill_system::{BatteryReport, BatterySample};
    let now = chrono::Utc::now().timestamp().max(864000) as u32;
    BatteryReport {
        device: "BAT0".into(),
        capacity_percent: Some(94.),
        charge_cycles: Some(132),
        energy_full_wh: Some(51.2),
        energy_design_wh: Some(54.4),
        history_error: None,
        battery_count: 1,
        history: (0..2400)
            .map(|i| BatterySample {
                timestamp: now - 864000 + i * 360,
                percentage: if i % 240 > 210 {
                    72. + (i % 30) as f64 * 0.6
                } else {
                    87. - (i % 240) as f64 * 0.06
                },
                state: if i % 240 > 210 { 1 } else { 2 },
            })
            .collect(),
    }
}
fn setup(
    cx: &mut TestAppContext,
) -> (
    Entity<SettingsView>,
    &mut VisualTestContext,
    Receiver<Job>,
    Receiver<Action>,
) {
    let (backend, preferences) = Backend::fixture();
    let snapshot = backend.snapshot();
    let (system, commands) = crate::system_backend::SystemBackend::fixture(fixture());
    let mut system_snapshot = system.snapshot();
    system_snapshot.battery_report = Ok(battery_fixture()).into();
    cx.update(|cx| {
        macstill_ui::init(cx);
        let store = cx.new(|_| Store {
            backend,
            snapshot,
            system,
            system_snapshot,
            _refresh: Task::ready(()),
        });
        cx.set_global(SharedStore(store));
    });
    let (view, cx) = cx.add_window_view(SettingsView::new);
    cx.update(|w, cx| {
        w.resize(size(px(960.), px(720.)));
        w.bounds_changed(cx);
        w.draw(cx).clear(cx);
    });
    (view, cx, commands, preferences)
}
fn click(cx: &mut VisualTestContext, id: impl Into<ElementId>) {
    let id = id.into();
    let center = cx.update(|w, _| {
        gpui_base::test_support::find(w, &[], &id)
            .unwrap_or_else(|| panic!("Missing {id:?}"))
            .bounds()
            .center()
    });
    cx.simulate_click(center, Modifiers::default());
    cx.update(|w, cx| w.draw(cx).clear(cx));
}
fn show(view: &Entity<SettingsView>, page: Page, cx: &mut VisualTestContext) {
    cx.update(|w, cx| {
        view.update(cx, |v, cx| v.set_page(page, w, cx));
        w.draw(cx).clear(cx);
    });
}
#[gpui::test]
fn navigation_search_history_and_all_pages_render_at_supported_sizes(cx: &mut TestAppContext) {
    let (view, cx, commands, preferences) = setup(cx);
    click(cx, ("nav", 3usize));
    cx.update(|_, cx| assert_eq!(view.read(cx).page, Page::Sound));
    click(cx, "navigation-back");
    cx.update(|_, cx| assert_eq!(view.read(cx).page, Page::General));
    click(cx, "navigation-forward");
    cx.update(|_, cx| assert_eq!(view.read(cx).page, Page::Sound));
    for dark in [false, true] {
        for dimensions in [size(px(960.), px(720.)), size(px(760.), px(540.))] {
            cx.update(|w, cx| {
                let store = view.read(cx).store.clone();
                store.update(cx, |s, _| s.snapshot.preferences.dark = dark);
                w.resize(dimensions);
                w.bounds_changed(cx);
            });
            for page in Page::ALL {
                show(&view, page, cx);
            }
        }
    }
    cx.update(|w, cx| {
        let input = view.read(cx).navigation_search.clone();
        input.update(cx, |s, cx| s.set_value("microphone", w, cx));
        w.draw(cx).clear(cx);
        assert!(gpui_base::test_support::find(w, &[], &("nav", 3usize).into()).is_some());
        assert!(gpui_base::test_support::find(w, &[], &("nav", 0usize).into()).is_none());
    });
    assert!(commands.try_recv().is_err());
    assert!(preferences.try_recv().is_err());
}
#[gpui::test]
fn system_toggle_queues_one_real_command_and_busy_state_blocks_repeat_clicks(
    cx: &mut TestAppContext,
) {
    let (view, cx, commands, _) = setup(cx);
    show(&view, Page::Wifi, cx);
    click(cx, "wifi-switch");
    assert!(matches!(
        commands.try_recv().unwrap(),
        Job::Apply(SystemCommand::SetWifi(false))
    ));
    // No optimistic system-state mutation; the read-back worker owns actual values.
    cx.update(|_, cx| {
        let s = view.read(cx).system_state(cx);
        assert!(s.busy);
        assert!(s.system.network.value.unwrap().wifi_enabled);
    });
    click(cx, "wifi-switch");
    assert!(commands.try_recv().is_err());
}
#[gpui::test]
fn general_reference_rows_preserve_navigation_and_disable_unavailable_sections(
    cx: &mut TestAppContext,
) {
    let (view, cx, commands, preferences) = setup(cx);
    click(cx, super::eid("general-open", "Software Update"));
    cx.update(|_, cx| assert_eq!(view.read(cx).page, Page::General));
    click(cx, super::eid("general-open", "About"));
    cx.update(|_, cx| assert_eq!(view.read(cx).page, Page::About));
    show(&view, Page::General, cx);
    click(cx, super::eid("general-open", "Language & Region"));
    cx.update(|_, cx| assert_eq!(view.read(cx).page, Page::LanguageRegion));
    show(&view, Page::General, cx);
    click(cx, super::eid("general-open", "Date & Time"));
    cx.update(|_, cx| assert_eq!(view.read(cx).page, Page::DateTime));
    assert!(commands.try_recv().is_err());
    assert!(preferences.try_recv().is_err());
}

#[gpui::test]
fn general_hides_removed_controls_and_opens_new_pages(cx: &mut TestAppContext) {
    let (view, cx, commands, _) = setup(cx);
    cx.update(|w, _| {
        for id in [
            "edit-hostname",
            "edit-pretty-hostname",
            "general-dock",
            "general-shortcuts",
            "system-refresh",
        ] {
            assert!(gpui_base::test_support::find(w, &[], &id.into()).is_none());
        }
    });
    click(cx, super::eid("general-open", "Storage"));
    cx.update(|_, cx| assert_eq!(view.read(cx).page, Page::Storage));
    show(&view, Page::General, cx);
    click(cx, super::eid("general-open", "Login Items"));
    cx.update(|_, cx| assert_eq!(view.read(cx).page, Page::LoginItems));
    assert!(commands.try_recv().is_err());
}
#[gpui::test]
fn sound_tabs_select_devices_and_mute_the_selected_input(cx: &mut TestAppContext) {
    let (view, cx, commands, preferences) = setup(cx);
    cx.update(|_, cx| {
        let store = view.read(cx).store.clone();
        store.update(cx, |store, _| {
            let audio = store.system_snapshot.system.audio.value.as_mut().unwrap();
            let mut input = audio.outputs[0].clone();
            input.id = 8;
            input.description = "Studio Microphone".into();
            input.direction = macstill_system::AudioDirection::Input;
            audio.inputs.push(input);
            let mut output = audio.outputs[0].clone();
            output.id = 9;
            output.description = "Studio Headphones".into();
            output.is_default = false;
            audio.outputs.push(output);
        });
    });
    show(&view, Page::Sound, cx);
    click(cx, "sound-startup");
    assert!(commands.try_recv().is_err());
    click(cx, "sound-input");
    cx.update(|_, cx| assert!(view.read(cx).sound_input));
    assert!(commands.try_recv().is_err());
    let audio = cx.update(|_, cx| view.read(cx).system_state(cx).system.audio.value);
    click(cx, super::eid("audio-mute", "Input:8"));
    assert!(matches!(
        commands.try_recv().unwrap(),
        Job::Apply(SystemCommand::SetMute {
            direction: macstill_system::AudioDirection::Input,
            id: 8,
            muted: true
        })
    ));
    finish_job(&view, cx, |state| state.system.audio.value = audio);
    click(cx, "sound-output");
    click(cx, super::eid("audio-select", "Output:9"));
    assert!(matches!(
        commands.try_recv().unwrap(),
        Job::Apply(SystemCommand::SetDefaultAudio {
            direction: macstill_system::AudioDirection::Output,
            id: 9
        })
    ));
    assert!(preferences.try_recv().is_err());
}

#[gpui::test]
fn volume_slider_submits_only_on_release(cx: &mut TestAppContext) {
    let (view, cx, commands, _) = setup(cx);
    show(&view, Page::Sound, cx);
    let position = cx.update(|w, _| {
        let b = gpui_base::test_support::find(w, &[], &"slider-bar-container".into())
            .unwrap()
            .bounds();
        point(b.left() + b.size.width * 0.75, b.center().y)
    });
    cx.simulate_click(position, Modifiers::default());
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(
        matches!(commands.try_recv().unwrap(), Job::Apply(SystemCommand::SetVolume {direction: macstill_system::AudioDirection::Output, id: 7, percent}) if (percent - 75.).abs() <= 1.)
    );
}

fn set_draft(view: &Entity<SettingsView>, index: usize, value: &str, cx: &mut VisualTestContext) {
    cx.update(|w, cx| {
        let input = view.read(cx).editor.as_ref().unwrap().fields[index]
            .input
            .clone();
        input.update(cx, |input, cx| input.set_value(value, w, cx));
        w.draw(cx).clear(cx);
    });
}
fn finish_job(
    view: &Entity<SettingsView>,
    cx: &mut VisualTestContext,
    edit: impl FnOnce(&mut crate::system_backend::Snapshot),
) {
    cx.update(|w, cx| {
        let store = view.read(cx).store.clone();
        store.read(cx).system.finish_fixture_job(edit);
        view.update(cx, |s, cx| s.sync_system(cx));
        w.draw(cx).clear(cx);
    });
}
#[gpui::test]
fn wifi_join_masks_password_validates_locally_and_submits_selected_device(cx: &mut TestAppContext) {
    let (view, cx, jobs, _) = setup(cx);
    show(&view, Page::Wifi, cx);
    click(
        cx,
        "join-wifi:/org/freedesktop/NetworkManager/AccessPoint/1",
    );
    set_draft(&view, 2, "short", cx);
    click(cx, "save-draft");
    assert!(jobs.try_recv().is_err());
    cx.update(|_, cx| {
        assert!(view.read(cx).editor.as_ref().unwrap().error.is_some());
        assert!(
            view.read(cx).editor.as_ref().unwrap().fields[2]
                .input
                .read(cx)
                .presentation()
                .is_masked()
        );
    });
    set_draft(&view, 2, "studio-passphrase", cx);
    click(cx, "save-draft");
    let Job::JoinWifi { config, device } = jobs.try_recv().unwrap() else {
        panic!("expected join job")
    };
    assert_eq!(device, "/org/freedesktop/NetworkManager/Devices/1");
    assert_eq!(config.ssid, b"Studio");
    assert!(
        matches!(config.security,macstill_system::WifiSecurity::WpaPsk {password} if password=="studio-passphrase")
    );
    assert!(jobs.try_recv().is_err());
    finish_job(&view, cx, |_| {});
    cx.update(|_, cx| assert!(view.read(cx).editor.is_none()));
}
#[gpui::test]
fn cancel_wifi_draft_never_changes_system_settings(cx: &mut TestAppContext) {
    let (view, cx, jobs, _) = setup(cx);
    show(&view, Page::Wifi, cx);
    click(cx, "join-other-network");
    set_draft(&view, 0, "Hidden Wi-Fi", cx);
    set_draft(&view, 2, "sensitive-password", cx);
    click(cx, "cancel-draft");
    assert!(jobs.try_recv().is_err());
    cx.update(|_, cx| assert!(view.read(cx).editor.is_none()));
}
#[gpui::test]
fn language_edit_preserves_unedited_locale_categories(cx: &mut TestAppContext) {
    let (view, cx, jobs, _) = setup(cx);
    show(&view, Page::LanguageRegion, cx);
    click(cx, "edit-language-region");
    set_draft(&view, 0, "zh_CN.UTF-8", cx);
    click(cx, "save-draft");
    let Job::Apply(SystemCommand::SetLocale(assignments)) = jobs.try_recv().unwrap() else {
        panic!("expected locale write")
    };
    assert!(assignments.contains(&"LANG=zh_CN.UTF-8".into()));
    assert!(assignments.contains(&"LC_PAPER=de_DE.UTF-8".into()));
}
#[gpui::test]
fn keyboard_sheet_submits_layouts_and_conversion_choice(cx: &mut TestAppContext) {
    let (view, cx, jobs, _) = setup(cx);
    show(&view, Page::Keyboard, cx);
    click(cx, "edit-input-sources");
    set_draft(&view, 0, "us,fr", cx);
    set_draft(&view, 2, ",", cx);
    click(cx, "save-draft");
    assert!(
        matches!(jobs.try_recv().unwrap(),Job::Apply(SystemCommand::SetX11Keyboard {keyboard,convert:false}) if keyboard.layout=="us,fr" && keyboard.variant==",")
    );
}
#[gpui::test]
fn power_action_requires_explicit_confirmation_and_cancel_does_nothing(cx: &mut TestAppContext) {
    let (view, cx, jobs, _) = setup(cx);
    show(&view, Page::LockScreen, cx);
    click(cx, "power-action:Reboot");
    assert!(jobs.try_recv().is_err());
    click(cx, "cancel-confirmation");
    assert!(jobs.try_recv().is_err());
    click(cx, "power-action:Reboot");
    click(cx, "confirm-action");
    assert!(matches!(
        jobs.try_recv().unwrap(),
        Job::Apply(SystemCommand::PowerAction(
            macstill_system::PowerAction::Reboot
        ))
    ));
}
#[gpui::test]
fn bluetooth_forget_has_confirmation_and_uses_selected_device(cx: &mut TestAppContext) {
    let (view, cx, jobs, _) = setup(cx);
    show(&view, Page::Bluetooth, cx);
    click(cx, "bluetooth-device-details:/org/bluez/hci0/dev_TEST");
    click(cx, "forget-bluetooth-device");
    assert!(jobs.try_recv().is_err());
    click(cx, "confirm-action");
    assert!(
        matches!(jobs.try_recv().unwrap(),Job::Apply(SystemCommand::RemoveBluetoothDevice {device}) if device=="/org/bluez/hci0/dev_TEST")
    );
}
#[gpui::test]
fn connection_details_load_asynchronously_and_ip_editor_uses_selected_profile(
    cx: &mut TestAppContext,
) {
    let (view, cx, jobs, _) = setup(cx);
    show(&view, Page::Network, cx);
    click(
        cx,
        "network-details:/org/freedesktop/NetworkManager/Settings/3",
    );
    assert!(
        matches!(jobs.try_recv().unwrap(),Job::Query(macstill_system::SystemQuery::Connection {connection}) if connection.ends_with("/3"))
    );
    finish_job(&view, cx, |s| {
        s.connection = Ok(macstill_system::ConnectionDetails {
            path: "/org/freedesktop/NetworkManager/Settings/3".into(),
            name: "Home".into(),
            uuid: "test".into(),
            kind: "802-11-wireless".into(),
            autoconnect: true,
            ipv4: Some(macstill_system::IpConfiguration {
                method: "auto".into(),
                addresses: vec![],
                gateway: None,
                dns: vec![],
                dns_search: vec![],
                ignore_auto_dns: false,
            }),
            ipv6: None,
        })
        .into()
    });
    click(cx, "edit-ip:TCP/IP · IPv4");
    click(cx, ("draft-choice", 1usize));
    set_draft(&view, 0, "192.0.2.9/24", cx);
    set_draft(&view, 1, "192.0.2.1", cx);
    set_draft(&view, 2, "9.9.9.9", cx);
    click(cx, "save-draft");
    assert!(
        matches!(jobs.try_recv().unwrap(),Job::Apply(SystemCommand::SetConnectionIp {connection,family:macstill_system::IpFamily::Ipv4,config}) if connection.ends_with("/3") && config.addresses[0].address=="192.0.2.9" && config.dns==["9.9.9.9"])
    );
}
#[gpui::test]
fn failed_write_keeps_the_editor_and_navigation_drops_drafts(cx: &mut TestAppContext) {
    let (view, cx, jobs, _) = setup(cx);
    show(&view, Page::Keyboard, cx);
    click(cx, "edit-input-sources");
    click(cx, "save-draft");
    assert!(jobs.try_recv().is_ok());
    finish_job(&view, cx, |s| s.error = Some("Permission denied".into()));
    cx.update(|_, cx| {
        let editor = view.read(cx).editor.as_ref().unwrap();
        assert!(!editor.pending);
        assert_eq!(editor.error.as_deref(), Some("Permission denied"));
    });
    click(cx, "cancel-draft");
    show(&view, Page::General, cx);
    cx.update(|_, cx| assert!(view.read(cx).editor.is_none()));
}
#[test]
fn enterprise_wifi_is_not_misrepresented_as_personal_security() {
    let mut ap = macstill_system::AccessPoint {
        path: "/test".into(),
        ssid: "Corporate".into(),
        ssid_bytes: b"Corporate".to_vec(),
        strength: 99,
        flags: 1,
        wpa_flags: 0,
        rsn_flags: 0x200,
    };
    assert!(super::editors::wifi_security(&ap).is_none());
    ap.rsn_flags = 0x400;
    assert!(matches!(
        super::editors::wifi_security(&ap),
        Some(super::editors::Security::Sae)
    ));
}

/// Run explicitly when a real GPU renderer is available; all data is isolated.
#[test]
#[ignore = "requires a real offscreen GPU renderer; saves PNGs for visual review"]
fn sonoma_visual_review() {
    use gpui_kit::{AppContext, HeadlessAppContext};
    let mut context = HeadlessAppContext::with_platform(
        gpui_kit::platform::current_platform(true).text_system(),
        std::sync::Arc::new(()),
        gpui_platform_test::current_headless_renderer,
    );
    context.update(|cx| {
        macstill_ui::init(cx);
        let (backend, _) = Backend::fixture();
        let snapshot = backend.snapshot();
        let (system, _) = crate::system_backend::SystemBackend::fixture(fixture());
        let mut system_snapshot = system.snapshot();
        system_snapshot.battery_report = Ok(battery_fixture()).into();
        let store = cx.new(|_| Store {
            backend,
            snapshot,
            system,
            system_snapshot,
            _refresh: Task::ready(()),
        });
        cx.set_global(SharedStore(store));
    });
    let handle = context
        .open_window(size(px(960.), px(720.)), |w, cx| {
            cx.new(|cx| SettingsView::new(w, cx))
        })
        .unwrap();
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/settings-review");
    std::fs::create_dir_all(&directory).unwrap();
    for (page, name, dark) in [
        (Page::General, "general", false),
        (Page::Sound, "sound", false),
        (Page::Sound, "sound-dark", true),
        (Page::Dock, "dock", false),
        (Page::Dock, "dock-dark", true),
        (Page::Battery, "battery", false),
        (Page::ControlCenter, "control-center", false),
        (Page::ControlCenter, "control-center-dark", true),
        (Page::Battery, "battery-dark", true),
        (Page::Wifi, "wifi", false),
        (Page::Keyboard, "keyboard", false),
        (Page::Mouse, "mouse", false),
        (Page::Trackpad, "trackpad", false),
        (Page::Trackpad, "trackpad-dark", true),
        (Page::LanguageRegion, "language", false),
        (Page::LockScreen, "lock-screen", false),
        (Page::Bluetooth, "bluetooth-dark", true),
    ] {
        handle
            .update(&mut context, |view, w, cx| {
                let store = view.store.clone();
                store.update(cx, |store, _| store.snapshot.preferences.dark = dark);
                view.set_page(page, w, cx);
            })
            .unwrap();
        context.run_until_parked();
        context
            .update_window(handle.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        context
            .capture_screenshot(handle.into())
            .unwrap()
            .save(directory.join(format!("{name}.png")))
            .unwrap();
    }
    for (name, small) in [("join-sheet", false), ("join-sheet-small", true)] {
        handle
            .update(&mut context, |view, w, cx| {
                let store = view.store.clone();
                store.update(cx, |store, _| store.snapshot.preferences.dark = false);
                w.resize(if small {
                    size(px(760.), px(540.))
                } else {
                    size(px(960.), px(720.))
                });
                w.bounds_changed(cx);
                view.set_page(Page::Wifi, w, cx);
                view.join_network(
                    "/org/freedesktop/NetworkManager/Devices/1".into(),
                    None,
                    super::editors::Security::Personal,
                    w,
                    cx,
                );
                view.editor.as_mut().unwrap().error =
                    Some("Enter a network name and a valid password.".into());
            })
            .unwrap();
        context
            .update_window(handle.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        context.run_until_parked();
        context
            .update_window(handle.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        context
            .capture_screenshot(handle.into())
            .unwrap()
            .save(directory.join(format!("{name}.png")))
            .unwrap();
    }
    for (tab, name, small) in [
        (1, "trackpad-scroll", false),
        (2, "trackpad-gestures", false),
        (0, "trackpad-small", true),
    ] {
        handle
            .update(&mut context, |view, w, cx| {
                let store = view.store.clone();
                store.update(cx, |store, _| store.snapshot.preferences.dark = false);
                w.resize(if small {
                    size(px(760.), px(540.))
                } else {
                    size(px(960.), px(720.))
                });
                w.bounds_changed(cx);
                view.set_page(Page::Trackpad, w, cx);
                view.pointer_tab = tab;
            })
            .unwrap();
        context
            .update_window(handle.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        context.run_until_parked();
        context
            .update_window(handle.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        context
            .capture_screenshot(handle.into())
            .unwrap()
            .save(directory.join(format!("{name}.png")))
            .unwrap();
    }
    println!("Screenshots: {}", directory.display());
}

#[gpui::test]
fn modal_tab_navigation_stays_in_the_sheet_and_escape_cancels(cx: &mut TestAppContext) {
    let (view, cx, jobs, _) = setup(cx);
    show(&view, Page::Keyboard, cx);
    click(cx, "edit-input-sources");
    for _ in 0..18 {
        cx.simulate_keystrokes("tab");
        cx.update(|w, cx| {
            w.draw(cx).clear(cx);
            assert!(view.read(cx).sheet_focus.contains_focused(w, cx));
        });
    }
    cx.simulate_keystrokes("escape");
    cx.update(|w, cx| {
        w.draw(cx).clear(cx);
        assert!(view.read(cx).editor.is_none());
    });
    assert!(jobs.try_recv().is_err());
}
#[gpui::test]
fn small_window_keeps_sheet_actions_inside_the_viewport(cx: &mut TestAppContext) {
    let (view, cx, jobs, _) = setup(cx);
    cx.update(|w, cx| {
        w.resize(size(px(760.), px(540.)));
        w.bounds_changed(cx);
    });
    show(&view, Page::Wifi, cx);
    click(cx, "join-other-network");
    cx.update(|w, _| {
        for id in ["cancel-draft", "save-draft"] {
            let bounds = gpui_base::test_support::find(w, &[], &id.into())
                .unwrap()
                .bounds();
            assert!(bounds.top() >= px(0.) && bounds.bottom() <= w.viewport_size().height);
        }
    });
    assert!(jobs.try_recv().is_err());
}
#[gpui::test]
fn manual_time_requires_ntp_off_and_submits_utc_microseconds(cx: &mut TestAppContext) {
    let (view, cx, jobs, _) = setup(cx);
    show(&view, Page::DateTime, cx);
    click(cx, "edit-manual-time");
    cx.update(|_, cx| assert!(view.read(cx).editor.is_none()));
    assert!(jobs.try_recv().is_err());
    finish_job(&view, cx, |s| {
        s.system.date_time.value.as_mut().unwrap().ntp = false
    });
    click(cx, "edit-manual-time");
    set_draft(&view, 0, "1970-01-02 00:00:00", cx);
    click(cx, "save-draft");
    assert!(matches!(
        jobs.try_recv().unwrap(),
        Job::Apply(SystemCommand::SetTime {
            unix_microseconds: 86_400_000_000
        })
    ));
}

#[gpui::test]
fn trackpad_tabs_and_tap_to_click_dispatch_without_optimistic_state(cx: &mut TestAppContext) {
    use macstill_system::pointer::*;
    let (view, cx, jobs, _) = setup(cx);
    show(&view, Page::Trackpad, cx);
    click(cx, "pointer-toggle-TrackpadTapToClick");
    assert!(matches!(
        jobs.try_recv().unwrap(),
        Job::Apply(SystemCommand::SetPointer(PointerCommand {
            setting: PointerSetting::TrackpadTapToClick,
            value: PointerValue::Bool(false)
        }))
    ));
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx)
                .system_state(cx)
                .system
                .pointer
                .value
                .unwrap()
                .boolean(PointerSetting::TrackpadTapToClick),
            Some(true)
        )
    });
    finish_job(&view, cx, |_| {});
    click(cx, ("pointer-tab", 1usize));
    cx.update(|_, cx| assert_eq!(view.read(cx).pointer_tab, 1));
    click(cx, "pointer-toggle-TrackpadNaturalScroll");
    assert!(matches!(
        jobs.try_recv().unwrap(),
        Job::Apply(SystemCommand::SetPointer(PointerCommand {
            setting: PointerSetting::TrackpadNaturalScroll,
            value: PointerValue::Bool(false)
        }))
    ));
}
#[gpui::test]
fn mouse_tracking_slider_converts_percent_to_linux_speed_only_on_release(cx: &mut TestAppContext) {
    use macstill_system::pointer::*;
    let (view, cx, jobs, _) = setup(cx);
    show(&view, Page::Mouse, cx);
    let position = cx.update(|w, _| {
        let bounds = gpui_base::test_support::find_all(w, &[], &"slider-bar-container".into())
            .into_iter()
            .find(|element| {
                element
                    .path()
                    .contains(&"slider-control:pointer-MouseSpeed".into())
            })
            .expect("tracking slider")
            .bounds();
        point(bounds.left() + bounds.size.width * 0.75, bounds.center().y)
    });
    cx.simulate_click(position, Modifiers::default());
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(
        matches!(jobs.try_recv().unwrap(),Job::Apply(SystemCommand::SetPointer(PointerCommand {setting:PointerSetting::MouseSpeed,value:PointerValue::Number(value)})) if (value-0.5).abs()<0.02)
    );
    assert!(jobs.try_recv().is_err());
}
#[gpui::test]
fn pointer_controls_unavailable_or_locked_do_not_dispatch(cx: &mut TestAppContext) {
    use macstill_system::pointer::*;
    let (view, cx, jobs, _) = setup(cx);
    finish_job(&view, cx, |state| {
        let control = state
            .system
            .pointer
            .value
            .as_mut()
            .unwrap()
            .controls
            .iter_mut()
            .find(|c| c.setting == PointerSetting::TrackpadTapToClick)
            .unwrap();
        control.writable = false;
    });
    show(&view, Page::Trackpad, cx);
    click(cx, "pointer-toggle-TrackpadTapToClick");
    click(cx, "pointer-force-click");
    assert!(jobs.try_recv().is_err());
    finish_job(&view, cx, |state| {
        state.system.pointer = Err(macstill_system::SystemError::new(
            macstill_system::ErrorKind::Unsupported,
            "pointer",
            "Unsupported desktop",
        ))
        .into()
    });
    show(&view, Page::Mouse, cx);
    click(cx, "pointer-toggle-MouseLeftHanded");
    assert!(jobs.try_recv().is_err());
}
#[gpui::test]
fn mouse_and_trackpad_entry_points_and_all_tabs_render_at_small_sizes(cx: &mut TestAppContext) {
    let (view, cx, jobs, _) = setup(cx);
    for dark in [false, true] {
        for page in [Page::Mouse, Page::Trackpad] {
            cx.update(|w, cx| {
                w.resize(size(px(760.), px(540.)));
                w.bounds_changed(cx);
                let store = view.read(cx).store.clone();
                store.update(cx, |s, _| s.snapshot.preferences.dark = dark);
            });
            show(&view, page, cx);
            for tab in 0usize..if page == Page::Trackpad { 3 } else { 2 } {
                click(cx, ("pointer-tab", tab));
            }
        }
    }
    assert!(jobs.try_recv().is_err());
}

#[gpui::test]
fn battery_low_power_menu_dispatches_real_profiles_and_period_tabs_do_not_write(
    cx: &mut TestAppContext,
) {
    let (view, cx, commands, _) = setup(cx);
    show(&view, Page::Battery, cx);
    click(cx, ("battery-period", 1usize));
    assert!(view.read_with(cx, |view, _| view.battery_days));
    assert!(commands.try_recv().is_err());
    click(cx, "battery-low-power");
    click(cx, super::eid("battery-low-power-option", "power-saver"));
    assert!(
        matches!(commands.try_recv().unwrap(),Job::Apply(SystemCommand::SetPowerProfile(profile)) if profile=="power-saver")
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view
            .system_state(cx)
            .system
            .power_profiles
            .value
            .unwrap()
            .active),
        "balanced"
    );
}
#[gpui::test]
fn battery_options_preserve_performance_mode_and_health_details_are_read_only(
    cx: &mut TestAppContext,
) {
    let (view, cx, commands, _) = setup(cx);
    view.update(cx, |view, cx| {
        view.store.update(cx, |store, _| {
            store
                .system_snapshot
                .system
                .power_profiles
                .value
                .as_mut()
                .unwrap()
                .available
                .push("performance".into())
        })
    });
    show(&view, Page::Battery, cx);
    click(cx, "battery-health-info");
    cx.simulate_keystrokes("escape");
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(commands.try_recv().is_err());
    view.update(cx, |view, cx| {
        view.scroll.set_offset(gpui_test::point(px(0.), px(-800.)));
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    click(cx, "battery-options");
    click(cx, super::eid("battery-profile", "performance"));
    assert!(
        matches!(commands.try_recv().unwrap(),Job::Apply(SystemCommand::SetPowerProfile(profile)) if profile=="performance")
    );
}

fn wifi_reference_fixture() -> SystemSnapshot {
    let mut system = fixture();
    let network = system.network.value.as_mut().unwrap();
    network.saved_connections[0].name = "Tahoe".into();
    network
        .saved_connections
        .push(macstill_system::SavedConnection {
            path: "/org/freedesktop/NetworkManager/Settings/4".into(),
            name: "Home".into(),
            kind: "802-11-wireless".into(),
            uuid: "other-profile".into(),
        });
    let radio = &mut network.devices[0];
    radio.active_connection = Some("/org/freedesktop/NetworkManager/ActiveConnection/7".into());
    radio.active_profile = Some(network.saved_connections[0].path.clone());
    radio.active_access_point = Some("/org/freedesktop/NetworkManager/AccessPoint/2".into());
    for (id, name, strength) in [
        (2, "Tahoe", 90),
        (3, "Home", 70),
        (4, "Coffee House", 62),
        (5, "xfinitywifi", 30),
    ] {
        radio.access_points.push(macstill_system::AccessPoint {
            path: format!("/org/freedesktop/NetworkManager/AccessPoint/{id}"),
            ssid: name.into(),
            ssid_bytes: name.as_bytes().to_vec(),
            strength,
            flags: u32::from(id != 5),
            wpa_flags: 0,
            rsn_flags: if id == 5 { 0 } else { 256 },
        });
    }
    system
}

fn wifi_details_fixture() -> macstill_system::ConnectionDetails {
    macstill_system::ConnectionDetails {
        path: "/org/freedesktop/NetworkManager/Settings/3".into(),
        name: "Tahoe".into(),
        uuid: "test-uuid".into(),
        kind: "802-11-wireless".into(),
        autoconnect: true,
        ipv4: Some(macstill_system::IpConfiguration {
            method: "manual".into(),
            addresses: vec![macstill_system::IpAddress {
                address: "192.168.5.9".into(),
                prefix: 24,
            }],
            gateway: Some("192.168.5.1".into()),
            dns: vec![],
            dns_search: vec![],
            ignore_auto_dns: false,
        }),
        ipv6: None,
    }
}

#[gpui::test]
fn wifi_reference_known_network_menu_preserves_connect_and_detail_actions(cx: &mut TestAppContext) {
    let (view, cx, jobs, _) = setup(cx);
    finish_job(&view, cx, |s| {
        s.system.network = wifi_reference_fixture().network
    });
    show(&view, Page::Wifi, cx);
    click(
        cx,
        "wifi-options:/org/freedesktop/NetworkManager/Settings/4",
    );
    click(
        cx,
        "wifi-connect:/org/freedesktop/NetworkManager/Settings/4:/org/freedesktop/NetworkManager/Devices/1",
    );
    assert!(
        matches!(jobs.try_recv().unwrap(), Job::Apply(SystemCommand::ActivateConnection { connection, device })
        if connection.ends_with("/4") && device.ends_with("/1"))
    );
    finish_job(&view, cx, |_| {});
    click(
        cx,
        "wifi-current-details:/org/freedesktop/NetworkManager/Devices/1",
    );
    assert!(
        matches!(jobs.try_recv().unwrap(), Job::Query(macstill_system::SystemQuery::Connection { connection }) if connection.ends_with("/3"))
    );
}

#[gpui::test]
fn wifi_reference_details_cancel_does_not_save_and_ok_retains_failed_draft(
    cx: &mut TestAppContext,
) {
    let (view, cx, jobs, _) = setup(cx);
    finish_job(&view, cx, |s| {
        s.system.network = wifi_reference_fixture().network
    });
    show(&view, Page::Wifi, cx);
    for save in [false, true] {
        click(
            cx,
            "wifi-current-details:/org/freedesktop/NetworkManager/Devices/1",
        );
        assert!(matches!(jobs.try_recv().unwrap(), Job::Query(_)));
        finish_job(&view, cx, |s| {
            s.connection = Ok(wifi_details_fixture()).into()
        });
        click(cx, "wifi-low-data");
        click(cx, "wifi-limit-tracking");
        click(cx, "connection-autoconnect");
        assert!(jobs.try_recv().is_err());
        if !save {
            click(cx, "cancel-wifi-details");
            cx.update(|_, cx| assert!(view.read(cx).connection_details.is_none()));
        } else {
            click(cx, "close-connection");
            assert!(
                matches!(jobs.try_recv().unwrap(), Job::Apply(SystemCommand::SetConnectionAutoconnect { connection, enabled: false }) if connection.ends_with("/3"))
            );
            cx.simulate_keystrokes("escape");
            cx.update(|_, cx| assert!(view.read(cx).connection_details.is_some()));
            finish_job(&view, cx, |s| {
                s.error = Some("Authorization was denied".into())
            });
            cx.update(|_, cx| {
                let draft = view.read(cx).wifi_details.as_ref().unwrap();
                assert!(!draft.pending);
            });
            click(cx, "close-connection");
            assert!(matches!(
                jobs.try_recv().unwrap(),
                Job::Apply(SystemCommand::SetConnectionAutoconnect { enabled: false, .. })
            ));
            finish_job(&view, cx, |s| {
                s.connection.value.as_mut().unwrap().autoconnect = false
            });
            cx.update(|_, cx| assert!(view.read(cx).connection_details.is_none()));
        }
    }
    assert!(jobs.try_recv().is_err());
}

#[gpui::test]
fn wifi_reference_forget_still_requires_confirmation(cx: &mut TestAppContext) {
    let (view, cx, jobs, _) = setup(cx);
    finish_job(&view, cx, |s| {
        s.system.network = wifi_reference_fixture().network
    });
    show(&view, Page::Wifi, cx);
    click(
        cx,
        "wifi-current-details:/org/freedesktop/NetworkManager/Devices/1",
    );
    assert!(matches!(jobs.try_recv().unwrap(), Job::Query(_)));
    finish_job(&view, cx, |s| {
        s.connection = Ok(wifi_details_fixture()).into()
    });
    click(cx, "forget-network");
    assert!(jobs.try_recv().is_err());
    click(cx, "cancel-confirmation");
    assert!(jobs.try_recv().is_err());
    click(cx, "forget-network");
    click(cx, "confirm-action");
    assert!(
        matches!(jobs.try_recv().unwrap(), Job::Apply(SystemCommand::ForgetConnection { connection }) if connection.ends_with("/3"))
    );
}

#[test]
#[ignore = "requires a real offscreen GPU renderer; saves Wi-Fi reference screenshots"]
fn wifi_reference_visual_review() {
    use gpui_kit::{AppContext, HeadlessAppContext};
    let mut context = HeadlessAppContext::with_platform(
        gpui_kit::platform::current_platform(true).text_system(),
        std::sync::Arc::new(()),
        gpui_platform_test::current_headless_renderer,
    );
    let _channels = context.update(|cx| {
        macstill_ui::init(cx);
        let (backend, preferences) = Backend::fixture();
        let snapshot = backend.snapshot();
        let (system, commands) =
            crate::system_backend::SystemBackend::fixture(wifi_reference_fixture());
        let system_snapshot = system.snapshot();
        let store = cx.new(|_| Store {
            backend,
            snapshot,
            system,
            system_snapshot,
            _refresh: Task::ready(()),
        });
        cx.set_global(SharedStore(store));
        (preferences, commands)
    });
    let handle = context
        .open_window(size(px(960.), px(720.)), |w, cx| {
            cx.new(|cx| SettingsView::new(w, cx))
        })
        .unwrap();
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/settings-review");
    std::fs::create_dir_all(&directory).unwrap();
    for (name, dark, small, details) in [
        ("wifi-reference", false, false, false),
        ("wifi-reference-dark", true, false, false),
        ("wifi-details-reference", false, false, true),
        ("wifi-details-reference-dark", true, false, true),
        ("wifi-details-reference-small", false, true, true),
        ("wifi-reference-small", false, true, false),
    ] {
        handle
            .update(&mut context, |view, w, cx| {
                view.store.update(cx, |store, _| {
                    store.snapshot.preferences.dark = dark;
                    store.system_snapshot.connection_path =
                        Some("/org/freedesktop/NetworkManager/Settings/3".into());
                    store.system_snapshot.connection = Ok(wifi_details_fixture()).into();
                });
                w.resize(if small {
                    size(px(760.), px(540.))
                } else {
                    size(px(960.), px(720.))
                });
                w.bounds_changed(cx);
                view.set_page(Page::Wifi, w, cx);
                if details {
                    view.connection_details =
                        Some("/org/freedesktop/NetworkManager/Settings/3".into());
                }
            })
            .unwrap();
        context.run_until_parked();
        context
            .update_window(handle.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        context
            .capture_screenshot(handle.into())
            .unwrap()
            .save(directory.join(format!("{name}.png")))
            .unwrap();
    }
}

#[gpui::test]
fn control_center_sidebar_and_visibility_choices_save_real_preferences(cx: &mut TestAppContext) {
    use macstill_config::{ControlCenterModule as Module, MenuBarVisibility as Visibility};
    let (view, cx, jobs, preferences) = setup(cx);
    let index = Page::ALL
        .iter()
        .position(|page| *page == Page::ControlCenter)
        .unwrap();
    click(cx, ("nav", index));
    assert_eq!(view.read_with(cx, |view, _| view.page), Page::ControlCenter);
    click(cx, super::eid("cc-module", "wifi"));
    click(cx, super::eid("cc-visibility", "wifi:always"));
    assert!(
        matches!(preferences.try_recv().unwrap(),Action::Save(value) if value.control_center.visibility(Module::Wifi)==Visibility::Always)
    );
    assert!(jobs.try_recv().is_err());
    click(cx, super::eid("cc-module", "stage_manager"));
    assert!(preferences.try_recv().is_err());
    click(cx, super::eid("cc-visibility", "stage_manager:always"));
    assert!(
        matches!(preferences.try_recv().unwrap(), Action::Save(value)
        if value.control_center.visibility(Module::StageManager) == Visibility::Always)
    );
    assert!(jobs.try_recv().is_err());
}
#[gpui::test]
fn control_center_battery_options_compose_without_system_writes(cx: &mut TestAppContext) {
    let (view, cx, jobs, preferences) = setup(cx);
    show(&view, Page::ControlCenter, cx);
    view.update(cx, |view, cx| {
        view.scroll.set_offset(gpui_test::point(px(0.), px(-1000.)));
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    click(cx, "cc-battery-control-center");
    assert!(
        matches!(preferences.try_recv().unwrap(),Action::Save(value) if value.control_center.battery_in_control_center)
    );
    click(cx, "cc-battery-menu-bar");
    assert!(
        matches!(preferences.try_recv().unwrap(),Action::Save(value) if value.control_center.battery_in_control_center && !value.control_center.battery_in_menu_bar)
    );
    assert!(jobs.try_recv().is_err());
}

#[gpui::test]
fn reference_navigation_keeps_scroll_and_supports_search_keyboard(cx: &mut TestAppContext) {
    let (view, cx, commands, preferences) = setup(cx);
    view.update(cx, |s, _| s.scroll.set_offset(point(px(0.), px(-140.))));
    show(&view, Page::Appearance, cx);
    click(cx, "navigation-back");
    assert_eq!(view.read_with(cx, |s, _| s.scroll.offset().y), px(-140.));
    cx.update(|w, cx| {
        let input = view.read(cx).navigation_search.clone();
        input.update(cx, |input, cx| {
            input.set_value("microphone", w, cx);
            input.focus(w, cx);
        });
        w.draw(cx).clear(cx);
    });
    cx.simulate_keystrokes("enter");
    assert_eq!(view.read_with(cx, |s, _| s.page), Page::Sound);
    click(cx, "clear-settings-search");
    assert!(view.read_with(cx, |s, cx| s.navigation_search.read(cx).value().is_empty()));
    assert!(commands.try_recv().is_err());
    assert!(preferences.try_recv().is_err());
}

#[test]
#[ignore = "native GPU visual matrix for the local macOS 15 reference redraw"]
fn reference_redraw_visual_review() {
    use gpui_kit::{AppContext, HeadlessAppContext};
    let mut context = HeadlessAppContext::with_platform(
        gpui_kit::platform::current_platform(true).text_system(),
        std::sync::Arc::new(()),
        gpui_platform_test::current_headless_renderer,
    );
    context.update(|cx| {
        macstill_ui::init(cx);
        let (backend, _) = Backend::fixture();
        let mut snapshot = backend.snapshot();
        snapshot.apps = [
            ("org.gnome.Nautilus.desktop", "Files", "finder.png"),
            ("firefox.desktop", "Firefox", "safari.png"),
            ("org.gnome.clocks.desktop", "Clocks", "clock.png"),
        ]
        .into_iter()
        .map(|(id, name, icon)| macstill_apps::DesktopApp {
            id: id.into(),
            name: name.into(),
            desktop_file: std::path::PathBuf::from("/fixture/unused.desktop"),
            icon: Some(SettingsView::asset(icon)),
            class_aliases: Vec::new(),
            startup_class: None,
        })
        .collect();
        let (system, _) = crate::system_backend::SystemBackend::fixture(fixture());
        let mut system_snapshot = system.snapshot();
        system_snapshot.battery_report = Ok(battery_fixture()).into();
        let store = cx.new(|_| Store {
            backend,
            snapshot,
            system,
            system_snapshot,
            _refresh: Task::ready(()),
        });
        cx.set_global(SharedStore(store));
    });
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/settings-reference-review");
    std::fs::create_dir_all(&directory).unwrap();
    let page_filter = std::env::var("MACSTILL_REVIEW_PAGE").ok();
    for page in Page::ALL {
        if page_filter.as_ref().is_some_and(|filter| {
            !filter
                .split(',')
                .any(|name| name == format!("{page:?}").to_lowercase())
        }) {
            continue;
        }
        for (variant, dark, width, height) in [
            ("light", false, 760., 760.),
            ("dark", true, 900., 900.),
            ("compact", false, 760., 540.),
        ] {
            let handle = context
                .open_window(size(px(width), px(height)), |w, cx| {
                    let view = cx.new(|cx| SettingsView::new(w, cx));
                    view.update(cx, |view, cx| {
                        view.store
                            .update(cx, |store, _| store.snapshot.preferences.dark = dark);
                        view.set_page(page, w, cx);
                    });
                    view
                })
                .unwrap();
            context.run_until_parked();
            for _ in 0..2 {
                context
                    .update_window(handle.into(), |_, w, cx| w.draw(cx).clear(cx))
                    .unwrap();
                context.run_until_parked();
            }
            context
                .capture_screenshot(handle.into())
                .unwrap()
                .save(directory.join(format!("{:?}-{variant}.png", page).to_lowercase()))
                .unwrap();
            handle
                .update(&mut context, |_, w, _| w.remove_window())
                .unwrap();
            context.run_until_parked();
        }
    }
}

#[gpui::test]
fn mission_control_options_live_on_dock_page_and_save(cx: &mut TestAppContext) {
    let (view, cx, jobs, preferences) = setup(cx);
    show(&view, Page::Dock, cx);
    view.update(cx, |view, cx| {
        view.scroll.set_offset(point(px(0.), px(-1000.)));
        cx.notify();
    });
    cx.update(|w, cx| w.draw(cx).clear(cx));
    click(cx, "mission-group");
    assert!(
        matches!(preferences.try_recv().unwrap(), Action::Save(value) if value.mission_control.group_by_application)
    );
    click(cx, "mission-animate");
    assert!(
        matches!(preferences.try_recv().unwrap(), Action::Save(value) if !value.mission_control.animate)
    );
    assert!(jobs.try_recv().is_err());
}
