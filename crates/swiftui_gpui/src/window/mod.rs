use gpui_kit::{prelude::*, *};

/// Window controls scoped by ID, usable in any custom title bar.
pub fn traffic_lights(id: impl Into<ElementId>) -> Div {
    let id = id.into();
    let light = |name: &'static str, label: &'static str, color: u32| {
        gpui_kit::base::Button::new((id.clone(), name))
            .accessibility_label(label)
            .size(px(12.))
            .flex_shrink_0()
            .rounded_full()
            .bg(rgb(color))
            .focus_visible(|s| s.border_1().border_color(rgb(0x3478f6)))
    };
    div()
        .flex()
        .gap(px(8.))
        .h(px(32.))
        .items_center()
        .child(light("close", "Close window", 0xff5f57).on_click(|_, w, _| w.remove_window()))
        .child(
            light("minimize", "Minimize window", 0xfebc2e).on_click(|_, w, _| w.minimize_window()),
        )
        .child(light("zoom", "Zoom window", 0x28c840).on_click(|_, w, _| w.zoom_window()))
}
