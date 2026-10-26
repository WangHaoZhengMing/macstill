//! Context-menu navigation, input isolation and rendering checks.
use super::*;
fn command(enabled: bool) -> MenuRow {
    MenuRow::Command {
        label: "Command".into(),
        shortcut: None,
        enabled,
        checked: false,
        dot: None,
        run: Rc::new(|_, _| {}),
    }
}
#[test]
fn keyboard_skips_noncommands_and_wraps() {
    let rows = vec![
        MenuRow::Header("Header".into()),
        command(false),
        MenuRow::Separator,
        command(true),
        MenuRow::Submenu {
            label: "View".into(),
            rows: vec![command(true)],
        },
    ];
    assert_eq!(next_selectable(&rows, None, true), Some(3));
    assert_eq!(next_selectable(&rows, Some(3), true), Some(4));
    assert_eq!(next_selectable(&rows, Some(4), true), Some(3));
    assert_eq!(next_selectable(&rows, Some(3), false), Some(4));
    assert_eq!(
        next_selectable(&[command(false), MenuRow::Separator], None, true),
        None
    );
    assert_eq!(next_selectable(&[], None, true), None);
}
#[test]
fn rendered_menu_routes_input_and_keeps_submenus_open() -> anyhow::Result<()> {
    use std::{cell::Cell, sync::Arc};
    struct Harness {
        menu: gpui_kit::Entity<ContextMenu>,
        leaked: Rc<Cell<usize>>,
    }
    impl Render for Harness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let leaked = self.leaked.clone();
            div()
                .size_full()
                .bg(rgb(0x8F9DA8))
                .key_context("Browser")
                .on_action(move |_: &crate::actions::SelectNext, _, _| leaked.set(leaked.get() + 1))
                .children((0..12).map(|i| {
                    div()
                        .absolute()
                        .left(px((i * 45) as f32))
                        .top(px(0.))
                        .w(px(20.))
                        .h_full()
                        .bg(rgb(0xCAD5DE))
                }))
                .child(self.menu.clone())
        }
    }
    let native = gpui_kit::platform::current_platform(true);
    let mut app = gpui_kit::HeadlessAppContext::with_platform(
        native.text_system(),
        Arc::new(crate::ui::assets::Assets),
        gpui_kit::platform::current_headless_renderer,
    );
    app.update(|cx| {
        gpui_kit::init(cx);
        crate::actions::bind(cx);
        theme::set_dark(std::env::var("FINDER_APPEARANCE").as_deref() == Ok("dark"));
    });
    let calls = Rc::new(Cell::new(0));
    let leaked = Rc::new(Cell::new(0));
    let dismissed = Rc::new(Cell::new(0));
    let mut popup = None;
    let handle = app.open_window(gpui_kit::size(px(640.), px(420.)), |window, cx| {
        let run: Command = {
            let calls = calls.clone();
            Rc::new(move |_, _| calls.set(calls.get() + 1))
        };
        let rows = vec![
            MenuRow::Command {
                label: "Open".into(),
                shortcut: Some("Ctrl+↓".into()),
                enabled: true,
                checked: false,
                dot: None,
                run: run.clone(),
            },
            MenuRow::Command {
                label: "Paste Item".into(),
                shortcut: Some("Ctrl+V".into()),
                enabled: false,
                checked: false,
                dot: None,
                run: run.clone(),
            },
            MenuRow::Separator,
            MenuRow::Command {
                label: "Selected choice".into(),
                shortcut: None,
                enabled: true,
                checked: true,
                dot: None,
                run: run.clone(),
            },
            MenuRow::Submenu {
                label: "Tags".into(),
                rows: vec![MenuRow::Command {
                    label: "Red".into(),
                    shortcut: None,
                    enabled: true,
                    checked: true,
                    dot: Some(rgb(0xFF3B30).into()),
                    run,
                }],
            },
        ];
        let menu =
            cx.new(|cx| ContextMenu::new(rows, point(px(615.), px(400.)), "sans-serif".into(), cx));
        let count = dismissed.clone();
        cx.subscribe(&menu, move |_, _: &DismissEvent, _| {
            count.set(count.get() + 1)
        })
        .detach();
        window.focus(&menu.focus_handle(cx), cx);
        popup = Some(menu.clone());
        cx.new(|_| Harness {
            menu,
            leaked: leaked.clone(),
        })
    })?;
    let menu = popup.unwrap();
    app.run_until_parked();
    let draw = |app: &mut gpui_kit::HeadlessAppContext| -> anyhow::Result<()> {
        let arena = app.update_window(handle.into(), |_, window, cx| window.draw(cx))?;
        app.update(|cx| arena.clear(cx));
        Ok(())
    };
    draw(&mut app)?;
    let send = |app: &mut gpui_kit::HeadlessAppContext, key: &str| -> anyhow::Result<()> {
        app.update_window(handle.into(), |_, window, cx| {
            window.dispatch_keystroke(gpui_kit::Keystroke::parse(key).unwrap(), cx)
        })?;
        Ok(())
    };
    send(&mut app, "down")?;
    app.update(|cx| assert_eq!(menu.read(cx).state.highlighted, vec![Some(0)]));
    assert_eq!(
        leaked.get(),
        0,
        "browser shortcuts must not run inside menus"
    );
    draw(&mut app)?;
    send(&mut app, "down")?;
    app.update(|cx| assert_eq!(menu.read(cx).state.highlighted, vec![Some(3)]));
    draw(&mut app)?;
    send(&mut app, "down")?;
    draw(&mut app)?;
    send(&mut app, "right")?;
    draw(&mut app)?;
    app.update(|cx| {
        assert_eq!(menu.read(cx).state.open_path, vec![4]);
        assert_eq!(menu.read(cx).state.highlighted, vec![Some(4), Some(0)]);
    });
    if let Some(path) = std::env::var_os("FINDER_MENU_PREVIEW") {
        app.capture_screenshot(handle.into())?.save(path)?;
    }
    send(&mut app, "left")?;
    app.update(|cx| assert!(menu.read(cx).state.open_path.is_empty()));
    // Pointer travel from the parent into a submenu keeps it open.
    app.update(|cx| menu.update(cx, |menu, cx| menu.highlight(0, 0, false, cx)));
    draw(&mut app)?;
    let parent_point = app.update(|cx| {
        let menu = menu.read(cx);
        let body = menu.scrolls[0].bounds();
        let y = menu
            .rows
            .iter()
            .take(4)
            .map(|row| menu.metrics.height(row))
            .sum::<f32>();
        body.origin + point(px(25.), px(y + 11.))
    });
    app.update_window(handle.into(), |_, window, cx| {
        window.simulate_mouse_move(parent_point, cx)
    })?;
    draw(&mut app)?;
    app.update(|cx| assert_eq!(menu.read(cx).state.open_path, vec![4]));
    let child_point = app.update(|cx| menu.read(cx).scrolls[1].bounds().center());
    app.update_window(handle.into(), |_, window, cx| {
        window.simulate_mouse_move(child_point, cx)
    })?;
    draw(&mut app)?;
    app.update(|cx| {
        assert_eq!(menu.read(cx).state.open_path, vec![4]);
        assert_eq!(menu.read(cx).state.highlighted[1], Some(0));
    });
    // Render both device scales through the same geometry and hitboxes.
    for scale in [1., 2.] {
        app.update_window(handle.into(), |_, window, _| window.set_scale_factor(scale))?;
        draw(&mut app)?;
        app.capture_screenshot(handle.into())?;
        app.update(|cx| {
            for scroll in menu.read(cx).scrolls.iter().take(2) {
                let bounds = scroll.bounds();
                assert!(bounds.left() >= px(35.) && bounds.right() <= px(605.));
                assert!(bounds.top() >= px(35.) && bounds.bottom() <= px(385.));
            }
        });
    }
    // A disabled row must remain inert even when activated directly.
    app.update_window(handle.into(), |_, window, cx| {
        menu.update(cx, |menu, cx| menu.activate(0, 1, window, cx))
    })?;
    assert_eq!(calls.get(), 0);
    draw(&mut app)?;
    send(&mut app, "left")?;
    draw(&mut app)?;
    send(&mut app, "home")?;
    draw(&mut app)?;
    send(&mut app, "enter")?;
    assert_eq!(calls.get(), 1);
    assert_eq!(dismissed.get(), 1);
    draw(&mut app)?;
    send(&mut app, "escape")?;
    assert_eq!(dismissed.get(), 2);
    // A real pointer click activates exactly once.
    let open_point =
        app.update(|cx| menu.read(cx).scrolls[0].bounds().origin + point(px(25.), px(11.)));
    app.update_window(handle.into(), |_, window, cx| {
        window.simulate_mouse_move(open_point, cx);
        window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: MouseButton::Left,
                position: open_point,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: MouseButton::Left,
                position: open_point,
                modifiers: Default::default(),
                click_count: 1,
            }),
            cx,
        );
    })?;
    assert_eq!(calls.get(), 2);
    assert_eq!(dismissed.get(), 3);
    // Outside click dismisses without executing a command.
    app.update_window(handle.into(), |_, window, cx| {
        window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: MouseButton::Left,
                position: point(px(5.), px(5.)),
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
    })?;
    assert_eq!(dismissed.get(), 4);
    assert_eq!(calls.get(), 2);
    // Long menus scroll the keyboard selection into view.
    app.update(|cx| {
        menu.update(cx, |menu, cx| {
            menu.rows = (0..100).map(|_| command(true)).collect();
            menu.state = MenuState::default();
            cx.notify();
        })
    });
    draw(&mut app)?;
    send(&mut app, "end")?;
    draw(&mut app)?;
    app.update(|cx| {
        assert_eq!(menu.read(cx).state.highlighted, vec![Some(99)]);
        assert!(menu.read(cx).scrolls[0].offset().y < px(0.));
    });
    Ok(())
}
