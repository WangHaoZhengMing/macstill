//! Independent sessions and recovery journals for every connected output.
use super::*;

struct OutputCompositor(String);
impl Compositor for OutputCompositor {
    fn scene(&mut self) -> Result<Scene> {
        let mut scene = SystemCompositor.scene()?;
        for monitor in &mut scene.monitors {
            monitor.focused = monitor.name == self.0;
        }
        Ok(scene)
    }
    fn apply(&mut self, op: Operation) -> Result<()> {
        SystemCompositor.apply(op)
    }
}
pub struct MultiManager {
    managers: BTreeMap<String, Manager<OutputCompositor, FileJournal>>,
    journal: PathBuf,
    enabled: bool,
    pub snapshot: Snapshot,
}
impl MultiManager {
    pub fn new(journal: FileJournal) -> Self {
        Self {
            managers: BTreeMap::new(),
            journal: journal.0,
            enabled: false,
            snapshot: Snapshot {
                available: true,
                ..Default::default()
            },
        }
    }
    pub fn recover(&mut self) -> Result<()> {
        let result = self.recover_sessions();
        self.snapshot.available = result.is_ok();
        if let Err(error) = &result {
            self.snapshot.error = Some(format!("Recovery: {error:#}"));
        }
        result
    }
    fn recover_sessions(&mut self) -> Result<()> {
        // Restore legacy single-output sessions as well.
        Manager::new(SystemCompositor, FileJournal(self.journal.clone())).recover()?;
        if let (Some(parent), Some(stem)) = (self.journal.parent(), self.journal.file_stem()) {
            let prefix = format!("{}.", stem.to_string_lossy());
            for entry in fs::read_dir(parent)? {
                let path = entry?.path();
                if path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with(&prefix))
                    && path.extension().is_some_and(|e| e == "json")
                {
                    Manager::new(SystemCompositor, FileJournal(path)).recover()?;
                }
            }
        }
        self.sync_outputs()
    }
    fn sync_outputs(&mut self) -> Result<()> {
        let scene = SystemCompositor.scene()?;
        for monitor in &scene.monitors {
            if self.managers.contains_key(&monitor.name) {
                continue;
            }
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            monitor.name.hash(&mut hash);
            let journal = FileJournal(
                self.journal
                    .with_extension(format!("{:016x}.json", hash.finish())),
            );
            let mut manager = Manager::new(OutputCompositor(monitor.name.clone()), journal);
            manager.recover()?;
            if self.enabled {
                manager.handle(Command::Enable)?;
                if let Some(address) = &scene.active {
                    SystemCompositor.apply(Operation::Focus(address.clone()))?;
                }
            }
            self.managers.insert(monitor.name.clone(), manager);
        }
        let removed: Vec<_> = self
            .managers
            .keys()
            .filter(|name| !scene.monitors.iter().any(|m| &m.name == *name))
            .cloned()
            .collect();
        for name in removed {
            self.managers
                .get_mut(&name)
                .unwrap()
                .handle(Command::Disable)?;
            self.managers.remove(&name);
        }
        Ok(())
    }
    fn publish(&mut self) {
        let focused = SystemCompositor
            .scene()
            .ok()
            .and_then(|s| s.monitors.into_iter().find(|m| m.focused))
            .map(|m| m.name);
        self.snapshot = focused
            .as_ref()
            .and_then(|n| self.managers.get(n))
            .or_else(|| self.managers.values().next())
            .map(|m| m.snapshot.clone())
            .unwrap_or_default();
        self.snapshot.enabled = self.enabled || self.managers.values().any(|m| m.snapshot.enabled);
        self.snapshot.outputs = self.managers.values().map(|m| m.snapshot.clone()).collect();
    }
    pub fn handle(&mut self, command: Command) -> Result<()> {
        let result = self.dispatch(command);
        let available = self.snapshot.available;
        self.publish();
        self.snapshot.available &= available;
        if let Err(e) = &result {
            self.snapshot.error = Some(format!("{e:#}"));
        }
        result
    }
    fn dispatch(&mut self, command: Command) -> Result<()> {
        if !self.snapshot.available {
            self.recover()?;
        }
        if let Command::OnOutput(name, command) = command {
            return self
                .managers
                .get_mut(&name)
                .ok_or_else(|| anyhow::anyhow!("Output disconnected"))?
                .handle(*command);
        }
        let plain = match &command {
            Command::Animate(c) => c.as_ref(),
            c => c,
        };
        if matches!(plain, Command::Enable | Command::Disable | Command::Toggle) {
            let enable = match plain {
                Command::Enable => true,
                Command::Disable => false,
                _ => !self.snapshot.enabled,
            };
            let original_focus = SystemCompositor.scene()?.active;
            self.enabled = false;
            self.sync_outputs()?;
            self.enabled = enable;
            let mut errors = Vec::new();
            for manager in self.managers.values_mut() {
                // Starting all outputs must not steal focus between monitors.
                let action = if enable {
                    Command::Enable
                } else {
                    Command::Disable
                };
                let action = if matches!(&command, Command::Animate(_)) {
                    Command::Animate(Box::new(action))
                } else {
                    action
                };
                if let Err(e) = manager.handle(action) {
                    errors.push(e.to_string());
                }
            }
            if let Some(address) = original_focus {
                let _ = SystemCompositor.apply(Operation::Focus(address));
            }
            ensure!(errors.is_empty(), "{}", errors.join("; "));
            return Ok(());
        }
        let scene = SystemCompositor.scene()?;
        if let Some(manager) = scene
            .monitors
            .iter()
            .find(|m| m.focused)
            .and_then(|m| self.managers.get_mut(&m.name))
        {
            manager.handle(command)?;
        }
        Ok(())
    }
    pub fn tick(&mut self) -> Result<()> {
        let result = (|| {
            self.sync_outputs()?;
            for manager in self.managers.values_mut() {
                if manager.snapshot.enabled {
                    manager.tick()?;
                }
            }
            Ok(())
        })();
        self.publish();
        result
    }
}
