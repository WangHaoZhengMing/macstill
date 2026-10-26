//! Exercise real browser handlers across virtual and filesystem locations.
use super::*;

#[test]
fn history_preserves_trash_search_and_tag_destinations() -> anyhow::Result<()> {
    let fixture = std::env::temp_dir().join(format!(
        "finder-navigation-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
    ));
    let first = fixture.join("first");
    let second = fixture.join("second");
    std::fs::create_dir_all(&first)?;
    std::fs::create_dir_all(&second)?;
    std::fs::write(first.join("report.txt"), b"report")?;
    let native = gpui_kit::platform::current_platform(true);
    let mut app = gpui_kit::HeadlessAppContext::with_platform(
        native.text_system(),
        Arc::new(crate::ui::assets::Assets),
        gpui_kit::platform::current_headless_renderer,
    );
    app.update(|cx| {
        gpui_kit::init(cx);
        crate::ui::window::init(cx);
    });
    let _window = app.open_window(gpui_kit::size(px(1280.), px(760.)), |window, cx| {
        let font = t::ui_font(&cx.text_system().all_font_names()).into();
        let view = cx.new(|cx| Browser::new(font, window, cx));
        view.update(cx, |browser, cx| {
            browser.navigate(first.clone(), cx);
            browser.navigate(Location::Trash, cx);
            browser.refresh(&[], cx);
            assert_eq!(browser.navigation.location, Location::Trash);
            browser.new_folder(&NewFolder, window, cx);
            assert_eq!(std::fs::read_dir(&first).unwrap().count(), 1);
            browser.go_back(&GoBack, window, cx);
            assert_eq!(browser.navigation.location, Location::Folder(first.clone()));
            browser.go_forward(&GoForward, window, cx);
            assert_eq!(browser.navigation.location, Location::Trash);
            let search = Location::Search {
                root: first.clone(),
                query: "report".into(),
                folder_scope: true,
            };
            browser.navigate(search.clone(), cx);
            browser.navigate(second.clone(), cx);
            browser.go_back(&GoBack, window, cx);
            assert_eq!(browser.navigation.location, search);
            assert!(matches!(
                browser.search.session.as_ref().map(|session| session.scope),
                Some(Scope::Folder)
            ));
            browser.go_forward(&GoForward, window, cx);
            assert_eq!(
                browser.navigation.location,
                Location::Folder(second.clone())
            );
            browser.show_tag(Tag::Red, cx);
            browser.navigate(Location::Trash, cx);
            browser.go_back(&GoBack, window, cx);
            assert_eq!(
                browser.navigation.location,
                Location::Tag {
                    root: second.clone(),
                    tag: Tag::Red
                }
            );
            assert_eq!(
                browser
                    .search
                    .session
                    .as_ref()
                    .and_then(|session| session.tag),
                Some(Tag::Red)
            );
        });
        cx.new(|cx| gpui_kit::base::Root::new(view, window, cx))
    })?;
    app.run_until_parked();
    drop(app);
    std::fs::remove_dir_all(fixture)?;
    Ok(())
}

#[test]
fn context_menu_dispatches_after_restoring_browser_focus() -> anyhow::Result<()> {
    let native = gpui_kit::platform::current_platform(true);
    let mut app = gpui_kit::HeadlessAppContext::with_platform(
        native.text_system(),
        Arc::new(crate::ui::assets::Assets),
        gpui_kit::platform::current_headless_renderer,
    );
    app.update(|cx| {
        gpui_kit::init(cx);
        crate::ui::window::init(cx);
        crate::actions::bind(cx);
    });
    let mut browser = None;
    let handle = app.open_window(gpui_kit::size(px(1280.), px(760.)), |window, cx| {
        let font = t::ui_font(&cx.text_system().all_font_names()).into();
        let view = cx.new(|cx| Browser::new(font, window, cx));
        view.update(cx, |this, cx| {
            this.open_menu(None, gpui_kit::point(px(350.), px(150.)), window, cx)
        });
        browser = Some(view.clone());
        cx.new(|cx| gpui_kit::base::Root::new(view, window, cx))
    })?;
    let browser = browser.unwrap();
    app.run_until_parked();
    for key in ["end", "right", "enter"] {
        let arena = app.update_window(handle.into(), |_, window, cx| window.draw(cx))?;
        app.update(|cx| arena.clear(cx));
        app.update_window(handle.into(), |_, window, cx| {
            window.dispatch_keystroke(gpui_kit::Keystroke::parse(key).unwrap(), cx);
        })?;
    }
    app.update_window(handle.into(), |_, window, cx| {
        let browser = browser.read(cx);
        assert!(browser.view.mode == ViewMode::Icons);
        assert!(browser.menu.is_none());
        assert!(browser.focus.is_focused(window));
    })?;
    Ok(())
}
