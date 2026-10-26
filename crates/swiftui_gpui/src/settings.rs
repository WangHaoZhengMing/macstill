//! Reusable System Settings presentation. Callers own navigation and preferences.
use crate::{Button, Palette, caption, panel};
use gpui_kit::{prelude::*, *};

pub fn settings_sidebar(p: Palette) -> Div {
    div()
        .w(px(216.))
        .h_full()
        .flex_shrink_0()
        .bg(rgb(p.sidebar))
        .border_r_1()
        .border_color(rgb(p.line))
        .px(px(10.))
        .pb(px(10.))
        .flex()
        .flex_col()
        .gap(px(2.))
}

/// Keyboard-accessible navigation row; attach the application's navigation listener.
pub fn settings_navigation(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    icon: impl IntoElement,
    selected: bool,
    p: Palette,
) -> Button {
    let label = label.into();
    Button::new(id)
        .accessibility_label(label.clone())
        .h(px(30.))
        .flex_shrink_0()
        .w_full()
        .px(px(7.))
        .rounded(px(5.))
        .flex()
        .gap(px(8.))
        .items_center()
        .justify_start()
        .cursor_pointer()
        .bg(if selected {
            rgb(p.accent).into()
        } else {
            transparent_black()
        })
        .text_color(rgb(if selected { p.accent_text } else { p.text }))
        .hover(move |s| s.bg(rgb(if selected { p.accent } else { p.hover })))
        .focus_visible(move |s| s.border_1().border_color(rgb(p.accent)))
        .child(div().size(px(22.)).flex_shrink_0().child(icon))
        .child(div().text_size(px(13.)).child(label))
}

pub fn settings_toolbar(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    p: Palette,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(52.))
        .flex_shrink_0()
        .px(px(22.))
        .flex()
        .items_center()
        .text_color(rgb(p.text))
        .on_mouse_down(MouseButton::Left, |_, w, _| w.start_window_move())
        .child(
            div()
                .text_size(px(16.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.into()),
        )
}

pub fn settings_group(p: Palette) -> Div {
    panel(p).rounded(px(10.)).flex().flex_col()
}

/// Label on the left, application-owned controls on the right.
pub fn settings_row(label: impl Into<SharedString>, p: Palette) -> Div {
    div()
        .min_h(px(42.))
        .px(px(12.))
        .py(px(8.))
        .flex()
        .items_center()
        .gap(px(14.))
        .text_color(rgb(p.text))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(13.))
                .child(label.into()),
        )
}

pub fn settings_divider(p: Palette) -> Div {
    div().mx(px(14.)).h(px(0.5)).flex_shrink_0().bg(rgb(p.line))
}

/// A small desktop preview with a selected border, styled like macOS Appearance.
pub fn appearance_option(
    id: impl Into<ElementId>,
    dark: bool,
    selected: bool,
    p: Palette,
) -> Button {
    let label = if dark { "Dark" } else { "Light" };
    let (sky, wave, window, sidebar, line) = if dark {
        ("#162139", "#da6223", "#242426", "#303033", "#4d4d51")
    } else {
        ("#66c8f2", "#ffb235", "#ffffff", "#ececef", "#dcdcdf")
    };
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="80" viewBox="0 0 120 80"><defs><linearGradient id="sky" x2="1" y2="1"><stop stop-color="{sky}"/><stop offset="1" stop-color="#216bb3"/></linearGradient><linearGradient id="wave" x2=".2" y2="1"><stop stop-color="{wave}"/><stop offset="1" stop-color="#d64622"/></linearGradient></defs><rect width="120" height="80" rx="5" fill="url(#sky)"/><path d="M0 14C40 38 58-4 120 9V80H0Z" fill="url(#wave)"/><path d="M0 38C42 61 50 6 120 22" fill="none" stroke="#ffdc76" stroke-width="8" opacity=".7"/><rect x="0" y="0" width="120" height="7" fill="{window}" opacity=".8"/><circle cx="6" cy="3.5" r="1.4" fill="#25252a"/><rect x="8" y="13" width="70" height="14" rx="3" fill="{window}" stroke="white" stroke-opacity=".5"/><rect x="28" y="34" width="84" height="45" rx="5" fill="{window}"/><path d="M33 34h20v45H33a5 5 0 0 1-5-5V39a5 5 0 0 1 5-5Z" fill="{sidebar}"/><circle cx="35" cy="41" r="2.4" fill="#ff6058"/><circle cx="43" cy="41" r="2.4" fill="#ffbd2e"/><circle cx="51" cy="41" r="2.4" fill="#28c840"/><rect x="33" y="50" width="15" height="4" rx="1" fill="#4b8ef5"/><rect x="33" y="59" width="15" height="3" rx="1" fill="{line}"/><rect x="60" y="50" width="43" height="8" rx="2" fill="{sidebar}"/><rect x="60" y="63" width="43" height="8" rx="2" fill="{sidebar}"/></svg>"##
    );
    Button::new(id)
        .accessibility_label(format!("{label} appearance"))
        .w(px(100.))
        .flex()
        .flex_col()
        .gap(px(6.))
        .cursor_pointer()
        .focus_visible(move |s| s.rounded(px(7.)).bg(rgb(p.hover)))
        .child(
            div()
                .w_full()
                .h(px(70.))
                .p(px(3.))
                .rounded(px(8.))
                .border_2()
                .border_color(rgb(if selected { p.accent } else { p.line }))
                .child(
                    img(std::sync::Arc::new(Image::from_bytes(
                        ImageFormat::Svg,
                        svg.into_bytes(),
                    )))
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .rounded(px(4.)),
                ),
        )
        .child(
            caption(label, p)
                .w_full()
                .text_center()
                .text_size(px(11.))
                .text_color(rgb(if selected { p.accent } else { p.text })),
        )
}
