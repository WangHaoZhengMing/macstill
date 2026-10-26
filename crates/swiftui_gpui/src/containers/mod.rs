use crate::Palette;
use gpui_kit::{prelude::*, *};

pub fn panel(p: Palette) -> Div {
    div()
        .bg(rgb(p.surface))
        .border_1()
        .border_color(rgb(p.line))
        .rounded(px(12.))
        .overflow_hidden()
}
pub fn caption(label: impl Into<SharedString>, p: Palette) -> Div {
    div()
        .text_size(px(12.))
        .text_color(rgb(p.muted))
        .child(label.into())
}
/// A vertical form; children can be sections or application-owned rows.
pub fn form() -> Div {
    div().flex().flex_col().gap(px(18.))
}
pub fn section(title: impl Into<SharedString>, p: Palette) -> Div {
    form()
        .gap(px(8.))
        .child(caption(title, p).font_weight(FontWeight::SEMIBOLD))
}
pub fn sidebar(p: Palette) -> Div {
    div()
        .w(px(232.))
        .h_full()
        .flex_shrink_0()
        .bg(rgb(p.sidebar))
        .border_r_1()
        .border_color(rgb(p.line))
        .p(px(14.))
        .flex()
        .flex_col()
        .gap(px(4.))
}
