//! Private per-session channel for presenting the About and System Information windows.
use crate::request::Request;
use anyhow::{Context, Result, bail};
use std::{
    hash::{Hash, Hasher},
    io::{Read, Write},
    os::unix::{
        fs::{FileTypeExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    time::Duration,
};

fn path() -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let session = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")
        .or_else(|| std::env::var_os("WAYLAND_DISPLAY"))
        .or_else(|| std::env::var_os("DISPLAY"));
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    session.hash(&mut hash);
    Some(PathBuf::from(runtime).join(format!("macstill-info-{:016x}.sock", hash.finish())))
}

pub fn request(request: &Request) -> bool {
    let Some(path) = path() else { return false };
    request_at(&path, request)
}

fn request_at(path: &std::path::Path, request: &Request) -> bool {
    let Ok(mut stream) = UnixStream::connect(path) else {
        return false;
    };
    let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
    let Ok(bytes) = serde_json::to_vec(request) else {
        return false;
    };
    bytes.len() <= 4096 && stream.write_all(&bytes).is_ok()
}

pub struct Server {
    listener: UnixListener,
    path: PathBuf,
}
impl Server {
    pub fn bind() -> Result<Option<Self>> {
        let Some(path) = path() else { return Ok(None) };
        Self::bind_at(path).map(Some)
    }
    fn bind_at(path: PathBuf) -> Result<Self> {
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            if !meta.file_type().is_socket() {
                bail!("System Information control path is not a socket");
            }
            if UnixStream::connect(&path).is_ok() {
                bail!("System Information is already running");
            }
            std::fs::remove_file(&path)?;
        }
        let listener =
            UnixListener::bind(&path).context("Cannot bind System Information control socket")?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        Ok(Self { listener, path })
    }
    pub fn receive(&self) -> Option<Request> {
        let (stream, _) = self.listener.accept().ok()?;
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .ok()?;
        let mut bytes = Vec::new();
        stream.take(4097).read_to_end(&mut bytes).ok()?;
        if bytes.len() > 4096 {
            return None;
        }
        serde_json::from_slice(&bytes).ok()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_windows_reuse_the_session_socket_and_regular_files_are_preserved() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("info.sock");
        std::fs::write(&path, "keep").unwrap();
        assert!(Server::bind_at(path.clone()).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep");
        std::fs::remove_file(&path).unwrap();
        let server = Server::bind_at(path.clone()).unwrap();
        for request in [Request::About, Request::Report, Request::About] {
            assert!(request_at(&path, &request));
            assert_eq!(server.receive(), Some(request));
        }
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(server);
        assert!(!path.exists());
    }
}
