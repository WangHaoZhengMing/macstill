//! The icons file view.
use super::*;

impl Browser {
    // Icons

    pub(in crate::browser) fn icon_grid(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let w = self.content_width(window);
        let cols = (((w - 2. * GRID_PAD + GRID_GAP) / (CELL_W + GRID_GAP)).floor() as usize).max(1);
        self.view.grid_cols = cols;
        let rows = self.listing.rows.len().div_ceil(cols);
        let list = uniform_list(
            "grid",
            rows,
            cx.processor(move |this, range: Range<usize>, window, cx| {
                range
                    .map(|r| this.grid_row(r, cols, window, cx))
                    .collect::<Vec<_>>()
            }),
        )
        .flex_1()
        .pt(px(self.band_h()))
        .track_scroll(&self.view.scroll);
        crate::ui::scrolling::vertical("icon-grid", &self.view.scroll, list).into_any_element()
    }
    pub(in crate::browser) fn grid_row(
        &mut self,
        r: usize,
        cols: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focus.is_focused(window);
        let mut row = div()
            .w_full()
            .h(px(CELL_H + GRID_GAP))
            .flex()
            .gap(px(GRID_GAP))
            .px(px(GRID_PAD))
            .pt(px(GRID_GAP));
        for ix in r * cols..((r + 1) * cols).min(self.listing.rows.len()) {
            let e = self.listing.rows[ix].entry.clone();
            let src = self.icon_source(&e, 128, cx);
            let sel = self.is_selected(ix);
            let rename = self.rename_box(&e.path);
            let drag = self.drag_value(&e, sel, cx);
            row = row.child(
                div()
                    .id(("cell", ix))
                    .w(px(CELL_W))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .size(px(ICON + 10.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .when(sel, |d| d.bg(t::icon_selected()))
                            .child(
                                img(src)
                                    .size(px(ICON))
                                    .object_fit(gpui_kit::ObjectFit::Contain),
                            ),
                    )
                    .children(rename)
                    .when(
                        self.commands
                            .renaming
                            .as_ref()
                            .is_none_or(|r| r.path != e.path),
                        |d| {
                            d.child(
                                div()
                                    .max_w(px(CELL_W))
                                    .px(px(5.))
                                    .rounded(px(4.))
                                    .text_size(px(12.))
                                    .line_height(px(15.))
                                    .text_center()
                                    .line_clamp(2)
                                    .when(sel, |d| {
                                        d.bg(if focused {
                                            t::selection()
                                        } else {
                                            t::selection_inactive()
                                        })
                                    })
                                    .when(sel && focused, |d| d.text_color(gpui_kit::white()))
                                    .child(icon_label(&e.name)),
                            )
                        },
                    )
                    .child(tag_dots(self.tags.of(&e.path), 8.))
                    .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                        window.focus(&this.focus, cx);
                        if ev.click_count() >= 2 {
                            this.open_row(ix, cx)
                        } else {
                            this.click_select(ix, &ev.modifiers(), cx)
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, e: &gpui_kit::MouseDownEvent, window, cx| {
                            this.open_menu(Some(ix), e.position, window, cx)
                        }),
                    )
                    .when(self.capabilities().copy, |el| dnd::drag_source(el, drag))
                    .when(e.is_dir, |el| dnd::folder_target(el, e.path.clone(), cx)),
            );
        }
        row.into_any_element()
    }
}

/// An icon's label: names too long for two lines lose their middle, as in
/// Finder ("anki_2024_10_0…762.png"), so the extension stays visible.
pub(super) fn icon_label(name: &str) -> String {
    const MAX: usize = 24;
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= MAX {
        return name.to_string();
    }
    let tail = 10;
    let head: String = chars[..MAX - tail - 1].iter().collect();
    let end: String = chars[chars.len() - tail..].iter().collect();
    format!("{head}\u{2026}{end}")
}

const ICON: f32 = 64.;
const CELL_W: f32 = 104.;
const CELL_H: f32 = 116.;
const GRID_PAD: f32 = 18.;
const GRID_GAP: f32 = 12.;
