//! Toolkit-independent global menus. Transport uses the existing DBusMenu protocol.
pub mod dbus;
pub mod exporter;
pub mod host;
pub mod wayland;
pub use exporter::Publisher;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
    pub checked: Option<bool>,
    pub radio: bool,
    pub submenu: bool,
    pub shortcut: Vec<Vec<String>>,
    pub children: Vec<Entry>,
}
impl Entry {
    pub fn item(id: i32, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            enabled: true,
            ..Self::default()
        }
    }
    pub fn menu(id: i32, label: impl Into<String>, children: Vec<Self>) -> Self {
        Self {
            submenu: true,
            children,
            ..Self::item(id, label)
        }
    }
    pub fn find(&self, id: i32) -> Option<&Self> {
        if self.id == id {
            Some(self)
        } else {
            self.children.iter().find_map(|e| e.find(id))
        }
    }
    pub fn actionable(&self, id: i32) -> bool {
        if !self.enabled {
            return false;
        }
        if self.id == id {
            return !self.separator && !self.submenu;
        }
        self.children.iter().any(|e| e.actionable(id))
    }
    pub fn shortcut_label(&self) -> String {
        self.shortcut
            .iter()
            .map(|keys| keys.join("+"))
            .collect::<Vec<_>>()
            .join(", ")
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        fn visit(e: &Entry, depth: usize, ids: &mut std::collections::HashSet<i32>) -> bool {
            depth <= 12
                && ids.len() < 4096
                && ids.insert(e.id)
                && e.children.iter().all(|child| visit(child, depth + 1, ids))
        }
        anyhow::ensure!(
            self.id == 0 && visit(self, 0, &mut Default::default()),
            "invalid menu IDs, size or depth"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Endpoint {
    /// A unique bus owner, never a recyclable well-known name.
    pub owner: String,
    pub path: String,
}
