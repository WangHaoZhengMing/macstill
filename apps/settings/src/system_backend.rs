//! Blocking system calls run on a worker; pairing cancellation uses a separate connection.
use crate::request::Page;
use macstill_system::{
    ConnectionDetails, State, SystemApi, SystemCommand, SystemQuery, SystemQueryResult,
    SystemSnapshot, WifiConnection,
};
use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub enum Job {
    AirDrop(macstill_airdrop::Visibility),
    AirDropDirectory(std::path::PathBuf),
    LoginEnabled(String, bool),
    AddLogin(String),
    ArrangeDisplays(Vec<(String, i32, i32)>),
    Apply(SystemCommand),
    Query(SystemQuery),
    JoinWifi {
        config: WifiConnection,
        device: String,
    },
}
#[derive(Clone)]
pub struct Snapshot {
    pub revision: u64,
    pub system: SystemSnapshot,
    pub airdrop: State<macstill_airdrop::Snapshot>,
    pub storage: State<Vec<macstill_system::storage::Volume>>,
    pub login_items: State<macstill_system::login_items::Snapshot>,
    pub battery_report: State<macstill_system::BatteryReport>,
    pub busy: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    pub discovery: Vec<String>,
    pub monitors: Vec<macstill_hyprland::displays::Monitor>,
    pub display_error: Option<String>,
    pub connection_path: Option<String>,
    pub connection: State<ConnectionDetails>,
    pub pairing: Option<String>,
    pub cancelling_pairing: bool,
}
pub struct SystemBackend {
    state: Arc<Mutex<Snapshot>>,
    page: Arc<Mutex<Page>>,
    tx: SyncSender<Job>,
    stop: Arc<AtomicBool>,
    refresh: Arc<AtomicBool>,
    scale_tx: SyncSender<(String, f64)>,
}
impl SystemBackend {
    fn initial(system: SystemSnapshot, loading: bool) -> Snapshot {
        Snapshot {
            revision: 0,
            system,
            battery_report: State::default(),
            airdrop: State::default(),
            storage: State::default(),
            login_items: State::default(),
            busy: false,
            loading,
            error: None,
            notice: None,
            discovery: vec![],
            monitors: vec![],
            display_error: None,
            connection_path: None,
            connection: State::default(),
            pairing: None,
            cancelling_pairing: false,
        }
    }
    #[cfg(test)]
    pub fn fixture(system: SystemSnapshot) -> (Self, mpsc::Receiver<Job>) {
        let (tx, rx) = mpsc::sync_channel(1);
        let (scale_tx, _) = mpsc::sync_channel(1);
        (
            Self {
                state: Arc::new(Mutex::new(Self::initial(system, false))),
                page: Arc::new(Mutex::new(Page::General)),
                tx,
                stop: Arc::new(AtomicBool::new(false)),
                refresh: Arc::new(AtomicBool::new(false)),
                scale_tx,
            },
            rx,
        )
    }
    pub fn new(page: Page) -> Self {
        let state = Arc::new(Mutex::new(Self::initial(
            SystemSnapshot {
                pointer: State::default(),
                network: State::default(),
                bluetooth: State::default(),
                audio: State::default(),
                backlights: State::default(),
                power: State::default(),
                power_profiles: State::default(),
                date_time: State::default(),
                host: State::default(),
                locale: State::default(),
                session: State::default(),
                power_actions: State::default(),
            },
            true,
        )));
        let selected = Arc::new(Mutex::new(page));
        let stop = Arc::new(AtomicBool::new(false));
        let refresh = Arc::new(AtomicBool::new(true));
        let (tx, rx) = mpsc::sync_channel::<Job>(1);
        let (scale_tx, scale_rx) = mpsc::sync_channel::<(String, f64)>(1);
        let (shared, active, stopped, reload) = (
            state.clone(),
            selected.clone(),
            stop.clone(),
            refresh.clone(),
        );
        thread::spawn(move || {
            let api = SystemApi::new();
            let initial = api.snapshot();
            {
                let mut s = shared.lock().unwrap();
                s.system = initial;
                s.loading = false;
                s.revision += 1;
            }
            let mut battery_refreshed = Instant::now() - Duration::from_secs(60);
            let mut last_page = page;
            let mut refreshed = Instant::now();
            let mut discovery: HashSet<String> = HashSet::new();
            while !stopped.load(Ordering::Relaxed) {
                let page = *active.lock().unwrap();
                if page != Page::Bluetooth && !discovery.is_empty() {
                    for adapter in discovery.drain() {
                        let _ = api.bluetooth().stop_discovery(&adapter);
                    }
                    shared.lock().unwrap().discovery.clear();
                }
                let job = match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(job) => Some(job),
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                };
                let scale_job = scale_rx.try_recv().ok();
                let had_job = job.is_some() || scale_job.is_some();
                if let Some((name, scale)) = scale_job {
                    let result = macstill_hyprland::displays::set_scale(&name, scale);
                    let mut s = shared.lock().unwrap();
                    s.error = result.err().map(|e| e.to_string());
                    s.notice = s.error.is_none().then(|| "Display scaling saved.".into());
                }
                if let Some(job) = job {
                    let mut scan = None;
                    let result = match job {
                        Job::AirDropDirectory(path) => {
                            macstill_airdrop::send(macstill_airdrop::Command::SetReceiveDirectory {
                                path,
                            })
                            .map_err(|e| {
                                macstill_system::SystemError::new(
                                    macstill_system::ErrorKind::Failed,
                                    "AirDrop",
                                    e.to_string(),
                                )
                            })
                        }
                        Job::AirDrop(visibility) => macstill_airdrop::ensure_service()
                            .and_then(|_| {
                                macstill_airdrop::send(macstill_airdrop::Command::SetVisibility {
                                    visibility,
                                })
                            })
                            .map_err(|e| {
                                macstill_system::SystemError::new(
                                    macstill_system::ErrorKind::Failed,
                                    "AirDrop",
                                    e.to_string(),
                                )
                            }),
                        Job::LoginEnabled(id, enabled) => {
                            macstill_system::login_items::set_enabled(&id, enabled)
                        }
                        Job::AddLogin(id) => macstill_system::login_items::add(&id),
                        Job::ArrangeDisplays(positions) => {
                            macstill_hyprland::displays::set_arrangement(&positions).map_err(|e| {
                                macstill_system::SystemError::new(
                                    macstill_system::ErrorKind::Failed,
                                    "Arrange displays",
                                    e.to_string(),
                                )
                            })
                        }
                        Job::Apply(command) => {
                            scan = match &command {
                                SystemCommand::StartBluetoothDiscovery { adapter } => {
                                    Some((adapter.clone(), true))
                                }
                                SystemCommand::StopBluetoothDiscovery { adapter } => {
                                    Some((adapter.clone(), false))
                                }
                                _ => None,
                            };
                            api.apply(command)
                        }
                        Job::JoinWifi { config, device } => {
                            api.network().add_wifi(&config).and_then(|connection| {
                                api.network().activate(&connection, &device).map(|_| ())
                            })
                        }
                        Job::Query(query) => match api.query(query) {
                            Ok(SystemQueryResult::Connection(connection)) => {
                                shared.lock().unwrap().connection = Ok(*connection).into();
                                Ok(())
                            }
                            Ok(SystemQueryResult::Timezones(_)) => Ok(()),
                            Err(error) => {
                                shared.lock().unwrap().connection = Err(error.clone()).into();
                                Err(error)
                            }
                        },
                    };
                    if result.is_ok()
                        && let Some((adapter, start)) = scan
                    {
                        if start {
                            discovery.insert(adapter);
                        } else {
                            discovery.remove(&adapter);
                        }
                    }
                    let mut s = shared.lock().unwrap();
                    s.discovery = discovery.iter().cloned().collect();
                    s.error = result.err().map(|e| e.to_string());
                    s.notice = s.error.is_none().then(|| "Change submitted".into());
                    s.pairing = None;
                    s.cancelling_pairing = false;
                }
                let requested_refresh = reload.swap(false, Ordering::Relaxed);
                if had_job
                    || page != last_page
                    || requested_refresh
                    || refreshed.elapsed() >= Duration::from_secs(5)
                {
                    let mut next = shared.lock().unwrap().system.clone();
                    match page {
                        Page::AirDrop => {
                            let result = macstill_airdrop::ensure_service()
                                .and_then(|_| macstill_airdrop::snapshot())
                                .map_err(|e| {
                                    macstill_system::SystemError::new(
                                        macstill_system::ErrorKind::Unavailable,
                                        "AirDrop",
                                        e.to_string(),
                                    )
                                });
                            shared.lock().unwrap().airdrop = result.into();
                        }
                        Page::Storage => {
                            let storage = macstill_system::storage::snapshot().into();
                            shared.lock().unwrap().storage = storage;
                        }
                        Page::LoginItems => {
                            let items = macstill_system::login_items::snapshot().into();
                            shared.lock().unwrap().login_items = items;
                        }
                        Page::Mouse | Page::Trackpad => {
                            next.pointer = api.pointer().snapshot().into()
                        }
                        Page::Wifi | Page::Network => {
                            next.network = api.network().snapshot().into()
                        }
                        Page::Bluetooth => next.bluetooth = api.bluetooth().snapshot().into(),
                        Page::Sound => next.audio = api.audio().snapshot().into(),
                        Page::Displays => {
                            next.backlights = api.brightness().snapshot().into();
                            let result = macstill_hyprland::displays::monitors();
                            let mut s = shared.lock().unwrap();
                            match result {
                                Ok(monitors) => {
                                    s.monitors = monitors;
                                    s.display_error = None;
                                }
                                Err(e) => {
                                    s.monitors.clear();
                                    s.display_error = Some(e.to_string());
                                }
                            }
                        }
                        Page::Battery => {
                            next.power = api.power().snapshot().into();
                            next.power_profiles = api.power().profiles().into();
                            if requested_refresh
                                || page != last_page
                                || battery_refreshed.elapsed() >= Duration::from_secs(60)
                            {
                                let report = api.power().battery_report().into();
                                shared.lock().unwrap().battery_report = report;
                                battery_refreshed = Instant::now();
                            }
                        }
                        Page::DateTime => next.date_time = api.host().date_time().into(),
                        Page::General | Page::About => next.host = api.host().snapshot().into(),
                        Page::LanguageRegion | Page::Keyboard => {
                            next.locale = api.locale().snapshot().into()
                        }
                        Page::LockScreen => {
                            next.session = api.session().snapshot().into();
                            next.power_actions = api.session().power_actions().into();
                        }
                        _ => {}
                    }
                    let connection = shared.lock().unwrap().connection_path.clone();
                    let details = if matches!(page, Page::Network | Page::Wifi) {
                        connection.map(|path| api.network().connection(&path).into())
                    } else {
                        None
                    };
                    let mut s = shared.lock().unwrap();
                    s.system = next;
                    if let Some(details) = details {
                        s.connection = details;
                    }
                    if had_job {
                        s.busy = false;
                    }
                    s.revision += 1;
                    refreshed = Instant::now();
                }
                last_page = page;
            }
            for adapter in discovery {
                let _ = api.bluetooth().stop_discovery(&adapter);
            }
        });
        Self {
            state,
            page: selected,
            tx,
            stop,
            refresh,
            scale_tx,
        }
    }
    pub(crate) fn submit(&self, job: Job) -> anyhow::Result<()> {
        let mut s = self.state.lock().unwrap();
        anyhow::ensure!(
            !s.busy && !s.loading,
            "A system operation is still in progress"
        );
        let pairing = match &job {
            Job::Apply(SystemCommand::PairBluetooth { device }) => Some(device.clone()),
            _ => None,
        };
        let connection = match &job {
            Job::Query(SystemQuery::Connection { connection }) => Some(connection.clone()),
            _ => None,
        };
        self.tx
            .try_send(job)
            .map_err(|_| anyhow::anyhow!("System settings is busy; please try again"))?;
        if let Some(path) = connection {
            s.connection_path = Some(path);
            s.connection = State::default();
        }
        s.pairing = pairing;
        s.busy = true;
        s.error = None;
        s.notice = None;
        s.revision += 1;
        Ok(())
    }
    pub fn apply(&self, command: SystemCommand) -> anyhow::Result<()> {
        self.submit(Job::Apply(command))
    }
    pub fn query_connection(&self, connection: String) -> anyhow::Result<()> {
        self.submit(Job::Query(SystemQuery::Connection { connection }))
    }
    pub fn join_wifi(&self, config: WifiConnection, device: String) -> anyhow::Result<()> {
        self.submit(Job::JoinWifi { config, device })
    }
    pub fn cancel_pairing(&self) -> anyhow::Result<()> {
        let mut s = self.state.lock().unwrap();
        let device = s
            .pairing
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No pairing request is active"))?;
        anyhow::ensure!(
            !s.cancelling_pairing,
            "Pairing cancellation is already in progress"
        );
        s.cancelling_pairing = true;
        s.revision += 1;
        let state = self.state.clone();
        thread::spawn(move || {
            let result = SystemApi::new().bluetooth().cancel_pairing(&device);
            let mut s = state.lock().unwrap();
            if s.pairing.as_deref() == Some(&device) {
                if let Err(error) = result {
                    s.error = Some(error.to_string());
                    s.cancelling_pairing = false;
                }
                s.revision += 1;
            }
        });
        Ok(())
    }
    pub fn arrange_displays(&self, positions: Vec<(String, i32, i32)>) -> anyhow::Result<()> {
        self.submit(Job::ArrangeDisplays(positions))
    }
    pub fn set_scale(&self, name: String, scale: f64) -> anyhow::Result<()> {
        let mut s = self.state.lock().unwrap();
        anyhow::ensure!(
            !s.busy && !s.loading,
            "A system operation is still in progress"
        );
        self.scale_tx.try_send((name, scale))?;
        s.busy = true;
        s.error = None;
        s.notice = None;
        s.revision += 1;
        Ok(())
    }
    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().unwrap().clone()
    }
    pub fn clear_connection(&self) {
        let mut state = self.state.lock().unwrap();
        if state.connection_path.take().is_some() {
            state.connection = State::default();
            state.revision += 1;
        }
    }
    #[cfg(test)]
    pub fn finish_fixture_job(&self, update: impl FnOnce(&mut Snapshot)) {
        let mut state = self.state.lock().unwrap();
        state.busy = false;
        update(&mut state);
        state.revision += 1;
    }
    pub fn set_page(&self, page: Page) {
        *self.page.lock().unwrap() = page;
        self.refresh();
    }
    pub fn refresh(&self) {
        self.refresh.store(true, Ordering::Relaxed);
    }
    pub fn dismiss(&self) {
        let mut s = self.state.lock().unwrap();
        s.error = None;
        s.notice = None;
        s.revision += 1;
    }
}
impl Drop for SystemBackend {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
