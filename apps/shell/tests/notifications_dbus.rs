//! Run only on a private session bus; never send test notifications to the user's desktop.
use gio::glib::{self, variant::ToVariant};
use macstill::notifications::{Command, Notifications};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
fn connection() -> gio::DBusConnection {
    let address =
        gio::dbus_address_get_for_bus_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap();
    gio::DBusConnection::for_address_sync(
        &address,
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        gio::Cancellable::NONE,
    )
    .unwrap()
}
fn call(
    c: &gio::DBusConnection,
    method: &str,
    args: Option<&glib::Variant>,
) -> Result<glib::Variant, glib::Error> {
    c.call_sync(
        Some(NAME),
        PATH,
        NAME,
        method,
        args,
        None,
        gio::DBusCallFlags::NO_AUTO_START,
        2000,
        gio::Cancellable::NONE,
    )
}
fn wait(mut f: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !f() {
        assert!(Instant::now() < deadline, "notification worker timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn notify(
    c: &gio::DBusConnection,
    replaces: u32,
    timeout: i32,
    hints: HashMap<String, glib::Variant>,
) -> u32 {
    call(
        c,
        "Notify",
        Some(
            &(
                "Mail",
                replaces,
                "",
                "<b>Message</b>",
                "Hello &amp; <a href='https://example.invalid'>welcome</a>",
                vec!["default", "Open", "reply", "Reply"],
                hints,
                timeout,
            )
                .to_variant(),
        ),
    )
    .unwrap()
    .child_value(0)
    .get()
    .unwrap()
}
#[test]
#[ignore = "requires MACSTILL_NOTIFICATION_TEST=1 and a private dbus-run-session"]
fn private_session_protocol_and_lifecycle() {
    assert_eq!(
        std::env::var("MACSTILL_NOTIFICATION_TEST").as_deref(),
        Ok("1"),
        "Use a private session bus, never the desktop bus"
    );
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| {
            let server = Notifications::new(true);
            wait(|| server.snapshot().available || server.snapshot().diagnostic.is_some());
            assert!(
                server.snapshot().available,
                "{}",
                server.snapshot().diagnostic.unwrap_or_default()
            );
            let client = connection();
            let signals = Arc::new(Mutex::new(Vec::<(String, glib::Variant)>::new()));
            let captured = signals.clone();
            let subscription = client.subscribe_to_signal(
                Some(NAME),
                Some(NAME),
                None,
                Some(PATH),
                None,
                gio::DBusSignalFlags::NONE,
                move |signal| {
                    captured
                        .lock()
                        .unwrap()
                        .push((signal.signal_name.into(), signal.parameters.clone()));
                },
            );
            let caps = call(&client, "GetCapabilities", None)
                .unwrap()
                .child_value(0)
                .get::<Vec<String>>()
                .unwrap();
            assert_eq!(caps, vec!["actions", "body", "icon-static"]);
            let info = call(&client, "GetServerInformation", None)
                .unwrap()
                .get::<(String, String, String, String)>()
                .unwrap();
            assert_eq!(info.0, "Macstill");
            assert_eq!(info.3, "1.3");
            let id = notify(&client, 0, 0, HashMap::new());
            assert_ne!(id, 0);
            wait(|| server.snapshot().notifications.len() == 1);
            let n = server.snapshot().notifications.remove(0);
            assert_eq!(n.summary, "Message");
            assert_eq!(n.body, "Hello & welcome");
            assert!(n.active);
            assert_eq!(notify(&client, id, 0, HashMap::new()), id);
            wait(|| server.snapshot().notifications.len() == 1);
            // A different bus peer cannot overwrite or retract this notification.
            let stranger = connection();
            assert!(call(&stranger, "CloseNotification", Some(&(id,).to_variant())).is_err());
            let other = notify(&stranger, id, 0, HashMap::new());
            assert_ne!(other, id);
            server.send(Command::Dismiss(other));
            wait(|| server.snapshot().notifications.len() == 1);
            server.send(Command::Invoke(id, "reply".into()));
            wait(|| server.snapshot().notifications.is_empty());
            wait(|| {
                while context.pending() {
                    context.iteration(false);
                }
                signals.lock().unwrap().iter().any(|(name, args)| {
                    name == "NotificationClosed" && args.get::<(u32, u32)>() == Some((id, 2))
                })
            });
            let recorded = signals.lock().unwrap();
            let action = recorded
                .iter()
                .position(|(name, args)| {
                    name == "ActionInvoked"
                        && args.get::<(u32, String)>() == Some((id, "reply".into()))
                })
                .unwrap();
            let closed = recorded
                .iter()
                .position(|(name, args)| {
                    name == "NotificationClosed" && args.get::<(u32, u32)>() == Some((id, 2))
                })
                .unwrap();
            assert!(action < closed);
            drop(recorded);
            // Resident actions do not close the ID. App recall emits reason 3.
            let resident = notify(
                &client,
                0,
                0,
                HashMap::from([("resident".into(), true.to_variant())]),
            );
            server.send(Command::Invoke(resident, "default".into()));
            std::thread::sleep(Duration::from_millis(80));
            assert!(server.snapshot().notifications[0].active);
            call(
                &client,
                "CloseNotification",
                Some(&(resident,).to_variant()),
            )
            .unwrap();
            wait(|| server.snapshot().notifications.is_empty());
            wait(|| {
                while context.pending() {
                    context.iteration(false);
                }
                signals.lock().unwrap().iter().any(|(name, args)| {
                    name == "NotificationClosed" && args.get::<(u32, u32)>() == Some((resident, 3))
                })
            });
            assert!(
                call(
                    &client,
                    "CloseNotification",
                    Some(&(resident,).to_variant())
                )
                .is_err()
            );
            // Expired IDs stay only in history, with exactly one expiry signal and no actions.
            let timed = notify(&client, 0, 80, HashMap::new());
            wait(|| {
                server
                    .snapshot()
                    .notifications
                    .iter()
                    .any(|n| n.id == timed && !n.active)
            });
            wait(|| {
                while context.pending() {
                    context.iteration(false);
                }
                signals.lock().unwrap().iter().any(|(name, args)| {
                    name == "NotificationClosed" && args.get::<(u32, u32)>() == Some((timed, 1))
                })
            });
            assert!(server.snapshot().notifications[0].actions.is_empty());
            server.send(Command::Invoke(timed, "reply".into()));
            server.send(Command::Focus(true));
            wait(|| server.snapshot().focus);
            let quiet = notify(&client, 0, 0, HashMap::new());
            wait(|| {
                server
                    .snapshot()
                    .notifications
                    .iter()
                    .any(|n| n.id == quiet)
            });
            assert!(server.snapshot().banners().is_empty());
            server.send(Command::Focus(false));
            wait(|| !server.snapshot().focus);
            assert!(server.snapshot().banners().is_empty());
            // Transient notifications disappear completely after expiry.
            let transient = notify(
                &client,
                0,
                80,
                HashMap::from([("transient".into(), true.to_variant())]),
            );
            wait(|| {
                server
                    .snapshot()
                    .notifications
                    .iter()
                    .any(|n| n.id == transient)
            });
            wait(|| {
                !server
                    .snapshot()
                    .notifications
                    .iter()
                    .any(|n| n.id == transient)
            });
            // Invalid argument forms are rejected without changing history.
            let bad = (
                "Mail",
                0u32,
                "",
                "Title",
                "Body",
                vec!["unpaired"],
                HashMap::<String, glib::Variant>::new(),
                0i32,
            )
                .to_variant();
            assert!(call(&client, "Notify", Some(&bad)).is_err());
            // A second server reports the conflict; the original still owns the bus name.
            let competitor = Notifications::new(true);
            wait(|| competitor.snapshot().diagnostic.is_some());
            assert!(!competitor.snapshot().available);
            assert!(
                competitor
                    .snapshot()
                    .diagnostic
                    .unwrap()
                    .contains("already owned")
            );
            assert!(call(&client, "GetCapabilities", None).is_ok());
            competitor.shutdown();
            server.send(Command::Clear);
            wait(|| server.snapshot().notifications.is_empty());
            wait(|| {
                while context.pending() {
                    context.iteration(false);
                }
                signals.lock().unwrap().iter().any(|(name, args)| {
                    name == "NotificationClosed" && args.get::<(u32, u32)>() == Some((quiet, 2))
                })
            });
            assert_eq!(
                signals
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|(name, args)| name == "NotificationClosed"
                        && args.get::<(u32, u32)>() == Some((timed, 1)))
                    .count(),
                1
            );
            drop(subscription);
            server.shutdown();
            let replacement = Notifications::new(true);
            wait(|| replacement.snapshot().available);
            replacement.shutdown();
            client.close_sync(gio::Cancellable::NONE).unwrap();
            stranger.close_sync(gio::Cancellable::NONE).unwrap();
        })
        .unwrap();
}
