use crate::Palette;
use gpui_kit::{
    base::input::{Input, InputState},
    prelude::*,
    *,
};

/// Styles the existing single-line editor without replacing its editing state.
/// Placeholder, change subscriptions, and validation remain on `InputState`.
pub fn text_field(
    id: impl Into<ElementId>,
    state: &Entity<InputState>,
    p: Palette,
    cx: &App,
) -> Stateful<Div> {
    let focus = state.focus_handle(cx);
    let state = state.clone();
    div()
        .id(id)
        .track_focus(&focus)
        .flex()
        .items_center()
        .min_w_0()
        .h(px(32.))
        .px(px(10.))
        .rounded(px(8.))
        .border_1()
        .border_color(rgb(p.line))
        .bg(rgb(p.surface))
        .text_color(rgb(p.text))
        .text_size(px(13.))
        .focus(move |s| s.border_color(rgb(p.accent)))
        .child(div().flex_1().min_w_0().child(Input::new(&state)))
        .on_click(move |_, w, cx| state.update(cx, |s, cx| s.focus(w, cx)))
}
