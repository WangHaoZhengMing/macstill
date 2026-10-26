//! A harmless window client for the isolated Stage Manager compositor test.
use gpui_kit::{prelude::*, *};
struct Fixture {
    name: String,
    tick: u64,
    _refresh: Task<()>,
}
impl Render for Fixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .bg(rgb(0xf0f2fa))
            .text_color(rgb(0x242427))
            .p(px(24.))
            .child(self.name.clone())
            .child(
                div()
                    .mt(px(24.))
                    .w(px(60. + (self.tick % 8) as f32 * 25.))
                    .h(px(60.))
                    .bg(rgb(if self.tick % 2 == 0 {
                        0x247be8
                    } else {
                        0x39c9a3
                    })),
            )
            .child(format!("Live frame {}", self.tick))
            // Solid edge markers expose clipping at any animation position/scale.
            .child(
                div()
                    .absolute()
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .w(px(24.))
                    .bg(rgb(0xf020d0)),
            )
            .child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(px(24.))
                    .bg(rgb(0xf020d0)),
            )
    }
}
fn main() {
    let class = std::env::args().nth(1).unwrap_or_else(|| "Editor".into());
    gpui_kit::application().run(move |cx| {
        macstill_ui::init(cx);
        cx.open_window(
            WindowOptions {
                app_id: Some(format!("org.macstill.StageFixture.{class}")),
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(240.), px(130.)),
                    if class == "Small" {
                        size(px(240.), px(160.))
                    } else {
                        size(px(640.), px(420.))
                    },
                ))),
                ..Default::default()
            },
            |window, cx| {
                window.set_window_title(&format!("Stage fixture — {class}"));
                cx.new(|cx: &mut Context<Fixture>| {
                    let refresh = cx.spawn(async move |this, cx| {
                        loop {
                            cx.background_executor()
                                .timer(std::time::Duration::from_millis(200))
                                .await;
                            if this
                                .update(cx, |s, cx| {
                                    s.tick += 1;
                                    cx.notify();
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                    });
                    Fixture {
                        name: class,
                        tick: 0,
                        _refresh: refresh,
                    }
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
