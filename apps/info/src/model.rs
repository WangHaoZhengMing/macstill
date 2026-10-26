use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Category {
    Hardware,
    Audio,
    Bluetooth,
    Camera,
    Graphics,
    Memory,
    Pci,
    Power,
    Storage,
    Usb,
    Ethernet,
    Wifi,
    Software,
}
impl Category {
    pub const ALL: [Self; 13] = [
        Self::Hardware,
        Self::Audio,
        Self::Bluetooth,
        Self::Camera,
        Self::Graphics,
        Self::Memory,
        Self::Pci,
        Self::Power,
        Self::Storage,
        Self::Usb,
        Self::Ethernet,
        Self::Wifi,
        Self::Software,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Hardware => "Hardware",
            Self::Audio => "Audio",
            Self::Bluetooth => "Bluetooth",
            Self::Camera => "Camera",
            Self::Graphics => "Graphics/Displays",
            Self::Memory => "Memory",
            Self::Pci => "PCI",
            Self::Power => "Power",
            Self::Storage => "Storage",
            Self::Usb => "USB",
            Self::Ethernet => "Ethernet",
            Self::Wifi => "Wi-Fi",
            Self::Software => "System Software",
        }
    }
    pub fn group(self) -> &'static str {
        match self {
            Self::Ethernet | Self::Wifi => "Network",
            Self::Software => "Software",
            _ => "Hardware",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
    pub fields: Vec<(String, String)>,
}
impl Item {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            parent: None,
            fields: vec![],
        }
    }
    pub fn field(mut self, key: &str, value: impl Into<String>) -> Self {
        let value = value.into();
        if !value.trim().is_empty() {
            self.fields.push((key.into(), value));
        }
        self
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub category: Category,
    pub items: Vec<Item>,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub model: String,
    pub vendor: String,
    pub version: String,
    pub processor: String,
    pub graphics: String,
    pub memory: String,
    pub startup_disk: String,
    pub serial: String,
    pub operating_system: String,
    pub kernel: String,
    pub reports: Vec<Report>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            model: "Macstill".into(),
            vendor: String::new(),
            version: String::new(),
            processor: "Not available".into(),
            graphics: "Not available".into(),
            memory: "Not available".into(),
            startup_disk: "Not available".into(),
            serial: "Not available".into(),
            operating_system: "Linux".into(),
            kernel: String::new(),
            reports: vec![],
        }
    }
}
impl Snapshot {
    pub fn report(&self, category: Category) -> Option<&Report> {
        self.reports
            .iter()
            .find(|report| report.category == category)
    }
}
