//! The gallery file view.
use super::*;

impl Browser {
    // Gallery

    pub(in crate::browser) fn gallery(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.selection.cursor.is_none() && !self.listing.rows.is_empty() {
            self.selection.cursor = Some(0);
        }
        let w = self.content_width(window);
        self.view.strip_width = w;
        let preview = match self
            .selection
            .cursor
            .and_then(|i| self.listing.rows.get(i))
            .map(|r| r.entry.clone())
        {
            Some(e) => {
                let pictured = !e.is_dir && crate::services::icons::has_thumbnail(&e.path);
                let src = self.icon_source(&e, if pictured { 1024 } else { 256 }, cx);
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(24.))
                    .child(
                        img(src)
                            .when(pictured, |i| i.size_full())
                            .when(!pictured, |i| i.size(px(256.)))
                            .object_fit(gpui_kit::ObjectFit::Contain),
                    )
                    .into_any_element()
            }
            None => div().flex_1().into_any_element(),
        };
        // Only the tiles in view are built; spacers stand in for the rest.
        let n = self.listing.rows.len();
        let off = -(self.view.strip_scroll.offset().x / px(1.));
        let first = ((off / TILE).floor().max(0.) as usize).min(n);
        let last = (first + (w / TILE).ceil() as usize + 2).min(n);
        let focused = self.focus.is_focused(window);
        let mut strip = div()
            .id("filmstrip")
            .h(px(STRIP_H))
            .flex_none()
            .flex()
            .items_center()
            .px(px(12.))
            .overflow_x_scroll()
            .track_scroll(&self.view.strip_scroll)
            .border_t_1()
            .border_color(t::separator())
            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
            .child(div().w(px(first as f32 * TILE)).h(px(1.)).flex_none());
        for i in first..last {
            let e = self.listing.rows[i].entry.clone();
            let src = self.icon_source(&e, 128, cx);
            let sel = self.selection.cursor == Some(i);
            let drag = self.drag_value(&e, self.is_selected(i), cx);
            strip = strip.child(
                div()
                    .id(("tile", i))
                    .w(px(TILE))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .size(px(74.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .when(sel, |d| {
                                d.bg(if focused {
                                    t::selection()
                                } else {
                                    t::selection_inactive()
                                })
                            })
                            .child(
                                img(src)
                                    .size(px(64.))
                                    .object_fit(gpui_kit::ObjectFit::Contain),
                            ),
                    )
                    .child(
                        div()
                            .w(px(TILE - 6.))
                            .text_size(px(t::SMALL_SIZE))
                            .text_center()
                            .truncate()
                            .child(e.name.clone()),
                    )
                    .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                        window.focus(&this.focus, cx);
                        if ev.click_count() >= 2 {
                            this.open_row(i, cx)
                        } else {
                            this.select(i, cx)
                        }
                    }))
                    .when(self.capabilities().copy, |el| dnd::drag_source(el, drag)),
            );
        }
        strip = strip.child(div().w(px((n - last) as f32 * TILE)).h(px(1.)).flex_none());
        div()
            .flex_1()
            .min_h_0()
            .pt(px(self.band_h()))
            .flex()
            .flex_col()
            .child(preview)
            .child(
                div()
                    .relative()
                    .h(px(STRIP_H))
                    .flex_none()
                    .overflow_hidden()
                    .child(strip.lock_scroll_axis())
                    .child(
                        gpui_kit::component::scroll::ScrollableMask::vertical_wheel(
                            &self.view.strip_scroll,
                        )
                        .id("gallery-wheel"),
                    )
                    .child(crate::ui::scrolling::bar(
                        "gallery-bar",
                        &self.view.strip_scroll,
                        true,
                    )),
            )
            .into_any_element()
    }
}

const STRIP_H: f32 = 118.;
pub(super) const TILE: f32 = 86.;
