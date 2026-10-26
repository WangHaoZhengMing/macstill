//! Small, per-Hyprland-session IPC channel. Reuse the shell's renderer for requested windows.
use anyhow::{Context, Result, bail};
use std::{
    hash::{Hash, Hasher},
    io::Write,
    os::unix::{
        fs::{FileTypeExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
};
pub fn path() -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let instance = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    instance.hash(&mut h);
    Some(PathBuf::from(runtime).join(format!("macstill-{:016x}.sock", h.finish())))
}
pub fn request(command: &str) -> bool {
    if !matches!(
        command,
        "launchpad"
            | "mission-toggle"
            | "mission-open"
            | "mission-close"
            | "mission-gesture"
            | "mission-prepare"
            | "mission-unprepare"
            | "stage-toggle"
            | "stage-on"
            | "stage-off"
            | "stage-next"
            | "stage-ungroup"
            | "stage-desktop"
            | "stage-hide-recent"
            | "stage-show-recent"
    ) {
        return false;
    }
    let Some(path) = path() else { return false };
    UnixStream::connect(path)
        .and_then(|mut s| s.write_all(command.as_bytes()))
        .is_ok()
}
pub struct Server {
    pub listener: UnixListener,
    path: PathBuf,
}
impl Server {
    pub fn bind() -> Result<Self> {
        let path = path().context("Hyprland session is missing")?;
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            if !meta.file_type().is_socket() {
                bail!("Control path is not a socket: {}", path.display())
            }
            if UnixStream::connect(&path).is_ok() {
                bail!("A Macstill shell is already running")
            }
            std::fs::remove_file(&path)?;
        }
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        Ok(Self { listener, path })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
