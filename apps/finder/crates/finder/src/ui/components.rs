//! Reusable controls and asset helpers.
use crate::{
    model::tag::Tag,
    ui::{assets::symbol, theme as t},
};
use gpui_kit::{ClickEvent, div, prelude::*, px, svg};

// Pieces

/// An SF Symbol (or its stand-in) at `pt` points: symbols sit in a square
/// 1.25 em frame, so this is the box that shows them at that size.
pub(crate) fn sym(name: &str, pt: f32, color: impl Into<gpui_kit::Hsla>) -> gpui_kit::Svg {
    svg()
        .path(symbol(name))
        .size(px(pt * 1.25))
        .flex_none()
        .text_color(color.into())
}
pub(crate) fn capsule(id: &'static str) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .h(px(t::CAPSULE_H))
        .flex_none()
        .flex()
        .items_center()
        .px_1()
        .gap_1()
        .rounded_full()
        .bg(t::bar_bg())
}
/// Tag colors as overlapping dots, as Finder shows them beside a name.
pub(crate) fn tag_dots(tags: &[Tag], d: f32) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .children(tags.iter().enumerate().map(move |(i, tag)| {
            div()
                .size(px(d))
                .rounded_full()
                .bg(tag.color())
                .border(px(1.))
                .border_color(t::content_bg())
                .when(i > 0, |el| el.ml(px(-d * 0.4)))
        }))
}
/// Where a button's menu drops from: just below the click.
pub(crate) fn menu_point(e: &ClickEvent) -> gpui_kit::Point<gpui_kit::Pixels> {
    let p = e.position();
    gpui_kit::point(p.x - px(12.), p.y + px(18.))
}
/// One icon button inside a capsule.
pub(crate) fn tool_button(
    id: impl Into<gpui_kit::ElementId>,
    name: &str,
    on: bool,
) -> gpui_kit::component::button::Button {
    use gpui_kit::component::{
        Icon, Selectable, Sizable,
        button::{Button, ButtonVariants},
    };
    Button::new(id)
        .ghost()
        .small()
        .w(px(36.))
        .h(px(30.))
        .flex_none()
        .selected(on)
        // Keep explicit icon sizing: Kit's small-button icon slot otherwise forces 14px.
        .child(Icon::empty().path(symbol(name)).size(px(20.)))
}

impl crate::model::entry::Entry {
    /// The icon asset for this entry.
    pub fn icon(&self) -> &'static str {
        if self.is_dir {
            return "icons/folder.svg";
        }
        match self
            .path
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "exe" | "msi" | "app" | "lnk" => "icons/app.svg",
            _ => "icons/file.svg",
        }
    }
}

/// Convert decoded RGBA pixels to the renderer's BGRA representation.
pub(crate) fn render_image(mut image: image::RgbaImage) -> std::sync::Arc<gpui_kit::RenderImage> {
    for pixel in image.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    std::sync::Arc::new(gpui_kit::RenderImage::new(vec![image::Frame::new(image)]))
}
