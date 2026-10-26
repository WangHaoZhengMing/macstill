//! Radio lease, advertisements and receiver lifecycle.
use super::*;
struct Tasks(Vec<JoinHandle<()>>);
impl Drop for Tasks {
    fn drop(&mut self) {
        for t in &self.0 {
            t.abort()
        }
    }
}
pub(super) struct Session {
    pub(super) connection: zbus::Connection,
    pub(super) lease: u64,
    pub(super) index: u32,
    tasks: Tasks,
    pub(super) _registration: Option<owly_airdrop::driver::mdns::Registration>,
    pub(super) _advert: Option<bluez::Registration>,
    pub(super) until: u64,
}
impl Session {
    pub(super) async fn start(
        state: &Shared,
        events: mpsc::UnboundedSender<Internal>,
        seconds: u32,
    ) -> Result<Self> {
        let connection = system_connection().await?;
        let (lease, link): (u64, HashMap<String, OwnedValue>) =
            radio(&connection).await?.call("Acquire", &(0u32,)).await?;
        let result = async {
            let index = link
                .get("index")
                .context("Missing AWDL interface index")?
                .downcast_ref::<u32>()?;
            let address: Ipv6Addr = link
                .get("address")
                .context("Missing AWDL address")?
                .downcast_ref::<String>()?
                .parse()?;
            let destination = state.lock().unwrap().receive_directory.clone();
            tokio::fs::create_dir_all(&destination).await?;
            let name = std::fs::read_to_string("/etc/hostname")
                .unwrap_or_else(|_| "Macstill".into())
                .trim()
                .to_string();
            let identity = Identity::load_or_generate(&state_dir())?;
            let (receiver, mut incoming) = Receiver::bind(
                SocketAddrV6::new(address, 8771, 0, index),
                &identity,
                Options::new(Config::new(&name), destination),
            )
            .await?;
            let mut mdns = Mdns::new(index);
            if let Some(v) = link.get("announce") {
                if let Ok((count, interval, pause)) = <(u32, u32, u32)>::try_from(v.try_clone()?) {
                    mdns = mdns.with_burst(Some(Burst {
                        count,
                        interval: Duration::from_micros(interval.into()),
                        pause: Duration::from_micros(pause.into()),
                    }));
                }
            }
            let id = service_id()?;
            let registration = mdns
                .register(Service {
                    instance: id.clone(),
                    host: format!("macstill-{id}"),
                    port: 8771,
                    addr: address,
                    flags: RECEIVER_FLAGS,
                })
                .await?;
            let mut tasks = Tasks(vec![tokio::spawn(receiver.run())]);
            let receiver_epoch = service_id()?;
            let events_rx = events.clone();
            tasks.0.push(tokio::spawn(async move {
                while let Some(e) = incoming.recv().await {
                    if events_rx
                        .send(Internal::Incoming(receiver_epoch.clone(), e))
                        .is_err()
                    {
                        break;
                    }
                }
            }));
            let mut browse = mdns.browse().await?;
            let discovered = events.clone();
            tasks.0.push(tokio::spawn(async move {
                while let Some(e) = browse.events.recv().await {
                    match e {
                        BrowseEvent::Found(f) => {
                            let e = discovered.clone();
                            tokio::spawn(async move {
                                let addr = SocketAddrV6::new(f.addr, f.port, 0, index);
                                discover(addr, f.instance, f.flags, e).await;
                            });
                        }
                        BrowseEvent::Lost(id) => {
                            let _ = discovered.send(Internal::Lost(id));
                        }
                    }
                }
            }));
            if std::env::var_os("MACSTILL_AIRDROP_RADIO_BUS").is_some() {
                if let Ok(addr) = std::env::var("MACSTILL_AIRDROP_TEST_PEER")
                    .unwrap_or_default()
                    .parse::<SocketAddrV6>()
                {
                    if addr.ip().is_loopback() {
                        let e = events.clone();
                        tasks.0.push(tokio::spawn(async move {
                            discover(addr, "test-peer".into(), RECEIVER_FLAGS, e).await;
                        }));
                    }
                }
            }
            let advert = if std::env::var_os("MACSTILL_AIRDROP_RADIO_BUS").is_some() {
                None
            } else {
                match bluez::BluezAdvertiser::new().await {
                    Ok(a) => {
                        let mut tag = [0; 3];
                        let _ = getrandom::fill(&mut tag);
                        match a
                            .advertise(owly_airdrop::proto::ble::Advert::anonymous(tag))
                            .await
                        {
                            Ok(a) => Some(a),
                            Err(e) => {
                                change(state, |s| {
                                    s.warning =
                                        Some(format!("Bluetooth discovery is unavailable: {e}"))
                                });
                                None
                            }
                        }
                    }
                    Err(e) => {
                        change(state, |s| {
                            s.warning = Some(format!("Bluetooth discovery is unavailable: {e}"))
                        });
                        None
                    }
                }
            };
            Ok::<_, anyhow::Error>((index, tasks, registration, advert))
        }
        .await;
        match result {
            Ok((index, tasks, registration, advert)) => Ok(Self {
                connection,
                lease,
                index,
                tasks,
                _registration: Some(registration),
                _advert: advert,
                until: if seconds == 0 {
                    0
                } else {
                    macstill_airdrop::now() + u64::from(seconds.min(600))
                },
            }),
            Err(e) => {
                let _ = radio(&connection)
                    .await?
                    .call::<_, _, ()>("Release", &(lease,))
                    .await;
                Err(e)
            }
        }
    }
    pub(super) async fn stop(mut self) {
        for t in self.tasks.0.drain(..) {
            t.abort();
            let _ = t.await;
        }
        drop(self._advert);
        drop(self._registration);
        if let Ok(r) = radio(&self.connection).await {
            let _: Result<(), _> = r.call("Release", &(self.lease,)).await;
        }
    }
}
