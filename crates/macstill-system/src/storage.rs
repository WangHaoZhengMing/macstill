//! Filesystem capacity reads for Settings, called on its background worker.
use crate::{ErrorKind, Result, SystemError};
use std::process::Command;

#[derive(Clone, Debug)]
pub struct Volume {
    pub device: String,
    pub mount: String,
    pub total: u64,
    pub used: u64,
    pub available: u64,
}
pub fn snapshot() -> Result<Vec<Volume>> {
    let output = Command::new("df")
        .args(["-B1", "--output=source,size,used,avail,target", "--local"])
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| SystemError::io("Read storage", e))?;
    if !output.status.success() {
        return Err(SystemError::new(
            ErrorKind::Failed,
            "Read storage",
            String::from_utf8_lossy(&output.stderr),
        ));
    }
    let mut volumes = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines().skip(1) {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 5
            || fields[0].starts_with("/dev/loop")
            || (!fields[0].starts_with("/dev/") && fields[4] != "/")
        {
            continue;
        }
        let parse = |value: &str| {
            value
                .parse::<u64>()
                .map_err(|_| SystemError::protocol("Read storage"))
        };
        let volume = Volume {
            device: fields[0].into(),
            total: parse(fields[1])?,
            used: parse(fields[2])?,
            available: parse(fields[3])?,
            mount: fields[4..].join(" "),
        };
        if volume.total > 0 {
            volumes.push(volume);
        }
    }
    volumes.sort_by(|a, b| a.mount.cmp(&b.mount));
    Ok(volumes)
}

/// Encode spaces and non-ASCII mount names correctly for the file manager.
pub fn location_uri(path: &str) -> String {
    use gio::prelude::*;
    gio::File::for_path(path).uri().to_string()
}
