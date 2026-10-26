//! External volume discovery via util-linux; mount only after an explicit sidebar click.
use serde_json::Value;
use std::{path::PathBuf, process::Command};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Drive {
    pub device: PathBuf,
    pub label: String,
    pub mountpoint: Option<PathBuf>,
    pub locked: bool,
}

fn flag(value: &Value) -> bool {
    value
        .as_bool()
        .unwrap_or_else(|| value.as_u64() == Some(1) || value.as_str() == Some("1"))
}
fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

pub fn parse(bytes: &[u8]) -> Result<Vec<Drive>, String> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|e| format!("Invalid lsblk output: {e}"))?;
    let roots = value["blockdevices"]
        .as_array()
        .ok_or("lsblk returned no device list")?;
    fn visit(node: &Value, external: bool, model: Option<&str>, out: &mut Vec<Drive>) {
        let external = external
            || flag(&node["rm"])
            || flag(&node["hotplug"])
            || matches!(text(node, "tran"), Some("usb" | "firewire" | "ieee1394"));
        let model = text(node, "model").or(model);
        let children = node["children"].as_array();
        let has_children = children.is_some_and(|c| !c.is_empty());
        if external && !has_children && text(node, "fstype").is_some_and(|fs| fs != "swap") {
            if let Some(device) = text(node, "name").filter(|name| name.starts_with("/dev/")) {
                let mountpoint = node["mountpoints"]
                    .as_array()
                    .and_then(|points| {
                        points
                            .iter()
                            .filter_map(Value::as_str)
                            .find(|s| s.starts_with('/'))
                    })
                    .or_else(|| text(node, "mountpoint").filter(|s| s.starts_with('/')))
                    .map(PathBuf::from);
                let label = text(node, "label").map(str::to_owned).unwrap_or_else(|| {
                    let name = device.rsplit('/').next().unwrap_or(device);
                    model.map_or_else(|| name.to_owned(), |model| format!("{model} ({name})"))
                });
                out.push(Drive {
                    device: device.into(),
                    label,
                    mountpoint,
                    locked: text(node, "fstype") == Some("crypto_LUKS"),
                });
            }
        }
        if let Some(children) = children {
            for child in children {
                visit(child, external, model, out);
            }
        }
    }
    let mut drives = Vec::new();
    for node in roots {
        visit(node, false, None, &mut drives);
    }
    drives.sort_by(|a, b| a.device.cmp(&b.device));
    drives.dedup_by(|a, b| a.device == b.device);
    Ok(drives)
}

/// Mounts which vanished, including a volume which was unmounted but remains connected.
pub fn removed_mounts(previous: &[Drive], current: &[Drive]) -> Vec<PathBuf> {
    previous
        .iter()
        .filter_map(|drive| drive.mountpoint.as_ref())
        .filter(|path| {
            !current
                .iter()
                .any(|drive| drive.mountpoint.as_ref() == Some(path))
        })
        .cloned()
        .collect()
}

pub fn discover() -> Result<Vec<Drive>, String> {
    let columns = "NAME,LABEL,MODEL,TRAN,HOTPLUG,RM,TYPE,FSTYPE,MOUNTPOINTS";
    let mut output = Command::new("lsblk")
        .args(["--json", "--tree", "--paths", "--output", columns])
        .output()
        .map_err(|e| format!("Install util-linux (lsblk) to detect external drives: {e}"))?;
    if !output.status.success() {
        output = Command::new("lsblk")
            .args([
                "--json",
                "--tree",
                "--paths",
                "--output",
                &columns.replace("MOUNTPOINTS", "MOUNTPOINT"),
            ])
            .output()
            .map_err(|e| e.to_string())?;
    }
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    parse(&output.stdout)
}

pub fn mount(drive: &Drive) -> Result<PathBuf, String> {
    if drive.locked {
        return Err(
            "This volume is encrypted. Unlock it in your system disk utility first.".into(),
        );
    }
    let output = Command::new("udisksctl")
        .arg("mount")
        .arg("--block-device")
        .arg(&drive.device)
        .arg("--no-user-interaction")
        .output()
        .map_err(|e| format!("Install udisks2 to mount external drives: {e}"))?;
    if !output.status.success() {
        // Another desktop process may have mounted the volume concurrently.
        if let Some(path) = discover()?
            .into_iter()
            .find(|d| d.device == drive.device)
            .and_then(|d| d.mountpoint)
        {
            return Ok(path);
        }
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    discover()?
        .into_iter()
        .find(|d| d.device == drive.device)
        .and_then(|d| d.mountpoint)
        .ok_or("Drive has no accessible mount point yet. Please try again.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_partitions_inherit_usb_parent_and_use_real_mountpoints() {
        let data = br#"{"blockdevices":[{"name":"/dev/sda","tran":"usb","rm":false,"model":"USB SSD","children":[{"name":"/dev/sda1","label":"Backup","fstype":"exfat","mountpoints":[null,"/media/me/Backup Drive"]},{"name":"/dev/sda2","fstype":"ext4","mountpoints":[null]}]},{"name":"/dev/nvme0n1","fstype":"ext4","mountpoints":["/"]},{"name":"/dev/loop0","fstype":"squashfs","mountpoints":["/snap/app"]}]}"#;
        let drives = parse(data).unwrap();
        assert_eq!(drives.len(), 2);
        assert_eq!(drives[0].label, "Backup");
        assert_eq!(drives[0].mountpoint, Some("/media/me/Backup Drive".into()));
        assert_eq!(drives[1].mountpoint, None);
    }
    #[test]
    fn supports_removable_cards_locked_drives_and_older_mountpoint_column() {
        let drives = parse(br#"{"blockdevices":[{"name":"/dev/mmcblk0p1","rm":1,"fstype":"vfat","mountpoint":"/run/media/me/Card"},{"name":"/dev/sdb1","hotplug":true,"fstype":"crypto_LUKS"},{"name":"/dev/sdc1","rm":true,"fstype":"swap"}]}"#).unwrap();
        assert_eq!(drives.len(), 2);
        assert_eq!(drives[0].mountpoint, Some("/run/media/me/Card".into()));
        assert!(drives[1].locked);
    }
    #[test]
    fn unmount_is_detected_even_when_device_stays_connected() {
        let mounted = Drive {
            device: "/dev/sda1".into(),
            label: "Backup".into(),
            mountpoint: Some("/mnt/Backup".into()),
            locked: false,
        };
        let unmounted = Drive {
            mountpoint: None,
            ..mounted.clone()
        };
        assert_eq!(
            removed_mounts(&[mounted.clone()], &[unmounted]),
            vec![PathBuf::from("/mnt/Backup")]
        );
        assert!(removed_mounts(&[mounted.clone()], &[mounted]).is_empty());
    }
}
