//! Full user-service flow over an isolated D-Bus and IPv6 loopback. No physical radio.
use macstill_airdrop::{Command, Snapshot};
use owly_airdrop::{
    driver::{
        receiver::{Options, Receiver, ReceiverEvent},
        sender::{Payload, Sender},
        tls::Identity,
    },
    proto::{mdns::RECEIVER_FLAGS, receiver::Config, sender},
};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    net::{Ipv6Addr, SocketAddrV6},
    process::{Child, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use zbus::zvariant::{OwnedValue, Value};
struct Children(Vec<Child>);
impl Drop for Children {
    fn drop(&mut self) {
        for p in self.0.iter_mut().rev() {
            let _ = p.kill();
            let _ = p.wait();
        }
    }
}
struct FakeRadio(Arc<AtomicU64>);
#[zbus::interface(name = "io.github.t4t5.Owly.Radio1")]
impl FakeRadio {
    fn probe(&self) -> String {
        r#"{"hardware":true,"mode":"shared","missing":[],"status":"tested"}"#.into()
    }
    fn acquire(&self, _duration: u32) -> (u64, HashMap<String, OwnedValue>) {
        self.0.store(42, Ordering::SeqCst);
        (
            42,
            HashMap::from([
                ("index".into(), Value::from(1u32).try_to_owned().unwrap()),
                ("address".into(), Value::from("::1").try_to_owned().unwrap()),
            ]),
        )
    }
    fn release(&self, _lease: u64) {
        self.0.store(0, Ordering::SeqCst);
    }
    #[zbus(property)]
    fn lease(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
    #[zbus(property)]
    fn peers(&self) -> Vec<HashMap<String, OwnedValue>> {
        vec![]
    }
}
async fn state(api: &zbus::Proxy<'_>) -> Snapshot {
    let json: String = api.call("GetSnapshot", &()).await.unwrap();
    serde_json::from_str(&json).unwrap()
}
async fn command(api: &zbus::Proxy<'_>, cmd: Command) {
    let _: () = api
        .call("Execute", &(serde_json::to_string(&cmd).unwrap(),))
        .await
        .unwrap();
}
async fn wait(api: &zbus::Proxy<'_>, f: impl Fn(&Snapshot) -> bool) -> Snapshot {
    for _ in 0..100 {
        let s = state(api).await;
        if f(&s) {
            return s;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out: {:?}", state(api).await);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn prompts_before_receiving_releases_radio_and_expires_visibility() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let exe = [
        "/usr/bin/dbus-daemon",
        "/opt/macstill/runtime/bin/dbus-daemon",
    ]
    .into_iter()
    .find(|p| std::path::Path::new(p).is_file())
    .expect("Install dbus-daemon for this integration test");
    let mut bus = std::process::Command::new(exe)
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut address = String::new();
    BufReader::new(bus.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let address = address.trim().to_string();
    let mut children = Children(vec![bus]);
    let lease = Arc::new(AtomicU64::new(0));
    let _provider = zbus::connection::Builder::address(address.as_str())
        .unwrap()
        .name("io.github.t4t5.Owly")
        .unwrap()
        .serve_at("/io/github/t4t5/Owly", FakeRadio(lease.clone()))
        .unwrap()
        .build()
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config");
    let identity = Identity::load_or_generate(&root.path().join("remote-identity")).unwrap();
    let (receiver, mut remote_events) = Receiver::bind(
        SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0),
        &identity,
        Options::new(
            Config::new("Test Mac"),
            root.path().join("remote-downloads"),
        ),
    )
    .await
    .unwrap();
    let remote_address = receiver.local_addr().unwrap();
    let remote_task = tokio::spawn(receiver.run());

    let child = std::process::Command::new(env!("CARGO_BIN_EXE_macstill-airdropd"))
        .env("DBUS_SESSION_BUS_ADDRESS", &address)
        .env("MACSTILL_AIRDROP_RADIO_BUS", &address)
        .env("MACSTILL_AIRDROP_TEST_PEER", remote_address.to_string())
        .env("HOME", root.path())
        .env("XDG_CONFIG_HOME", config)
        .env("XDG_STATE_HOME", root.path().join("state"))
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    children.0.push(child);
    let client = zbus::connection::Builder::address(address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let api: zbus::Proxy<'_> = zbus::proxy::Builder::new(&client)
        .destination(macstill_airdrop::NAME)
        .unwrap()
        .path(macstill_airdrop::PATH)
        .unwrap()
        .interface(macstill_airdrop::INTERFACE)
        .unwrap()
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    for _ in 0..100 {
        if api.call::<_, _, String>("GetSnapshot", &()).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    wait(&api, |s| s.available).await;
    command(&api, Command::Enable { seconds: 60 }).await;
    wait(&api, |s| s.enabled()).await;
    assert_eq!(lease.load(Ordering::SeqCst), 42);
    let payload = root.path().join("hello.txt");
    std::fs::write(&payload, b"AirDrop integration test").unwrap();
    let send_path = payload.clone();
    let task = tokio::spawn(async move {
        Sender::new(sender::Config::new("Test iPhone", "001122334455"))
            .unwrap()
            .send(
                SocketAddrV6::new(Ipv6Addr::LOCALHOST, 8771, 0, 1),
                &Payload::Files(vec![send_path]),
                RECEIVER_FLAGS,
                None,
            )
            .await
            .unwrap()
    });
    let pending = wait(&api, |s| s.transfers.iter().any(|t| t.state == "pending")).await;
    assert!(!task.is_finished(), "must wait for user consent");
    let id = pending
        .transfers
        .iter()
        .find(|t| t.state == "pending")
        .unwrap()
        .id
        .clone();
    command(
        &api,
        Command::Answer {
            id: id.clone(),
            accept: true,
        },
    )
    .await;
    assert!(matches!(task.await.unwrap(), sender::Outcome::Delivered));
    let done = wait(&api, |s| {
        s.transfers
            .iter()
            .any(|t| t.id == id && t.state == "completed")
    })
    .await;
    let path = &done.transfers.iter().find(|t| t.id == id).unwrap().files[0];
    assert_eq!(std::fs::read(path).unwrap(), b"AirDrop integration test");

    let found = wait(&api, |s| s.peers.iter().any(|p| p.name == "Test Mac")).await;
    let peer = found
        .peers
        .iter()
        .find(|p| p.name == "Test Mac")
        .unwrap()
        .id
        .clone();
    std::fs::create_dir_all(root.path().join("remote-downloads")).unwrap();
    command(
        &api,
        Command::Send {
            peer,
            files: vec![payload.clone()],
        },
    )
    .await;
    let received = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(event) = remote_events.recv().await {
            match event {
                ReceiverEvent::Request(request) => request.accept(),
                ReceiverEvent::Received { paths, .. } => return paths,
                _ => {}
            }
        }
        panic!("remote receiver stopped")
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read(&received[0]).unwrap(),
        b"AirDrop integration test"
    );
    wait(&api, |s| {
        s.transfers
            .iter()
            .any(|t| !t.incoming && t.state == "completed")
    })
    .await;
    let send_path = payload.clone();
    let declined = tokio::spawn(async move {
        Sender::new(sender::Config::new("Test iPhone", "001122334455"))
            .unwrap()
            .send(
                SocketAddrV6::new(Ipv6Addr::LOCALHOST, 8771, 0, 1),
                &Payload::Files(vec![send_path]),
                RECEIVER_FLAGS,
                None,
            )
            .await
            .unwrap()
    });
    let pending = wait(&api, |s| s.transfers.iter().any(|t| t.state == "pending")).await;
    let id = pending
        .transfers
        .iter()
        .find(|t| t.state == "pending")
        .unwrap()
        .id
        .clone();
    command(&api, Command::Answer { id, accept: false }).await;
    assert!(matches!(declined.await.unwrap(), sender::Outcome::Declined));
    command(&api, Command::Disable).await;
    wait(&api, |s| s.state == "off").await;
    assert_eq!(lease.load(Ordering::SeqCst), 0);
    command(&api, Command::Enable { seconds: 1 }).await;
    wait(&api, |s| s.enabled()).await;
    wait(&api, |s| s.state == "off").await;
    assert_eq!(lease.load(Ordering::SeqCst), 0);
    remote_task.abort();
}
