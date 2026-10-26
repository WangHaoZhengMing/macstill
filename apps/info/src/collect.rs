//! Read-only local inventory. Missing or restricted files never trigger privilege escalation.
use crate::model::{Category, Item, Report, Snapshot};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

struct Reader<'a>(&'a Path);
impl Reader<'_> {
    fn path(&self, path: &str) -> PathBuf {
        self.0.join(path.trim_start_matches('/'))
    }
    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.path(path))
            .unwrap_or_default()
            .trim()
            .to_owned()
    }
    fn names(&self, path: &str) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(self.path(path))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();
        names.sort();
        names
    }
    fn link_name(&self, path: &str) -> String {
        fs::read_link(self.path(path))
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_default()
    }
    fn report(
        &self,
        category: Category,
        base: &str,
        names: Vec<String>,
        fields: &[(&str, &str)],
    ) -> Report {
        let present = self.path(base).is_dir();
        let items = names
            .into_iter()
            .map(|name| {
                let mut item = Item::new(&name, &name);
                for (key, file) in fields {
                    item = item.field(key, self.read(&format!("{base}/{name}/{file}")));
                }
                item
            })
            .collect();
        Report {
            category,
            items,
            note: (!present).then(|| "This information is not exposed by the system.".into()),
        }
    }
}

fn fallback(value: String, default: &str) -> String {
    if value.is_empty() {
        default.into()
    } else {
        value
    }
}

fn useful_dmi(value: String) -> String {
    if [
        "default string",
        "to be filled by o.e.m.",
        "system product name",
        "not specified",
        "none",
        "unknown",
    ]
    .contains(&value.to_ascii_lowercase().as_str())
    {
        String::new()
    } else {
        value
    }
}

pub fn bytes(value: u64) -> String {
    let (divisor, unit) = if value >= 1 << 40 {
        ((1u64 << 40) as f64, "TB")
    } else if value >= 1 << 30 {
        ((1u64 << 30) as f64, "GB")
    } else if value >= 1 << 20 {
        ((1u64 << 20) as f64, "MB")
    } else {
        (1024., "KB")
    };
    format!("{:.1} {unit}", value as f64 / divisor)
}

fn properties(text: &str, separator: char) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once(separator))
        .map(|(key, value)| (key.trim().into(), value.trim().trim_matches('"').to_owned()))
        .collect()
}

fn usb_parent(id: &str) -> Option<String> {
    if id.starts_with("usb") {
        None
    } else if let Some((parent, _)) = id.rsplit_once('.') {
        Some(parent.into())
    } else {
        id.split_once('-').map(|(bus, _)| format!("usb{bus}"))
    }
}

pub fn collect(root: &Path) -> Snapshot {
    let r = Reader(root);
    let mut snapshot = Snapshot {
        model: fallback(
            useful_dmi(r.read("sys/class/dmi/id/product_name")),
            "Macstill Computer",
        ),
        vendor: useful_dmi(r.read("sys/class/dmi/id/sys_vendor")),
        version: useful_dmi(r.read("sys/class/dmi/id/product_version")),
        serial: fallback(
            useful_dmi(r.read("sys/class/dmi/id/product_serial")),
            "Not available",
        ),
        kernel: r.read("proc/sys/kernel/osrelease"),
        ..Snapshot::default()
    };
    let cpu = r.read("proc/cpuinfo");
    let cpu_fields = properties(&cpu, ':');
    snapshot.processor = ["model name", "Hardware", "Processor"]
        .iter()
        .find_map(|key| cpu_fields.get(*key))
        .cloned()
        .unwrap_or_else(|| "Not available".into());
    let mem = properties(&r.read("proc/meminfo"), ':');
    snapshot.memory = mem
        .get("MemTotal")
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
        .map(|kb| bytes(kb.saturating_mul(1024)))
        .unwrap_or_else(|| "Not available".into());
    let os_text = fallback(r.read("etc/os-release"), &r.read("usr/lib/os-release"));
    let os = properties(&os_text, '=');
    snapshot.operating_system = os
        .get("PRETTY_NAME")
        .or_else(|| os.get("NAME"))
        .cloned()
        .unwrap_or_else(|| "Linux".into());
    snapshot.startup_disk = r
        .read("proc/self/mountinfo")
        .lines()
        .find_map(|line| {
            let (mount, source) = line.split_once(" - ")?;
            if mount.split_whitespace().nth(4)? != "/" {
                return None;
            }
            source
                .split_whitespace()
                .nth(1)
                .map(|source| source.replace("\\040", " "))
        })
        .unwrap_or_else(|| "Not available".into());

    let mut graphics = r.report(
        Category::Graphics,
        "sys/class/drm",
        r.names("sys/class/drm")
            .into_iter()
            .filter(|name| {
                name.strip_prefix("card")
                    .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
            })
            .collect(),
        &[
            ("Vendor ID", "device/vendor"),
            ("Device ID", "device/device"),
            ("VRAM (bytes)", "device/mem_info_vram_total"),
        ],
    );
    for item in &mut graphics.items {
        let driver = r.link_name(&format!("sys/class/drm/{}/device/driver", item.id));
        if !driver.is_empty() {
            item.fields.push(("Driver".into(), driver.clone()));
            item.name = format!("{} ({driver})", item.id);
        }
        for connector in r
            .names("sys/class/drm")
            .into_iter()
            .filter(|name| name.starts_with(&format!("{}-", item.id)))
        {
            let status = r.read(&format!("sys/class/drm/{connector}/status"));
            if status == "connected" {
                let modes = r.read(&format!("sys/class/drm/{connector}/modes"));
                item.fields.push((
                    connector,
                    fallback(modes.lines().next().unwrap_or("").into(), "Connected"),
                ));
            }
        }
    }
    snapshot.graphics = graphics
        .items
        .iter()
        .map(|item| item.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    if snapshot.graphics.is_empty() {
        snapshot.graphics = "Not available".into();
    }
    let processor_count = cpu
        .lines()
        .filter(|line| {
            line.split_once(':')
                .is_some_and(|(key, _)| key.trim() == "processor")
        })
        .count();
    let hardware = Item::new("hardware", "Hardware Overview")
        .field("Model Name", &snapshot.model)
        .field("Manufacturer", &snapshot.vendor)
        .field("Model Version", &snapshot.version)
        .field("Processor", &snapshot.processor)
        .field(
            "Logical Processors",
            if processor_count > 0 {
                processor_count.to_string()
            } else {
                "Not available".into()
            },
        )
        .field("Memory", &snapshot.memory)
        .field("Serial Number", &snapshot.serial)
        .field("Startup Disk", &snapshot.startup_disk);
    snapshot.reports.push(Report {
        category: Category::Hardware,
        items: vec![hardware],
        note: None,
    });
    let audio = r.read("proc/asound/cards");
    snapshot.reports.push(Report {
        category: Category::Audio,
        items: if audio.is_empty() {
            vec![]
        } else {
            vec![Item::new("audio", "Audio Devices").field("ALSA Cards", audio)]
        },
        note: None,
    });
    snapshot.reports.push(r.report(
        Category::Bluetooth,
        "sys/class/bluetooth",
        r.names("sys/class/bluetooth"),
        &[("Address", "address"), ("Device Name", "name")],
    ));
    let mut cameras = r.report(
        Category::Camera,
        "sys/class/video4linux",
        r.names("sys/class/video4linux"),
        &[("Name", "name"), ("Device Number", "dev")],
    );
    for item in &mut cameras.items {
        if let Some((_, name)) = item.fields.first() {
            item.name = name.clone();
        }
    }
    snapshot.reports.push(cameras);
    snapshot.reports.push(graphics);
    snapshot.reports.push(Report {
        category: Category::Memory,
        items: if mem.is_empty() {
            vec![]
        } else {
            vec![Item {
                id: "memory".into(),
                name: "Physical Memory".into(),
                parent: None,
                fields: ["MemTotal", "MemAvailable", "SwapTotal", "SwapFree"]
                    .into_iter()
                    .filter_map(|key| mem.get(key).map(|value| (key.into(), value.clone())))
                    .collect(),
            }]
        },
        note: None,
    });
    snapshot.reports.push(r.report(
        Category::Pci,
        "sys/bus/pci/devices",
        r.names("sys/bus/pci/devices"),
        &[
            ("Vendor ID", "vendor"),
            ("Device ID", "device"),
            ("Class", "class"),
            ("Subsystem Vendor", "subsystem_vendor"),
            ("Subsystem Device", "subsystem_device"),
        ],
    ));
    snapshot.reports.push(r.report(
        Category::Power,
        "sys/class/power_supply",
        r.names("sys/class/power_supply"),
        &[
            ("Type", "type"),
            ("Model", "model_name"),
            ("Manufacturer", "manufacturer"),
            ("Status", "status"),
            ("Charge (%)", "capacity"),
            ("Online", "online"),
            ("Serial Number", "serial_number"),
        ],
    ));
    let mut storage = r.report(
        Category::Storage,
        "sys/block",
        r.names("sys/block")
            .into_iter()
            .filter(|n| !n.starts_with("loop") && !n.starts_with("ram"))
            .collect(),
        &[
            ("Model", "device/model"),
            ("Vendor", "device/vendor"),
            ("Removable", "removable"),
            ("Read Only", "ro"),
        ],
    );
    for item in &mut storage.items {
        if let Ok(sectors) = r
            .read(&format!("sys/block/{}/size", item.id))
            .parse::<u64>()
        {
            item.fields
                .push(("Capacity".into(), bytes(sectors.saturating_mul(512))));
        }
        item.fields
            .push(("Device".into(), format!("/dev/{}", item.id)));
        if let Some((_, model)) = item.fields.iter().find(|(key, _)| key == "Model") {
            item.name = model.clone();
        }
    }
    snapshot.reports.push(storage);
    let mut usb = r.report(
        Category::Usb,
        "sys/bus/usb/devices",
        r.names("sys/bus/usb/devices")
            .into_iter()
            .filter(|name| !name.contains(':'))
            .collect(),
        &[
            ("Product ID", "idProduct"),
            ("Vendor ID", "idVendor"),
            ("Version", "bcdDevice"),
            ("Serial Number", "serial"),
            ("Speed (Mb/s)", "speed"),
            ("Manufacturer", "manufacturer"),
            ("Bus", "busnum"),
            ("Device Number", "devnum"),
            ("Maximum Power", "bMaxPower"),
        ],
    );
    for item in &mut usb.items {
        item.parent = usb_parent(&item.id);
        item.name = fallback(
            r.read(&format!("sys/bus/usb/devices/{}/product", item.id)),
            &item.id,
        );
    }
    snapshot.reports.push(usb);
    for (category, wifi) in [(Category::Ethernet, false), (Category::Wifi, true)] {
        snapshot.reports.push(
            r.report(
                category,
                "sys/class/net",
                r.names("sys/class/net")
                    .into_iter()
                    .filter(|name| name != "lo")
                    .filter(|name| {
                        r.path(&format!("sys/class/net/{name}/wireless")).is_dir() == wifi
                    })
                    .collect(),
                &[
                    ("Hardware Address", "address"),
                    ("State", "operstate"),
                    ("MTU", "mtu"),
                    ("Speed (Mb/s)", "speed"),
                    ("Interface Type", "type"),
                ],
            ),
        );
    }
    snapshot.reports.push(Report {
        category: Category::Software,
        items: vec![
            Item::new("software", "System Software Overview")
                .field("System Version", &snapshot.operating_system)
                .field("Kernel Version", &snapshot.kernel)
                .field("Architecture", std::env::consts::ARCH)
                .field("Hostname", r.read("proc/sys/kernel/hostname")),
        ],
        note: None,
    });
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;
    fn write(root: &Path, file: &str, value: &str) {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value).unwrap();
    }
    #[test]
    fn inventory_reads_real_units_and_usb_hierarchy_and_handles_missing_data() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(
            root,
            "proc/meminfo",
            "MemTotal: 33554432 kB\nMemAvailable: 16777216 kB",
        );
        write(
            root,
            "proc/cpuinfo",
            "processor: 0\nmodel name: Test CPU\nprocessor: 1\nmodel name: Test CPU\n",
        );
        write(root, "etc/os-release", "PRETTY_NAME=\"Test Linux 1.0\"\n");
        write(root, "sys/class/dmi/id/product_name", "Test Notebook");
        write(root, "sys/bus/usb/devices/usb1/product", "USB 3.1 Bus");
        write(root, "sys/bus/usb/devices/1-2/product", "Bus Pirate 5");
        write(root, "sys/bus/usb/devices/1-2/idVendor", "1209");
        write(root, "sys/bus/usb/devices/1-2/idProduct", "7332");
        write(root, "sys/bus/usb/devices/1-2.3/product", "Nested Device");
        write(
            root,
            "sys/bus/usb/devices/1-2:1.0/interface",
            "Not a duplicate device",
        );
        write(root, "sys/block/nvme0n1/size", "2097152");
        write(
            root,
            "proc/self/mountinfo",
            "1 0 0:1 / / rw - ext4 /dev/nvme0n1p2 rw\n",
        );
        let data = collect(root);
        assert_eq!(data.model, "Test Notebook");
        assert_eq!(data.memory, "32.0 GB");
        assert_eq!(data.processor, "Test CPU");
        assert_eq!(data.operating_system, "Test Linux 1.0");
        assert_eq!(data.startup_disk, "/dev/nvme0n1p2");
        assert_eq!(data.serial, "Not available");
        let usb = data.report(Category::Usb).unwrap();
        assert_eq!(usb.items.len(), 3);
        assert_eq!(
            usb.items
                .iter()
                .find(|i| i.id == "1-2")
                .unwrap()
                .parent
                .as_deref(),
            Some("usb1")
        );
        assert_eq!(
            usb.items
                .iter()
                .find(|i| i.id == "1-2.3")
                .unwrap()
                .parent
                .as_deref(),
            Some("1-2")
        );
        assert!(
            data.report(Category::Storage).unwrap().items[0]
                .fields
                .contains(&("Capacity".into(), "1.0 GB".into()))
        );
        let empty = collect(tempfile::tempdir().unwrap().path());
        assert_eq!(empty.memory, "Not available");
        assert!(empty.report(Category::Usb).unwrap().items.is_empty());
        assert!(empty.report(Category::Usb).unwrap().note.is_some());
    }
}
