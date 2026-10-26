//! Shared Finder file list for folders, trash and search results.
use super::*;

impl Browser {
    // Expanding folders in place

    pub(in crate::browser) fn expand(&mut self, ix: usize, cx: &mut Context<Self>) {
        if !self.capabilities().open || self.search.session.is_some() {
            return;
        }
        let Some(r) = self.listing.rows.get_mut(ix) else {
            return;
        };
        if !r.entry.is_dir || r.expanded {
            return;
        }
        r.expanded = true;
        self.refresh(&[], cx);
    }
    pub(in crate::browser) fn collapse(&mut self, ix: usize) {
        let depth = self.listing.rows[ix].depth;
        let end = (ix + 1..self.listing.rows.len())
            .find(|&j| self.listing.rows[j].depth <= depth)
            .unwrap_or(self.listing.rows.len());
        self.listing.rows.drain(ix + 1..end);
        self.listing.rows[ix].expanded = false;
        if let Some(s) = self.selection.cursor
            && s > ix
        {
            self.selection.cursor = if s < end {
                Some(ix)
            } else {
                Some(s - (end - ix - 1))
            };
        }
    }
    pub(in crate::browser) fn toggle(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.listing.rows[ix].expanded {
            self.collapse(ix)
        } else {
            self.expand(ix, cx)
        }
        cx.notify();
    }
    pub(in crate::browser) fn column_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let col = |id: &'static str, label: &'static str, key: SortKey, cx: &mut Context<Self>| {
            let on = self.listing.sort.0 == key;
            div()
                .id(id)
                .h_full()
                .flex_shrink_0()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(4.))
                .px(px(8.))
                .when(on, |d| {
                    d.font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .text_color(t::text())
                })
                .child(div().flex_1().truncate().child(label))
                .when(on, |d| {
                    d.child(sym(
                        if self.listing.sort.1 {
                            "chevron.up@semibold"
                        } else {
                            "chevron.down@semibold"
                        },
                        8.,
                        t::text_secondary(),
                    ))
                })
                .on_click(cx.listener(move |this, _, _, cx| this.set_sort(key, cx)))
        };
        let divider = || div().w(px(1.)).h(px(14.)).flex_none().bg(t::separator());
        div()
            .h(px(t::HEADER_H))
            .flex_none()
            .flex()
            .items_center()
            .px(px(t::ROW_INSET))
            .border_b_1()
            .border_color(t::separator())
            .text_size(px(t::HEADER_SIZE))
            .text_color(t::text_secondary())
            .child(
                col("h-name", "Name", SortKey::Name, cx)
                    .flex_1()
                    .min_w_0()
                    .pl(px(46.)),
            )
            .child(divider())
            .child(
                col(
                    "h-date",
                    if self.in_trash() {
                        "Date Deleted"
                    } else {
                        "Date Modified"
                    },
                    SortKey::Modified,
                    cx,
                )
                .w(px(self.view.list_columns.date - 1.)),
            )
            .child(divider())
            .child(col("h-size", "Size", SortKey::Size, cx).w(px(self.view.list_columns.size - 1.)))
            .child(divider())
            .child(col("h-kind", "Kind", SortKey::Kind, cx).w(px(self.view.list_columns.kind - 1.)))
    }
    pub(in crate::browser) fn render_rows(
        &mut self,
        range: std::ops::Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let focused = self.focus.is_focused(window);
        let searching = self.search.session.is_some();
        range
            .map(|ix| {
                // Past the last row: Finder keeps the stripes going.
                if ix >= self.listing.rows.len() {
                    return div()
                        .id(("filler", ix))
                        .w_full()
                        .h(px(t::ROW_H))
                        .px(px(t::ROW_INSET))
                        .child(
                            div()
                                .size_full()
                                .rounded(px(t::ROW_RADIUS))
                                .when(ix % 2 == 1, |d| d.bg(t::zebra())),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            window.focus(&this.focus, cx);
                            this.selection.cursor = None;
                            this.selection.marked.clear();
                            cx.notify();
                        }))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(|this, e: &gpui_kit::MouseDownEvent, window, cx| {
                                this.open_menu(None, e.position, window, cx)
                            }),
                        )
                        .into_any_element();
                }
                let entry = self.listing.rows[ix].entry.clone();
                let src = self.icon_source(&entry, 32, cx);
                let sel = self.is_selected(ix);
                let drag = self.drag_value(&entry, sel, cx);
                let r = &self.listing.rows[ix];
                let lit = sel && focused;
                let rename = self.rename_box(&entry.path);
                let (fg, fg2): (gpui_kit::Hsla, gpui_kit::Hsla) = if lit {
                    (gpui_kit::white(), gpui_kit::white().opacity(0.85))
                } else {
                    (t::text().into(), t::text_secondary().into())
                };
                let indent = r.depth as f32 * t::INDENT;
                div()
                    .w_full()
                    .px(px(t::ROW_INSET))
                    .flex()
                    .child(
                        div()
                            .id(("row", ix))
                            .flex_1()
                            .min_w_0()
                            .h(px(t::ROW_H))
                            .flex()
                            .items_center()
                            .rounded(px(t::ROW_RADIUS))
                            .when(ix % 2 == 1, |d| d.bg(t::zebra()))
                            .when(sel, |d| {
                                d.bg(if focused {
                                    t::selection()
                                } else {
                                    t::selection_inactive()
                                })
                            })
                            .text_size(px(t::TEXT_SIZE))
                            .text_color(fg)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .pl(px(4. + indent))
                                    .child(
                                        div()
                                            .id(("disclosure", ix))
                                            .w(px(18.))
                                            .h_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .when(r.entry.is_dir && !searching, |d| {
                                                d.child(sym(
                                                    if r.expanded {
                                                        "chevron.down@semibold"
                                                    } else {
                                                        "chevron.right@semibold"
                                                    },
                                                    8.,
                                                    fg2,
                                                ))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    cx.stop_propagation();
                                                    this.toggle(ix, cx);
                                                }))
                                            }),
                                    )
                                    .child(
                                        img(src)
                                            .size(px(t::ROW_ICON))
                                            .flex_none()
                                            .ml(px(2.))
                                            .mr(px(6.)),
                                    )
                                    .child(match rename {
                                        Some(b) => b,
                                        None => div()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .items_center()
                                            .gap(px(5.))
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .truncate()
                                                    .child(r.entry.name.clone()),
                                            )
                                            .child(tag_dots(self.tags.of(&r.entry.path), 8.))
                                            .into_any_element(),
                                    }),
                            )
                            .child(
                                div()
                                    .w(px(self.view.list_columns.date))
                                    .flex_none()
                                    .px(px(9.))
                                    .truncate()
                                    .text_color(fg2)
                                    .child(files::format_date(r.entry.modified)),
                            )
                            .child(
                                div()
                                    .w(px(self.view.list_columns.size))
                                    .flex_none()
                                    .px(px(9.))
                                    .flex()
                                    .justify_end()
                                    .text_color(fg2)
                                    .child(files::format_size(&r.entry)),
                            )
                            .child(
                                div()
                                    .w(px(self.view.list_columns.kind))
                                    .flex_none()
                                    .px(px(9.))
                                    .truncate()
                                    .text_color(fg2)
                                    .child(r.entry.kind.clone()),
                            )
                            .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| {
                                window.focus(&this.focus, cx);
                                if e.click_count() >= 2 {
                                    this.open_row(ix, cx);
                                } else {
                                    this.click_select(ix, &e.modifiers(), cx);
                                }
                            }))
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(
                                    move |this, e: &gpui_kit::MouseDownEvent, window, cx| {
                                        this.open_menu(Some(ix), e.position, window, cx)
                                    },
                                ),
                            )
                            .when(self.capabilities().copy, |el| dnd::drag_source(el, drag))
                            .when(entry.is_dir && !self.in_trash(), |el| {
                                dnd::folder_target(el, entry.path.clone(), cx)
                            }),
                    )
                    .into_any_element()
            })
            .collect()
    }
    pub(in crate::browser) fn list(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let available = (window.viewport_size().height / px(1.)
            - self.band_h()
            - t::PATH_BAR_H
            - t::STATUS_BAR_H)
            .max(0.);
        let fill = (available / t::ROW_H).floor() as usize;
        let body: AnyElement = if let Some(e) = &self.listing.error {
            div()
                .flex_1()
                .pt(px(self.band_h()))
                .flex()
                .items_center()
                .justify_center()
                .text_color(t::text_secondary())
                .child(e.clone())
                .into_any_element()
        } else {
            uniform_list(
                "rows",
                self.listing.rows.len().max(fill),
                cx.processor(Self::render_rows),
            )
            .flex_1()
            .pt(px(self.band_h()))
            .track_scroll(&self.view.scroll)
            .into_any_element()
        };
        // The column header floats over the rows, in the toolbar band.
        crate::ui::scrolling::vertical("file-list", &self.view.scroll, body)
    }
}
