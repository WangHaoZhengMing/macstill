//! Each Stage Manager session owns one workspace on one output; MultiManager coordinates all outputs.
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    hash::{Hash, Hasher},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
};

mod outputs;
#[cfg(test)]
mod tests;
pub use outputs::MultiManager;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: i64,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Window {
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub modal: bool,
    pub address: String,
    pub pid: i64,
    pub class: String,
    #[serde(default)]
    pub title: String,
    pub workspace: Workspace,
    pub monitor: i64,
    pub at: [i32; 2],
    pub size: [i32; 2],
    pub floating: bool,
    #[serde(default)]
    pub fullscreen: u8,
    #[serde(default, rename = "fullscreenClient")]
    pub fullscreen_client: u8,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default = "mapped")]
    pub mapped: bool,
    #[serde(default)]
    pub grouped: Vec<String>,
}
fn mapped() -> bool {
    true
}
impl Window {
    fn identity(&self, other: &Self) -> bool {
        self.address == other.address && self.pid == other.pid && self.class == other.class
    }
    fn auxiliary(&self) -> bool {
        self.parent.is_some() || self.modal || self.class.starts_with("xdg-desktop-portal")
    }
    fn eligible(&self) -> bool {
        !self.auxiliary()
            && self.mapped
            && self.pid > 0
            && self.size.iter().all(|size| *size > 0)
            && !self.hidden
            && !self.pinned
            && self.grouped.is_empty()
            && self.workspace.id > 0
            && !self.workspace.name.starts_with("macstill-stage-")
            && self.class != "org.macstill.Desktop"
            && crate::validate_address(&self.address).is_ok()
    }
    fn group(&self) -> String {
        if self.class.is_empty() {
            self.address.clone()
        } else {
            self.class.to_lowercase()
        }
    }
}
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Monitor {
    pub id: i64,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
    #[serde(default)]
    pub transform: u32,
    #[serde(default)]
    pub reserved: [i32; 4],
    #[serde(default)]
    pub focused: bool,
    #[serde(rename = "activeWorkspace")]
    pub workspace: Workspace,
}
impl Monitor {
    fn area(&self, sidebar: bool) -> Result<Rect> {
        ensure!(
            self.scale.is_finite() && self.scale > 0. && self.width > 0 && self.height > 0,
            "Invalid output geometry"
        );
        let (w, h) = if self.transform % 2 == 1 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        };
        let [left, top, right, bottom] = self.reserved;
        let sidebar = if sidebar { 156 } else { 0 };
        let rect = Rect {
            at: [self.x + left + sidebar + 12, self.y + top + 12],
            size: [
                (w as f64 / self.scale) as i32 - left - right - sidebar - 24,
                (h as f64 / self.scale) as i32 - top - bottom - 24,
            ],
        };
        ensure!(
            rect.size[0] >= 200 && rect.size[1] >= 120,
            "This output is too small for Stage Manager"
        );
        Ok(rect)
    }
}
#[derive(Clone, Debug)]
pub struct Scene {
    pub windows: Vec<Window>,
    pub monitors: Vec<Monitor>,
    pub active: Option<String>,
}

impl Scene {
    fn focused_main(&self) -> Option<&str> {
        let mut address = self.active.as_deref()?;
        for _ in 0..self.windows.len() {
            let window = self.windows.iter().find(|w| w.address == address)?;
            let Some(parent) = window.parent.as_deref() else {
                return Some(address);
            };
            address = parent;
        }
        None // Ignore malformed/cyclic relationships.
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub at: [i32; 2],
    pub size: [i32; 2],
}
impl Rect {
    fn fit(&self, area: &Self) -> Self {
        let size = [
            self.size[0].clamp(1, area.size[0]),
            self.size[1].clamp(1, area.size[1]),
        ];
        Self {
            size,
            at: [
                self.at[0].clamp(area.at[0], area.at[0] + area.size[0] - size[0]),
                self.at[1].clamp(area.at[1], area.at[1] + area.size[1] - size[1]),
            ],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tracked {
    pub original: Window,
    pub group: String,
    pub rect: Rect,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub version: u32,
    pub output: String,
    pub home: Workspace,
    pub parking: String,
    pub active: Option<String>,
    pub original_focus: Option<String>,
    pub windows: Vec<Tracked>,
}
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Group {
    pub id: String,
    pub classes: Vec<String>,
    pub titles: Vec<String>,
    pub count: usize,
    pub windows: Vec<Window>,
}
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Snapshot {
    pub outputs: Vec<Snapshot>,
    pub available: bool,
    pub enabled: bool,
    pub busy: bool,
    pub visible: bool,
    pub output: Option<String>,
    pub workspace: Option<i64>,
    pub bounds: Option<Rect>,
    pub active: Option<String>,
    pub groups: Vec<Group>,
    pub error: Option<String>,
    pub transition: Option<Transition>,
    pub desktop: bool,
    pub recent_hidden: bool,
    pub occluded: bool,
    pub fullscreen: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TransitionWindow {
    pub window: Window,
    pub rect: Rect,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    /// None: group switch; Some(true): enable; Some(false): restore desktop.
    pub lifecycle: Option<bool>,
    pub generation: u64,
    pub ready: bool,
    pub from: Vec<TransitionWindow>,
    pub to: Vec<TransitionWindow>,
    pub bounds: Rect,
    pub incoming_slot: usize,
}
#[derive(Clone, Debug)]
pub enum Command {
    OnOutput(String, Box<Command>),
    Toggle,
    Enable,
    Disable,
    Activate(String),
    ActivateWindow(Window),
    Merge(String),
    Ungroup,
    Separate(String),
    Next,
    ShowDesktop,
    SetRecentHidden(bool),
    Animate(Box<Command>),
    PresentTransition(u64),
    FinishTransition(u64),
}
#[derive(Clone, Debug, PartialEq)]
pub enum Operation {
    Move {
        address: String,
        workspace: Workspace,
    },
    Float {
        address: String,
        enabled: bool,
    },
    Fullscreen {
        address: String,
        internal: u8,
        client: u8,
    },
    Place {
        address: String,
        rect: Rect,
    },
    Focus(String),
}
pub trait Compositor {
    fn scene(&mut self) -> Result<Scene>;
    fn apply(&mut self, operation: Operation) -> Result<()>;
}
pub trait Journal {
    fn read(&self) -> Result<Option<Session>>;
    fn write(&mut self, session: Option<&Session>) -> Result<()>;
}

pub struct Manager<C: Compositor, J: Journal> {
    compositor: C,
    journal: J,
    session: Option<Session>,
    last_saved: Option<Session>,
    restore_pending: bool,
    // Runtime history is separate from the immutable restoration journal.
    recent_groups: Vec<String>,
    group_focus: BTreeMap<String, Window>,
    transition_generation: u64,
    transition_started: Option<std::time::Instant>,
    pub snapshot: Snapshot,
}
impl<C: Compositor, J: Journal> Manager<C, J> {
    pub fn new(compositor: C, journal: J) -> Self {
        Self {
            compositor,
            journal,
            session: None,
            last_saved: None,
            restore_pending: false,
            recent_groups: Vec::new(),
            group_focus: BTreeMap::new(),
            transition_generation: 0,
            transition_started: None,
            snapshot: Snapshot {
                available: true,
                ..Snapshot::default()
            },
        }
    }
    pub fn recover(&mut self) -> Result<()> {
        let result = (|| -> Result<()> {
            let record = self.journal.read()?;
            if let Some(session) = &record {
                validate_session(session)?;
            }
            self.session = record;
            self.disable()
        })();
        self.snapshot.enabled = self.session.is_some();
        if let Err(error) = &result {
            self.snapshot.error = Some(format!("Recovery: {error:#}"));
            self.snapshot.available = self.session.is_some();
        }
        result
    }
    pub fn handle(&mut self, command: Command) -> Result<()> {
        ensure!(
            self.snapshot.available,
            "Resolve the Stage Manager recovery error before enabling it"
        );
        ensure!(
            !self.restore_pending || matches!(command, Command::Toggle | Command::Disable),
            "Retry turning Stage Manager off to finish restoring windows"
        );
        self.snapshot.error = None;
        let result = match command {
            Command::Toggle if self.session.is_some() => self.disable(),
            Command::Toggle | Command::Enable => self.enable(),
            Command::Disable => self.disable(),
            Command::FinishTransition(generation) => self.finish_transition(generation),
            Command::PresentTransition(generation) => self.present_transition(generation),
            Command::Animate(command) => match *command {
                Command::Enable => self.enable_animated(),
                Command::Toggle if self.session.is_none() => self.enable_animated(),
                Command::Toggle
                    if self
                        .snapshot
                        .transition
                        .as_ref()
                        .is_some_and(|t| t.lifecycle == Some(false)) =>
                {
                    self.lifecycle_transition(true)
                }
                Command::Disable | Command::Toggle => self.lifecycle_transition(false),
                Command::Activate(_)
                | Command::ActivateWindow(_)
                | Command::Next
                | Command::ShowDesktop => self.change(*command, true),
                _ => anyhow::bail!("Unsupported animated action"),
            },
            command => self.change(command, false),
        };
        if let Err(error) = &result {
            self.snapshot.error = Some(format!("{error:#}"));
        }
        self.snapshot.enabled = self.session.is_some();
        result
    }
    fn save(&mut self) -> Result<()> {
        if self.session != self.last_saved {
            self.journal.write(self.session.as_ref())?;
            self.last_saved = self.session.clone();
        }
        Ok(())
    }
    fn enable(&mut self) -> Result<()> {
        self.start_session(false)
    }
    fn enable_animated(&mut self) -> Result<()> {
        self.start_session(true)
    }
    fn start_session(&mut self, animated: bool) -> Result<()> {
        if self.session.is_some() {
            if animated
                && self
                    .snapshot
                    .transition
                    .as_ref()
                    .is_some_and(|t| t.lifecycle == Some(false))
            {
                return self.lifecycle_transition(true);
            }
            return Ok(());
        }
        let scene = self.compositor.scene()?;
        let monitor = scene
            .monitors
            .iter()
            .find(|monitor| monitor.focused)
            .ok_or_else(|| anyhow::anyhow!("No focused output"))?;
        ensure!(
            monitor.workspace.id > 0 && !monitor.workspace.name.starts_with("macstill-stage-"),
            "Open a normal workspace before enabling Stage Manager"
        );
        let area = monitor.area(true)?;
        let windows: Vec<_> = scene
            .windows
            .iter()
            .filter(|w| {
                w.workspace.id == monitor.workspace.id && w.monitor == monitor.id && w.eligible()
            })
            .map(|window| Tracked {
                original: window.clone(),
                group: window.group(),
                rect: Rect {
                    at: window.at,
                    size: window.size,
                }
                .fit(&area),
            })
            .collect();
        let active = windows
            .iter()
            .find(|window| scene.focused_main() == Some(window.original.address.as_str()))
            .or_else(|| windows.first())
            .map(|window| window.group.clone());
        self.session = Some(Session {
            version: 1,
            output: monitor.name.clone(),
            home: monitor.workspace.clone(),
            parking: format!("macstill-stage-{}-{}", std::process::id(), monitor.id),
            active,
            original_focus: scene.active.clone(),
            windows,
        });
        if let Err(error) = self.save() {
            self.session = None;
            return Err(error);
        }
        self.remember_focus(&scene);
        if animated {
            return self.lifecycle_transition(true);
        }
        if let Err(error) = self.arrange(&scene, true) {
            let rollback = self.disable();
            return Err(match rollback {
                Ok(()) => error,
                Err(rollback) => {
                    anyhow::anyhow!("{error}; restoring windows also failed: {rollback}")
                }
            });
        }
        self.publish(&scene);
        Ok(())
    }
    fn lifecycle_transition(&mut self, entering: bool) -> Result<()> {
        if self.session.is_none() {
            return Ok(());
        }
        if self.restore_pending {
            return self.disable();
        }
        let scene = self.compositor.scene()?;
        self.publish(&scene);
        let session = self.session.as_ref().unwrap();
        let Some(bounds) = self.snapshot.bounds.clone() else {
            return self.disable();
        };
        let from: Vec<_> = session
            .windows
            .iter()
            .filter_map(|tracked| {
                let live = scene
                    .windows
                    .iter()
                    .find(|w| tracked.original.identity(w))?;
                (live.workspace.id == session.home.id).then(|| TransitionWindow {
                    window: live.clone(),
                    rect: Rect {
                        at: live.at,
                        size: live.size,
                    },
                })
            })
            .collect();
        let to = session
            .windows
            .iter()
            .filter_map(|tracked| {
                let live = scene
                    .windows
                    .iter()
                    .find(|w| tracked.original.identity(w))?;
                if live.workspace.id != session.home.id && live.workspace.name != session.parking {
                    return None;
                }
                if entering && Some(&tracked.group) != session.active.as_ref() {
                    return None;
                }
                Some(TransitionWindow {
                    window: live.clone(),
                    rect: if entering {
                        tracked.rect.clone()
                    } else {
                        Rect {
                            at: tracked.original.at,
                            size: tracked.original.size,
                        }
                    },
                })
            })
            .collect();
        self.transition_generation += 1;
        self.snapshot.transition = Some(Transition {
            lifecycle: Some(entering),
            generation: self.transition_generation,
            ready: false,
            from,
            to,
            bounds,
            incoming_slot: 0,
        });
        self.transition_started = Some(std::time::Instant::now());
        Ok(())
    }
    fn change(&mut self, command: Command, animated: bool) -> Result<()> {
        let mut scene = self.compositor.scene()?;
        self.remember(&scene)?;
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Stage Manager is off"))?;
        let previous = session.active.clone();
        match command {
            Command::SetRecentHidden(hidden) => self.snapshot.recent_hidden = hidden,
            Command::ShowDesktop => {
                self.snapshot.desktop = !self.snapshot.desktop;
                session.active = if self.snapshot.desktop {
                    None
                } else {
                    self.recent_groups
                        .iter()
                        .find(|id| session.windows.iter().any(|w| &w.group == *id))
                        .cloned()
                };
            }
            Command::ActivateWindow(window) => {
                let tracked = session
                    .windows
                    .iter()
                    .find(|w| w.original.identity(&window))
                    .ok_or_else(|| anyhow::anyhow!("This window has closed"))?;
                ensure!(
                    scene.windows.iter().any(|w| w.identity(&window)),
                    "This window has closed"
                );
                let group = tracked.group.clone();
                self.group_focus.insert(group.clone(), window.clone());
                session.active = Some(group);
                scene.active = Some(window.address);
                self.snapshot.desktop = false;
            }
            Command::Activate(id) => {
                ensure!(
                    session.windows.iter().any(|w| w.group == id),
                    "This window group has closed"
                );
                session.active = Some(id);
                self.snapshot.desktop = false;
            }
            Command::Merge(id) => {
                let active = session.active.clone().unwrap_or_else(|| id.clone());
                ensure!(
                    session.windows.iter().any(|w| w.group == id),
                    "This window group has closed"
                );
                for window in &mut session.windows {
                    if window.group == id {
                        window.group = active.clone();
                    }
                }
                session.active = Some(active);
                self.snapshot.desktop = false;
            }
            Command::Ungroup | Command::Separate(_) => {
                let target = match &command {
                    Command::Separate(id) => Some(id.clone()),
                    _ => session.active.clone(),
                };
                let was_active = target == session.active;
                for window in &mut session.windows {
                    if Some(&window.group) == target.as_ref() {
                        window.group = window.original.group();
                    }
                }
                if was_active {
                    session.active = session
                        .windows
                        .iter()
                        .find(|w| scene.active.as_ref() == Some(&w.original.address))
                        .or_else(|| session.windows.first())
                        .map(|w| w.group.clone());
                }
            }
            Command::Next => {
                let mut ids = Vec::new();
                for window in &session.windows {
                    if !ids.contains(&window.group) {
                        ids.push(window.group.clone());
                    }
                }
                if !ids.is_empty() {
                    let index = ids
                        .iter()
                        .position(|id| Some(id) == session.active.as_ref())
                        .unwrap_or(0);
                    session.active = Some(ids[(index + 1) % ids.len()].clone());
                    self.snapshot.desktop = false;
                }
            }
            _ => {}
        }
        self.snapshot.transition = None;
        self.transition_started = None;
        if animated && previous != session.active {
            let monitor = scene
                .monitors
                .iter()
                .find(|m| m.name == session.output)
                .ok_or_else(|| anyhow::anyhow!("Stage output disconnected"))?;
            let area = monitor.area(false)?;
            self.transition_generation += 1;
            let presentation = |active: &Option<String>, from: bool| {
                let focused = if from {
                    scene.active.as_ref()
                } else {
                    active
                        .as_ref()
                        .and_then(|id| self.group_focus.get(id))
                        .map(|w| &w.address)
                };
                let mut windows: Vec<_> = session
                    .windows
                    .iter()
                    .filter(|w| Some(&w.group) == active.as_ref())
                    .map(|w| TransitionWindow {
                        window: w.original.clone(),
                        rect: if from {
                            scene
                                .windows
                                .iter()
                                .find(|live| {
                                    w.original.identity(live)
                                        && live.workspace.id == session.home.id
                                })
                                .map(|live| Rect {
                                    at: live.at,
                                    size: live.size,
                                })
                                .unwrap_or_else(|| w.rect.fit(&area))
                        } else {
                            w.rect.fit(&area)
                        },
                    })
                    .collect();
                windows.sort_by_key(|w| Some(&w.window.address) == focused);
                windows
            };
            self.snapshot.transition = Some(Transition {
                lifecycle: None,
                generation: self.transition_generation,
                ready: false,
                from: presentation(&previous, true),
                to: presentation(&session.active, false),
                bounds: Rect {
                    at: [monitor.x, monitor.y],
                    size: if monitor.transform % 2 == 0 {
                        [
                            (monitor.width as f64 / monitor.scale) as i32,
                            (monitor.height as f64 / monitor.scale) as i32,
                        ]
                    } else {
                        [
                            (monitor.height as f64 / monitor.scale) as i32,
                            (monitor.width as f64 / monitor.scale) as i32,
                        ]
                    },
                },
                incoming_slot: self
                    .recent_groups
                    .iter()
                    .filter(|id| Some(*id) != previous.as_ref())
                    .position(|id| Some(id) == session.active.as_ref())
                    .unwrap_or(0),
            });
            self.transition_started = Some(std::time::Instant::now());
        }
        self.save()?;
        if self.snapshot.transition.is_none() {
            self.arrange(&scene, true)?;
        }
        self.publish(&scene);
        Ok(())
    }
    fn present_transition(&mut self, generation: u64) -> Result<()> {
        let Some(transition) = &self.snapshot.transition else {
            return Ok(());
        };
        if transition.generation != generation || transition.ready {
            return Ok(());
        }
        let scene = self.compositor.scene()?;
        let session = self.session.as_ref().unwrap();
        // The native scene already owns these surfaces. Move and focus the incoming
        // group now, while its normal render pass remains suppressed by the plugin.
        if transition.lifecycle.is_none() {
            self.arrange(&scene, true)?;
            self.snapshot.transition.as_mut().unwrap().ready = true;
            self.transition_started = Some(std::time::Instant::now());
            let scene = self.compositor.scene()?;
            self.publish(&scene);
            return Ok(());
        }
        // The compositor has accepted the surface scene before clients are parked.
        for tracked in &session.windows {
            if let Some(window) = scene.windows.iter().find(|w| tracked.original.identity(w))
                && window.workspace.name != session.parking
            {
                self.compositor.apply(Operation::Move {
                    address: window.address.clone(),
                    workspace: Workspace {
                        id: 0,
                        name: session.parking.clone(),
                    },
                })?;
            }
        }
        self.snapshot.transition.as_mut().unwrap().ready = true;
        self.transition_started = Some(std::time::Instant::now());
        self.publish(&scene);
        Ok(())
    }
    fn finish_transition(&mut self, generation: u64) -> Result<()> {
        if self.snapshot.transition.as_ref().map(|t| t.generation) != Some(generation) {
            return Ok(());
        }
        if self
            .snapshot
            .transition
            .as_ref()
            .is_some_and(|t| t.lifecycle == Some(false))
        {
            return self.disable();
        }
        let scene = self.compositor.scene()?;
        self.snapshot.transition = None;
        self.transition_started = None;
        self.remember(&scene)?;
        let session = self.session.as_mut().unwrap();
        if !self.snapshot.desktop
            && !session
                .windows
                .iter()
                .any(|w| Some(&w.group) == session.active.as_ref())
        {
            session.active = self
                .recent_groups
                .iter()
                .find(|id| session.windows.iter().any(|w| &w.group == *id))
                .cloned()
                .or_else(|| session.windows.first().map(|w| w.group.clone()));
            self.save()?;
        }
        let focus = self.session.as_ref().is_some_and(|s| {
            scene
                .monitors
                .iter()
                .any(|m| m.name == s.output && m.focused && m.workspace.id == s.home.id)
        });
        self.arrange(&scene, focus)?;
        self.publish(&scene);
        Ok(())
    }
    fn remember_focus(&mut self, scene: &Scene) {
        let Some(session) = &self.session else { return };
        self.group_focus.retain(|group, focus| {
            session
                .windows
                .iter()
                .any(|w| w.group == *group && w.original.identity(focus))
        });
        if let Some(tracked) = session
            .windows
            .iter()
            .find(|w| scene.active.as_ref() == Some(&w.original.address))
        {
            self.group_focus
                .insert(tracked.group.clone(), tracked.original.clone());
        }
    }
    // Preserve user changes within a stage and release windows explicitly moved elsewhere.
    fn remember(&mut self, scene: &Scene) -> Result<()> {
        let Some(session) = &mut self.session else {
            return Ok(());
        };
        session.windows.retain(|tracked| {
            scene.windows.iter().any(|window| {
                tracked.original.identity(window)
                    && window.mapped
                    && !window.auxiliary()
                    && !window.pinned
                    && window.grouped.is_empty()
                    && (window.workspace.id == session.home.id
                        || window.workspace.name == session.parking)
            })
        });
        for tracked in &mut session.windows {
            if let Some(window) = scene.windows.iter().find(|w| tracked.original.identity(w))
                && window.workspace.id == session.home.id
                && window.floating
                && window.fullscreen == 0
            {
                tracked.rect = Rect {
                    at: window.at,
                    size: window.size,
                };
            }
        }
        let Some(monitor) = scene.monitors.iter().find(|m| m.name == session.output) else {
            return Ok(());
        };
        let area = monitor.area(true)?;
        for window in scene.windows.iter().filter(|w| {
            w.eligible() && w.workspace.id == session.home.id && w.monitor == monitor.id
        }) {
            if session
                .windows
                .iter()
                .any(|tracked| tracked.original.address == window.address)
            {
                continue;
            }
            let group = session
                .windows
                .iter()
                .find(|tracked| tracked.original.group() == window.group())
                .map(|tracked| tracked.group.clone())
                .unwrap_or_else(|| window.group());
            session.windows.push(Tracked {
                original: window.clone(),
                group,
                rect: Rect {
                    at: window.at,
                    size: window.size,
                }
                .fit(&area),
            });
        }
        self.remember_focus(scene);
        self.save()
    }
    pub fn tick(&mut self) -> Result<()> {
        if self.restore_pending {
            return Ok(());
        }
        if self.session.is_none() {
            return Ok(());
        }
        let scene = self.compositor.scene()?;
        if !scene
            .monitors
            .iter()
            .any(|m| Some(&m.name) == self.session.as_ref().map(|s| &s.output))
        {
            return self.disable();
        }
        // A compositor focus/workspace switch can expose our parking workspace,
        // including while Show Desktop has cleared the active group. Recover only
        // our own parking space; ordinary user workspace switches stay untouched.
        let escaped_parking = self.session.as_ref().is_some_and(|session| {
            scene.monitors.iter().any(|monitor| {
                monitor.name == session.output && monitor.workspace.name == session.parking
            })
        });
        if escaped_parking {
            self.snapshot.desktop = false;
            self.snapshot.transition = None;
            self.transition_started = None;
        }
        let new_window = self.session.as_ref().and_then(|session| {
            scene
                .windows
                .iter()
                .filter(|w| {
                    w.eligible()
                        && w.workspace.id == session.home.id
                        && scene
                            .monitors
                            .iter()
                            .any(|m| m.name == session.output && m.id == w.monitor)
                        && !session.windows.iter().any(|t| t.original.identity(w))
                })
                .max_by_key(|w| scene.active.as_ref() == Some(&w.address))
                .map(|w| w.address.clone())
        });
        self.remember(&scene)?;
        if let Some(address) = &new_window {
            // A newly mapped main window wins even if compositor focus arrives later.
            self.snapshot.transition = None;
            self.transition_started = None;
            let session = self.session.as_mut().unwrap();
            session.active = session
                .windows
                .iter()
                .find(|w| &w.original.address == address)
                .map(|w| w.group.clone());
            self.snapshot.desktop = false;
        }
        if let Some(started) = self.transition_started {
            let transition = self.snapshot.transition.as_ref().unwrap();
            let generation = transition.generation;
            let visible = self.session.as_ref().is_some_and(|s| {
                scene
                    .monitors
                    .iter()
                    .any(|m| m.name == s.output && m.workspace.id == s.home.id)
            });
            if started.elapsed() >= std::time::Duration::from_millis(2000) || !visible {
                return self.finish_transition(generation);
            }
            self.publish(&scene);
            return Ok(());
        }
        let focused_main = scene.focused_main();
        let session = self.session.as_mut().unwrap();
        let previous = session.active.clone();
        if new_window.is_none()
            && let Some(active) = session.windows.iter().find(|w| {
                focused_main == Some(w.original.address.as_str())
                    && (!self.snapshot.desktop
                        || scene.windows.iter().any(|live| {
                            w.original.identity(live) && live.workspace.id == session.home.id
                        }))
            })
        {
            session.active = Some(active.group.clone());
            self.snapshot.desktop = false;
        }
        if !self.snapshot.desktop
            && !session
                .windows
                .iter()
                .any(|w| Some(&w.group) == session.active.as_ref())
        {
            session.active = self
                .recent_groups
                .iter()
                .find(|id| session.windows.iter().any(|w| &w.group == *id))
                .cloned()
                .or_else(|| session.windows.first().map(|w| w.group.clone()));
        }
        let parked_focus = scene.windows.iter().any(|w| {
            scene.active.as_ref() == Some(&w.address) && w.workspace.name == session.parking
        });
        let managed_focus = session
            .windows
            .iter()
            .any(|w| scene.active.as_ref() == Some(&w.original.address));
        let empty_focus_here = scene.active.is_none()
            && scene.monitors.iter().any(|m| {
                m.name == session.output && m.focused && m.workspace.id == session.home.id
            });
        let focus = escaped_parking
            || new_window.is_some()
            || (session.active != previous || parked_focus) && (managed_focus || empty_focus_here);
        self.save()?;
        self.arrange(&scene, focus && new_window.is_none())?;
        if let Some(address) = new_window {
            // Keep a newly opened app's focused chooser above its main window.
            if focused_main != Some(address.as_str()) || scene.active.as_ref() == Some(&address) {
                self.compositor.apply(Operation::Focus(address))?;
            }
        }
        if escaped_parking {
            let restored = self.compositor.scene()?;
            self.publish(&restored);
        } else {
            self.publish(&scene);
        }
        Ok(())
    }
    fn arrange(&mut self, scene: &Scene, focus: bool) -> Result<()> {
        let session = self.session.as_ref().unwrap();
        let monitor = scene
            .monitors
            .iter()
            .find(|m| m.name == session.output)
            .ok_or_else(|| anyhow::anyhow!("Stage output disconnected"))?;
        let area = monitor.area(false)?;
        // Park background groups before showing the active group.
        for active in [false, true] {
            for tracked in &session.windows {
                if (Some(&tracked.group) == session.active.as_ref()) != active {
                    continue;
                }
                let Some(window) = scene.windows.iter().find(|w| tracked.original.identity(w))
                else {
                    continue;
                };
                if !active {
                    if window.workspace.name != session.parking {
                        self.compositor.apply(Operation::Move {
                            address: window.address.clone(),
                            workspace: Workspace {
                                id: 0,
                                name: session.parking.clone(),
                            },
                        })?;
                    }
                } else {
                    // A live stage belongs to the user: polling must not undo a drag,
                    // resize or fullscreen request. Only place windows on entry.
                    if self.snapshot.enabled
                        && window.workspace.id == session.home.id
                        && window.floating
                    {
                        continue;
                    }
                    if window.workspace.id != session.home.id {
                        self.compositor.apply(Operation::Move {
                            address: window.address.clone(),
                            workspace: session.home.clone(),
                        })?;
                    }
                    if window.fullscreen != 0 || window.fullscreen_client != 0 {
                        self.compositor.apply(Operation::Fullscreen {
                            address: window.address.clone(),
                            internal: 0,
                            client: 0,
                        })?;
                    }
                    if !window.floating {
                        self.compositor.apply(Operation::Float {
                            address: window.address.clone(),
                            enabled: true,
                        })?;
                    }
                    let rect = tracked.rect.fit(&area);
                    if window.at != rect.at || window.size != rect.size {
                        self.compositor.apply(Operation::Place {
                            address: window.address.clone(),
                            rect,
                        })?;
                    }
                }
            }
        }
        let focused_child = scene.windows.iter().any(|w| {
            scene.active.as_ref() == Some(&w.address)
                && w.auxiliary()
                && session.windows.iter().any(|tracked| {
                    scene.focused_main() == Some(tracked.original.address.as_str())
                        && Some(&tracked.group) == session.active.as_ref()
                })
        });
        if focus
            && !focused_child
            && let Some(window) = session
                .windows
                .iter()
                .find(|w| Some(&w.group) == session.active.as_ref())
        {
            let address = scene
                .active
                .as_ref()
                .filter(|address| {
                    session.windows.iter().any(|w| {
                        &w.original.address == *address && Some(&w.group) == session.active.as_ref()
                    })
                })
                .or_else(|| {
                    session
                        .active
                        .as_ref()
                        .and_then(|id| self.group_focus.get(id))
                        .filter(|focus| {
                            session.windows.iter().any(|w| {
                                Some(&w.group) == session.active.as_ref()
                                    && w.original.identity(focus)
                            })
                        })
                        .map(|focus| &focus.address)
                })
                .unwrap_or(&window.original.address)
                .clone();
            self.compositor.apply(Operation::Focus(address))?;
        }
        Ok(())
    }
    fn publish(&mut self, scene: &Scene) {
        let Some(session) = &self.session else {
            self.recent_groups.clear();
            self.group_focus.clear();
            self.snapshot = Snapshot {
                available: true,
                ..Snapshot::default()
            };
            return;
        };
        let mut groups = BTreeMap::<String, Group>::new();
        for tracked in &session.windows {
            let group = groups
                .entry(tracked.group.clone())
                .or_insert_with(|| Group {
                    id: tracked.group.clone(),
                    ..Group::default()
                });
            if !group.classes.contains(&tracked.original.class) {
                group.classes.push(tracked.original.class.clone());
            }
            let title = scene
                .windows
                .iter()
                .find(|w| tracked.original.identity(w))
                .map(|w| w.title.clone())
                .unwrap_or_else(|| tracked.original.title.clone());
            group.titles.push(title);
            group.windows.push(
                scene
                    .windows
                    .iter()
                    .find(|w| tracked.original.identity(w))
                    .cloned()
                    .unwrap_or_else(|| tracked.original.clone()),
            );
            group.count += 1;
        }
        self.snapshot.enabled = true;
        self.snapshot.output = Some(session.output.clone());
        self.snapshot.workspace = Some(session.home.id);
        self.snapshot.bounds = scene
            .monitors
            .iter()
            .find(|m| m.name == session.output)
            .map(|m| Rect {
                at: [m.x, m.y],
                size: if m.transform % 2 == 0 {
                    [
                        (m.width as f64 / m.scale) as i32,
                        (m.height as f64 / m.scale) as i32,
                    ]
                } else {
                    [
                        (m.height as f64 / m.scale) as i32,
                        (m.width as f64 / m.scale) as i32,
                    ]
                },
            });
        self.snapshot.active = session.active.clone();
        self.snapshot.visible = scene
            .monitors
            .iter()
            .any(|m| m.name == session.output && m.workspace.id == session.home.id);
        self.snapshot.fullscreen = false;
        self.snapshot.occluded = false;
        if let Some(monitor) = scene.monitors.iter().find(|m| m.name == session.output) {
            for tracked in &session.windows {
                if Some(&tracked.group) != session.active.as_ref() {
                    continue;
                }
                if let Some(window) = scene.windows.iter().find(|w| tracked.original.identity(w)) {
                    self.snapshot.fullscreen |= window.workspace.id == session.home.id
                        && (window.fullscreen != 0 || window.fullscreen_client != 0);
                    self.snapshot.occluded |= window.at[0] < monitor.x + 156;
                }
            }
        }
        self.recent_groups.retain(|id| groups.contains_key(id));
        for id in groups.keys() {
            if !self.recent_groups.contains(id) {
                self.recent_groups.push(id.clone());
            }
        }
        if let Some(active) = &session.active {
            self.recent_groups.retain(|id| id != active);
            self.recent_groups.insert(0, active.clone());
        }
        // The remembered front window is also the front of its thumbnail pile.
        for (id, group) in &mut groups {
            if let Some(focus) = self.group_focus.get(id) {
                group.windows.sort_by_key(|window| !window.identity(focus));
            }
        }
        self.snapshot.groups = self
            .recent_groups
            .iter()
            .filter_map(|id| groups.remove(id))
            .collect();
    }
    fn disable(&mut self) -> Result<()> {
        let Some(session) = &self.session else {
            return Ok(());
        };
        self.restore_pending = true;
        validate_session(session)?;
        let scene = self.compositor.scene()?;
        let mut errors = Vec::new();
        for tracked in &session.windows {
            let original = &tracked.original;
            let Some(window) = scene
                .windows
                .iter()
                .find(|window| original.identity(window))
            else {
                continue;
            };
            // An explicit user move out of the managed workspaces is not undone.
            if window.workspace.id != session.home.id && window.workspace.name != session.parking {
                continue;
            }
            let restore = (|| -> Result<()> {
                self.compositor.apply(Operation::Move {
                    address: original.address.clone(),
                    workspace: original.workspace.clone(),
                })?;
                self.compositor.apply(Operation::Fullscreen {
                    address: original.address.clone(),
                    internal: 0,
                    client: 0,
                })?;
                self.compositor.apply(Operation::Float {
                    address: original.address.clone(),
                    enabled: true,
                })?;
                let rect = Rect {
                    at: original.at,
                    size: original.size,
                };
                let rect = if scene.monitors.iter().any(|m| m.id == original.monitor) {
                    rect
                } else if let Some(monitor) = scene
                    .monitors
                    .iter()
                    .find(|m| m.focused)
                    .or_else(|| scene.monitors.first())
                {
                    rect.fit(&monitor.area(false)?)
                } else {
                    rect
                };
                self.compositor.apply(Operation::Place {
                    address: original.address.clone(),
                    rect,
                })?;
                if !original.floating {
                    self.compositor.apply(Operation::Float {
                        address: original.address.clone(),
                        enabled: false,
                    })?;
                }
                self.compositor.apply(Operation::Fullscreen {
                    address: original.address.clone(),
                    internal: original.fullscreen,
                    client: original.fullscreen_client,
                })?;
                Ok(())
            })();
            if let Err(error) = restore {
                errors.push(format!("{}: {error:#}", original.class));
            }
        }
        if !errors.is_empty() {
            bail!(
                "Some windows could not be restored. Try turning Stage Manager off again. {}",
                errors.join("; ")
            );
        }
        if let Some(focus) = &session.original_focus
            && scene.windows.iter().any(|w| {
                &w.address == focus && session.windows.iter().any(|t| t.original.identity(w))
            })
        {
            self.compositor.apply(Operation::Focus(focus.clone()))?;
        }
        self.journal.write(None)?;
        self.session = None;
        self.transition_started = None;
        self.restore_pending = false;
        self.last_saved = None;
        self.publish(&scene);
        Ok(())
    }
}

fn validate_session(session: &Session) -> Result<()> {
    ensure!(
        session.version == 1
            && session.home.id > 0
            && session.parking.starts_with("macstill-stage-"),
        "Invalid Stage Manager recovery record"
    );
    ensure!(
        session
            .parking
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-'),
        "Invalid recovery workspace"
    );
    let mut addresses = HashSet::new();
    for tracked in &session.windows {
        crate::validate_address(&tracked.original.address)?;
        ensure!(
            addresses.insert(&tracked.original.address)
                && tracked.original.workspace.id > 0
                && tracked.original.pid > 0
                && tracked.original.size.iter().all(|v| *v > 0)
                && tracked.original.fullscreen <= 3
                && tracked.original.fullscreen_client <= 3,
            "Invalid saved window state"
        );
    }
    Ok(())
}

pub struct SystemCompositor;
impl Compositor for SystemCompositor {
    fn scene(&mut self) -> Result<Scene> {
        let mut windows: Vec<Window> = serde_json::from_str(&crate::request("j/clients")?)?;
        if let Ok(reply) = crate::request("macstill-stage relationships")
            && let Ok(relations) = serde_json::from_str::<Vec<serde_json::Value>>(&reply)
        {
            for window in &mut windows {
                if let Some(relation) = relations.iter().find(|r| {
                    r["address"].as_str() == Some(&window.address)
                        && r["pid"].as_i64() == Some(window.pid)
                }) {
                    window.parent = relation["parent"].as_str().map(str::to_owned);
                    window.modal = relation["modal"].as_bool().unwrap_or(false);
                }
            }
        }
        let monitors = serde_json::from_str(&crate::request("j/monitors")?)?;
        let active: serde_json::Value = serde_json::from_str(&crate::request("j/activewindow")?)?;
        Ok(Scene {
            windows,
            monitors,
            active: active["address"].as_str().map(str::to_owned),
        })
    }
    fn apply(&mut self, operation: Operation) -> Result<()> {
        match operation {
            Operation::Move { address, workspace } => {
                crate::validate_address(&address)?;
                let destination = if workspace.id > 0 {
                    workspace.id.to_string()
                } else {
                    ensure!(
                        workspace.name.starts_with("macstill-stage-")
                            && workspace
                                .name
                                .bytes()
                                .all(|c| c.is_ascii_alphanumeric() || c == b'-'),
                        "Invalid private workspace"
                    );
                    format!("name:{}", workspace.name)
                };
                crate::dispatch(
                    &format!("movetoworkspacesilent {destination},address:{address}"),
                    &format!(
                        "hl.dsp.window.move({{workspace = \"{destination}\", follow = false, window = \"address:{address}\"}})"
                    ),
                )
            }
            Operation::Float { address, enabled } => {
                crate::validate_address(&address)?;
                crate::dispatch(
                    &format!(
                        "{} address:{address}",
                        if enabled { "setfloating" } else { "settiled" }
                    ),
                    &format!(
                        "hl.dsp.window.float({{action = \"{}\", window = \"address:{address}\"}})",
                        if enabled { "on" } else { "off" }
                    ),
                )
            }
            Operation::Fullscreen {
                address,
                internal,
                client,
            } => {
                crate::validate_address(&address)?;
                ensure!(internal <= 3 && client <= 3, "Invalid fullscreen state");
                let legacy = format!("dispatch fullscreenstate {internal} {client} set");
                crate::dispatch_with(
                    &format!("fullscreenstate {internal} {client} set"),
                    &format!(
                        "hl.dsp.window.fullscreen_state({{internal = {internal}, client = {client}, action = \"set\", window = \"address:{address}\"}})"
                    ),
                    |message| {
                        // Legacy fullscreenstate is focused-window-only. Focus explicitly only after Lua was rejected.
                        if message == legacy {
                            crate::focus(&address)?;
                        }
                        crate::request(message)
                    },
                )
            }
            Operation::Place { address, rect } => {
                crate::validate_address(&address)?;
                ensure!(rect.size.iter().all(|v| *v > 0), "Invalid stage geometry");
                crate::dispatch(
                    &format!(
                        "resizewindowpixel exact {} {},address:{address}",
                        rect.size[0], rect.size[1]
                    ),
                    &format!(
                        "hl.dsp.window.resize({{x = {}, y = {}, relative = false, window = \"address:{address}\"}})",
                        rect.size[0], rect.size[1]
                    ),
                )?;
                crate::dispatch(
                    &format!(
                        "movewindowpixel exact {} {},address:{address}",
                        rect.at[0], rect.at[1]
                    ),
                    &format!(
                        "hl.dsp.window.move({{x = {}, y = {}, relative = false, window = \"address:{address}\"}})",
                        rect.at[0], rect.at[1]
                    ),
                )
            }
            Operation::Focus(address) => crate::focus(&address),
        }
    }
}

pub struct FileJournal(PathBuf);
impl FileJournal {
    pub fn for_session() -> Result<Self> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .ok_or_else(|| anyhow::anyhow!("No user runtime directory"))?;
        let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")?;
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        signature.hash(&mut hash);
        Ok(Self(PathBuf::from(runtime).join(format!(
            "macstill-stage-{:016x}.json",
            hash.finish()
        ))))
    }
}
impl Journal for FileJournal {
    fn read(&self) -> Result<Option<Session>> {
        match fs::symlink_metadata(&self.0) {
            Ok(meta) => {
                ensure!(
                    meta.file_type().is_file() && meta.len() <= 4 * 1024 * 1024,
                    "Invalid Stage Manager recovery file"
                );
                Ok(Some(serde_json::from_slice(&fs::read(&self.0)?)?))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    fn write(&mut self, session: Option<&Session>) -> Result<()> {
        if let Some(session) = session {
            validate_session(session)?;
            let path = self.0.with_extension(format!("{}.tmp", std::process::id()));
            let result = (|| -> Result<()> {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&path)?;
                file.write_all(&serde_json::to_vec(session)?)?;
                file.sync_all()?;
                fs::rename(&path, &self.0)?;
                Ok(())
            })();
            if result.is_err() {
                let _ = fs::remove_file(&path);
            }
            result
        } else {
            match fs::remove_file(&self.0) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.into()),
            }
        }
    }
}

/// Load relationship metadata before any window scheduling, including initial enable.
pub fn connect_native() -> Option<()> {
    if crate::request("macstill-stage ping").ok().as_deref() != Some("macstill-stage-v4") {
        let path = std::env::var_os("MACSTILL_STAGE_PLUGIN")
            .map(PathBuf::from)
            .or_else(|| {
                let exe = std::env::current_exe().ok()?;
                let dir = exe.parent()?;
                [
                    dir.join("libmacstill-stage.so"),
                    dir.join("../lib/macstill/libmacstill-stage.so"),
                ]
                .into_iter()
                .find(|p| p.is_file())
            })?;
        if !path.is_absolute() || !path.is_file() {
            return None;
        }
        let _ = crate::request(&format!("plugin load {}", path.display()));
        if crate::request("macstill-stage ping").ok().as_deref() != Some("macstill-stage-v4") {
            return None;
        }
    }
    Some(())
}
