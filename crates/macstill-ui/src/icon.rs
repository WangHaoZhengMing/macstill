use gpui_kit::{prelude::*, *};
use std::path::PathBuf;
pub fn icon(path: Option<PathBuf>, size: f32) -> AnyElement {
    if let Some(p) = path {
        return img(p)
            .size(px(size))
            .object_fit(ObjectFit::Contain)
            .into_any_element();
    }
    div()
        .size(px(size))
        .rounded(px(size * 0.22))
        .bg(rgb(0xb6c3dc))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(size * 0.5))
        .text_color(rgb(0xffffff))
        .child("A")
        .into_any_element()
}
