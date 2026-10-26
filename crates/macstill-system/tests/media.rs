use gio::glib::{
    Variant,
    variant::{ObjectPath, ToVariant},
};
use macstill_system::{
    ErrorKind, Result, SystemApi,
    dbus::{Bus, Properties, Transport},
    media::{Command, PATH, PLAYER, Player},
};
use std::{cell::RefCell, collections::HashMap};
#[derive(Default)]
struct Fake {
    calls: RefCell<Vec<(String, String, Variant)>>,
}
impl Transport for Fake {
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
        assert_eq!(bus, Bus::Session);
        assert!(!interactive);
        self.calls
            .borrow_mut()
            .push((service.into(), method.into(), args.clone()));
        if method == "ListNames" {
            return Ok((vec![
                "org.mpris.MediaPlayer2.paused",
                "org.example.unrelated",
                "org.mpris.MediaPlayer2.playing",
            ],)
                .to_variant());
        }
        assert_eq!(path, PATH);
        if method == "GetAll" {
            let iface = args.child_value(0).str().unwrap().to_string();
            let p = if iface == PLAYER {
                HashMap::from([
                    (
                        "PlaybackStatus".into(),
                        if service.ends_with("playing") {
                            "Playing"
                        } else {
                            "Paused"
                        }
                        .to_variant(),
                    ),
                    (
                        "Metadata".into(),
                        HashMap::from([
                            ("xesam:title", "Song".to_variant()),
                            ("xesam:artist", vec!["Artist A", "Artist B"].to_variant()),
                            ("xesam:album", "Album".to_variant()),
                            (
                                "mpris:trackid",
                                ObjectPath::try_from("/track/one").unwrap().to_variant(),
                            ),
                            ("mpris:length", 120000000i64.to_variant()),
                            ("mpris:artUrl", "file:///tmp/cover.png".to_variant()),
                        ])
                        .to_variant(),
                    ),
                    ("CanControl".into(), true.to_variant()),
                    ("CanPlay".into(), true.to_variant()),
                    ("CanPause".into(), true.to_variant()),
                    ("CanGoNext".into(), true.to_variant()),
                    ("CanGoPrevious".into(), false.to_variant()),
                    ("CanSeek".into(), true.to_variant()),
                    ("Volume".into(), 0.7f64.to_variant()),
                ])
            } else {
                Properties::from([("Identity".into(), "Fake Player".to_variant())])
            };
            return Ok((p,).to_variant());
        }
        assert!(interface == PLAYER || interface == "org.freedesktop.DBus.Properties");
        Ok(().to_variant())
    }
}
#[test]
fn discovers_metadata_and_prioritizes_playing_over_preferred_paused() {
    let api = SystemApi::with_backends(Fake::default(), macstill_system::audio::PactlRunner);
    let p = api
        .media()
        .current(Some("org.mpris.MediaPlayer2.paused"))
        .unwrap()
        .unwrap();
    assert!(p.playing());
    assert_eq!(p.artists, ["Artist A", "Artist B"]);
    assert_eq!(p.title, "Song");
    assert_eq!(p.identity, "Fake Player");
    assert_eq!(p.length, 120000000);
    assert_eq!(p.volume, 0.7);
    assert_eq!(p.track_id.as_deref(), Some("/track/one"));
}
#[test]
fn targets_selected_player_and_uses_typed_seek_volume_arguments() {
    let fake = Fake::default();
    let api = SystemApi::with_backends(&fake, macstill_system::audio::PactlRunner);
    let p = api.media().current(None).unwrap().unwrap();
    for command in [
        Command::PlayPause,
        Command::Next,
        Command::SetPosition(5000000),
        Command::Volume(0.5),
    ] {
        api.media().command(&p, command).unwrap()
    }
    let calls = fake.calls.borrow();
    for (service, _, _) in calls
        .iter()
        .filter(|(_, m, _)| m != "ListNames" && m != "GetAll")
    {
        assert_eq!(service, &p.service)
    }
    let position = &calls.iter().find(|(_, m, _)| m == "SetPosition").unwrap().2;
    assert_eq!(position.type_().as_str(), "(ox)");
    assert_eq!(position.child_value(1).get::<i64>(), Some(5000000));
    let volume = &calls.iter().find(|(_, m, _)| m == "Set").unwrap().2;
    assert_eq!(volume.type_().as_str(), "(ssv)");
}
#[test]
fn unsupported_and_invalid_controls_do_not_send_messages() {
    let fake = Fake::default();
    let api = SystemApi::with_backends(&fake, macstill_system::audio::PactlRunner);
    let p = Player {
        service: "org.mpris.MediaPlayer2.test".into(),
        can_control: true,
        ..Player::default()
    };
    assert_eq!(
        api.media().command(&p, Command::Previous).unwrap_err().kind,
        ErrorKind::Unsupported
    );
    assert_eq!(
        api.media()
            .command(&p, Command::Volume(f64::NAN))
            .unwrap_err()
            .kind,
        ErrorKind::InvalidInput
    );
    assert!(fake.calls.borrow().is_empty());
}
impl Transport for &Fake {
    fn call(
        &self,
        b: Bus,
        s: &str,
        p: &str,
        i: &str,
        m: &str,
        a: Variant,
        x: bool,
    ) -> Result<Variant> {
        (**self).call(b, s, p, i, m, a, x)
    }
}
