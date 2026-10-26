//! Shared macOS-style context-menu presentation, extracted from the shell's Dock.
//! Callers own opening, placement, dismissal, and command execution.
use crate::{Button, Palette};
use gpui_kit::{prelude::*, *};

pub fn menu(id: impl Into<ElementId>, p: Palette) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::Menu)
        .tab_group()
        .w(px(224.))
        .p(px(5.))
        .rounded(px(9.))
        .border_1()
        .border_color(if p.dark {
            rgba(0xffffff26)
        } else {
            rgba(0x00000024)
        })
        .bg(p.material(crate::theme::Material::Menu))
        .shadow_lg()
        .text_size(px(13.))
        .text_color(rgb(p.text))
        .flex()
        .flex_col()
        .gap(px(1.))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .on_click(|_, _, cx| cx.stop_propagation())
}

/// A neutral menu command. Unlike a push button, it has no individual border.
/// Retains the underlying button's keyboard activation and disabled behavior.
pub fn menu_item(id: impl Into<ElementId>, label: impl Into<SharedString>, p: Palette) -> Button {
    let label = label.into();
    Button::new(id)
        .role(Role::MenuItem)
        .accessibility_label(label.clone())
        .w_full()
        .h(px(25.))
        .px(px(9.))
        .rounded(px(5.))
        .flex()
        .items_center()
        .justify_start()
        .gap(px(12.))
        .text_size(px(13.))
        .text_color(rgb(p.text))
        .cursor_pointer()
        .hover(move |d| d.bg(rgb(p.accent)).text_color(rgb(p.accent_text)))
        .focus_visible(move |d| d.bg(rgb(p.accent)).text_color(rgb(p.accent_text)))
        .styles(|s| s.disabled(|s| s.opacity(0.4).cursor_default()))
        .child(div().flex_1().min_w_0().truncate().child(label))
}
pub fn menu_separator(p: Palette) -> Div {
    div()
        .h(px(1.))
        .flex_shrink_0()
        .mx(px(7.))
        .my(px(4.))
        .bg(if p.dark {
            rgba(0xffffff24)
        } else {
            rgba(0x00000020)
        })
}
pub fn menu_header(label: impl Into<SharedString>, p: Palette) -> Div {
    div()
        .px(px(9.))
        .py(px(5.))
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(p.muted))
        .child(label.into())
}
