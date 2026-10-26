use super::super::{
    Action, Backend, DesktopView, Mode, OpenWindows, SharedStore, Store, control_center,
};
use crate::hyprland::stage::{Command, Group, Snapshot};
use gpui_test::{
    self as gpui, ElementId, Entity, Modifiers, MouseButton, Task, TestAppContext,
    VisualTestContext, prelude::*, px, size,
};
use std::sync::mpsc::Receiver;

fn fixture() -> Snapshot {
    Snapshot {
        available: true,
        enabled: true,
        visible: true,
        output: Some("STAGE-TEST".into()),
        active: Some("editor".into()),
        groups: vec![
            Group {
                id: "editor".into(),
                classes: vec!["Editor".into()],
                titles: vec!["A new idea.txt".into()],
                count: 1,
                ..Group::default()
            },
            Group {
                id: "browser".into(),
                classes: vec!["Browser".into()],
                titles: vec!["Hyprland Documentation".into(), "Project notes".into()],
                count: 2,
                ..Group::default()
            },
            Group {
                id: "terminal".into(),
                classes: vec!["Terminal".into()],
                titles: vec!["macstill — cargo build".into()],
                count: 1,
                ..Group::default()
            },
        ],
        ..Snapshot::default()
    }
}
fn store(cx: &mut gpui_test::App) -> (Entity<Store>, Receiver<Action>) {
    macstill_ui::init(cx);
    let (backend, actions) = Backend::fixture_stage(fixture());
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
fn install_content_fixture(store: &Entity<Store>, cx: &mut gpui_test::App) {
    let mut content = super::content::Content::fixture();
    store.update(cx, |store, cx| {
        for (index, group) in store.snapshot.stage.groups.iter_mut().enumerate() {
            for member in 0..group.count {
                let window=serde_json::from_value::<crate::hyprland::stage::Window>(serde_json::json!({
                    "address":format!("0x{:x}",100+index*10+member),"pid":200+index,
                    "class":group.classes[0],"title":group.titles[member],
                    "workspace":{"id":1,"name":"1"},"monitor":0,"at":[240,130],"size":[640,420],"floating":true
                })).unwrap();
                let mut pixels=image::RgbaImage::from_pixel(640,420,image::Rgba([248,246,244,255]));
                for (x,y,pixel) in pixels.enumerate_pixels_mut() {
                    if y<28 { *pixel=image::Rgba([214,207,202,255]); }
                    else if x<130 { *pixel=image::Rgba([235,221,209,255]); }
                    else if (60..350).contains(&y) && y%54<28 && x>155 && x<570 {
                        *pixel=image::Rgba([223-(index as u8*25),180+(index as u8*15),95,255]);
                    }
                }
                content.images.insert(super::content::target(&window),(1,std::sync::Arc::new(gpui_test::RenderImage::new(vec![image::Frame::new(pixels)]))));
                group.windows.push(window);
            }
        }
        cx.notify();
    });
    cx.set_global(content);
}
fn click(cx: &mut VisualTestContext, id: &str) {
    let center = cx.update(|window, _| {
        gpui_base::test_support::find(window, &[], &ElementId::from(id.to_owned()))
            .unwrap()
            .bounds()
            .center()
    });
    cx.simulate_click(center, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
}
fn context_menu(cx: &mut VisualTestContext, id: &str) {
    let center = cx.update(|window, _| {
        gpui_base::test_support::find(window, &[], &ElementId::from(id.to_owned()))
            .unwrap()
            .bounds()
            .center()
    });
    cx.simulate_mouse_down(center, MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(center, MouseButton::Right, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
}
fn setup(
    cx: &mut TestAppContext,
    mode: Mode,
) -> (
    Entity<DesktopView>,
    &mut VisualTestContext,
    Receiver<Action>,
) {
    let (_, actions) = cx.update(store);
    let (view, cx) = cx.add_window_view(|window, cx| DesktopView::new(mode, window, cx));
    cx.update(|window, cx| {
        window.resize(if mode == Mode::Stage {
            size(px(1280.), px(800.))
        } else {
            size(px(800.), px(700.))
        });
        window.bounds_changed(cx);
        window.draw(cx).clear(cx);
    });
    (view, cx, actions)
}

#[gpui::test]
fn dragging_a_pile_commits_only_when_released_on_stage(cx: &mut TestAppContext) {
    let (view, cx, actions) = setup(cx, Mode::Stage);
    let center = cx.update(|window, _| {
        gpui_base::test_support::find(window, &[], &ElementId::from("stage-select:browser"))
            .unwrap()
            .bounds()
            .center()
    });
    cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
    let destination = gpui_test::point(px(450.), center.y);
    cx.simulate_mouse_move(destination, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        actions.try_recv().is_err(),
        "dragging must not change membership before drop"
    );
    assert!(view.read_with(cx, |view, _| view.stage_state.press.as_ref().unwrap().moved));
    cx.simulate_mouse_up(destination, MouseButton::Left, Modifiers::default());
    assert!(
        matches!(actions.try_recv().unwrap(), Action::Stage(Command::Merge(id)) if id == "browser")
    );
    assert!(
        actions.try_recv().is_err(),
        "drop must not also activate the group"
    );
}

#[gpui::test]
fn dropping_back_in_the_strip_cancels_without_activation(cx: &mut TestAppContext) {
    let (_, cx, actions) = setup(cx, Mode::Stage);
    let center = cx.update(|window, _| {
        gpui_base::test_support::find(window, &[], &ElementId::from("stage-select:browser"))
            .unwrap()
            .bounds()
            .center()
    });
    cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        gpui_test::point(px(450.), center.y),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(center, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(center, MouseButton::Left, Modifiers::default());
    assert!(actions.try_recv().is_err());
}

#[gpui::test]
fn hidden_strip_reveals_at_edge_and_fullscreen_suppresses_it(cx: &mut TestAppContext) {
    let (view, cx, _) = setup(cx, Mode::Stage);
    view.update(cx, |view, cx| {
        view.store
            .update(cx, |store, _| store.snapshot.stage.recent_hidden = true);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_mouse_move(
        gpui_test::point(px(1.), px(300.)),
        None,
        Modifiers::default(),
    );
    view.update(cx, |view, cx| {
        view.stage_state.edge_enter =
            Some(std::time::Instant::now() - std::time::Duration::from_millis(200));
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(view.read_with(cx, |view, _| view.stage_state.revealed));
    view.update(cx, |view, cx| {
        view.store
            .update(cx, |store, _| store.snapshot.stage.fullscreen = true);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(view.read_with(
        cx,
        |view, _| view.stage_state.slide.as_ref().unwrap().0.target < 0.
    ));
}

#[gpui::test]
fn repeated_off_sync_is_safe_without_capture_resources(cx: &mut TestAppContext) {
    let (store, _) = cx.update(store);
    cx.update(|cx| {
        store.update(cx, |store, _| store.snapshot.stage.enabled = false);
        super::sync(cx);
        super::sync(cx);
    });
}

#[gpui::test]
fn reduced_motion_switches_captured_groups_immediately(cx: &mut TestAppContext) {
    let (store, actions) = cx.update(store);
    cx.update(|cx| {
        install_content_fixture(&store, cx);
        cx.set_reduce_motion(true);
    });
    let (_view, cx) = cx.add_window_view(|window, cx| DesktopView::new(Mode::Stage, window, cx));
    cx.update(|window, cx| {
        window.resize(size(px(1280.), px(800.)));
        window.bounds_changed(cx);
        window.draw(cx).clear(cx);
    });
    click(cx, "stage-select:browser");
    assert!(
        matches!(actions.try_recv().unwrap(),Action::Stage(Command::Activate(id)) if id=="browser")
    );
}

#[gpui::test]
fn captures_without_native_plugin_switch_immediately(cx: &mut TestAppContext) {
    let (store, actions) = cx.update(store);
    cx.update(|cx| install_content_fixture(&store, cx));
    let (_view, cx) = cx.add_window_view(|window, cx| DesktopView::new(Mode::Stage, window, cx));
    cx.update(|window, cx| {
        window.resize(size(px(1280.), px(800.)));
        window.bounds_changed(cx);
        window.draw(cx).clear(cx);
    });
    click(cx, "stage-select:browser");
    assert!(
        matches!(actions.try_recv().unwrap(),Action::Stage(Command::Activate(id)) if id=="browser")
    );
}

#[gpui::test]
fn strip_excludes_the_active_group_and_keeps_background_groups_clickable(cx: &mut TestAppContext) {
    let (_, cx, actions) = setup(cx, Mode::Stage);
    cx.update(|window, _| {
        assert!(
            gpui_base::test_support::find(window, &[], &ElementId::from("stage-select:editor"))
                .is_none()
        );
        assert!(
            gpui_base::test_support::find(window, &[], &ElementId::from("stage-add:editor"))
                .is_none()
        );
        assert!(
            gpui_base::test_support::find(window, &[], &ElementId::from("stage-select:browser"))
                .is_some()
        );
        for id in ["stage-actions", "stage-add:browser", "stage-off"] {
            assert!(gpui_base::test_support::find(window, &[], &ElementId::from(id)).is_none());
        }
    });
    click(cx, "stage-select:terminal");
    assert!(
        matches!(actions.try_recv().unwrap(), Action::Stage(Command::Activate(id)) if id == "terminal")
    );
}

#[gpui::test]
fn stage_cards_dispatch_selection_and_busy_state_prevents_duplicate_actions(
    cx: &mut TestAppContext,
) {
    let (_, cx, actions) = setup(cx, Mode::Stage);
    click(cx, "stage-select:browser");
    assert!(
        matches!(actions.try_recv().unwrap(), Action::Stage(Command::Activate(id)) if id == "browser")
    );
    click(cx, "stage-select:browser");
    assert!(actions.try_recv().is_err());
}
#[gpui::test]
fn stage_group_merge_and_exit_use_real_backend_commands(cx: &mut TestAppContext) {
    let (_, cx, actions) = setup(cx, Mode::Stage);
    context_menu(cx, "stage-select:browser");
    click(cx, "stage-add:browser");
    assert!(
        matches!(actions.try_recv().unwrap(), Action::Stage(Command::Merge(id)) if id == "browser")
    );
}
#[gpui::test]
fn stage_context_menu_requests_restoration(cx: &mut TestAppContext) {
    let (_, cx, actions) = setup(cx, Mode::Stage);
    context_menu(cx, "stage-select:browser");
    click(cx, "stage-off");
    assert!(matches!(
        actions.try_recv().unwrap(),
        Action::Stage(Command::Disable)
    ));
}
#[gpui::test]
fn control_center_stage_toggle_is_connected(cx: &mut TestAppContext) {
    let (_, cx, actions) = setup(cx, Mode::ControlCenter);
    click(cx, "cc-stage");
    assert!(matches!(
        actions.try_recv().unwrap(),
        Action::Stage(Command::Animate(command)) if matches!(*command, Command::Toggle)
    ));
}

#[test]
#[ignore = "requires a real offscreen GPU renderer; saves Stage Manager sidebar images"]
fn stage_visual_review() {
    use gpui_kit::{AppContext, HeadlessAppContext};
    let mut context = HeadlessAppContext::with_platform(
        gpui_kit::platform::current_platform(true).text_system(),
        std::sync::Arc::new(()),
        gpui_platform_test::current_headless_renderer,
    );
    let (store, _actions) = context.update(store);
    context.update(|cx| install_content_fixture(&store, cx));
    let handle = context
        .open_window(size(px(1280.), px(800.)), |window, cx| {
            cx.new(|cx| DesktopView::new(Mode::Stage, window, cx))
        })
        .unwrap();
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/stage-review");
    std::fs::create_dir_all(&directory).unwrap();
    for (name, dark, small) in [
        ("sidebar", false, false),
        ("sidebar-dark", true, false),
        ("sidebar-small", false, true),
    ] {
        context.update(|cx| {
            store.update(cx, |store, cx| {
                store.snapshot.preferences.dark = dark;
                cx.notify();
            })
        });
        handle
            .update(&mut context, |_, window, cx| {
                window.resize(size(px(1280.), px(if small { 380. } else { 800. })));
                window.bounds_changed(cx);
                cx.notify();
            })
            .unwrap();
        context.run_until_parked();
        context
            .update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        context
            .capture_screenshot(handle.into())
            .unwrap()
            .save(directory.join(format!("{name}.png")))
            .unwrap();
    }
}

#[gpui::test]
fn stage_selection_is_routed_to_its_own_output(cx: &mut TestAppContext) {
    let (view, cx, actions) = setup(cx, Mode::Stage);
    view.update(cx, |view, cx| {
        view.stage_state.output = Some("DP-2".into());
        view.stage_command(Command::Activate("browser".into()), cx);
    });
    assert!(
        matches!(actions.try_recv().unwrap(), Action::Stage(Command::OnOutput(name, command))
        if name == "DP-2" && matches!(*command, Command::Activate(ref group) if group == "browser"))
    );
}

#[gpui::test]
fn desktop_reveal_requests_animation_unless_reduced_motion(cx: &mut TestAppContext) {
    let (view, cx, actions) = setup(cx, Mode::Desktop);
    view.update(cx, |view, cx| view.stage_command(Command::ShowDesktop, cx));
    assert!(
        matches!(actions.try_recv().unwrap(), Action::Stage(Command::Animate(command)) if matches!(*command, Command::ShowDesktop))
    );
}

#[gpui::test]
fn desktop_reveal_respects_reduced_motion(cx: &mut TestAppContext) {
    let (view, cx, actions) = setup(cx, Mode::Desktop);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    view.update(cx, |view, cx| view.stage_command(Command::ShowDesktop, cx));
    assert!(matches!(
        actions.try_recv().unwrap(),
        Action::Stage(Command::ShowDesktop)
    ));
}
