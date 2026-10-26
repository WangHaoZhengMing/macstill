//! Run with: dbus-run-session -- cargo test -p macstill --test tray_bus -- --ignored
use gio::glib::{self, variant::ToVariant};
use macstill::tray::{Command, Tray};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};
#[test]
#[ignore = "requires an isolated session bus"]
fn registers_reads_menus_dispatches_events_and_removes_exited_items() {
    let tray = Tray::new();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (stop_tx, stop_rx) = mpsc::channel();
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let log = events.clone();
    let server = std::thread::spawn(move || {
        let ctx = glib::MainContext::new();
        ctx.with_thread_default(||{
 let c=gio::DBusConnection::for_address_sync(&std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap(),gio::DBusConnectionFlags::AUTHENTICATION_CLIENT|gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,None,gio::Cancellable::NONE).unwrap();
 let xml=r#"<node><interface name="org.kde.StatusNotifierItem"><property name="Title" type="s" access="read"/><property name="Status" type="s" access="read"/><property name="IconPixmap" type="a(iiay)" access="read"/><property name="Menu" type="o" access="read"/><property name="ItemIsMenu" type="b" access="read"/><method name="Activate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method></interface><interface name="com.canonical.dbusmenu"><method name="AboutToShow"><arg type="i" direction="in"/><arg type="b" direction="out"/></method><method name="GetLayout"><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="as" direction="in"/><arg type="u" direction="out"/><arg type="(ia{sv}av)" direction="out"/></method><method name="Event"><arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="in"/><arg type="u" direction="in"/></method></interface></node>"#;
 let node=gio::DBusNodeInfo::for_xml(xml).unwrap();let activation=log.clone();
 let _item=c.register_object("/StatusNotifierItem",&node.lookup_interface("org.kde.StatusNotifierItem").unwrap()).property(|_,_,_,_,name|match name{"Title"=>"Test tray".to_variant(),"Status"=>"Active".to_variant(),"IconPixmap"=>vec![(1i32,1i32,vec![255u8,20,40,60])].to_variant(),"Menu"=>glib::variant::ObjectPath::try_from("/Menu").unwrap().to_variant(),_=>false.to_variant()}).method_call(move|_,_,_,_,method,_,inv|{activation.lock().unwrap().push(method.into());inv.return_value(None);}).build().unwrap();
 let _menu=c.register_object("/Menu",&node.lookup_interface("com.canonical.dbusmenu").unwrap()).method_call(move|_,_,_,_,method,args,inv|{match method{"AboutToShow"=>inv.return_value(Some(&(false,).to_variant())),"GetLayout"=>{let leaf=(7i32,HashMap::from([("label","_Open".to_variant())]),Vec::<glib::Variant>::new()).to_variant();let root=(0i32,HashMap::<String,glib::Variant>::new(),vec![leaf]);inv.return_value(Some(&(1u32,root).to_variant()));},_=>{log.lock().unwrap().push(format!("Event:{}",args.child_value(0).get::<i32>().unwrap()));inv.return_value(None);}}}).build().unwrap();
 let deadline=Instant::now()+Duration::from_secs(5);loop{let result=c.call_sync(Some("org.kde.StatusNotifierWatcher"),"/StatusNotifierWatcher","org.kde.StatusNotifierWatcher","RegisterStatusNotifierItem",Some(&("/StatusNotifierItem",).to_variant()),None,gio::DBusCallFlags::NONE,1000,gio::Cancellable::NONE);if result.is_ok(){break}assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(50));}
 ready_tx.send(()).unwrap();while stop_rx.try_recv().is_err(){ctx.iteration(false);std::thread::sleep(Duration::from_millis(5));}c.close_sync(gio::Cancellable::NONE).unwrap();
 }).unwrap();
    });
    ready_rx.recv_timeout(Duration::from_secs(6)).unwrap();
    let wait = |condition: &dyn Fn() -> bool| {
        let deadline = Instant::now() + Duration::from_secs(6);
        while !condition() {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for tray state"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    wait(&|| !tray.snapshot().items.is_empty());
    let second_host = Tray::new();
    wait(&|| !second_host.snapshot().items.is_empty());
    let item = tray.snapshot().items[0].clone();
    assert_eq!(item.title, "Test tray");
    assert!(item.icon.unwrap().starts_with(b"\x89PNG"));
    tray.send(Command::Activate(item.key.clone()));
    wait(&|| events.lock().unwrap().contains(&"Activate".into()));
    tray.send(Command::Menu(item.key.clone()));
    wait(&|| !tray.snapshot().entries.is_empty());
    assert_eq!(tray.snapshot().entries[0].label, "Open");
    tray.send(Command::Event(item.key, 7));
    wait(&|| events.lock().unwrap().contains(&"Event:7".into()));
    stop_tx.send(()).unwrap();
    server.join().unwrap();
    wait(&|| tray.snapshot().items.is_empty());
    assert!(tray.snapshot().selected.is_none());
    wait(&|| second_host.snapshot().items.is_empty());
}
