//! Opt-in startup events. Each process writes its own Chrome trace JSONL stream.
use std::{fs::OpenOptions, io::Write, sync::OnceLock, time::Instant};

fn timestamp() -> u64 {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN.get_or_init(Instant::now).elapsed().as_micros() as u64
}
fn record(name: &str, phase: &str) {
    let Some(root) = std::env::var_os("MACSTILL_DIAGNOSTICS_DIR") else {
        return;
    };
    let pid = std::process::id();
    let path = std::path::PathBuf::from(root).join(format!("trace-{pid}.jsonl"));
    let value = serde_json::json!({"name": name, "ph": phase, "ts": timestamp(),
        "wall_unix_us": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_micros(),
        "pid": pid, "tid": format!("{:?}", std::thread::current().id())});
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{value}");
    }
}
pub fn event(name: &str) {
    record(name, "i");
}
pub struct Span(&'static str, bool);
pub fn span(name: &'static str) -> Span {
    let enabled = std::env::var_os("MACSTILL_DIAGNOSTICS_DIR").is_some();
    if enabled {
        record(name, "B");
    }
    Span(name, enabled)
}
impl Drop for Span {
    fn drop(&mut self) {
        if self.1 {
            record(self.0, "E");
        }
    }
}
