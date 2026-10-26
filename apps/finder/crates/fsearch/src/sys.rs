//! Linux paths and process helpers. Index paths preserve arbitrary Unix bytes.
use std::path::{Path, PathBuf};
#[derive(Clone, Copy)]
pub enum Qos {
    Interactive,
    Initiated,
    Utility,
}
pub fn set_qos(_: Qos) {}
pub fn release_memory() {
    #[cfg(target_env = "gnu")]
    unsafe {
        libc::malloc_trim(0);
    }
}
pub fn try_lock(f: &std::fs::File) -> bool {
    f.try_lock().is_ok()
}
pub fn os_path(p: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(p))
}
pub fn index_path(p: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    p.as_os_str().as_bytes().to_vec()
}
pub fn index_path_str(s: &str) -> String {
    s.to_owned()
}
pub fn home() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/".into())
}
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("FSEARCH_DIR") {
        return d.into();
    }
    std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(home()).join(".local/share")).join("fsearch")
}
