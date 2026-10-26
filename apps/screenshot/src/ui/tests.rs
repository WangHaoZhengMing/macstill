use super::{Overlay, Session, image};
use crate::{
    capture::{self, Desktop},
    model::{Mode, Preferences, Rect, Request},
};
use gpui_kit::{px, size};
use std::{path::PathBuf, sync::Arc};
#[test]
#[ignore = "requires offscreen GPU rendering; saves screenshot UI review PNGs"]
fn screenshot_visual_review() {
    use gpui_kit::{AppContext, HeadlessAppContext};
    let mut context = HeadlessAppContext::with_platform(
        gpui_kit::platform::current_platform(true).text_system(),
        Arc::new(()),
        gpui_platform_test::current_headless_renderer,
    );
    let pixels = image::RgbaImage::from_fn(1280, 800, |x, y| {
        image::Rgba([(25 + x / 9) as u8, (45 + y / 6) as u8, 150, 255])
    });
    let background = image(&pixels);
    let session = context.update(|cx| {
        macstill_ui::init(cx);
        cx.new(|_| Session {
            desktop: Some(Arc::new(Desktop {
                screens: vec![capture::Screen {
                    name: "fixture".into(),
                    rect: Rect {
                        x: 0.,
                        y: 0.,
                        w: 1280.,
                        h: 800.,
                    },
                    pixels: Arc::new(pixels),
                    focused: true,
                }],
                windows: vec![],
            })),
            request: Request::default(),
            prefs: Preferences::default(),
            mode: Mode::Region,
            area: Some(Rect {
                x: 250.,
                y: 150.,
                w: 700.,
                h: 420.,
            }),
            drag: None,
            hovered: None,
            screen: 0,
            options: false,
            error: None,
            busy: false,
            windows: vec![],
        })
    });
    let view = context
        .open_window(size(px(1280.), px(800.)), |w, cx| {
            cx.new(|cx| {
                let focus = cx.focus_handle();
                w.focus(&focus, cx);
                let observe = cx.observe(&session, |_, _, cx| cx.notify());
                Overlay {
                    session: session.clone(),
                    index: 0,
                    background,
                    focus,
                    _observe: observe,
                }
            })
        })
        .unwrap();
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/screenshot-review");
    std::fs::create_dir_all(&directory).unwrap();
    for (name, options) in [("selection", false), ("options", true)] {
        context.update(|cx| {
            session.update(cx, |s, cx| {
                s.options = options;
                cx.notify();
            })
        });
        context.run_until_parked();
        context
            .update_window(view.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        context
            .capture_screenshot(view.into())
            .unwrap()
            .save(directory.join(format!("{name}.png")))
            .unwrap();
    }
}
