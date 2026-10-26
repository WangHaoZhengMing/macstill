use crate::Palette;
pub use gpui_kit::base::Button;
use gpui_kit::{prelude::*, *};

/// A bordered push button with gpui-base's focus, keyboard, and disabled behavior.
pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, p: Palette) -> Button {
    let label = label.into();
    Button::new(id)
        .accessibility_label(label.clone())
        .px(px(12.))
        .py(px(6.))
        .rounded(px(7.))
        .border_1()
        .border_color(rgb(p.line))
        .bg(rgb(p.surface))
        .text_color(rgb(p.text))
        .text_size(px(12.))
        .cursor_pointer()
        .hover(move |s| s.bg(rgb(p.hover)))
        .active(move |s| s.bg(rgb(p.sidebar)))
        .focus_visible(move |s| s.border_color(rgb(p.accent)))
        .styles(|s| s.disabled(|s| s.opacity(0.45).cursor_default()))
        .child(label)
}
