use super::super::{Action, Backend, Page, SettingsView, SharedStore, Store};
use super::{Monitor, scale_choices};
use crate::system_backend::{Job, SystemBackend};
use gpui_test::{self as gpui, TestAppContext, VisualTestContext};
use gpui_test::{App, ElementId, Entity, Modifiers, Task, point, prelude::*, px, size};
use macstill_system::{Backlight, SystemCommand, SystemSnapshot};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

fn monitors() -> Vec<Monitor> {
    vec![
        Monitor {
            name: "eDP-1".into(),
            description: "Built-in Retina Display".into(),
            width: 3840,
            height: 2160,
            x: 0,
            y: 0,
            scale: 2.,
            refresh_rate: 60.,
            transform: 0,
        },
        Monitor {
            name: "DP-1".into(),
            description: "Studio Display".into(),
            width: 2560,
            height: 1440,
            x: 1920,
            y: -300,
            scale: 1.25,
            refresh_rate: 59.94,
            transform: 1,
        },
    ]
}

fn install(cx: &mut App) -> (Receiver<Job>, Receiver<Action>) {
    macstill_ui::init(cx);
    let (backend, preferences) = Backend::fixture();
    let snapshot = backend.snapshot();
    let system = SystemSnapshot {
        pointer: Default::default(),
        network: Default::default(),
        bluetooth: Default::default(),
        audio: Default::default(),
        backlights: Ok(vec![Backlight {
            name: "intel_backlight".into(),
            current: 50,
            maximum: 100,
            percent: 50.,
        }])
        .into(),
        power: Default::default(),
        power_profiles: Default::default(),
        date_time: Default::default(),
        host: Default::default(),
        locale: Default::default(),
        session: Default::default(),
        power_actions: Default::default(),
    };
    let (system, jobs) = SystemBackend::fixture(system);
    let mut system_snapshot = system.snapshot();
    system_snapshot.monitors = monitors();
    let store = cx.new(|_| Store {
        backend,
        snapshot,
        system,
        system_snapshot,
        _refresh: Task::ready(()),
    });
    cx.set_global(SharedStore(store));
    (jobs, preferences)
}

fn setup(
    cx: &mut TestAppContext,
) -> (
    Entity<SettingsView>,
    &mut VisualTestContext,
    Receiver<Job>,
    Receiver<Action>,
) {
    let (jobs, preferences) = cx.update(install);
    let (view, cx) = cx.add_window_view(SettingsView::new);
    cx.update(|w, cx| {
        w.resize(size(px(960.), px(800.)));
        w.bounds_changed(cx);
        view.update(cx, |v, cx| v.set_page(Page::Displays, w, cx));
        w.draw(cx).clear(cx);
    });
    (view, cx, jobs, preferences)
}

fn click(cx: &mut VisualTestContext, id: &str) {
    let center = cx.update(|w, _| {
        gpui_base::test_support::find(w, &[], &ElementId::from(id.to_owned()))
            .unwrap()
            .bounds()
            .center()
    });
    cx.simulate_click(center, Modifiers::default());
    cx.update(|w, cx| w.draw(cx).clear(cx));
}

#[gpui::test]
fn display_selection_updates_scale_controls_without_writing_settings(cx: &mut TestAppContext) {
    let (view, cx, jobs, preferences) = setup(cx);
    click(cx, "display-select-DP-1");
    cx.update(|w, cx| {
        assert_eq!(view.read(cx).display_selection.as_deref(), Some("DP-1"));
        assert!(gpui_base::test_support::find(w, &[], &"display-scale-DP-1-125%".into()).is_some());
        assert!(
            gpui_base::test_support::find(w, &[], &"display-scale-eDP-1-200%".into()).is_none()
        );
    });
    // Connector identity keeps the chosen display stable across a compositor refresh.
    cx.update(|w, cx| {
        view.read(cx).store.clone().update(cx, |store, cx| {
            store.system_snapshot.monitors.reverse();
            cx.notify();
        });
        w.draw(cx).clear(cx);
        assert!(gpui_base::test_support::find(w, &[], &"display-scale-DP-1-125%".into()).is_some());
    });
    click(cx, "display-arrange");
    cx.simulate_keystrokes("escape");
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(jobs.try_recv().is_err());
    assert!(preferences.try_recv().is_err());
}

#[gpui::test]
fn display_brightness_still_dispatches_a_hardware_command(cx: &mut TestAppContext) {
    let (view, cx, jobs, _) = setup(cx);
    cx.update(|w, cx| {
        view.read(cx).scroll.set_offset(point(px(0.), px(-1000.)));
        w.draw(cx).clear(cx);
    });
    let position = cx.update(|w, _| {
        let bounds = gpui_base::test_support::find(w, &[], &"slider-bar-container".into())
            .unwrap()
            .bounds();
        point(bounds.left() + bounds.size.width * 0.75, bounds.center().y)
    });
    cx.simulate_click(position, Modifiers::default());
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(
        matches!(jobs.try_recv().unwrap(), Job::Apply(SystemCommand::SetBrightness { name, percent })
        if name == "intel_backlight" && (percent - 75.).abs() <= 1.)
    );
}

#[test]
fn display_scaling_samples_keep_the_current_supported_scale() {
    let mut display = monitors().remove(0);
    for scale in display.scales() {
        display.scale = scale;
        let samples = scale_choices(&display);
        assert!(samples.len() <= 5);
        assert!(samples.contains(&scale));
        assert!(samples.windows(2).all(|pair| pair[0] > pair[1]));
        assert!(samples.iter().all(|scale| display.scales().contains(scale)));
    }
}

#[test]
#[ignore = "requires a real offscreen GPU renderer; saves Displays screenshots"]
fn display_visual_review() {
    use gpui_kit::{AppContext, HeadlessAppContext};
    let mut context = HeadlessAppContext::with_platform(
        gpui_kit::platform::current_platform(true).text_system(),
        std::sync::Arc::new(()),
        gpui_platform_test::current_headless_renderer,
    );
    let _channels = context.update(install);
    let handle = context
        .open_window(size(px(960.), px(800.)), |w, cx| {
            cx.new(|cx| SettingsView::new(w, cx))
        })
        .unwrap();
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/settings-review");
    std::fs::create_dir_all(&directory).unwrap();
    for (name, dark, small, selected, empty) in [
        ("displays", false, false, 0, false),
        ("displays-dark", true, false, 1, false),
        ("displays-small", false, true, 0, false),
        ("displays-small-dark", true, true, 1, false),
        ("displays-unavailable", false, false, 0, true),
    ] {
        handle
            .update(&mut context, |view, w, cx| {
                view.store.update(cx, |store, _| {
                    store.snapshot.preferences.dark = dark;
                    store.system_snapshot.monitors = if empty { vec![] } else { monitors() };
                    store.system_snapshot.display_error =
                        empty.then(|| "Hyprland is not running".into());
                });
                w.resize(if small {
                    size(px(760.), px(540.))
                } else {
                    size(px(960.), px(800.))
                });
                w.bounds_changed(cx);
                view.set_page(Page::Displays, w, cx);
                view.display_selection =
                    monitors().get(selected).map(|monitor| monitor.name.clone());
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

#[gpui::test]
fn arrangement_drag_is_a_draft_until_apply(cx: &mut TestAppContext) {
    let (_, cx, jobs, _) = setup(cx);
    click(cx, "display-arrange");
    let center = cx.debug_bounds("arrange-output-DP-1").unwrap().center();
    cx.simulate_mouse_down(center, gpui_test::MouseButton::Left, Modifiers::default());
    let destination = point(center.x, center.y + px(35.));
    cx.simulate_mouse_move(
        destination,
        gpui_test::MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        destination,
        gpui_test::MouseButton::Left,
        Modifiers::default(),
    );
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(jobs.try_recv().is_err());
    click(cx, "arrangement-apply");
    let Job::ArrangeDisplays(positions) = jobs.try_recv().unwrap() else {
        panic!("expected arrangement")
    };
    assert_eq!(positions.len(), 2);
    assert_ne!(positions.iter().find(|p| p.0 == "DP-1").unwrap().2, -300);
}

#[test]
#[ignore = "renders the arrangement editor with a real offscreen GPU"]
fn arrangement_visual_review() {
    use gpui_kit::{AppContext, HeadlessAppContext};
    let mut context = HeadlessAppContext::with_platform(
        gpui_kit::platform::current_platform(true).text_system(),
        std::sync::Arc::new(()),
        gpui_platform_test::current_headless_renderer,
    );
    let _channels = context.update(install);
    let handle = context
        .open_window(size(px(420.), px(370.)), |_, cx| {
            let store = cx.global::<SharedStore>().0.clone();
            cx.new(|cx| {
                super::arrangement::Arrangement::new(
                    monitors(),
                    store,
                    super::settings_palette(false),
                    cx,
                )
            })
        })
        .unwrap();
    context
        .update_window(handle.into(), |_, w, cx| w.draw(cx).clear(cx))
        .unwrap();
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/settings-review");
    std::fs::create_dir_all(&directory).unwrap();
    context
        .capture_screenshot(handle.into())
        .unwrap()
        .save(directory.join("arrangement.png"))
        .unwrap();
}
