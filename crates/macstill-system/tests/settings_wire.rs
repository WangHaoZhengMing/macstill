//! Real private GDBus peer: exercise the new complex message signatures without
//! using system services, network hardware or power actions on the host.
use gio::glib::{
    self,
    variant::{ObjectPath, ToVariant},
};
use macstill_system::{
    dbus::{GioTransport, Properties},
    host::HostSettings,
    locale::LocaleSettings,
    network::Network,
    *,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, mpsc},
};

#[test]
fn extended_settings_round_trip_through_a_private_gdbus_server() {
    let dir = tempfile::tempdir().unwrap();
    let address = format!("unix:tmpdir={}", dir.path().display());
    let (tx, rx) = mpsc::channel();
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let calls = recorded.clone();
    let thread = std::thread::spawn(move || {
        let context = glib::MainContext::new();
        context.with_thread_default(|| {
            let main_loop = glib::MainLoop::new(Some(&context), false);
            let server = gio::DBusServer::new_sync(&address, gio::DBusServerFlags::NONE, &gio::dbus_generate_guid(), None, None::<&gio::Cancellable>).unwrap();
            let connections = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            server.connect_new_connection(move |_, connection| {
                let interfaces = [
                    ("/org/freedesktop/locale1", "org.freedesktop.locale1", r#"<method name="SetLocale"><arg type="as" direction="in"/><arg type="b" direction="in"/></method><method name="SetX11Keyboard"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="b" direction="in"/><arg type="b" direction="in"/></method>"#),
                    ("/org/freedesktop/timedate1", "org.freedesktop.timedate1", r#"<method name="SetTime"><arg type="x" direction="in"/><arg type="b" direction="in"/><arg type="b" direction="in"/></method>"#),
                    ("/org/freedesktop/NetworkManager/Settings", "org.freedesktop.NetworkManager.Settings", r#"<method name="AddConnection"><arg type="a{sa{sv}}" direction="in"/><arg type="o" direction="out"/></method>"#),
                ];
                for (path, interface, methods) in interfaces {
                    let xml = format!("<node><interface name=\"{interface}\">{methods}</interface></node>");
                    let info = gio::DBusNodeInfo::for_xml(&xml).unwrap().lookup_interface(interface).unwrap();
                    let calls = calls.clone();
                    connection.register_object(path, &info).method_call(move |_,_,_,_,method,args,invocation| {
                        calls.lock().unwrap().push((method.to_string(), args.clone()));
                        let reply = if method == "AddConnection" {
                            (ObjectPath::try_from("/org/freedesktop/NetworkManager/Settings/1").unwrap(),).to_variant()
                        } else { ().to_variant() };
                        invocation.return_value(Some(&reply));
                    }).build().unwrap();
                }
                connections.borrow_mut().push(connection.clone());
                true
            });
            server.start();
            tx.send((server.client_address().to_string(), main_loop.clone())).unwrap();
            main_loop.run();
            server.stop();
        }).unwrap();
    });
    let (address, main_loop) = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    let connection = gio::DBusConnection::for_address_sync(
        &address,
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT,
        None,
        None::<&gio::Cancellable>,
    )
    .unwrap();
    let transport = GioTransport::from_connections(Some(connection), None);
    LocaleSettings(&transport)
        .set_locale(&["LANG=en_US.UTF-8".into()])
        .unwrap();
    LocaleSettings(&transport)
        .set_x11_keyboard(
            &X11Keyboard {
                layout: "us".into(),
                model: "pc105".into(),
                variant: "".into(),
                options: "compose:ralt".into(),
            },
            false,
        )
        .unwrap();
    HostSettings(&transport)
        .set_time(1_800_000_000_000_000)
        .unwrap();
    let path = Network(&transport)
        .add_wifi(&WifiConnection {
            name: "Private test".into(),
            ssid: b"Test".to_vec(),
            hidden: false,
            autoconnect: false,
            security: WifiSecurity::WpaPsk {
                password: "test-only-secret".into(),
            },
        })
        .unwrap();
    assert_eq!(path, "/org/freedesktop/NetworkManager/Settings/1");
    // Stop the peer before inspecting payloads so assertion failures cannot leave it running.
    main_loop.quit();
    thread.join().unwrap();
    let calls = recorded.lock().unwrap();
    assert_eq!(calls.len(), 4);
    assert_eq!(
        calls[0].1.get::<(Vec<String>, bool)>(),
        Some((vec!["LANG=en_US.UTF-8".into()], true))
    );
    assert_eq!(
        calls[1]
            .1
            .get::<(String, String, String, String, bool, bool)>(),
        Some((
            "us".into(),
            "pc105".into(),
            "".into(),
            "compose:ralt".into(),
            false,
            true
        ))
    );
    assert_eq!(
        calls[2].1.get::<(i64, bool, bool)>(),
        Some((1_800_000_000_000_000, false, true))
    );
    let (settings,): (HashMap<String, Properties>,) = calls[3].1.get().unwrap();
    assert_eq!(
        settings["802-11-wireless-security"]["psk"].str(),
        Some("test-only-secret")
    );
}
