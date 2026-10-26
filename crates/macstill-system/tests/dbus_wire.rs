//! Real private peer bus: validate GDBus marshalling and remote error mapping
//! without changing the user's system bus or any physical device.
use gio::glib::{self, variant::ToVariant};
use macstill_system::{
    ErrorKind,
    dbus::{Bus, GioTransport, Transport},
    network::Network,
};
use std::sync::{Arc, Mutex, mpsc};
#[test]
fn real_gdbus_property_write_and_permission_failure() {
    let dir = tempfile::tempdir().unwrap();
    let address = format!("unix:tmpdir={}", dir.path().display());
    let (tx, rx) = mpsc::channel();
    let written = Arc::new(Mutex::new(None));
    let record = written.clone();
    let thread = std::thread::spawn(move || {
        let context = glib::MainContext::new();
        context.with_thread_default(|| {
   let main_loop=glib::MainLoop::new(Some(&context),false);
   let server=gio::DBusServer::new_sync(&address,gio::DBusServerFlags::NONE,&gio::dbus_generate_guid(),None,None::<&gio::Cancellable>).unwrap();
   let connections=std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
   server.connect_new_connection(move |_,connection| {
    let xml=r#"<node><interface name="org.freedesktop.NetworkManager"><method name="Enable"><arg type="b" direction="in"/></method><property name="WirelessEnabled" type="b" access="readwrite"/></interface></node>"#;
    let info=gio::DBusNodeInfo::for_xml(xml).unwrap().lookup_interface("org.freedesktop.NetworkManager").unwrap();
    let record=record.clone();
    connection.register_object("/org/freedesktop/NetworkManager",&info)
     .property(|_,_,_,_,_| false.to_variant())
     .set_property(move |_,_,_,_,property,value| { assert_eq!(property,"WirelessEnabled");*record.lock().unwrap()=value.get::<bool>();true })
     .method_call(|_,_,_,_,method,_,invocation| { assert_eq!(method,"Enable");invocation.return_dbus_error("org.freedesktop.DBus.Error.AccessDenied","Denied by isolated test policy"); })
     .build().unwrap();
    connections.borrow_mut().push(connection.clone());true
   });
   server.start();tx.send((server.client_address().to_string(),main_loop.clone())).unwrap();main_loop.run();server.stop();
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
    Network(&transport).set_wifi(true).unwrap();
    assert_eq!(*written.lock().unwrap(), Some(true));
    let error = Network(&transport).set_enabled(false).unwrap_err();
    assert_eq!(error.kind, ErrorKind::PermissionDenied);
    let value = transport
        .call(
            Bus::System,
            "org.freedesktop.NetworkManager",
            "/org/freedesktop/NetworkManager",
            "org.freedesktop.DBus.Properties",
            "Get",
            ("org.freedesktop.NetworkManager", "WirelessEnabled").to_variant(),
            false,
        )
        .unwrap();
    assert_eq!(value.type_().as_str(), "(v)");
    main_loop.quit();
    thread.join().unwrap();
}
