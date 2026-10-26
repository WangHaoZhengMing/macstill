use super::{About, Data, SystemReport, Windows, tree_rows};
use crate::model::{Category, Item, Report, Snapshot};
use gpui_test::{self as gpui, TestAppContext, VisualTestContext};
use gpui_test::{ElementId, Entity, Modifiers, prelude::*, px, size};

fn fixture() -> Snapshot {
    Snapshot {
        model: "Macstill Notebook".into(),
        vendor: "Example Hardware".into(),
        version: "16-inch".into(),
        processor: "8-Core Intel Core i9".into(),
        graphics: "AMD Radeon Pro · 8 GB".into(),
        memory: "32.0 GB".into(),
        startup_disk: "/dev/nvme0n1p2".into(),
        serial: "Not available".into(),
        operating_system: "Test Linux 1.0".into(),
        kernel: "6.12.0".into(),
        reports: vec![
            Report {
                category: Category::Hardware,
                items: vec![
                    Item::new("hardware", "Hardware Overview")
                        .field("Model Name", "Macstill Notebook")
                        .field("Processor", "8-Core Intel Core i9")
                        .field("Memory", "32.0 GB"),
                ],
                note: None,
            },
            Report {
                category: Category::Usb,
                items: vec![
                    Item::new("usb1", "USB 3.1 Bus")
                        .field("Manufacturer", "Linux Foundation")
                        .field("Speed (Mb/s)", "10000"),
                    Item {
                        parent: Some("usb1".into()),
                        ..Item::new("1-2", "Bus Pirate 5")
                            .field("Product ID", "7332")
                            .field("Vendor ID", "1209")
                            .field("Version", "1.01")
                            .field("Serial Number", "Example device")
                            .field("Speed (Mb/s)", "12")
                            .field("Manufacturer", "Bus Pirate")
                            .field("Bus", "1")
                            .field("Device Number", "42")
                            .field("Maximum Power", "100mA")
                    },
                ],
                note: None,
            },
            Report {
                category: Category::Memory,
                items: vec![],
                note: Some("No memory information is exposed.".into()),
            },
        ],
    }
}

fn install(cx: &mut gpui_test::App, dark: bool) -> Entity<Data> {
    macstill_ui::init(cx);
    cx.set_global(Windows::default());
    cx.new(|_| Data {
        snapshot: fixture(),
        loading: false,
        dark,
    })
}

fn click(cx: &mut VisualTestContext, id: impl Into<ElementId>) {
    let id = id.into();
    let position = cx.update(|w, _| {
        gpui_base::test_support::find(w, &[], &id)
            .unwrap()
            .bounds()
            .center()
    });
    cx.simulate_click(position, Modifiers::default());
    cx.update(|w, cx| w.draw(cx).clear(cx));
}

#[gpui::test]
fn more_info_opens_a_separate_report_and_reuses_it(cx: &mut TestAppContext) {
    let data = cx.update(|cx| install(cx, false));
    let (about, cx) = cx.add_window_view(|_, cx| About::new(data, cx));
    cx.update(|w, cx| {
        w.resize(size(px(350.), px(620.)));
        w.bounds_changed(cx);
        w.draw(cx).clear(cx);
    });
    let before = cx.update(|_, cx| cx.windows().len());
    click(cx, "more-info");
    cx.update(|_, cx| {
        assert!(cx.global::<Windows>().report.is_some());
        assert_eq!(cx.windows().len(), before + 1);
        assert!(about.read(cx).error.is_none());
    });
    click(cx, "more-info");
    cx.update(|_, cx| assert_eq!(cx.windows().len(), before + 1));
    cx.update(|_, cx| {
        cx.global::<Windows>()
            .report
            .unwrap()
            .update(cx, |_, w, _| w.remove_window())
            .unwrap();
    });
    click(cx, "more-info");
    cx.update(|_, cx| assert_eq!(cx.windows().len(), before + 1));
}

#[gpui::test]
fn hardware_sidebar_and_usb_selection_update_the_report_and_groups_collapse(
    cx: &mut TestAppContext,
) {
    let data = cx.update(|cx| install(cx, false));
    let (view, cx) = cx.add_window_view(|_, cx| SystemReport::new(data, cx));
    cx.update(|w, cx| {
        w.resize(size(px(1000.), px(740.)));
        w.bounds_changed(cx);
        w.draw(cx).clear(cx);
    });
    click(cx, "info-category-USB");
    click(cx, "info-item-1-2");
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).category, Category::Usb);
        assert_eq!(view.read(cx).selected.as_deref(), Some("1-2"));
    });
    click(cx, "info-category-Memory");
    cx.update(|_, cx| assert!(view.read(cx).selected.is_none()));
    click(cx, ("info-group", 0usize));
    cx.update(|w, _| {
        assert!(gpui_base::test_support::find(w, &[], &"info-category-USB".into()).is_none())
    });
}

#[test]
fn usb_tree_order_places_parents_before_children_and_keeps_orphans() {
    let mut report = fixture().report(Category::Usb).unwrap().clone();
    report.items.reverse();
    report.items.push(Item {
        parent: Some("missing".into()),
        ..Item::new("orphan", "Orphan")
    });
    let rows = tree_rows(&report);
    assert_eq!(report.items[rows[0].0].id, "usb1");
    assert_eq!(report.items[rows[1].0].id, "1-2");
    assert_eq!(rows[1].1, 1);
    assert_eq!(rows.len(), 3);
}

#[test]
#[ignore = "requires a real offscreen GPU renderer; writes About and System Information PNGs"]
fn info_visual_review() {
    use gpui_kit::{AppContext, HeadlessAppContext};
    let mut context = HeadlessAppContext::with_platform(
        gpui_kit::platform::current_platform(true).text_system(),
        std::sync::Arc::new(()),
        gpui_platform_test::current_headless_renderer,
    );
    let data = context.update(|cx| install(cx, false));
    let about_data = data.clone();
    let about = context
        .open_window(size(px(350.), px(620.)), |_, cx| {
            cx.new(|cx| About::new(about_data, cx))
        })
        .unwrap();
    let report = context
        .open_window(size(px(1000.), px(740.)), |_, cx| {
            cx.new(|cx| SystemReport::new(data.clone(), cx))
        })
        .unwrap();
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/info-review");
    std::fs::create_dir_all(&directory).unwrap();
    for (name, dark, small) in [
        ("light", false, false),
        ("dark", true, false),
        ("small", false, true),
    ] {
        context.update(|cx| {
            data.update(cx, |data, cx| {
                data.dark = dark;
                cx.notify();
            })
        });
        report
            .update(&mut context, |view, w, cx| {
                view.category = Category::Usb;
                view.selected = Some("1-2".into());
                w.resize(if small {
                    size(px(640.), px(460.))
                } else {
                    size(px(1000.), px(740.))
                });
                w.bounds_changed(cx);
                cx.notify();
            })
            .unwrap();
        context.run_until_parked();
        for (window, prefix) in [(about.into(), "about"), (report.into(), "report")] {
            context
                .update_window(window, |_, w, cx| w.draw(cx).clear(cx))
                .unwrap();
            context
                .capture_screenshot(window)
                .unwrap()
                .save(directory.join(format!("{prefix}-{name}.png")))
                .unwrap();
        }
    }
}
