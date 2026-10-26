use std::time::{Duration, Instant, SystemTime};

pub const HISTORY_LIMIT: usize = 100;
pub const ACTIVE_LIMIT: usize = 256;

#[derive(Clone)]
pub struct Notification {
    pub id: u32,
    pub app: String,
    pub summary: String,
    pub body: String,
    pub icon: Option<std::sync::Arc<Vec<u8>>>,
    pub actions: Vec<(String, String)>,
    pub received: SystemTime,
    pub urgency: u8,
    pub active: bool,
    pub resident: bool,
    pub transient: bool,
    pub(super) owner: String,
    pub(super) deadline: Option<Instant>,
    pub(super) banner_until: Option<Instant>,
}
#[derive(Clone, Default)]
pub struct Snapshot {
    pub revision: u64,
    pub available: bool,
    pub diagnostic: Option<String>,
    pub focus: bool,
    pub notifications: Vec<Notification>,
}
impl Snapshot {
    pub fn banners(&self) -> Vec<Notification> {
        let now = Instant::now();
        self.notifications
            .iter()
            .filter(|n| n.active && n.banner_until.is_some_and(|t| t > now))
            .take(3)
            .cloned()
            .collect()
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Signal {
    Closed(u32, u32),
    Action(u32, String),
}
#[derive(Default)]
pub(super) struct Model {
    pub snapshot: Snapshot,
    next_id: u32,
    visible_centers: usize,
}
pub(super) struct Incoming {
    pub owner: String,
    pub replaces: u32,
    pub app: String,
    pub summary: String,
    pub body: String,
    pub icon: Option<std::sync::Arc<Vec<u8>>>,
    pub actions: Vec<(String, String)>,
    pub urgency: u8,
    pub resident: bool,
    pub transient: bool,
    pub timeout: i32,
}
impl Model {
    pub fn notify(&mut self, incoming: Incoming, now: Instant) -> Result<u32, &'static str> {
        let replace = self
            .snapshot
            .notifications
            .iter()
            .position(|n| n.id == incoming.replaces && n.active && n.owner == incoming.owner);
        if replace.is_none()
            && self
                .snapshot
                .notifications
                .iter()
                .filter(|n| n.active)
                .count()
                >= ACTIVE_LIMIT
        {
            return Err("Too many active notifications");
        }
        let id = if let Some(index) = replace {
            self.snapshot.notifications.remove(index).id
        } else {
            loop {
                self.next_id = self.next_id.wrapping_add(1).max(1);
                if !self
                    .snapshot
                    .notifications
                    .iter()
                    .any(|n| n.id == self.next_id)
                {
                    break self.next_id;
                }
            }
        };
        let timeout = match incoming.timeout {
            0 => None,
            -1 if incoming.urgency == 2 => None,
            -1 => Some(Duration::from_secs(6)),
            ms => Some(Duration::from_millis(ms.max(1) as u64)),
        };
        self.snapshot.notifications.insert(
            0,
            Notification {
                id,
                app: incoming.app,
                summary: incoming.summary,
                body: incoming.body,
                icon: incoming.icon,
                actions: incoming.actions,
                received: SystemTime::now(),
                urgency: incoming.urgency,
                active: true,
                resident: incoming.resident,
                transient: incoming.transient,
                owner: incoming.owner,
                deadline: timeout.map(|d| now + d),
                banner_until: (!self.snapshot.focus && self.visible_centers == 0).then(|| {
                    now + timeout
                        .unwrap_or(Duration::from_secs(6))
                        .min(Duration::from_secs(6))
                }),
            },
        );
        self.trim();
        self.changed();
        Ok(id)
    }
    fn changed(&mut self) {
        self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
    }
    fn trim(&mut self) {
        let mut closed = 0;
        self.snapshot.notifications.retain(|n| {
            if n.active {
                true
            } else {
                closed += 1;
                closed <= HISTORY_LIMIT
            }
        });
    }
    pub fn set_focus(&mut self, value: bool) {
        self.snapshot.focus = value;
        if value {
            self.hide_banners();
        }
        self.changed();
    }
    pub fn center(&mut self, visible: bool) {
        self.visible_centers = if visible {
            self.visible_centers.saturating_add(1)
        } else {
            self.visible_centers.saturating_sub(1)
        };
        if visible {
            self.hide_banners();
        }
    }
    pub fn hide_banners(&mut self) {
        for n in &mut self.snapshot.notifications {
            n.banner_until = None;
        }
        self.changed();
    }
    pub fn close(&mut self, id: u32, reason: u32, remove: bool) -> Vec<Signal> {
        let Some(index) = self.snapshot.notifications.iter().position(|n| n.id == id) else {
            return vec![];
        };
        let n = &mut self.snapshot.notifications[index];
        let signals = if n.active {
            vec![Signal::Closed(id, reason)]
        } else {
            vec![]
        };
        n.active = false;
        n.actions.clear();
        n.deadline = None;
        n.banner_until = None;
        if remove || n.transient {
            self.snapshot.notifications.remove(index);
        }
        self.trim();
        self.changed();
        signals
    }
    pub fn clear(&mut self) -> Vec<Signal> {
        let signals = self
            .snapshot
            .notifications
            .iter()
            .filter(|n| n.active)
            .map(|n| Signal::Closed(n.id, 2))
            .collect();
        self.snapshot.notifications.clear();
        self.changed();
        signals
    }
    pub fn invoke(&mut self, id: u32, key: &str) -> Vec<Signal> {
        let Some(n) = self
            .snapshot
            .notifications
            .iter()
            .find(|n| n.id == id && n.active && n.actions.iter().any(|a| a.0 == key))
        else {
            return vec![];
        };
        let resident = n.resident;
        let mut signals = vec![Signal::Action(id, key.into())];
        if !resident {
            signals.extend(self.close(id, 2, true));
        }
        signals
    }
    pub fn tick(&mut self, now: Instant) -> Vec<Signal> {
        let expired: Vec<_> = self
            .snapshot
            .notifications
            .iter()
            .filter(|n| n.active && n.deadline.is_some_and(|t| t <= now))
            .map(|n| n.id)
            .collect();
        let mut signals = vec![];
        for id in expired {
            signals.extend(self.close(id, 1, false));
        }
        let mut changed = false;
        for n in &mut self.snapshot.notifications {
            if n.banner_until.is_some_and(|t| t <= now) {
                n.banner_until = None;
                changed = true;
            }
        }
        if changed {
            self.changed();
        }
        signals
    }
}
/// Plain text only: never execute markup, load inline images, or follow links.
pub(super) fn plain_text(value: &str, limit: usize) -> String {
    let bounded: String = value.chars().take(limit * 4).collect();
    let mut text = String::new();
    let mut rest = bounded.as_str();
    while !rest.is_empty() {
        if rest.starts_with('<') {
            let after = &rest[1..];
            let tag = after.strip_prefix('/').unwrap_or(after);
            if tag.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && let Some(end) = rest.find('>')
            {
                rest = &rest[end + 1..];
                continue;
            }
        }
        let c = rest.chars().next().unwrap();
        rest = &rest[c.len_utf8()..];
        if (!c.is_control() || c == '\n' || c == '\t')
            && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            text.push(c);
        }
    }
    text = text
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    text.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn incoming() -> Incoming {
        Incoming {
            owner: ":1.1".into(),
            replaces: 0,
            app: "Mail".into(),
            summary: "New mail".into(),
            body: "Hello".into(),
            icon: None,
            actions: vec![("default".into(), "Open".into())],
            urgency: 1,
            resident: false,
            transient: false,
            timeout: 1000,
        }
    }
    #[test]
    fn replacement_resets_deadline_and_cannot_replace_another_sender() {
        let mut m = Model::default();
        let now = Instant::now();
        let id = m.notify(incoming(), now).unwrap();
        let mut replacement = incoming();
        replacement.replaces = id;
        assert_eq!(
            id,
            m.notify(replacement, now + Duration::from_millis(500))
                .unwrap()
        );
        assert!(m.tick(now + Duration::from_secs(1)).is_empty());
        let mut other = incoming();
        other.owner = ":1.2".into();
        other.replaces = id;
        assert_ne!(id, m.notify(other, now).unwrap());
        assert_eq!(m.snapshot.notifications.len(), 2);
    }
    #[test]
    fn expiry_retains_history_but_invalidates_actions_and_emits_once() {
        let mut m = Model::default();
        let now = Instant::now();
        let id = m.notify(incoming(), now).unwrap();
        assert_eq!(
            m.tick(now + Duration::from_secs(1)),
            vec![Signal::Closed(id, 1)]
        );
        assert_eq!(m.snapshot.notifications.len(), 1);
        assert!(!m.snapshot.notifications[0].active);
        assert!(m.invoke(id, "default").is_empty());
        assert!(m.tick(now + Duration::from_secs(2)).is_empty());
        assert!(m.close(id, 2, true).is_empty());
        assert!(m.snapshot.notifications.is_empty());
    }
    #[test]
    fn focus_suppresses_all_banners_without_losing_history_or_replaying() {
        let mut m = Model::default();
        let now = Instant::now();
        m.notify(incoming(), now).unwrap();
        m.set_focus(true);
        m.notify(incoming(), now).unwrap();
        assert!(m.snapshot.banners().is_empty());
        m.set_focus(false);
        assert!(m.snapshot.banners().is_empty());
        assert_eq!(m.snapshot.notifications.len(), 2);
        m.center(true);
        m.notify(incoming(), now).unwrap();
        assert!(m.snapshot.banners().is_empty());
    }
    #[test]
    fn resident_transient_and_never_expire() {
        let mut m = Model::default();
        let now = Instant::now();
        let mut n = incoming();
        n.resident = true;
        n.timeout = 0;
        let id = m.notify(n, now).unwrap();
        assert_eq!(
            m.invoke(id, "default"),
            vec![Signal::Action(id, "default".into())]
        );
        assert!(m.tick(now + Duration::from_secs(99)).is_empty());
        assert!(m.snapshot.notifications[0].active);
        let mut n = incoming();
        n.transient = true;
        let transient = m.notify(n, now).unwrap();
        assert_eq!(
            m.tick(now + Duration::from_secs(1)),
            vec![Signal::Closed(transient, 1)]
        );
        assert_eq!(m.snapshot.notifications.len(), 1);
    }
    #[test]
    fn clear_all_closes_only_live_ids_and_history_is_bounded() {
        let mut m = Model::default();
        let now = Instant::now();
        for _ in 0..150 {
            let id = m.notify(incoming(), now).unwrap();
            m.close(id, 1, false);
        }
        assert_eq!(m.snapshot.notifications.len(), HISTORY_LIMIT);
        let id = m.notify(incoming(), now).unwrap();
        assert_eq!(m.clear(), vec![Signal::Closed(id, 2)]);
        assert!(m.snapshot.notifications.is_empty());
    }
    #[test]
    fn moving_between_outputs_keeps_banners_hidden_until_both_windows_close() {
        let mut m = Model::default();
        let now = Instant::now();
        m.center(true);
        m.center(true);
        m.center(false);
        m.notify(incoming(), now).unwrap();
        assert!(m.snapshot.banners().is_empty());
        m.center(false);
        m.notify(incoming(), now).unwrap();
        assert_eq!(m.snapshot.banners().len(), 1);
    }
    #[test]
    fn critical_defaults_do_not_expire_and_active_limit_returns_an_error() {
        let mut m = Model::default();
        let now = Instant::now();
        for _ in 0..ACTIVE_LIMIT {
            let mut n = incoming();
            n.urgency = 2;
            n.timeout = -1;
            m.notify(n, now).unwrap();
        }
        assert!(m.tick(now + Duration::from_secs(60)).is_empty());
        assert!(m.notify(incoming(), now).is_err());
        let mut replacement = incoming();
        replacement.replaces = m.snapshot.notifications[0].id;
        assert!(m.notify(replacement, now).is_ok());
    }
    #[test]
    fn safe_text_is_bounded_and_markup_is_inert() {
        assert_eq!(
            plain_text(
                "<b>Hello</b> &amp; <a href='https://bad'>world</a>\u{202e}\0",
                100
            ),
            "Hello & world"
        );
        assert_eq!(plain_text("你好世界", 2), "你好");
        assert_eq!(plain_text("2 < 3 > 1", 100), "2 < 3 > 1");
        assert_eq!(plain_text("Hello <unclosed", 100), "Hello <unclosed");
    }
}
