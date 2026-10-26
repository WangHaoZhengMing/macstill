use super::*;
use std::sync::Arc;

#[test]
fn linux_window_has_no_transparent_sidebar_or_empty_regions() -> anyhow::Result<()> {
    let native = gpui_kit::platform::current_platform(true);
    let mut app = gpui_kit::HeadlessAppContext::with_platform(
        native.text_system(),
        Arc::new(assets::Assets),
        gpui_kit::platform::current_headless_renderer,
    );
    app.update(|cx| {
        gpui_kit::init(cx);
        window_chrome::init(cx);
        load_local_fonts(cx);
    });
    let (width, height) = std::env::var("FINDER_UI_SIZE")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?))
        })
        .unwrap_or((1280., 760.));
    let window = app.open_window(size(px(width), px(height)), |window, cx| {
        sync_window_theme(window, cx);
        let font: SharedString = theme::ui_font(&cx.text_system().all_font_names()).into();
        let browser = cx.new(|cx| Browser::new(font, window, cx));
        if std::env::var_os("FINDER_UI_MENU").is_some() {
            browser.update(cx, |browser, cx| {
                browser.test_open_menu(window, cx);
            });
        }
        cx.new(|cx| gpui_kit::base::Root::new(browser, window, cx))
    })?;
    app.run_until_parked();
    let arena = app.update_window(window.into(), |_, window, cx| window.draw(cx))?;
    app.update(|cx| arena.clear(cx));
    let image = app.capture_screenshot(window.into())?;
    let r = (window_chrome::RADIUS * image.width() as f32 / width).ceil() as u32;
    assert!(
        image
            .enumerate_pixels()
            .filter(|(x, y, _)| (*x >= r && *x < image.width() - r)
                || (*y >= r && *y < image.height() - r))
            .all(|(_, _, pixel)| pixel[3] == 255),
        "interior application pixels must remain opaque"
    );
    // GPUI's headless renderer clears to opaque black; untouched corner pixels
    // appear black here, while the native transparent surface reveals the desktop.
    for (x, y) in [
        (0, 0),
        (image.width() - 1, 0),
        (0, image.height() - 1),
        (image.width() - 1, image.height() - 1),
    ] {
        assert_eq!(
            image.get_pixel(x, y).0,
            [0, 0, 0, 255],
            "rounded corner was painted over"
        );
    }
    if let Some(path) = std::env::var_os("FINDER_UI_PREVIEW") {
        image.save(path)?;
    }
    // A short window must scroll the sidebar without moving its file pane.
    let mut handles = None;
    let small = app.open_window(size(px(1056.), px(420.)), |window, cx| {
        sync_window_theme(window, cx);
        let font: SharedString = theme::ui_font(&cx.text_system().all_font_names()).into();
        let browser = cx.new(|cx| Browser::new(font, window, cx));
        handles = Some(browser.read(cx).test_scroll_handles());
        cx.new(|cx| gpui_kit::base::Root::new(browser, window, cx))
    })?;
    app.run_until_parked();
    let arena = app.update_window(small.into(), |_, window, cx| window.draw(cx))?;
    app.update(|cx| arena.clear(cx));
    let (sidebar, files) = handles.unwrap();
    use gpui_kit::component::scroll::ScrollbarHandle;
    let files_before = files.offset();
    app.update_window(small.into(), |_, window, cx| {
        let position = gpui_kit::point(px(80.), px(180.));
        window.simulate_mouse_move(position, cx);
        window.dispatch_event(
            gpui_kit::PlatformInput::ScrollWheel(gpui_kit::ScrollWheelEvent {
                position,
                delta: gpui_kit::ScrollDelta::Lines(gpui_kit::point(0., -1000.)),
                ..Default::default()
            }),
            cx,
        );
    })?;
    app.run_until_parked();
    assert!(
        sidebar.offset().y < px(0.),
        "sidebar did not scroll in a short window"
    );
    assert!(
        (sidebar.offset().y + sidebar.max_offset().y).abs() < px(1.),
        "sidebar could not reach its last item"
    );
    assert_eq!(
        files.offset(),
        files_before,
        "sidebar wheel event leaked into the file pane"
    );
    Ok(())
}
