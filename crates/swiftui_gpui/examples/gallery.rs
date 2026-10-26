use gpui_kit::{base::input::InputState, prelude::*, *};
use swiftui_gpui::{
    Palette, button, caption, form, panel, section, sidebar, text_field, toggle, traffic_lights,
};

struct Gallery {
    dark: bool,
    enabled: bool,
    clicks: usize,
    input: Entity<InputState>,
}
impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::new(self.dark);
        swiftui_gpui::set_theme(p, cx);
        div()
            .tab_group()
            .size_full()
            .flex()
            .bg(rgb(p.background))
            .text_color(rgb(p.text))
            .font_family(cx.global::<swiftui_gpui::Fonts>().text.clone())
            .child(
                sidebar(p)
                    .child(traffic_lights("gallery-window"))
                    .child(div().mt(px(16.)).child("Components"))
                    .child(caption("Built on gpui-base", p)),
            )
            .child(
                form()
                    .flex_1()
                    .p(px(24.))
                    .child(
                        section("Appearance", p).child(
                            toggle("dark-mode", self.dark, p)
                                .accessibility_label("Dark appearance")
                                .on_change(cx.listener(|s, checked: &bool, _, cx| {
                                    s.dark = *checked;
                                    cx.notify();
                                })),
                        ),
                    )
                    .child(
                        section("Controls", p).child(
                            panel(p)
                                .p(px(16.))
                                .flex()
                                .items_center()
                                .gap(px(12.))
                                .child(
                                    button("apply", format!("Apply ({})", self.clicks), p)
                                        .on_click(cx.listener(|s, _, _, cx| {
                                            s.clicks += 1;
                                            cx.notify();
                                        })),
                                )
                                .child(button("disabled", "Unavailable", p).disabled(true))
                                .child(
                                    toggle("enabled", self.enabled, p)
                                        .accessibility_label("Enabled")
                                        .on_change(cx.listener(|s, checked: &bool, _, cx| {
                                            s.enabled = *checked;
                                            cx.notify();
                                        })),
                                ),
                        ),
                    )
                    .child(section("Text field", p).child(text_field("name", &self.input, p, cx))),
            )
    }
}
fn main() {
    gpui_kit::application().run(|cx| {
        swiftui_gpui::init(cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(800.), px(480.)),
                    cx,
                ))),
                titlebar: None,
                ..Default::default()
            },
            |w, cx| {
                w.set_window_title("swiftui_gpui — Components");
                let input = cx.new(|cx| InputState::new(w, cx).placeholder("Type here…"));
                cx.new(|_| Gallery {
                    dark: false,
                    enabled: true,
                    clicks: 0,
                    input,
                })
            },
        )
        .unwrap();
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}
