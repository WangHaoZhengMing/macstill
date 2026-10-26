//! Compact translucent surfaces shared by desktop popovers.
use crate::{Palette, caption};
use gpui_kit::{
    base::slider::{Slider, SliderIndicator, SliderState, SliderTrack},
    prelude::*,
    *,
};

// The compositor filters the backdrop; these layers supply tint and rims.
// Keep the fill translucent so wallpaper color survives the blur underneath.
// Avoid exterior shadows: layer-shell backdrop blur can include their alpha,
// creating a broad dark halo outside the panel.
pub fn control_center_panel(p: Palette) -> Div {
    let (top, bottom, outer, inner) = if p.dark {
        (0x24242c94, 0x181820ad, 0x00000020, 0xffffff10)
    } else {
        (0xf5f6fca0, 0xe2e5ef94, 0x34344316, 0xffffff28)
    };
    div()
        .w(px(315.))
        .p(px(10.))
        .rounded(px(18.))
        .border_1()
        .border_color(rgba(outer))
        .bg(linear_gradient(
            180.,
            linear_color_stop(rgba(top), 0.),
            linear_color_stop(rgba(bottom), 1.),
        ))
        .shadow(vec![
            BoxShadow::new(px(0.), px(1.), rgba(inner).into())
                .blur_radius(px(1.))
                .inset(),
        ])
        .flex()
        .flex_col()
        .gap(px(8.))
        .text_color(rgb(p.text))
        .text_size(px(12.))
}
pub fn control_center_tile(p: Palette) -> Div {
    let (top, bottom, edge) = if p.dark {
        (0xffffff1f, 0xffffff10, 0xffffff0e)
    } else {
        (0xffffff66, 0xffffff38, 0xffffff18)
    };
    div()
        .rounded(px(12.))
        .border_1()
        .border_color(rgba(edge))
        .bg(linear_gradient(
            180.,
            linear_color_stop(rgba(top), 0.),
            linear_color_stop(rgba(bottom), 1.),
        ))
        .overflow_hidden()
}
pub fn control_center_slider(
    label: &'static str,
    state: &Entity<SliderState>,
    available: bool,
    p: Palette,
    cx: &App,
) -> Div {
    control_center_slider_content(label, state, available, p, cx, None, None)
}

/// Slider with caller-supplied glyph and an optional audio-output control.
pub fn control_center_slider_content(
    label: &'static str,
    state: &Entity<SliderState>,
    available: bool,
    p: Palette,
    cx: &App,
    icon: Option<AnyElement>,
    trailing: Option<AnyElement>,
) -> Div {
    let value = (state.read(cx).value().end() / 100.).clamp(0., 1.);
    let slider = Slider::new(state)
        .accessibility_label(label)
        .disabled(!available)
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .h(px(24.))
        .rounded_full()
        .border_1()
        .border_color(rgba(0x00000016))
        .overflow_hidden()
        .bg(if p.dark {
            rgba(0x00000028)
        } else {
            rgba(0x55556b22)
        })
        .child(
            SliderTrack::new(state)
                .disabled(!available)
                .w_full()
                .h(px(24.))
                .flex()
                .items_center()
                .relative()
                .child(
                    SliderIndicator::new(state)
                        .relative()
                        .w_full()
                        .h(px(24.))
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .h_full()
                                .w(relative(value))
                                .min_w(px(24.))
                                .rounded_full()
                                .bg(rgb(0xffffff))
                                .flex()
                                .justify_end()
                                .items_center()
                                .child(
                                    div()
                                        .size(px(23.))
                                        .flex_shrink_0()
                                        .rounded_full()
                                        .border_1()
                                        .border_color(rgba(0x00000020))
                                        .bg(rgb(0xffffff))
                                        .shadow_sm(),
                                ),
                        ),
                )
                .when_some(icon, |track, icon| {
                    track.child(
                        div()
                            .absolute()
                            .left(px(7.))
                            .top(px(5.))
                            .size(px(13.))
                            .child(icon),
                    )
                }),
        );
    control_center_tile(p)
        .p(px(10.))
        .text_size(px(12.))
        .line_height(relative(1.2))
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div()
                .flex()
                .justify_between()
                .child(label)
                .when(!available, |d| {
                    d.child(caption("Unavailable", p).text_size(px(10.)))
                }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(slider)
                .when_some(trailing, |d, control| d.child(control)),
        )
}
