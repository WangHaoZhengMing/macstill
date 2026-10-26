//! Layout and navigation shared by the Launchpad view and its regression tests.
#[derive(Clone, Copy, Debug)]
pub struct Grid {
    pub columns: usize,
    pub rows: usize,
    pub icon: f32,
    pub label_width: f32,
    pub width: f32,
    pub height: f32,
}
impl Grid {
    pub fn new(width: f32, height: f32) -> Self {
        let columns = if width >= 900. {
            7
        } else if width >= 650. {
            5
        } else {
            4
        };
        let rows = if height >= 600. {
            4
        } else if height >= 440. {
            3
        } else {
            2
        };
        let pitch = width / (columns + 1) as f32;
        let icon = (height * 0.098).round().clamp(40., 106.).min(pitch * 0.72);
        Self {
            columns,
            rows,
            icon,
            label_width: (pitch - 14.).clamp(60., 148.),
            width,
            height,
        }
    }
    pub fn capacity(self) -> usize {
        self.columns * self.rows
    }
    pub fn center(self, slot: usize) -> (f32, f32) {
        (
            (slot % self.columns + 1) as f32 * self.width / (self.columns + 1) as f32,
            (self.height * 0.14).max(self.height * 0.038 + 28. + self.icon / 2. + 14.)
                + (slot / self.columns) as f32 * (self.height * 0.155).max(self.icon + 42.),
        )
    }
    pub fn pages(self, count: usize) -> usize {
        count.div_ceil(self.capacity()).max(1)
    }
    pub fn page_for(self, selected: usize) -> usize {
        selected / self.capacity()
    }
    pub fn navigate(self, selected: usize, count: usize, direction: Direction) -> usize {
        if count == 0 {
            return 0;
        }
        let current = selected.min(count - 1);
        match direction {
            Direction::Left => current.saturating_sub(1),
            Direction::Right => (current + 1).min(count - 1),
            Direction::Up => current.saturating_sub(self.columns),
            Direction::Down => (current + self.columns).min(count - 1),
        }
    }
}
#[derive(Clone, Copy)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}
pub fn matches(name: &str, id: &str, query: &str) -> bool {
    let name = name.to_lowercase();
    let id = id.to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|part| name.contains(part) || id.contains(part))
}

pub const SHORTCUT_PREFIX: &str = "shortcut:";

/// Enabled shortcuts use the same artwork and label as the Dock. A bound app
/// appears through its shortcut rather than a second, differently named icon.
pub fn applications(
    apps: &[crate::catalog::DesktopApp],
    prefs: &crate::config::Preferences,
) -> Vec<crate::catalog::DesktopApp> {
    let shortcuts: Vec<_> = prefs.shortcuts.iter().filter(|s| s.visible).collect();
    let mut result: Vec<_> = apps
        .iter()
        .filter(|app| {
            !shortcuts
                .iter()
                .any(|s| s.effective_app_id() == Some(app.id.as_str()))
        })
        .cloned()
        .collect();
    result.extend(shortcuts.into_iter().map(|s| {
        crate::catalog::DesktopApp {
            id: format!("{SHORTCUT_PREFIX}{}", s.id),
            name: s.name.clone(),
            desktop_file: Default::default(),
            icon: s
                .custom_icon
                .clone()
                .or_else(|| Some(macstill_ui::asset(&s.asset))),
            startup_class: None,
            class_aliases: vec![],
        }
    }));
    result.sort_by_key(|a| a.name.to_lowercase());
    result
}

#[cfg(test)]
mod shortcut_tests {
    use super::applications;
    #[test]
    fn enabled_shortcuts_appear_with_artwork_and_replace_bound_app_entry() {
        let mut prefs = crate::config::Preferences::default();
        for shortcut in &mut prefs.shortcuts {
            shortcut.visible = false;
        }
        prefs.shortcuts[0].visible = true;
        prefs.shortcuts[0].app_id = Some("files.desktop".into());
        prefs.shortcuts[0].custom_icon = Some("/tmp/custom.png".into());
        let app = crate::catalog::DesktopApp {
            id: "files.desktop".into(),
            name: "Files".into(),
            desktop_file: Default::default(),
            icon: None,
            startup_class: None,
            class_aliases: vec![],
        };
        let items = applications(&[app.clone()], &prefs);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "shortcut:finder");
        assert_eq!(items[0].icon, prefs.shortcuts[0].custom_icon);
        assert_eq!(applications(&[], &prefs)[0].id, "shortcut:finder");
        prefs.shortcuts[0].visible = false;
        assert_eq!(applications(&[app], &prefs)[0].id, "files.desktop");
        assert!(applications(&[], &prefs).is_empty());
    }
}
