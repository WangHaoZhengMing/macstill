use macstill::{
    config::{self, Preferences},
    dock,
};

#[test]
fn resting_geometry_is_centered_and_magnification_has_a_fixed_anchor() {
    let slots = [true, true, true, true, false, true];
    let resting = dock::layout(&slots, 42., 1200., -1000., 0.);
    assert!((resting.left * 2. + resting.width - 1200.).abs() < 0.01);
    assert!(
        resting
            .cells
            .iter()
            .filter(|c| c.icon)
            .all(|c| (c.width - 42.).abs() < 0.01)
    );
    let pointer = resting.cells[2].x + 21.;
    let enlarged = dock::layout(&slots, 42., 1200., pointer, 1.);
    assert!((enlarged.cells[2].width - 72.).abs() < 0.01);
    assert!((enlarged.cells[1].width - enlarged.cells[3].width).abs() < 0.01);
    assert_eq!(enlarged.cells[4].width, dock::SEPARATOR);
    // Recomputing with the same pointer cannot drift or oscillate.
    let again = dock::layout(&slots, 42., 1200., pointer, 1.);
    assert_eq!(again.cells[2].width, enlarged.cells[2].width);
    assert_eq!(again.left, enlarged.left);
}
#[test]
fn a_crowded_dock_remains_inside_a_narrow_output() {
    for count in [2, 10, 30, 80] {
        let slots = vec![true; count];
        for viewport in [640., 1280., 3840.] {
            let layout = dock::layout(&slots, 72., viewport, viewport / 2., 1.);
            assert!(layout.left >= 0.);
            assert!(layout.width <= viewport);
            let last = layout.cells.last().unwrap();
            assert!(last.x + last.width <= viewport);
            assert!(layout.base > 0.);
        }
    }
}
#[test]
fn launch_bounce_is_continuous_and_settles() {
    assert_eq!(dock::bounce(0.), 0.);
    assert!((dock::bounce(0.16) - 16.).abs() < 0.001);
    assert!((dock::bounce(0.15999) - dock::bounce(0.16001)).abs() < 0.01);
    assert_eq!(dock::bounce(0.36), 0.);
    assert_eq!(dock::bounce(10.), 0.);
    for frame in 0..360 {
        assert!((0. ..=16.).contains(&dock::bounce(frame as f32 / 1000.)));
    }
}
#[test]
fn reorder_keeps_hidden_entries_and_ignores_invalid_drop_targets() {
    let mut order = vec!["a".into(), "hidden".into(), "b".into(), "c".into()];
    let active = vec!["a".into(), "b".into(), "c".into()];
    dock::move_to(&mut order, &active, "c", "a");
    assert_eq!(order, ["c", "a", "hidden", "b"]);
    dock::move_to(&mut order, &active, "c", "b");
    assert_eq!(order, ["a", "hidden", "b", "c"]);
    let before = order.clone();
    dock::move_to(&mut order, &active, "uninstalled", "a");
    assert_eq!(order, before);
    dock::move_to(&mut order, &active, "a", "a");
    assert_eq!(order, before);
}
#[test]
fn dock_settings_migrate_and_round_trip_without_resetting_existing_choices() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    std::fs::write(&path, r#"{"version":1,"dock_size":60,"dark":true}"#).unwrap();
    let mut p = config::read_at(&path).unwrap();
    assert_eq!(p.dock_size, 60);
    assert!(p.dark && p.dock_magnification && p.dock_show_running);
    assert!(!p.dock_auto_hide);
    p.dock_pinned_apps = vec!["org.example.App.desktop".into()];
    p.dock_order = vec!["settings".into(), "app:org.example.App.desktop".into()];
    p.dock_auto_hide = true;
    config::save_at(&path, &p).unwrap();
    assert_eq!(config::read_at(&path).unwrap(), p);
    assert_eq!(Preferences::default().dock_size, 42);
}
