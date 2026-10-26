//! Recursive inotify stream with a periodic rescan fallback for watch limits.
use notify::{RecursiveMode, Watcher};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    time::Duration,
};
pub const MUST_SCAN_SUBDIRS: u32 = 1;
pub const USER_DROPPED: u32 = 2;
pub const KERNEL_DROPPED: u32 = 4;
pub const HISTORY_DONE: u32 = 16;
pub struct Event {
    pub path: Vec<u8>,
    pub flags: u32,
    pub id: u64,
    pub stream: u32,
}
pub fn current_id() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_micros() as u64
}
pub struct Stream {
    stop: Arc<AtomicBool>,
}
impl Drop for Stream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
pub fn watch(_: u64, _: f64, tx: Sender<Vec<Event>>, stream: u32) -> Stream {
    let mut roots = vec![std::path::PathBuf::from("/")];
    if let Ok(entries) = std::fs::read_dir("/") {
        roots.extend(
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir() && !["/proc", "/sys", "/dev", "/run"].iter().any(|excluded| p == std::path::Path::new(excluded))),
        );
    }
    watch_roots(roots, tx, stream)
}

fn watch_roots(roots: Vec<std::path::PathBuf>, tx: Sender<Vec<Event>>, stream: u32) -> Stream {
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    std::thread::spawn(move || {
        let events = tx.clone();
        let mut watcher = notify::RecommendedWatcher::new(
            move |result: notify::Result<notify::Event>| {
                use std::os::unix::ffi::OsStrExt;
                let batch = match result {
                    Ok(event) => {
                        if matches!(event.kind, notify::EventKind::Access(_)) {
                            return;
                        }
                        event
                            .paths
                            .iter()
                            .filter_map(|p| p.parent())
                            .map(|p| Event { path: p.as_os_str().as_bytes().to_vec(), flags: 0, id: current_id(), stream })
                            .collect()
                    }
                    Err(_) => vec![Event { path: b"/".to_vec(), flags: MUST_SCAN_SUBDIRS, id: current_id(), stream }],
                };
                let _ = events.send(batch);
            },
            notify::Config::default().with_follow_symlinks(false),
        )
        .ok();
        if let Some(watcher) = &mut watcher {
            for path in roots {
                let mode = if path == std::path::Path::new("/") { RecursiveMode::NonRecursive } else { RecursiveMode::Recursive };
                if let Err(error) = watcher.watch(&path, mode) {
                    eprintln!("fsearch: cannot watch {}: {error}; periodic reconciliation remains enabled", path.display());
                }
            }
        }
        let _ = tx.send(vec![
            Event { path: b"/".to_vec(), flags: MUST_SCAN_SUBDIRS, id: current_id(), stream },
            Event { path: b"/".to_vec(), flags: HISTORY_DONE, id: current_id(), stream },
        ]);
        let mut ticks = 0;
        while !stopped.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_secs(1));
            ticks += 1;
            if ticks == 30 {
                ticks = 0;
                if tx.send(vec![Event { path: b"/".to_vec(), flags: MUST_SCAN_SUBDIRS, id: current_id(), stream }]).is_err() {
                    break;
                }
            }
        }
    });
    Stream { stop }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inotify_forwards_file_changes_at_directory_granularity() {
        use std::os::unix::ffi::OsStrExt;
        let dir = std::env::temp_dir().join(format!("finder-inotify-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let watcher = watch_roots(vec![dir.clone()], tx, 42);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let batch = rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())).unwrap();
            if batch.iter().any(|e| e.flags & HISTORY_DONE != 0) {
                break;
            }
        }
        std::fs::write(dir.join("new-file"), "data").unwrap();
        loop {
            let batch = rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())).unwrap();
            if batch.iter().any(|e| e.path == dir.as_os_str().as_bytes() && e.stream == 42 && e.flags == 0) {
                break;
            }
        }
        drop(watcher);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
