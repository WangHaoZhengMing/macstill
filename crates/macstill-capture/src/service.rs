use crate::Capture;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Target {
    pub address: String,
    pub pid: i64,
    pub class: String,
}
#[derive(Clone)]
pub struct Texture {
    pub revision: u64,
    pub rgba: Arc<image::RgbaImage>,
}
#[derive(Clone, Default)]
pub struct Snapshot {
    pub frames: BTreeMap<Target, Texture>,
    pub error: Option<String>,
}
/// One bounded capture worker. Dropping it disconnects and releases all window buffers.
pub struct Service {
    tx: mpsc::SyncSender<Vec<Target>>,
    shared: Arc<Mutex<Snapshot>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Service {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::sync_channel::<Vec<Target>>(1);
        let shared = Arc::new(Mutex::new(Snapshot::default()));
        let state = shared.clone();
        let worker = thread::spawn(move || {
            let mut targets = Vec::<Target>::new();
            let mut capture = None;
            let mut index = 0;
            let mut revision = 0;
            let mut retry = Instant::now();
            let mut failed = BTreeMap::<Target, Instant>::new();
            loop {
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(mut next) => {
                        next.truncate(24);
                        targets = next;
                        state
                            .lock()
                            .unwrap()
                            .frames
                            .retain(|key, _| targets.contains(key));
                        failed.retain(|key, _| targets.contains(key));
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                if targets.is_empty() {
                    capture = None;
                    continue;
                }
                if capture.is_none() {
                    if Instant::now() < retry {
                        continue;
                    }
                    match Capture::connect() {
                        Ok(connection) => {
                            capture = Some(connection);
                            state.lock().unwrap().error = None;
                        }
                        Err(error) => {
                            state.lock().unwrap().error = Some(error.to_string());
                            retry = Instant::now() + Duration::from_secs(5);
                            continue;
                        }
                    }
                }
                index %= targets.len();
                let target = targets[index].clone();
                index += 1;
                if failed
                    .get(&target)
                    .is_some_and(|retry| *retry > Instant::now())
                {
                    continue;
                }
                if !current_identity(&target) {
                    state.lock().unwrap().frames.remove(&target);
                    failed.insert(target, Instant::now() + Duration::from_secs(2));
                    continue;
                }
                match capture
                    .as_mut()
                    .unwrap()
                    .capture(&target.address, Duration::from_millis(500))
                {
                    Ok(image) => {
                        if !current_identity(&target) {
                            continue;
                        }
                        let image = image::DynamicImage::ImageRgba8(image)
                            .thumbnail(640, 480)
                            .into_rgba8();
                        let mut shared = state.lock().unwrap();
                        if !shared
                            .frames
                            .get(&target)
                            .is_some_and(|old| *old.rgba == image)
                        {
                            revision += 1;
                            shared.frames.insert(
                                target.clone(),
                                Texture {
                                    revision,
                                    rgba: Arc::new(image),
                                },
                            );
                        }
                        failed.remove(&target);
                        shared.error = None;
                    }
                    Err(error) => {
                        failed.insert(target, Instant::now() + Duration::from_secs(2));
                        state.lock().unwrap().error = Some(error.to_string());
                        // Reconnect after protocol/connection failure; a failed window is retried later.
                        capture = None;
                        retry = Instant::now() + Duration::from_millis(200);
                    }
                }
            }
        });
        Self {
            tx,
            shared,
            worker: Some(worker),
        }
    }
    pub fn update(&self, targets: Vec<Target>) {
        let _ = self.tx.try_send(targets);
    }
    pub fn snapshot(&self) -> Snapshot {
        self.shared.lock().unwrap().clone()
    }
}
fn matches_identity(clients: &serde_json::Value, target: &Target) -> bool {
    clients.as_array().is_some_and(|clients| {
        clients.iter().any(|w| {
            w["address"].as_str() == Some(&target.address)
                && w["pid"].as_i64() == Some(target.pid)
                && w["class"].as_str() == Some(&target.class)
        })
    })
}
fn current_identity(target: &Target) -> bool {
    macstill_hyprland::request("j/clients")
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .is_some_and(|clients| matches_identity(&clients, target))
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        let (replacement, _) = mpsc::sync_channel(1);
        drop(std::mem::replace(&mut self.tx, replacement));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Target, matches_identity};
    #[test]
    fn reused_addresses_and_exited_clients_do_not_receive_old_capture_identity() {
        let target = Target {
            address: "0x1".into(),
            pid: 12,
            class: "Editor".into(),
        };
        let clients = serde_json::json!([{"address":"0x1","pid":12,"class":"Editor"}]);
        assert!(matches_identity(&clients, &target));
        assert!(!matches_identity(
            &clients,
            &Target {
                pid: 13,
                ..target.clone()
            }
        ));
        assert!(!matches_identity(
            &clients,
            &Target {
                class: "Browser".into(),
                ..target.clone()
            }
        ));
        assert!(!matches_identity(&serde_json::json!([]), &target));
    }
}
