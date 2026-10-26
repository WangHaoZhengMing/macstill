//! Private per-session channel for activating Settings and opening a specific page.
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
    Some(PathBuf::from(runtime).join(format!("macstill-settings-{:016x}.sock", hash.finish())))
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
                bail!("Settings control path is not a socket");
            }
            if UnixStream::connect(&path).is_ok() {
                bail!("Settings is already running");
            }
            std::fs::remove_file(&path)?;
        }
        let listener = UnixListener::bind(&path).context("Cannot bind Settings control socket")?;
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
    use crate::request::Page;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn existing_settings_receives_all_navigation_targets() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("settings.sock");
        let server = Server::bind_at(socket.clone()).unwrap();
        assert_eq!(
            std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777,
            0o600
        );
        for request in [
            Request::Show(Page::Dock),
            Request::Bind("finder".into()),
            Request::Icon("org.example.App.desktop".into()),
        ] {
            assert!(request_at(&socket, &request));
            assert_eq!(server.receive(), Some(request));
        }
        drop(server);
        assert!(!socket.exists());
    }

    #[test]
    fn binding_preserves_existing_files_and_live_servers_but_recovers_stale_sockets() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("settings.sock");
        std::fs::write(&socket, "keep me").unwrap();
        assert!(Server::bind_at(socket.clone()).is_err());
        assert_eq!(std::fs::read_to_string(&socket).unwrap(), "keep me");
        std::fs::remove_file(&socket).unwrap();
        let server = Server::bind_at(socket.clone()).unwrap();
        assert!(Server::bind_at(socket.clone()).is_err());
        // Drain the liveness probe before sending a real activation request.
        assert!(server.receive().is_none());
        assert!(request_at(&socket, &Request::Show(Page::About)));
        assert_eq!(server.receive(), Some(Request::Show(Page::About)));
        drop(server);
        drop(UnixListener::bind(&socket).unwrap());
        assert!(Server::bind_at(socket).is_ok());
    }

    #[test]
    fn malformed_or_oversized_messages_do_not_block_the_next_request() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("settings.sock");
        let server = Server::bind_at(socket.clone()).unwrap();
        for message in [b"{broken".to_vec(), vec![b'x'; 4097]] {
            let mut client = UnixStream::connect(&socket).unwrap();
            client.write_all(&message).unwrap();
            drop(client);
            assert!(server.receive().is_none());
        }
        assert!(request_at(&socket, &Request::Show(Page::Applications)));
        assert_eq!(server.receive(), Some(Request::Show(Page::Applications)));
    }
}
