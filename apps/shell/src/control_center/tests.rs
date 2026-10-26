use super::*;
use macstill_system::{AudioDirection, AudioState, Backlight, BluetoothState, NetworkState};

fn audio() -> AudioState {
    serde_json::from_value(serde_json::json!({
        "server":"PipeWire", "inputs":[], "outputs":[
            {"id":2,"name":"hdmi","description":"HDMI","direction":"output","percent":80.,"muted":false,"is_default":false,"channels":[]},
            {"id":7,"name":"speakers","description":"Speakers","direction":"output","percent":35.,"muted":true,"is_default":true,"channels":[]}
        ]
    })).unwrap()
}
fn network() -> NetworkState {
    serde_json::from_value(serde_json::json!({
        "enabled":true,"wifi_enabled":true,"wifi_hardware_enabled":true,"connectivity":4,"state":70,
        "saved_connections":[],"permissions":{},"warnings":[],"devices":[{
            "path":"/device/wifi","interface":"wlan0","kind":2,"state":100,
            "active_connection":"/active/1","active_access_point":"/ap/active","access_points":[
                {"path":"/ap/nearby","ssid":"Nearby","ssid_bytes":[],"strength":99,"flags":0,"wpa_flags":0,"rsn_flags":0},
                {"path":"/ap/active","ssid":"家: Wi-Fi","ssid_bytes":[],"strength":70,"flags":0,"wpa_flags":0,"rsn_flags":0}
            ]
        }]
    })).unwrap()
}

#[::core::prelude::v1::test]
fn default_output_and_reported_backlight_are_used_for_commands() {
    let audio = audio();
    let lights = vec![
        Backlight {
            name: "z_backlight".into(),
            current: 90,
            maximum: 100,
            percent: 90.,
        },
        Backlight {
            name: "intel_backlight".into(),
            current: 30,
            maximum: 100,
            percent: 30.,
        },
    ];
    let status = Status::from_system(None, None, Some(&audio), Some(&lights));
    assert_eq!(status.volume, Some(35.));
    assert_eq!(status.brightness, Some(30.));
    assert!(
        matches!(status.command(ControlAction::Volume(42.)),Some(SystemCommand::SetVolume {direction:AudioDirection::Output,id:7,percent}) if percent==42.)
    );
    assert!(
        matches!(status.command(ControlAction::Brightness(65.)),Some(SystemCommand::SetBrightness {name,percent}) if name=="intel_backlight" && percent==65.)
    );
    // Audio absence cannot disable brightness or accidentally target a non-default sink.
    let mut audio = audio;
    for output in &mut audio.outputs {
        output.is_default = false;
    }
    let status = Status::from_system(None, None, Some(&audio), Some(&lights));
    assert!(status.command(ControlAction::Volume(30.)).is_none());
    assert!(status.command(ControlAction::Brightness(30.)).is_some());
}

#[::core::prelude::v1::test]
fn network_status_uses_active_ap_identity_without_text_parsing() {
    let mut network = network();
    let status = Status::from_system(Some(&network), None, None, None);
    assert_eq!(status.ssid, "家: Wi-Fi");
    assert_eq!(status.wifi, Some(true));
    assert!(matches!(
        status.command(ControlAction::Wifi(false)),
        Some(SystemCommand::SetWifi(false))
    ));
    network.wifi_enabled = false;
    assert!(
        Status::from_system(Some(&network), None, None, None)
            .ssid
            .is_empty()
    );
    network.devices.clear();
    assert!(
        Status::from_system(Some(&network), None, None, None)
            .command(ControlAction::Wifi(true))
            .is_none()
    );
}

#[::core::prelude::v1::test]
fn bluetooth_toggle_targets_the_adapter_that_was_displayed() {
    let bluetooth:BluetoothState=serde_json::from_value(serde_json::json!({"devices":[],"adapters":[
        {"path":"/org/bluez/hci0","name":"Off","powered":false,"discovering":false,"discoverable":false},
        {"path":"/org/bluez/hci1","name":"On","powered":true,"discovering":false,"discoverable":false}
    ]})).unwrap();
    let status = Status::from_system(None, Some(&bluetooth), None, None);
    assert_eq!(status.bluetooth, Some(true));
    assert!(
        matches!(status.command(ControlAction::Bluetooth(false)),Some(SystemCommand::SetBluetoothPowered {adapter,powered:false}) if adapter=="/org/bluez/hci1")
    );
}

#[::core::prelude::v1::test]
fn absent_services_never_generate_hardware_commands() {
    let status = Status::from_system(None, None, None, None);
    for action in [
        ControlAction::Wifi(true),
        ControlAction::Bluetooth(true),
        ControlAction::Volume(50.),
        ControlAction::Brightness(50.),
    ] {
        assert!(status.command(action).is_none());
    }
}
