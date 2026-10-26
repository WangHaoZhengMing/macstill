//! Shared overlay scrollbars and precise-input edge bounce for Finder panes.
use gpui_kit::base::ScrollBounce;
use gpui_kit::component::scroll::{Scrollbar, ScrollbarHandle, ScrollbarMode};
use gpui_kit::{div, hsla, prelude::*, px};

pub fn bar<H: ScrollbarHandle + Clone>(
    id: impl Into<gpui_kit::ElementId>,
    handle: &H,
    horizontal: bool,
) -> Scrollbar {
    let scrollbar = if horizontal {
        Scrollbar::horizontal(handle)
    } else {
        Scrollbar::vertical(handle)
    };
    scrollbar
        .id(id)
        .mode(ScrollbarMode::Scrolling)
        .styles(|styles| {
            styles
                .track(|style| style.bg(hsla(0., 0., 0., 0.)).width(px(10.)))
                .thumb(|style| {
                    style
                        .bg(hsla(0., 0., 0.5, 0.55))
                        .width(px(4.))
                        .inset(px(2.))
                        .radius(px(6.))
                })
                .thumb_hover(|style| style.bg(hsla(0., 0., 0.5, 0.8)).width(px(6.)))
                .thumb_active(|style| style.bg(hsla(0., 0., 0.5, 0.9)).width(px(6.)))
        })
}

pub fn vertical<H: ScrollbarHandle + Clone>(
    id: impl Into<gpui_kit::ElementId>,
    handle: &H,
    content: impl IntoElement,
) -> impl IntoElement {
    let id = id.into();
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(ScrollBounce::new((id.clone(), "bounce"), handle, content).enabled(true))
        .child(bar((id, "scrollbar"), handle, false))
}
