use macstill::launchpad::{self, Direction, Grid};

#[test]
fn builtin_finder_uses_one_shortcut_and_custom_binding_is_preserved() {
    let mut prefs = macstill::config::Preferences::default();
    prefs.shortcuts.retain(|s| s.id == "finder");
    let finder = macstill::catalog::DesktopApp {
        id: macstill::catalog::FINDER_APP_ID.into(),
        name: "Finder".into(),
        desktop_file: Default::default(),
        icon: None,
        startup_class: Some("org.macstill.Finder".into()),
        class_aliases: vec![],
    };
    let items = launchpad::applications(std::slice::from_ref(&finder), &prefs);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "shortcut:finder");
    prefs.shortcuts[0].app_id = Some("other-files.desktop".into());
    let items = launchpad::applications(std::slice::from_ref(&finder), &prefs);
    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|a| a.id == finder.id));
}

#[test]
fn desktop_layout_matches_swiftde_proportions() {
    let grid = Grid::new(1920., 1080.);
    assert_eq!((grid.columns, grid.rows, grid.capacity()), (7, 4, 28));
    assert_eq!(grid.icon, 106.);
    assert_eq!(grid.center(0), (240., 151.2));
    let last = grid.center(27);
    assert_eq!(last.0, 1680.);
    assert!((last.1 - 653.4).abs() < 0.01);
}
#[test]
fn every_installed_app_is_reachable_across_pages() {
    for count in [0usize, 1, 28, 29, 37, 140, 1001] {
        let grid = Grid::new(1920., 1080.);
        let apps: Vec<_> = (0..count).collect();
        let visited: Vec<_> = (0..grid.pages(count))
            .flat_map(|page| {
                apps.iter()
                    .skip(page * grid.capacity())
                    .take(grid.capacity())
                    .copied()
            })
            .collect();
        assert_eq!(apps, visited);
        assert_eq!(
            grid.page_for(count.saturating_sub(1)),
            grid.pages(count) - 1
        );
    }
}
#[test]
fn small_outputs_keep_icons_and_labels_separate_from_dock() {
    for (width, height) in [(520., 480.), (800., 600.), (1180., 760.), (2560., 1440.)] {
        let grid = Grid::new(width, height);
        for slot in 0..grid.capacity() {
            let (x, y) = grid.center(slot);
            assert!(x - grid.icon / 2. > 0.);
            assert!(x + grid.icon / 2. < width);
            assert!(y - grid.icon / 2. > height * 0.038 + 28.);
            assert!(y + grid.icon / 2. + 32. < height - 80.);
        }
    }
}
#[test]
fn keyboard_navigation_reaches_the_second_page_and_partial_last_row() {
    let grid = Grid::new(1920., 1080.);
    assert_eq!(grid.navigate(27, 37, Direction::Right), 28);
    assert_eq!(grid.page_for(28), 1);
    assert_eq!(grid.navigate(35, 37, Direction::Down), 36);
    assert_eq!(grid.navigate(36, 37, Direction::Right), 36);
    assert_eq!(grid.navigate(0, 37, Direction::Left), 0);
    assert_eq!(grid.navigate(0, 37, Direction::Up), 0);
    assert_eq!(grid.navigate(100, 0, Direction::Right), 0);
}
#[test]
fn search_handles_case_words_unicode_and_desktop_ids() {
    assert!(launchpad::matches(
        "Image Viewer",
        "org.gnome.Loupe.desktop",
        "IMAGE view"
    ));
    assert!(launchpad::matches(
        "Image Viewer",
        "org.gnome.Loupe.desktop",
        "loupe"
    ));
    assert!(launchpad::matches(
        "系统设置",
        "org.example.Settings.desktop",
        "系统"
    ));
    assert!(!launchpad::matches(
        "Image Viewer",
        "org.gnome.Loupe.desktop",
        "image terminal"
    ));
    assert!(launchpad::matches("App", "app.desktop", "  "));
}
