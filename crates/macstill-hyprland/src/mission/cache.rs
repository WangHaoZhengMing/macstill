//! One event-driven scene per process, shared by every overview output.
//! IPC events invalidate tables, not guessed fields: Hyprland's event payloads
//! are not complete snapshots. Subscribe before the first query to avoid a gap.
use super::{DesktopWorkspace, Scene, assemble};
use crate::stage::{Monitor, Window};
use anyhow::{Result, anyhow};
use std::{
    io::{ErrorKind, Read},
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, mpsc},
    thread,
    time::{Duration, Instant},
};

const CLIENTS: u8 = 1;
const MONITORS: u8 = 2;
const SPACES: u8 = 4;
const ALL: u8 = CLIENTS | MONITORS | SPACES;
const BATCH: Duration = Duration::from_millis(16);
type Snapshot = std::result::Result<Arc<Scene>, String>;
type Request = Arc<dyn Fn(&str) -> Result<String> + Send + Sync>;

#[derive(Default)]
struct Published {
    snapshot: Option<Snapshot>,
    listeners: Vec<async_channel::Sender<()>>,
}
enum Command {
    Refresh(mpsc::Sender<Snapshot>),
    Stop,
}
pub struct Cache {
    published: Arc<Mutex<Published>>,
    commands: mpsc::Sender<Command>,
    worker: Option<thread::JoinHandle<()>>,
}
pub fn shared() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| {
        Cache::start(
            crate::socket(".socket2.sock").ok(),
            Arc::new(crate::request),
            Duration::from_secs(5),
        )
    })
}
impl Cache {
    fn start(path: Option<PathBuf>, request: Request, reconcile: Duration) -> Self {
        let published = Arc::new(Mutex::new(Published::default()));
        let (commands, receive) = mpsc::channel();
        let state = published.clone();
        let worker = thread::spawn(move || run(path, request, reconcile, state, receive));
        Self {
            published,
            commands,
            worker: Some(worker),
        }
    }
    /// Nonblocking, suitable for the UI thread after a change notification.
    pub fn latest(&self) -> Result<Arc<Scene>> {
        self.published
            .lock()
            .unwrap()
            .snapshot
            .clone()
            .unwrap_or_else(|| Err("Scene cache is not ready".into()))
            .map_err(|e| anyhow!(e))
    }
    /// First caller may wait for initialization; warm callers only clone an Arc.
    pub fn snapshot(&self) -> Result<Arc<Scene>> {
        if let Ok(scene) = self.latest() {
            return Ok(scene);
        }
        self.refresh()
    }
    /// A mutation's read-after-write barrier. The worker owns all cache writes.
    pub fn refresh(&self) -> Result<Arc<Scene>> {
        let (send, receive) = mpsc::channel();
        self.commands.send(Command::Refresh(send))?;
        receive
            .recv_timeout(Duration::from_secs(4))?
            .map_err(|e| anyhow!(e))
    }
    /// Capacity-one wakeups coalesce; readers always take the latest snapshot.
    pub fn subscribe(&self) -> async_channel::Receiver<()> {
        let (send, receive) = async_channel::bounded(1);
        let mut state = self.published.lock().unwrap();
        if state.snapshot.is_some() {
            let _ = send.try_send(());
        }
        state.listeners.push(send);
        receive
    }
}
impl Drop for Cache {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn invalidation(line: &str) -> u8 {
    let name = line.split_once(">>").map(|(name, _)| name).unwrap_or("");
    match name {
        "openwindow" | "closewindow" | "movewindow" | "movewindowv2" | "windowtitle"
        | "windowtitlev2" | "changefloatingmode" | "fullscreen" | "pin" | "togglegroup"
        | "moveintogroup" | "moveoutofgroup" => CLIENTS,
        "focusedmon" | "focusedmonv2" => MONITORS,
        "workspace" | "workspacev2" | "activespecial" | "activespecialv2" | "createworkspace"
        | "createworkspacev2" | "destroyworkspace" | "destroyworkspacev2" | "moveworkspace"
        | "moveworkspacev2" | "renameworkspace" | "monitoradded" | "monitoraddedv2"
        | "monitorremoved" | "monitorremovedv2" | "configreloaded" => ALL,
        _ => 0,
    }
}
fn drain(stream: &mut UnixStream, bytes: &mut Vec<u8>, dirty: &mut u8) -> bool {
    let mut buffer = [0; 8192];
    for _ in 0..32 {
        match stream.read(&mut buffer) {
            Ok(0) => return false,
            Ok(n) => {
                bytes.extend_from_slice(&buffer[..n]);
                while let Some(end) = bytes.iter().position(|b| *b == b'\n') {
                    *dirty |= invalidation(&String::from_utf8_lossy(&bytes[..end]));
                    bytes.drain(..=end);
                }
                if bytes.len() > 1024 * 1024 {
                    bytes.clear();
                    *dirty = ALL;
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => return true,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return false,
        }
    }
    true
}
fn publish(state: &Mutex<Published>, snapshot: Snapshot) {
    let mut state = state.lock().unwrap();
    if state.snapshot.as_ref() == Some(&snapshot) {
        return;
    }
    state.snapshot = Some(snapshot);
    state.listeners.retain(|listener| {
        let _ = listener.try_send(());
        !listener.is_closed()
    });
}
fn run(
    path: Option<PathBuf>,
    request: Request,
    reconcile: Duration,
    published: Arc<Mutex<Published>>,
    commands: mpsc::Receiver<Command>,
) {
    let mut stream = None;
    let mut bytes = Vec::new();
    let mut reconnect = Instant::now();
    let mut next_reconcile = Instant::now();
    let mut retry = Instant::now();
    let mut dirty = ALL;
    let mut tables: (Vec<Window>, Vec<Monitor>, Vec<DesktopWorkspace>) = Default::default();
    loop {
        let mut waiters = Vec::new();
        match commands.recv_timeout(BATCH) {
            Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Ok(Command::Refresh(send)) => {
                dirty = ALL;
                waiters.push(send);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        while let Ok(command) = commands.try_recv() {
            match command {
                Command::Stop => return,
                Command::Refresh(send) => {
                    dirty = ALL;
                    waiters.push(send);
                }
            }
        }
        if stream.is_none() && Instant::now() >= reconnect {
            stream = path
                .as_ref()
                .and_then(|p| UnixStream::connect(p).ok())
                .filter(|s| s.set_nonblocking(true).is_ok());
            reconnect = Instant::now() + Duration::from_secs(1);
            if stream.is_some() {
                dirty = ALL;
                bytes.clear();
            }
        }
        if let Some(socket) = stream.as_mut() {
            if !drain(socket, &mut bytes, &mut dirty) {
                stream = None;
                dirty = ALL;
                bytes.clear();
            }
        }
        // IPC has no complete geometry-change event. Reconcile infrequently,
        // and always reload everything after reconnect or an explicit mutation.
        if Instant::now() >= next_reconcile {
            dirty = ALL;
        }
        if dirty == 0 || Instant::now() < retry && waiters.is_empty() {
            continue;
        }
        let result = (|| -> Result<Arc<Scene>> {
            let windows = if dirty & CLIENTS != 0 {
                serde_json::from_str(&request("j/clients")?)?
            } else {
                tables.0.clone()
            };
            let monitors = if dirty & MONITORS != 0 {
                serde_json::from_str(&request("j/monitors")?)?
            } else {
                tables.1.clone()
            };
            let spaces = if dirty & SPACES != 0 {
                serde_json::from_str(&request("j/workspaces")?)?
            } else {
                tables.2.clone()
            };
            let scene = Arc::new(assemble(windows.clone(), monitors.clone(), spaces.clone()));
            tables = (windows, monitors, spaces);
            Ok(scene)
        })()
        .map_err(|e| e.to_string());
        if dirty == ALL {
            next_reconcile = Instant::now() + reconcile;
        }
        dirty = if result.is_ok() { 0 } else { ALL };
        retry = Instant::now()
            + if result.is_ok() {
                Duration::ZERO
            } else {
                Duration::from_secs(1)
            };
        publish(&published, result.clone());
        for send in waiters {
            let _ = send.send(result.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, io::Write, os::unix::net::UnixListener};

    fn wait(mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !predicate() {
            assert!(Instant::now() < deadline, "cache did not converge");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn split_events_coalesce_and_all_subscribers_read_the_latest_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events");
        let listener = UnixListener::bind(&path).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let replies = Arc::new(Mutex::new(HashMap::from([
            ("j/clients", "[]".to_owned()),
            ("j/monitors", "[]".to_owned()),
            (
                "j/workspaces",
                r#"[{"id":1,"name":"one","monitor":"A"}]"#.to_owned(),
            ),
        ])));
        let log = calls.clone();
        let data = replies.clone();
        let cache = Cache::start(
            Some(path),
            Arc::new(move |command| {
                log.lock().unwrap().push(command.to_owned());
                Ok(data.lock().unwrap()[command].clone())
            }),
            Duration::from_secs(30),
        );
        assert_eq!(cache.snapshot().unwrap().spaces[0].name, "one");
        let (mut socket, _) = listener.accept().unwrap();
        let a = cache.subscribe();
        let b = cache.subscribe();
        a.try_recv().unwrap();
        b.try_recv().unwrap();
        calls.lock().unwrap().clear();
        socket.write_all(b"renameworkspace>>1,t").unwrap();
        thread::sleep(Duration::from_millis(60));
        assert!(
            calls.lock().unwrap().is_empty(),
            "incomplete frames cannot invalidate tables"
        );
        replies.lock().unwrap().insert(
            "j/workspaces",
            r#"[{"id":1,"name":"two","monitor":"A"}]"#.into(),
        );
        socket
            .write_all(b"wo\nworkspacev2>>1,two\nwindowtitlev2>>0x1,title\n")
            .unwrap();
        wait(|| cache.latest().is_ok_and(|s| s.spaces[0].name == "two"));
        assert_eq!(
            calls.lock().unwrap().len(),
            3,
            "a burst should query each table once"
        );
        a.try_recv().unwrap();
        b.try_recv().unwrap();
        calls.lock().unwrap().clear();
        socket
            .write_all(b"windowtitlev2>>0x1,title\nwindowtitle>>0x1\n")
            .unwrap();
        wait(|| !calls.lock().unwrap().is_empty());
        thread::sleep(Duration::from_millis(60));
        assert_eq!(*calls.lock().unwrap(), vec!["j/clients"]);
        assert!(
            a.try_recv().is_err(),
            "unchanged data must not redraw subscribers"
        );
        assert!(b.try_recv().is_err());
        // Publish twice without consuming: a single wakeup must expose the newest value.
        for name in ["three", "four"] {
            replies.lock().unwrap().insert(
                "j/workspaces",
                format!(r#"[{{"id":1,"name":"{name}","monitor":"A"}}]"#),
            );
            assert_eq!(cache.refresh().unwrap().spaces[0].name, name);
        }
        a.try_recv().unwrap();
        b.try_recv().unwrap();
        assert_eq!(cache.latest().unwrap().spaces[0].name, "four");
        assert!(a.try_recv().is_err());
        // Reconnect must re-snapshot even if the missed events are unavailable.
        replies.lock().unwrap().insert("j/workspaces", "[]".into());
        drop(socket);
        wait(|| cache.latest().is_ok_and(|s| s.spaces.is_empty()));
        listener.set_nonblocking(true).unwrap();
        let mut reconnected = None;
        wait(|| {
            reconnected = listener.accept().ok();
            reconnected.is_some()
        });
        let (mut socket, _) = reconnected.unwrap();
        replies
            .lock()
            .unwrap()
            .insert("j/workspaces", r#"[{"id":2,"name":"reconnected"}]"#.into());
        socket
            .write_all(b"createworkspacev2>>2,reconnected\n")
            .unwrap();
        wait(|| {
            cache
                .latest()
                .is_ok_and(|s| s.spaces.first().is_some_and(|s| s.id == 2))
        });
    }

    #[test]
    fn invalidation_ignores_unrelated_events_and_refreshes_focus_monitor_only() {
        assert_eq!(invalidation("focusedmon>>A,1"), MONITORS);
        assert_eq!(invalidation("openwindow>>0x1,1,App,title"), CLIENTS);
        assert_eq!(invalidation("monitorremovedv2>>1,A"), ALL);
        assert_eq!(invalidation("submap>>default"), 0);
    }
}
