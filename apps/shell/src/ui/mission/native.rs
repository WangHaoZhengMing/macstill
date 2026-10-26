//! Mission Control owns native surface presentation, never captured window images.
use super::{Rect, stage};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Condvar, Mutex},
    thread,
    time::Duration,
};

#[derive(Default)]
struct Pending {
    frame: Option<(i64, Vec<Value>)>,
    closed: bool,
    error: Option<String>,
}

pub(super) struct Native {
    pending: Arc<(Mutex<Pending>, Condvar)>,
    worker: Option<thread::JoinHandle<()>>,
    sources: BTreeMap<String, (Rect, f64)>,
}
impl Native {
    pub fn connect(output: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(
            stage::connect_native().is_some(),
            "Native window presentation is unavailable"
        );
        anyhow::ensure!(
            crate::hyprland::request("macstill-mission ping")? == "macstill-mission-v1",
            "Update the native window plugin to use Mission Control"
        );
        let mut sources = BTreeMap::new();
        if let Ok(reply) = crate::hyprland::request(&format!(
            "macstill-stage {}",
            json!({"output":output,"status":true})
        )) {
            if let Ok(value) = serde_json::from_str::<Value>(&reply) {
                if let Some(poses) = value["poses"].as_array() {
                    for pose in poses {
                        if let (Some(address), Ok(rect)) = (
                            pose["address"].as_str(),
                            serde_json::from_value::<[f32; 4]>(pose["rect"].clone()),
                        ) {
                            sources.insert(
                                address.into(),
                                (
                                    Rect {
                                        x: rect[0],
                                        y: rect[1],
                                        w: rect[2],
                                        h: rect[3],
                                    },
                                    pose["angle"].as_f64().unwrap_or(0.),
                                ),
                            );
                        }
                    }
                }
            }
        }
        Ok(Self::start_worker(
            sources,
            output.to_owned(),
            crate::hyprland::request,
        ))
    }
    fn start_worker(
        sources: BTreeMap<String, (Rect, f64)>,
        output: String,
        mut transport: impl FnMut(&str) -> anyhow::Result<String> + Send + 'static,
    ) -> Self {
        let pending = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let mailbox = pending.clone();
        let worker = thread::spawn(move || {
            let (lock, wake) = &*mailbox;
            let mut presented = false;
            let mut last_message: Option<String> = None;
            loop {
                let frame = {
                    let mut state = lock.lock().unwrap();
                    while state.frame.is_none() && !state.closed {
                        if last_message.is_some() {
                            let (next, timeout) =
                                wake.wait_timeout(state, Duration::from_secs(1)).unwrap();
                            state = next;
                            if timeout.timed_out() {
                                break;
                            }
                        } else {
                            state = wake.wait(state).unwrap();
                        }
                    }
                    if state.closed {
                        break;
                    }
                    state.frame.take()
                };
                if let Some((home, entries)) = frame {
                    last_message = Some(format!(
                        "macstill-mission {}",
                        json!({
                            "owner":std::process::id(), "output":output, "home":home,
                            "animate":false, "windows":entries
                        })
                    ));
                }
                let Some(message) = last_message.as_deref() else {
                    continue;
                };
                presented = true;
                // Keep ownership alive without forcing a UI redraw or scene
                // query. The plugin deduplicates unchanged geometry packets.
                let result = transport(message).and_then(|answer| {
                    anyhow::ensure!(answer.trim() == "ok", "{answer}");
                    Ok(())
                });
                if let Err(error) = result {
                    lock.lock().unwrap().error = Some(error.to_string());
                }
            }
            // Clear on the same worker, after any in-flight update.
            if presented {
                let _ = transport(&format!(
                    "macstill-mission {}",
                    json!({"owner":std::process::id(), "output":output, "clear":true})
                ));
            }
        });
        Self {
            pending,
            worker: Some(worker),
            sources,
        }
    }
    pub fn source(&self, window: &stage::Window, fallback: Rect) -> (Rect, f64) {
        if window.workspace.name.starts_with("macstill-stage-") {
            self.sources
                .get(&window.address)
                .copied()
                .unwrap_or((fallback, 0.))
        } else {
            (fallback, 0.)
        }
    }
    pub fn update(&self, home: i64, entries: Vec<Value>) -> anyhow::Result<()> {
        let (lock, wake) = &*self.pending;
        let mut state = lock.lock().unwrap();
        // Replace unsent geometry instead of queueing stale animation frames.
        state.frame = Some((home, entries));
        wake.notify_one();
        if let Some(error) = state.error.take() {
            anyhow::bail!(error);
        }
        Ok(())
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        let (lock, wake) = &*self.pending;
        {
            let mut state = lock.lock().unwrap();
            state.closed = true;
            state.frame = None;
            wake.notify_one();
        }
        // Do not allow a new overview to race an old owner's final clear.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub(super) fn overlay_unmapped() -> bool {
    let Ok(reply) = crate::hyprland::request("j/layers") else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(&reply) else {
        return false;
    };
    let Some(outputs) = value.as_object() else {
        return false;
    };
    !outputs
        .values()
        .filter_map(|o| o["levels"].as_object())
        .flat_map(|levels| levels.values())
        .filter_map(Value::as_array)
        .flatten()
        .any(|layer| layer["namespace"].as_str() == Some("macstill-mission-control"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    #[test]
    fn cancelled_preparation_does_not_clear_another_overview() {
        let native = Native::start_worker(BTreeMap::new(), "test".into(), |_| {
            panic!("An unused connection must not send a clear request")
        });
        // Join explicitly so a worker panic fails this test.
        let mut native = native;
        native.pending.0.lock().unwrap().closed = true;
        native.pending.1.notify_one();
        native.worker.take().unwrap().join().unwrap();
    }

    #[test]
    fn slow_transport_coalesces_frames_and_clears_last() {
        let (sent, received) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        let mut first = true;
        let native = Native::start_worker(BTreeMap::new(), "test".into(), move |message| {
            let value: Value =
                serde_json::from_str(message.strip_prefix("macstill-mission ").unwrap())?;
            sent.send(value).unwrap();
            if first {
                first = false;
                blocked.recv_timeout(Duration::from_secs(2)).unwrap();
            }
            Ok("ok".into())
        });
        native.update(1, vec![json!(1)]).unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(2)).unwrap()["windows"],
            json!([1])
        );
        // The UI can publish all these frames while the first IPC is blocked.
        for frame in 2..=100 {
            native.update(1, vec![json!(frame)]).unwrap();
        }
        release.send(()).unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(2)).unwrap()["windows"],
            json!([100])
        );
        drop(native);
        assert_eq!(
            received.recv_timeout(Duration::from_secs(2)).unwrap()["clear"],
            true
        );
        assert!(received.try_recv().is_err());
    }
}
