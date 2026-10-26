use super::super::{
    Action, Backend, DesktopView, Mode, OpenWindows, SharedStore, Store, control_center,
};
use crate::desktop as files;
use gpui_test::{
    self as gpui, ElementId, Entity, Modifiers, MouseButton, Task, TestAppContext,
    VisualTestContext, point, prelude::*, px, size,
};
use std::sync::mpsc::Receiver;
fn store(cx: &mut gpui_test::App) -> (Entity<Store>, Receiver<Action>) {
    macstill_ui::init(cx);
    let (backend, actions) = Backend::fixture_stage(crate::hyprland::stage::Snapshot::default());
    let mut snapshot = backend.snapshot();
    for (class, name, icon) in [
        ("Editor", "Notes", "notes.png"),
        ("Browser", "Safari", "safari.png"),
        ("Terminal", "Terminal", "folder.png"),
    ] {
        snapshot.apps.push(crate::catalog::DesktopApp {
            id: format!("{class}.desktop"),
            name: name.into(),
            desktop_file: format!("{class}.desktop").into(),
            icon: Some(macstill_ui::asset(icon)),
            class_aliases: Vec::new(),
            startup_class: Some(class.into()),
        });
    }
    let (notifications, _) = crate::notifications::Notifications::fixture();
    let notification_snapshot = notifications.snapshot();
    let control_status = cx.new(|_| control_center::StatusCache::fixture());
    let store = cx.new(|_| Store {
        backend,
        snapshot,
        notifications,
        notification_snapshot,
        control_status,
        tray: crate::tray::Tray::fixture(),
                menus: macstill_menu::host::Host::disabled(),
                menu_snapshot: Default::default(),
        _refresh: Task::ready(()),
        _notification_refresh: Task::ready(()),
        clock_minute: String::new(),
        _control: Task::ready(()),
    });
    cx.set_global(SharedStore(store.clone()));
    cx.set_global(OpenWindows::default());
    (store, actions)
}

fn setup<'a>(
    cx: &'a mut TestAppContext,
    root: &std::path::Path,
) -> (Entity<DesktopView>, &'a mut VisualTestContext) {
    cx.update(store);
    let (view, cx) = cx.add_window_view(|window, cx| DesktopView::new(Mode::Desktop, window, cx));
    view.update(cx, |s, cx| {
        s.desktop.root = root.into();
        s.desktop.entries = files::entries(root).unwrap();
        s.desktop.positions.clear();
        s.desktop.layout_path = root.join(".layout.json");
        cx.notify();
    });
    cx.update(|w, cx| {
        let id = w.display(cx).unwrap().id();
        cx.global_mut::<OpenWindows>().geometry.insert(
            id,
            gpui_test::Bounds::new(point(px(0.), px(0.)), size(px(800.), px(700.))),
        );
        w.resize(size(px(800.), px(700.)));
        w.bounds_changed(cx);
        w.draw(cx).clear(cx);
    });
    (view, cx)
}
fn file_center(
    cx: &mut VisualTestContext,
    view: &Entity<DesktopView>,
) -> gpui_test::Point<gpui_test::Pixels> {
    cx.update(|w, cx| {
        w.draw(cx).clear(cx);
        let s = view.read(cx);
        let path = &s.desktop.entries[0].path;
        let position = s
            .desktop
            .positions
            .get(path)
            .copied()
            .unwrap_or([f32::from(w.bounds().size.width) - 110., 44.]);
        point(px(position[0] + 46.), px(position[1] + 49.))
    })
}
#[gpui::test]
fn desktop_drag_persists_only_on_release_and_escape_restores(cx: &mut TestAppContext) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("notes.txt");
    std::fs::write(&path, "note").unwrap();
    let (view, cx) = setup(cx, temp.path());
    let center = file_center(cx, &view);
    cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        point(px(300.), px(250.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    assert!(!temp.path().join(".layout.json").exists());
    cx.simulate_mouse_up(
        point(px(300.), px(250.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    let saved = files::load_positions(&temp.path().join(".layout.json"));
    assert!(saved.contains_key(&path));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let center = file_center(cx, &view);
    cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        point(px(500.), px(400.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_keystrokes("escape");
    assert_eq!(
        view.read_with(cx, |s, _| s.desktop.positions[&path]),
        saved[&path]
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "note");
}
#[gpui::test]
fn desktop_right_click_targets_file_and_background_separately(cx: &mut TestAppContext) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("notes.txt");
    std::fs::write(&path, "note").unwrap();
    let (view, cx) = setup(cx, temp.path());
    let center = file_center(cx, &view);
    cx.simulate_mouse_down(center, MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(center, MouseButton::Right, Modifiers::default());
    assert_eq!(
        view.read_with(cx, |s, _| s.desktop.selected.clone()),
        Some(path)
    );
    cx.update(|w, cx| {
        w.draw(cx).clear(cx);
        assert!(gpui_base::test_support::find(w, &[], &ElementId::from("file-rename")).is_some());
    });
    cx.simulate_mouse_down(
        point(px(100.), px(100.)),
        MouseButton::Right,
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        point(px(100.), px(100.)),
        MouseButton::Right,
        Modifiers::default(),
    );
    assert!(view.read_with(cx, |s, _| s.desktop.selected.is_none()));
    cx.update(|w, cx| {
        w.draw(cx).clear(cx);
        assert!(
            gpui_base::test_support::find(w, &[], &ElementId::from("desktop-new-folder")).is_some()
        );
    });
}

#[gpui::test]
fn desktop_rename_editor_and_copy_use_the_selected_file(cx: &mut TestAppContext) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("notes.txt");
    std::fs::write(&path, "note").unwrap();
    let (view, cx) = setup(cx, temp.path());
    let center = file_center(cx, &view);
    cx.simulate_click(center, Modifiers::default());
    cx.simulate_keystrokes("ctrl-c");
    cx.update(|_,cx|{
        let clipboard=cx.read_from_clipboard().unwrap();
        assert!(matches!(&clipboard.entries()[0],gpui_test::ClipboardEntry::ExternalPaths(paths) if paths.paths()==[path.clone()]));
    });
    cx.simulate_keystrokes("f2");
    assert!(view.read_with(cx, |s, _| s.desktop.rename.is_some()));
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |s, _| s.desktop.rename.is_none()));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "note");
}

#[test]
#[ignore = "requires offscreen renderer; renders desktop menus and rename dialog"]
fn desktop_visual_review() {
    use gpui_kit::{AppContext, HeadlessAppContext};
    let mut context = HeadlessAppContext::with_platform(
        gpui_kit::platform::current_platform(true).text_system(),
        std::sync::Arc::new(()),
        gpui_platform_test::current_headless_renderer,
    );
    let (store, _) = context.update(store);
    let handle = context
        .open_window(size(px(1000.), px(720.)), |w, cx| {
            let id = w.display(cx).unwrap().id();
            cx.global_mut::<OpenWindows>().geometry.insert(
                id,
                gpui_test::Bounds::new(point(px(0.), px(0.)), size(px(1000.), px(720.))),
            );
            cx.new(|cx| DesktopView::new(Mode::Desktop, w, cx))
        })
        .unwrap();
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/desktop-review");
    std::fs::create_dir_all(&directory).unwrap();
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("Notes.txt"), "note").unwrap();
    std::fs::create_dir(temp.path().join("Projects")).unwrap();
    for (name, dark, rename, background) in [
        ("file-menu", false, false, false),
        ("file-menu-dark", true, false, false),
        ("rename", false, true, false),
        ("background-menu", false, false, true),
    ] {
        context.update(|cx| {
            store.update(cx, |s, cx| {
                s.snapshot.preferences.dark = dark;
                cx.notify();
            })
        });
        handle
            .update(&mut context, |s, w, cx| {
                s.desktop.root = temp.path().into();
                s.desktop.entries = files::entries(temp.path()).unwrap();
                s.desktop.positions.clear();
                s.desktop
                    .positions
                    .insert(temp.path().join("Notes.txt"), [420., 180.]);
                s.desktop.selected = if background {
                    None
                } else {
                    Some(temp.path().join("Notes.txt"))
                };
                s.desktop.menu = Some(point(px(520.), px(240.)));
                s.desktop.rename = None;
                if rename {
                    s.desktop_rename(w, cx);
                }
                cx.notify();
            })
            .unwrap();
        context.run_until_parked();
        context
            .update_window(handle.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        context
            .capture_screenshot(handle.into())
            .unwrap()
            .save(directory.join(format!("{name}.png")))
            .unwrap();
    }
}
