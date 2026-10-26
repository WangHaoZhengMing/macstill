use gio::glib::{self, variant::ToVariant};
use macstill_menu::{
    Endpoint, Entry, Publisher, dbus,
    host::{Host, Window},
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
fn model() -> Entry {
    Entry::menu(
        0,
        "",
        vec![Entry::menu(
            100,
            "_File",
            vec![
                Entry {
                    shortcut: vec![vec!["Control".into(), "n".into()]],
                    ..Entry::item(1, "New_Window")
                },
                Entry {
                    checked: Some(true),
                    radio: true,
                    ..Entry::item(2, "List")
                },
                Entry {
                    enabled: false,
                    ..Entry::item(3, "Disabled")
                },
                Entry::menu(4, "Dynamic", vec![]),
            ],
        )],
    )
}
#[test]
fn layout_roundtrip_preserves_labels_state_and_empty_submenus() {
    let root = model();
    let layout = dbus::serialize(&root, -1, &[]);
    assert_eq!(layout.type_().as_str(), "(ia{sv}av)");
    assert_eq!(dbus::layout(layout, 0), Some(root.clone()));
    let shallow = dbus::layout(dbus::serialize(&root, 1, &[]), 0).unwrap();
    assert!(shallow.children[0].submenu);
    assert!(shallow.children[0].children.is_empty());
    assert_eq!(
        dbus::properties(&root.children[0], &["enabled".into()]).len(),
        1
    );
}
#[test]
fn rejects_invalid_trees_and_never_activates_disabled_ancestors() {
    let mut root = model();
    assert!(root.actionable(1));
    assert!(!root.actionable(3));
    assert!(!root.actionable(4));
    root.children[0].enabled = false;
    assert!(!root.actionable(1));
    root.children.push(Entry::item(1, "Duplicate id"));
    assert!(root.validate().is_err());
    assert!(dbus::layout("not a menu".to_variant(), 0).is_none());
    let hidden = (
        5i32,
        std::collections::HashMap::from([("visible", false.to_variant())]),
        Vec::<glib::Variant>::new(),
    )
        .to_variant();
    assert!(dbus::layout(hidden, 0).is_none());
}
fn wait(mut predicate: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(6);
    while !predicate() {
        assert!(Instant::now() < end, "Timed out waiting for menu state");
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[test]
#[ignore = "run under a private dbus-run-session with MACSTILL_MENU_TEST_BUS=1"]
fn isolated_bus_lifecycle_routing_and_registrar() {
    assert_eq!(std::env::var("MACSTILL_MENU_TEST_BUS").as_deref(), Ok("1"));
    let a = Publisher::new(model(), None).unwrap();
    let b = Publisher::new(model(), None).unwrap();
    let windows = Arc::new(Mutex::new(vec![
        Window {
            address: "A".into(),
            endpoint: Some(a.endpoint.clone()),
            xid: None,
        },
        Window {
            address: "B".into(),
            endpoint: Some(b.endpoint.clone()),
            xid: None,
        },
    ]));
    let records = windows.clone();
    let host = Host::new(move || Ok(records.lock().unwrap().clone()));
    host.select(Some("A".into()));
    wait(|| host.snapshot().root.is_some());
    let first = host.snapshot();
    let session_a = first.session.unwrap();
    host.click(session_a.clone(), first.revision, 1);
    wait(|| a.try_event() == Some(1));
    assert_eq!(b.try_event(), None);
    host.select(Some("B".into()));
    wait(|| {
        host.snapshot()
            .session
            .as_ref()
            .is_some_and(|s| s.window == "B")
    });
    host.click(session_a, first.revision, 1); // stale callback after switching windows
    let second = host.snapshot();
    let session_b = second.session.unwrap();
    let mut disabled = model();
    disabled.children[0].children[0].enabled = false;
    b.update(disabled).unwrap();
    wait(|| host.snapshot().revision != second.revision);
    host.click(session_b.clone(), second.revision, 1); // stale revision
    host.click(session_b.clone(), host.snapshot().revision, 1); // disabled command
    // A successful later command is a barrier proving the invalid callbacks were processed.
    host.click(session_b.clone(), host.snapshot().revision, 2);
    wait(|| match b.try_event() {
        Some(id) => {
            assert_eq!(id, 2);
            true
        }
        None => false,
    });
    assert_eq!(a.try_event(), None);
    assert_eq!(b.try_event(), None);
    windows.lock().unwrap().retain(|w| w.address != "B");
    host.click(session_b, host.snapshot().revision, 2); // target closed before dispatch
    wait(|| host.snapshot().session.is_none());
    assert_eq!(b.try_event(), None);
    // Exact XID registration uses the same transport, without PID/app-name guessing.
    let x = Publisher::new(model(), Some(47)).unwrap();
    windows.lock().unwrap().push(Window {
        address: "X".into(),
        xid: Some(47),
        endpoint: None,
    });
    host.select(Some("X".into()));
    wait(|| {
        host.snapshot()
            .session
            .as_ref()
            .is_some_and(|s| s.endpoint == x.endpoint)
    });
    let snapshot = host.snapshot();
    host.click(snapshot.session.unwrap(), snapshot.revision, 1);
    wait(|| x.try_event() == Some(1));
    // Registrar ownership: another connection cannot unregister this window.
    let c = dbus::connect().unwrap();
    dbus::call(
        &c,
        dbus::REGISTRAR,
        dbus::REGISTRAR_PATH,
        dbus::REGISTRAR,
        "UnregisterWindow",
        Some(&(47u32,).to_variant()),
    )
    .unwrap();
    assert!(
        dbus::call(
            &c,
            dbus::REGISTRAR,
            dbus::REGISTRAR_PATH,
            dbus::REGISTRAR,
            "GetMenuForWindow",
            Some(&(47u32,).to_variant())
        )
        .is_ok()
    );
    let old_owner = x.endpoint.owner.clone();
    drop(x);
    wait(|| dbus::owner(&c, &old_owner).is_none());
    wait(|| host.snapshot().session.is_none());
    // A new service at the same path must get a new session, never inherit old commands.
    let replacement = Publisher::new(model(), None).unwrap();
    windows.lock().unwrap()[0].endpoint = Some(replacement.endpoint.clone());
    host.select(Some("A".into()));
    wait(|| {
        host.snapshot()
            .session
            .as_ref()
            .is_some_and(|s| s.endpoint == replacement.endpoint)
    });
    let e = Endpoint {
        owner: replacement.endpoint.owner.clone(),
        path: "invalid path".into(),
    };
    assert!(dbus::read(&c, &e).is_err());
    drop(host);
}
