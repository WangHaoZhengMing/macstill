use super::*;
use std::{cell::RefCell, os::unix::fs::PermissionsExt, rc::Rc};

#[derive(Clone)]
struct Fake {
    scene: Rc<RefCell<Scene>>,
    operations: Rc<RefCell<Vec<Operation>>>,
    fail_once: Rc<RefCell<Option<usize>>>,
    fail_restore: Rc<RefCell<bool>>,
}
impl Compositor for Fake {
    fn scene(&mut self) -> Result<Scene> {
        Ok(self.scene.borrow().clone())
    }
    fn apply(&mut self, operation: Operation) -> Result<()> {
        let index = self.operations.borrow().len();
        self.operations.borrow_mut().push(operation.clone());
        if *self.fail_once.borrow() == Some(index) {
            self.fail_once.replace(None);
            bail!("isolated dispatch failure");
        }
        if *self.fail_restore.borrow() && matches!(&operation, Operation::Place { .. }) {
            bail!("isolated placement failure");
        }
        let mut scene = self.scene.borrow_mut();
        if let Operation::Focus(address) = &operation {
            scene.active = Some(address.clone());
            if let Some(window) = scene
                .windows
                .iter()
                .find(|w| &w.address == address)
                .cloned()
                && let Some(monitor) = scene.monitors.iter_mut().find(|m| m.id == window.monitor)
            {
                monitor.workspace = window.workspace;
            }
            return Ok(());
        }
        let address = match &operation {
            Operation::Move { address, .. }
            | Operation::Place { address, .. }
            | Operation::Float { address, .. }
            | Operation::Fullscreen { address, .. } => address,
            _ => unreachable!(),
        };
        let window = scene
            .windows
            .iter_mut()
            .find(|w| &w.address == address)
            .ok_or_else(|| anyhow::anyhow!("closed"))?;
        match operation {
            Operation::Move { mut workspace, .. } => {
                if workspace.id == 0 {
                    workspace.id = 900;
                }
                window.workspace = workspace;
            }
            Operation::Float { enabled, .. } => window.floating = enabled,
            Operation::Fullscreen {
                internal, client, ..
            } => {
                window.fullscreen = internal;
                window.fullscreen_client = client;
            }
            Operation::Place { rect, .. } => {
                window.at = rect.at;
                window.size = rect.size;
            }
            _ => unreachable!(),
        }
        Ok(())
    }
}
#[derive(Clone, Default)]
struct MemoryJournal {
    session: Rc<RefCell<Option<Session>>>,
    fail: Rc<RefCell<bool>>,
}
impl Journal for MemoryJournal {
    fn read(&self) -> Result<Option<Session>> {
        Ok(self.session.borrow().clone())
    }
    fn write(&mut self, session: Option<&Session>) -> Result<()> {
        ensure!(!*self.fail.borrow(), "journal unavailable");
        self.session.replace(session.cloned());
        Ok(())
    }
}
fn window(address: &str, class: &str, workspace: i64) -> Window {
    Window {
        parent: None,
        modal: false,
        address: address.into(),
        pid: 100 + i64::from_str_radix(address.trim_start_matches("0x"), 16).unwrap(),
        class: class.into(),
        title: format!("{class} document"),
        workspace: Workspace {
            id: workspace,
            name: workspace.to_string(),
        },
        monitor: 0,
        at: [25, 80],
        size: [800, 600],
        floating: true,
        fullscreen: 0,
        fullscreen_client: 0,
        pinned: false,
        hidden: false,
        mapped: true,
        grouped: vec![],
    }
}
fn fixture() -> (Manager<Fake, MemoryJournal>, Fake, MemoryJournal) {
    let fake = Fake {
        scene: Rc::new(RefCell::new(Scene {
            windows: vec![
                window("0x1", "Editor", 1),
                window("0x2", "Browser", 1),
                window("0x3", "Browser", 1),
                window("0x4", "Other workspace", 2),
            ],
            monitors: vec![Monitor {
                id: 0,
                name: "DP-1".into(),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                scale: 1.,
                transform: 0,
                reserved: [0, 30, 0, 70],
                focused: true,
                workspace: Workspace {
                    id: 1,
                    name: "1".into(),
                },
            }],
            active: Some("0x1".into()),
        })),
        operations: Default::default(),
        fail_once: Default::default(),
        fail_restore: Default::default(),
    };
    let journal = MemoryJournal::default();
    (Manager::new(fake.clone(), journal.clone()), fake, journal)
}

#[test]
fn separating_a_background_group_preserves_other_groups_and_active_focus() {
    let (mut manager, fake, _) = fixture();
    fake.scene
        .borrow_mut()
        .windows
        .push(window("0x5", "Terminal", 1));
    fake.scene
        .borrow_mut()
        .windows
        .push(window("0x6", "Notes", 1));
    manager.handle(Command::Enable).unwrap();
    manager.handle(Command::Merge("browser".into())).unwrap();
    manager
        .handle(Command::Activate("terminal".into()))
        .unwrap();
    manager.handle(Command::Merge("notes".into())).unwrap();
    manager.handle(Command::Separate("editor".into())).unwrap();
    assert_eq!(manager.snapshot.active.as_deref(), Some("terminal"));
    assert_eq!(
        manager
            .snapshot
            .groups
            .iter()
            .find(|g| g.id == "terminal")
            .unwrap()
            .count,
        2
    );
    assert_eq!(
        manager
            .snapshot
            .groups
            .iter()
            .find(|g| g.id == "browser")
            .unwrap()
            .count,
        2
    );
    assert_eq!(fake.scene.borrow().active.as_deref(), Some("0x5"));
}

#[test]
fn user_geometry_and_fullscreen_are_not_undone_by_polling() {
    let (mut manager, fake, _) = fixture();
    manager.handle(Command::Enable).unwrap();
    fake.scene.borrow_mut().windows[0].at = [20, 90];
    fake.scene.borrow_mut().windows[0].size = [1200, 850];
    manager.tick().unwrap();
    assert_eq!(fake.scene.borrow().windows[0].at, [20, 90]);
    assert!(manager.snapshot.occluded);
    manager.handle(Command::Activate("browser".into())).unwrap();
    manager.handle(Command::Activate("editor".into())).unwrap();
    assert_eq!(fake.scene.borrow().windows[0].at, [20, 90]);
    fake.scene.borrow_mut().windows[0].fullscreen = 2;
    fake.scene.borrow_mut().windows[0].fullscreen_client = 2;
    fake.operations.borrow_mut().clear();
    manager.tick().unwrap();
    assert!(manager.snapshot.fullscreen);
    assert!(fake.operations.borrow().is_empty());
}

#[test]
fn desktop_reveal_stays_empty_until_return_or_explicit_activation() {
    let (mut manager, fake, journal) = fixture();
    let original = fake.scene.borrow().windows.clone();
    manager.handle(Command::Enable).unwrap();
    manager.handle(Command::ShowDesktop).unwrap();
    assert!(manager.snapshot.desktop);
    assert!(manager.snapshot.active.is_none());
    for _ in 0..3 {
        manager.tick().unwrap();
    }
    assert!(manager.snapshot.desktop && manager.snapshot.active.is_none());
    assert!(
        fake.scene.borrow().windows[0].workspace.name.starts_with("macstill-stage-")
    );
    manager.handle(Command::ShowDesktop).unwrap();
    assert!(!manager.snapshot.desktop);
    assert_eq!(manager.snapshot.active.as_deref(), Some("editor"));
    manager.handle(Command::ShowDesktop).unwrap();
    manager.handle(Command::Merge("browser".into())).unwrap();
    assert!(!manager.snapshot.desktop);
    assert_eq!(manager.snapshot.active.as_deref(), Some("browser"));
    manager.handle(Command::Disable).unwrap();
    assert_eq!(fake.scene.borrow().windows, original);
    assert!(journal.session.borrow().is_none());
}

#[test]
fn recent_visibility_does_not_change_membership_or_focus() {
    let (mut manager, fake, _) = fixture();
    manager.handle(Command::Enable).unwrap();
    let groups = manager.snapshot.groups.clone();
    fake.operations.borrow_mut().clear();
    manager.handle(Command::SetRecentHidden(true)).unwrap();
    assert!(manager.snapshot.recent_hidden);
    assert_eq!(manager.snapshot.active.as_deref(), Some("editor"));
    assert_eq!(
        manager
            .snapshot
            .groups
            .iter()
            .map(|g| &g.id)
            .collect::<Vec<_>>(),
        groups.iter().map(|g| &g.id).collect::<Vec<_>>()
    );
    assert!(
        !fake
            .operations
            .borrow()
            .iter()
            .any(|o| matches!(o, Operation::Move { .. } | Operation::Place { .. }))
    );
}

#[test]
fn animated_switch_focuses_incoming_clients_before_completion_and_ignores_stale_callbacks() {
    let (mut manager, fake, _) = fixture();
    manager.handle(Command::Enable).unwrap();
    manager
        .handle(Command::Animate(Box::new(Command::Activate(
            "browser".into(),
        ))))
        .unwrap();
    let first = manager.snapshot.transition.clone().unwrap();
    assert_eq!(first.from.len(), 1);
    assert_eq!(first.to.len(), 2);
    assert_eq!(fake.scene.borrow().windows[0].workspace.id, 1);
    manager
        .handle(Command::PresentTransition(first.generation))
        .unwrap();
    assert!(
        fake.scene.borrow().windows[..3]
            .iter()
            .all(|w| w.workspace.name.starts_with("macstill-stage-"))
    );
    assert!(fake.scene.borrow().windows[1..3].iter().all(|w| w.workspace.id == 1));
    assert!(matches!(fake.scene.borrow().active.as_deref(), Some("0x2" | "0x3")));
    manager.tick().unwrap();
    assert_eq!(manager.snapshot.active.as_deref(), Some("browser"));
    manager
        .handle(Command::Animate(Box::new(Command::Activate(
            "editor".into(),
        ))))
        .unwrap();
    let second = manager.snapshot.transition.as_ref().unwrap().generation;
    assert!(second > first.generation);
    manager
        .handle(Command::FinishTransition(first.generation))
        .unwrap();
    assert!(manager.snapshot.transition.is_some());
    manager.handle(Command::FinishTransition(second)).unwrap();
    assert!(manager.snapshot.transition.is_none());
    assert_eq!(fake.scene.borrow().windows[0].workspace.id, 1);
    assert_eq!(fake.scene.borrow().active.as_deref(), Some("0x1"));
}

#[test]
fn animation_timeout_and_renderer_crash_restore_native_windows() {
    let (mut manager, fake, journal) = fixture();
    let original = fake.scene.borrow().windows.clone();
    manager.handle(Command::Enable).unwrap();
    manager
        .handle(Command::Animate(Box::new(Command::Next)))
        .unwrap();
    manager.transition_started =
        Some(std::time::Instant::now() - std::time::Duration::from_secs(3));
    manager.tick().unwrap();
    assert!(manager.snapshot.transition.is_none());
    assert!(
        fake.scene
            .borrow()
            .windows
            .iter()
            .any(|w| w.class == "Browser" && w.workspace.id == 1)
    );
    manager
        .handle(Command::Animate(Box::new(Command::Next)))
        .unwrap();
    let mut restarted = Manager::new(fake.clone(), journal);
    restarted.recover().unwrap();
    assert_eq!(fake.scene.borrow().windows, original);
}

#[test]
fn leaving_the_workspace_during_animation_finishes_without_stealing_external_focus() {
    let (mut manager, fake, _) = fixture();
    manager.handle(Command::Enable).unwrap();
    manager
        .handle(Command::Animate(Box::new(Command::Next)))
        .unwrap();
    fake.scene.borrow_mut().monitors[0].workspace = Workspace {
        id: 2,
        name: "2".into(),
    };
    fake.scene.borrow_mut().active = Some("0x4".into());
    manager.tick().unwrap();
    assert!(manager.snapshot.transition.is_none() && !manager.snapshot.visible);
    assert_eq!(fake.scene.borrow().active.as_deref(), Some("0x4"));
}

#[test]
fn closing_the_incoming_group_during_animation_immediately_returns_to_a_surviving_group() {
    let (mut manager, fake, _) = fixture();
    manager.handle(Command::Enable).unwrap();
    manager
        .handle(Command::Animate(Box::new(Command::Next)))
        .unwrap();
    let generation = manager.snapshot.transition.as_ref().unwrap().generation;
    manager
        .handle(Command::PresentTransition(generation))
        .unwrap();
    fake.scene
        .borrow_mut()
        .windows
        .retain(|w| w.class != "Browser");
    fake.scene.borrow_mut().active = None;
    manager
        .handle(Command::FinishTransition(generation))
        .unwrap();
    assert_eq!(manager.snapshot.active.as_deref(), Some("editor"));
    assert_eq!(fake.scene.borrow().windows[0].workspace.id, 1);
}

#[test]
fn recent_groups_follow_activation_and_closed_active_group_returns_to_previous_group() {
    let (mut manager, fake, _) = fixture();
    fake.scene
        .borrow_mut()
        .windows
        .push(window("0x5", "Terminal", 1));
    manager.handle(Command::Enable).unwrap();
    manager
        .handle(Command::Activate("terminal".into()))
        .unwrap();
    manager.handle(Command::Activate("browser".into())).unwrap();
    manager.handle(Command::Activate("editor".into())).unwrap();
    assert_eq!(
        manager
            .snapshot
            .groups
            .iter()
            .map(|g| g.id.as_str())
            .collect::<Vec<_>>(),
        ["editor", "browser", "terminal"]
    );
    // Polling unchanged inventory does not disturb recent order.
    manager.tick().unwrap();
    assert_eq!(manager.snapshot.groups[1].id, "browser");
    fake.scene
        .borrow_mut()
        .windows
        .retain(|w| w.class != "Editor");
    fake.scene.borrow_mut().active = None;
    manager.tick().unwrap();
    assert_eq!(manager.snapshot.active.as_deref(), Some("browser"));
    assert_eq!(manager.snapshot.groups.len(), 2);
    manager.handle(Command::Merge("terminal".into())).unwrap();
    assert_eq!(manager.snapshot.groups.len(), 1);
    manager.handle(Command::Ungroup).unwrap();
    assert_eq!(manager.snapshot.groups.len(), 2);
}

#[test]
fn returning_to_a_group_restores_its_last_focused_window_and_user_geometry() {
    let (mut manager, fake, _) = fixture();
    manager.handle(Command::Enable).unwrap();
    manager.handle(Command::Activate("browser".into())).unwrap();
    fake.scene.borrow_mut().active = Some("0x3".into());
    fake.scene.borrow_mut().windows[2].at = [350, 180];
    manager.tick().unwrap();
    manager.handle(Command::Activate("editor".into())).unwrap();
    manager.handle(Command::Activate("browser".into())).unwrap();
    assert_eq!(fake.scene.borrow().active.as_deref(), Some("0x3"));
    assert_eq!(fake.scene.borrow().windows[2].at, [350, 180]);
    manager.handle(Command::Activate("editor".into())).unwrap();
    // A new client reusing the same address must not inherit old focus history.
    fake.scene.borrow_mut().windows[2].pid += 1000;
    manager.handle(Command::Activate("browser".into())).unwrap();
    assert_eq!(fake.scene.borrow().active.as_deref(), Some("0x2"));
}

#[test]
fn enable_groups_app_windows_parks_only_the_current_workspace_and_restores_original_layout() {
    let (mut manager, fake, journal) = fixture();
    let original = fake.scene.borrow().windows.clone();
    manager.handle(Command::Enable).unwrap();
    assert!(manager.snapshot.enabled && manager.snapshot.visible);
    assert_eq!(
        manager
            .snapshot
            .groups
            .iter()
            .find(|g| g.id == "browser")
            .unwrap()
            .count,
        2
    );
    assert!(
        fake.scene.borrow().windows[1]
            .workspace
            .name
            .starts_with("macstill-stage-")
    );
    assert!(fake.scene.borrow().windows[0].at[0] >= 156);
    assert_eq!(fake.scene.borrow().windows[3], original[3]);
    assert!(journal.session.borrow().is_some());
    manager.handle(Command::Activate("browser".into())).unwrap();
    assert_eq!(fake.scene.borrow().windows[1].workspace.id, 1);
    assert_eq!(fake.scene.borrow().windows[2].workspace.id, 1);
    assert_eq!(manager.snapshot.active.as_deref(), Some("browser"));
    manager.handle(Command::Merge("editor".into())).unwrap();
    assert_eq!(manager.snapshot.groups.len(), 1);
    assert_eq!(fake.scene.borrow().windows[0].workspace.id, 1);
    manager.handle(Command::Ungroup).unwrap();
    assert_eq!(manager.snapshot.groups.len(), 2);
    manager.handle(Command::Disable).unwrap();
    assert_eq!(fake.scene.borrow().windows, original);
    assert_eq!(fake.scene.borrow().active.as_deref(), Some("0x1"));
    assert!(!manager.snapshot.enabled);
    assert!(journal.session.borrow().is_none());
}

#[test]
fn journal_failure_precedes_all_window_mutations_and_dispatch_failure_rolls_back() {
    let (mut manager, fake, journal) = fixture();
    journal.fail.replace(true);
    assert!(manager.handle(Command::Enable).is_err());
    assert!(fake.operations.borrow().is_empty());
    journal.fail.replace(false);
    fake.fail_once.replace(Some(1));
    let original = fake.scene.borrow().windows.clone();
    assert!(manager.handle(Command::Enable).is_err());
    assert_eq!(fake.scene.borrow().windows, original);
    assert!(!manager.snapshot.enabled);
}

#[test]
fn failed_restore_keeps_the_journal_and_restart_recovers_without_touching_reused_addresses() {
    let (mut manager, fake, journal) = fixture();
    let original = fake.scene.borrow().windows.clone();
    manager.handle(Command::Enable).unwrap();
    fake.fail_restore.replace(true);
    assert!(manager.handle(Command::Disable).is_err());
    assert!(journal.session.borrow().is_some());
    let count = fake.operations.borrow().len();
    manager.tick().unwrap();
    assert_eq!(fake.operations.borrow().len(), count);
    assert!(manager.handle(Command::Activate("browser".into())).is_err());
    fake.fail_restore.replace(false);
    fake.scene.borrow_mut().windows[2].pid = 999;
    let replaced = fake.scene.borrow().windows[2].clone();
    let mut restarted = Manager::new(fake.clone(), journal.clone());
    restarted.recover().unwrap();
    assert_eq!(fake.scene.borrow().windows[0], original[0]);
    assert_eq!(fake.scene.borrow().windows[1], original[1]);
    assert_eq!(fake.scene.borrow().windows[2], replaced);
    assert!(journal.session.borrow().is_none());
}

#[test]
fn new_windows_join_stages_closed_windows_disappear_and_explicit_workspace_moves_are_respected() {
    let (mut manager, fake, _) = fixture();
    manager.handle(Command::Enable).unwrap();
    fake.scene
        .borrow_mut()
        .windows
        .push(window("0x5", "Terminal", 1));
    fake.scene.borrow_mut().active = Some("0x5".into());
    manager.tick().unwrap();
    assert_eq!(manager.snapshot.active.as_deref(), Some("terminal"));
    assert_eq!(manager.snapshot.groups.len(), 3);
    fake.scene.borrow_mut().windows[4].workspace = Workspace {
        id: 3,
        name: "3".into(),
    };
    fake.scene
        .borrow_mut()
        .windows
        .retain(|w| w.address != "0x2");
    manager.tick().unwrap();
    assert_eq!(manager.snapshot.groups.len(), 2);
    manager.handle(Command::Disable).unwrap();
    assert_eq!(
        fake.scene
            .borrow()
            .windows
            .iter()
            .find(|w| w.address == "0x5")
            .unwrap()
            .workspace
            .id,
        3
    );
}

#[test]
fn fullscreen_and_tiling_modes_are_restored_and_small_outputs_fail_without_changes() {
    let (mut manager, fake, _) = fixture();
    fake.scene.borrow_mut().windows[0].floating = false;
    fake.scene.borrow_mut().windows[0].fullscreen = 2;
    fake.scene.borrow_mut().windows[0].fullscreen_client = 2;
    let original = fake.scene.borrow().windows[0].clone();
    manager.handle(Command::Enable).unwrap();
    assert_eq!(fake.scene.borrow().windows[0].fullscreen, 0);
    assert!(fake.scene.borrow().windows[0].floating);
    manager.handle(Command::Disable).unwrap();
    assert_eq!(fake.scene.borrow().windows[0], original);
    fake.operations.borrow_mut().clear();
    fake.scene.borrow_mut().monitors[0].width = 300;
    assert!(manager.handle(Command::Enable).is_err());
    assert!(fake.operations.borrow().is_empty());
}

#[test]
fn private_recovery_file_is_atomic_and_rejects_corrupt_or_symlink_records() {
    let (mut manager, _, journal) = fixture();
    manager.handle(Command::Enable).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stage.json");
    let mut file = FileJournal(path.clone());
    file.write(journal.session.borrow().as_ref()).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(file.read().unwrap().is_some());
    file.write(None).unwrap();
    assert!(!path.exists());
    let target = dir.path().join("other");
    fs::write(&target, "keep").unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    assert!(file.read().is_err());
    assert_eq!(fs::read_to_string(target).unwrap(), "keep");
}

#[test]
fn newly_opened_windows_preserve_merged_application_groups() {
    let (mut manager, fake, _) = fixture();
    manager.handle(Command::Enable).unwrap();
    manager.handle(Command::Merge("browser".into())).unwrap();
    fake.scene
        .borrow_mut()
        .windows
        .push(window("0x5", "Browser", 1));
    fake.scene.borrow_mut().active = Some("0x5".into());
    manager.tick().unwrap();
    assert_eq!(manager.snapshot.groups.len(), 1);
    assert_eq!(manager.snapshot.groups[0].count, 4);
    assert_eq!(manager.snapshot.active.as_deref(), Some("editor"));
}

#[test]
fn external_workspaces_and_pinned_windows_do_not_lose_focus_and_dock_activation_returns_home() {
    let (mut manager, mut fake, _) = fixture();
    manager.handle(Command::Enable).unwrap();
    fake.scene.borrow_mut().monitors[0].workspace = Workspace {
        id: 2,
        name: "2".into(),
    };
    fake.scene.borrow_mut().active = Some("0x4".into());
    fake.scene
        .borrow_mut()
        .windows
        .retain(|w| w.address != "0x1");
    fake.operations.borrow_mut().clear();
    manager.tick().unwrap();
    assert!(!manager.snapshot.visible);
    assert!(
        fake.operations
            .borrow()
            .iter()
            .all(|op| !matches!(op, Operation::Focus(_)))
    );
    assert_eq!(fake.scene.borrow().active.as_deref(), Some("0x4"));
    fake.apply(Operation::Focus("0x2".into())).unwrap();
    manager.tick().unwrap();
    manager.tick().unwrap();
    assert!(manager.snapshot.visible);
    assert_eq!(
        fake.scene
            .borrow()
            .windows
            .iter()
            .find(|w| w.address == "0x2")
            .unwrap()
            .workspace
            .id,
        1
    );
    fake.scene
        .borrow_mut()
        .windows
        .iter_mut()
        .find(|w| w.address == "0x2")
        .unwrap()
        .pinned = true;
    fake.scene.borrow_mut().active = Some("0x2".into());
    fake.operations.borrow_mut().clear();
    manager.tick().unwrap();
    assert_eq!(fake.scene.borrow().active.as_deref(), Some("0x2"));
    assert!(
        fake.operations
            .borrow()
            .iter()
            .all(|op| !matches!(op, Operation::Focus(_)))
    );
}

#[test]
fn output_removal_restores_windows_inside_a_surviving_output() {
    let (mut manager, fake, journal) = fixture();
    manager.handle(Command::Enable).unwrap();
    fake.scene.borrow_mut().monitors[0].name = "DP-2".into();
    fake.scene.borrow_mut().monitors[0].id = 2;
    fake.scene.borrow_mut().monitors[0].x = 1920;
    manager.tick().unwrap();
    assert!(!manager.snapshot.enabled);
    assert!(journal.session.borrow().is_none());
    for window in fake
        .scene
        .borrow()
        .windows
        .iter()
        .filter(|w| w.workspace.id == 1)
    {
        assert!(window.at[0] >= 1920);
        assert!(window.at[0] + window.size[0] <= 3840);
    }
}

#[test]
fn lifecycle_animation_waits_for_presentation_then_restores_originals() {
    let (mut manager, fake, journal) = fixture();
    let original = fake.scene.borrow().windows.clone();
    manager
        .handle(Command::Animate(Box::new(Command::Enable)))
        .unwrap();
    let opening = manager.snapshot.transition.clone().unwrap();
    assert_eq!(opening.lifecycle, Some(true));
    assert_eq!(
        fake.scene.borrow().windows,
        original,
        "Do not park before compositor accepts presentation"
    );
    assert!(journal.session.borrow().is_some());
    manager
        .handle(Command::PresentTransition(opening.generation))
        .unwrap();
    manager
        .handle(Command::FinishTransition(opening.generation))
        .unwrap();
    assert!(manager.snapshot.enabled);
    assert!(manager.snapshot.transition.is_none());
    manager
        .handle(Command::Animate(Box::new(Command::Disable)))
        .unwrap();
    let closing = manager.snapshot.transition.clone().unwrap();
    assert_eq!(closing.lifecycle, Some(false));
    assert!(manager.snapshot.enabled, "Keep rendering throughout exit");
    assert!(
        journal.session.borrow().is_some(),
        "Keep recovery journal until restored"
    );
    manager
        .handle(Command::PresentTransition(closing.generation))
        .unwrap();
    manager
        .handle(Command::FinishTransition(opening.generation))
        .unwrap();
    assert!(
        manager.snapshot.enabled,
        "Ignore stale animation completion"
    );
    manager
        .handle(Command::FinishTransition(closing.generation))
        .unwrap();
    assert!(!manager.snapshot.enabled);
    assert_eq!(fake.scene.borrow().windows, original);
    assert!(journal.session.borrow().is_none());
}

#[test]
fn interrupted_enable_can_exit_and_exit_timeout_restores_windows() {
    let (mut manager, fake, _) = fixture();
    let original = fake.scene.borrow().windows.clone();
    manager
        .handle(Command::Animate(Box::new(Command::Enable)))
        .unwrap();
    let generation = manager.snapshot.transition.as_ref().unwrap().generation;
    manager
        .handle(Command::PresentTransition(generation))
        .unwrap();
    manager
        .handle(Command::Animate(Box::new(Command::Disable)))
        .unwrap();
    manager.transition_started =
        Some(std::time::Instant::now() - std::time::Duration::from_secs(3));
    manager.tick().unwrap();
    assert!(!manager.snapshot.enabled);
    assert_eq!(fake.scene.borrow().windows, original);
}

#[test]
fn two_outputs_keep_groups_parking_and_restoration_independent() {
    let (mut first, fake, _) = fixture();
    {
        let mut scene = fake.scene.borrow_mut();
        let mut monitor = scene.monitors[0].clone();
        monitor.id = 1;
        monitor.name = "DP-2".into();
        monitor.x = 1920;
        monitor.workspace = Workspace {
            id: 3,
            name: "3".into(),
        };
        monitor.focused = false;
        scene.monitors.push(monitor);
        for (address, class) in [("0x5", "Editor"), ("0x6", "Browser")] {
            let mut window = window(address, class, 3);
            window.monitor = 1;
            window.at[0] += 1920;
            scene.windows.push(window);
        }
    }
    first.handle(Command::Enable).unwrap();
    {
        let mut scene = fake.scene.borrow_mut();
        scene.monitors[0].focused = false;
        scene.monitors[1].focused = true;
        scene.active = Some("0x5".into());
    }
    let mut second = Manager::new(fake.clone(), MemoryJournal::default());
    second.handle(Command::Enable).unwrap();
    assert_ne!(
        first.session.as_ref().unwrap().parking,
        second.session.as_ref().unwrap().parking
    );
    second.handle(Command::Activate("browser".into())).unwrap();
    first.tick().unwrap();
    assert_eq!(first.snapshot.active.as_deref(), Some("editor"));
    assert_eq!(second.snapshot.active.as_deref(), Some("browser"));
    assert_eq!(
        first.snapshot.groups.iter().map(|g| g.count).sum::<usize>(),
        3
    );
    assert_eq!(
        second
            .snapshot
            .groups
            .iter()
            .map(|g| g.count)
            .sum::<usize>(),
        2
    );
    first.handle(Command::Disable).unwrap();
    second.tick().unwrap();
    assert!(second.snapshot.enabled);
    let scene = fake.scene.borrow();
    assert_eq!(
        scene
            .windows
            .iter()
            .find(|w| w.address == "0x6")
            .unwrap()
            .workspace
            .id,
        3
    );
    assert_eq!(
        scene
            .windows
            .iter()
            .find(|w| w.address == "0x5")
            .unwrap()
            .workspace
            .name,
        second.session.as_ref().unwrap().parking
    );
}

#[test]
fn new_main_window_is_activated_even_before_compositor_focus_changes() {
    let (mut manager, fake, _) = fixture();
    manager.handle(Command::Enable).unwrap();
    fake.scene
        .borrow_mut()
        .windows
        .push(window("0x5", "dehelper", 1));
    // Focus still belongs to Editor when the window-open event reaches the shell.
    manager.tick().unwrap();
    assert_eq!(manager.snapshot.active.as_deref(), Some("dehelper"));
    assert_eq!(fake.scene.borrow().active.as_deref(), Some("0x5"));
    assert_eq!(
        fake.scene
            .borrow()
            .windows
            .iter()
            .find(|w| w.address == "0x5")
            .unwrap()
            .workspace
            .id,
        1
    );
}

#[test]
fn transient_file_choosers_and_modal_dialogs_are_never_scheduled() {
    let (mut manager, fake, _) = fixture();
    let mut chooser = window("0x5", "org.gnome.Nautilus", 1);
    chooser.parent = Some("0x1".into());
    fake.scene.borrow_mut().windows.push(chooser.clone());
    manager.handle(Command::Enable).unwrap();
    let mut modal = window("0x6", "SaveAs", 1);
    modal.modal = true;
    fake.scene.borrow_mut().windows.push(modal.clone());
    fake.scene.borrow_mut().active = Some("0x6".into());
    fake.operations.borrow_mut().clear();
    manager.tick().unwrap();
    assert_eq!(manager.snapshot.active.as_deref(), Some("editor"));
    assert_eq!(manager.snapshot.groups.len(), 2);
    assert!(fake.operations.borrow().is_empty());
    let scene = fake.scene.borrow();
    assert_eq!(
        scene.windows.iter().find(|w| w.address == "0x5"),
        Some(&chooser)
    );
    assert_eq!(
        scene.windows.iter().find(|w| w.address == "0x6"),
        Some(&modal)
    );
    assert_eq!(scene.active.as_deref(), Some("0x6"));
}

#[test]
fn newly_launched_app_interrupts_an_old_switch_animation() {
    let (mut manager, fake, _) = fixture();
    manager.handle(Command::Enable).unwrap();
    manager
        .handle(Command::Animate(Box::new(Command::Next)))
        .unwrap();
    fake.scene
        .borrow_mut()
        .windows
        .push(window("0x5", "dehelper", 1));
    manager.tick().unwrap();
    assert!(manager.snapshot.transition.is_none());
    assert_eq!(manager.snapshot.active.as_deref(), Some("dehelper"));
    assert_eq!(fake.scene.borrow().active.as_deref(), Some("0x5"));
}

#[test]
fn new_app_with_focused_child_keeps_the_child_in_front() {
    let (mut manager, fake, _) = fixture();
    manager.handle(Command::Enable).unwrap();
    let main = window("0x5", "NewEditor", 1);
    let mut chooser = window("0x6", "Portal", 1);
    chooser.parent = Some(main.address.clone());
    fake.scene.borrow_mut().windows.extend([main, chooser]);
    fake.scene.borrow_mut().active = Some("0x6".into());
    manager.tick().unwrap();
    assert_eq!(manager.snapshot.active.as_deref(), Some("neweditor"));
    assert_eq!(fake.scene.borrow().active.as_deref(), Some("0x6"));
}

#[test]
fn enabling_with_a_focused_chooser_keeps_its_parent_group_visible() {
    let (mut manager, fake, _) = fixture();
    let mut chooser = window("0x5", "Portal", 1);
    chooser.parent = Some("0x2".into());
    fake.scene.borrow_mut().windows.push(chooser);
    fake.scene.borrow_mut().active = Some("0x5".into());
    manager.handle(Command::Enable).unwrap();
    assert_eq!(manager.snapshot.active.as_deref(), Some("browser"));
    assert_eq!(fake.scene.borrow().active.as_deref(), Some("0x5"));
}

#[test]
fn desktop_hide_and_restore_wait_for_animation_and_preserve_geometry() {
    let (mut manager, fake, _) = fixture();
    manager.handle(Command::Enable).unwrap();
    let original = fake.scene.borrow().windows[0].clone();
    manager
        .handle(Command::Animate(Box::new(Command::ShowDesktop)))
        .unwrap();
    let hide = manager.snapshot.transition.clone().unwrap();
    assert!(manager.snapshot.desktop);
    assert_eq!(hide.from.len(), 1);
    assert!(hide.to.is_empty());
    assert_eq!(fake.scene.borrow().windows[0].workspace.id, 1);
    manager
        .handle(Command::PresentTransition(hide.generation))
        .unwrap();
    assert_eq!(
        fake.scene.borrow().windows[0].workspace.name,
        manager.session.as_ref().unwrap().parking
    );
    manager
        .handle(Command::FinishTransition(hide.generation))
        .unwrap();
    assert!(manager.snapshot.desktop);
    manager
        .handle(Command::Animate(Box::new(Command::ShowDesktop)))
        .unwrap();
    let restore = manager.snapshot.transition.clone().unwrap();
    assert!(restore.from.is_empty());
    assert_eq!(restore.to.len(), 1);
    assert_eq!(restore.to[0].rect.at, original.at);
    assert_eq!(restore.to[0].rect.size, original.size);
    manager
        .handle(Command::PresentTransition(restore.generation))
        .unwrap();
    manager
        .handle(Command::FinishTransition(restore.generation))
        .unwrap();
    assert!(!manager.snapshot.desktop);
    assert_eq!(manager.snapshot.active.as_deref(), Some("editor"));
    assert_eq!(fake.scene.borrow().windows[0].workspace.id, 1);
}

#[test]
fn desktop_animation_can_reverse_before_finishing() {
    let (mut manager, _, _) = fixture();
    manager.handle(Command::Enable).unwrap();
    manager
        .handle(Command::Animate(Box::new(Command::ShowDesktop)))
        .unwrap();
    let old = manager.snapshot.transition.as_ref().unwrap().generation;
    manager.handle(Command::PresentTransition(old)).unwrap();
    manager
        .handle(Command::Animate(Box::new(Command::ShowDesktop)))
        .unwrap();
    let restore = manager.snapshot.transition.clone().unwrap();
    assert!(restore.generation > old);
    assert!(!restore.ready);
    manager.handle(Command::FinishTransition(old)).unwrap();
    assert!(manager.snapshot.transition.is_some());
    manager
        .handle(Command::FinishTransition(restore.generation))
        .unwrap();
    assert!(!manager.snapshot.desktop);
    assert_eq!(manager.snapshot.active.as_deref(), Some("editor"));
}

#[test]
fn overview_activation_selects_exact_window_and_rejects_reused_identity() {
    let (mut manager, fake, _) = fixture();
    manager.handle(Command::Enable).unwrap();
    let selected = fake
        .scene
        .borrow()
        .windows
        .iter()
        .find(|w| w.address == "0x3")
        .unwrap()
        .clone();
    manager
        .handle(Command::ActivateWindow(selected.clone()))
        .unwrap();
    assert_eq!(fake.scene.borrow().active.as_deref(), Some("0x3"));
    assert_eq!(manager.snapshot.active.as_deref(), Some("browser"));
    manager.handle(Command::Activate("editor".into())).unwrap();
    fake.scene
        .borrow_mut()
        .windows
        .iter_mut()
        .find(|w| w.address == "0x3")
        .unwrap()
        .pid += 1;
    assert!(manager.handle(Command::ActivateWindow(selected)).is_err());
    assert_eq!(fake.scene.borrow().active.as_deref(), Some("0x1"));
}

#[test]
fn escaped_parking_recovers_even_after_show_desktop_clears_active_group() {
    let (mut manager, fake, _) = fixture();
    manager.handle(Command::Enable).unwrap();
    manager.handle(Command::ShowDesktop).unwrap();
    let parking = manager.session.as_ref().unwrap().parking.clone();
    {
        let mut scene = fake.scene.borrow_mut();
        scene.monitors[0].workspace = Workspace {
            id: 900,
            name: parking,
        };
        scene.active = None;
    }
    manager.tick().unwrap();
    assert_eq!(fake.scene.borrow().monitors[0].workspace.id, 1);
    assert!(manager.snapshot.visible);
    assert!(!manager.snapshot.desktop);
    assert!(manager.snapshot.active.is_some());
}

#[test]
fn recovery_does_not_pull_user_back_from_an_ordinary_workspace() {
    let (mut manager, fake, _) = fixture();
    manager.handle(Command::Enable).unwrap();
    manager.handle(Command::ShowDesktop).unwrap();
    {
        let mut scene = fake.scene.borrow_mut();
        scene.monitors[0].workspace = Workspace {
            id: 2,
            name: "2".into(),
        };
        scene.active = Some("0x4".into());
    }
    fake.operations.borrow_mut().clear();
    manager.tick().unwrap();
    assert_eq!(fake.scene.borrow().monitors[0].workspace.id, 2);
    assert!(
        !fake
            .operations
            .borrow()
            .iter()
            .any(|op| matches!(op, Operation::Focus(_)))
    );
    assert!(manager.snapshot.desktop);
}
