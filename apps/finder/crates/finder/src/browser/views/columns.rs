//! The columns file view.
use super::*;

impl Browser {
    // Columns

    /// Columns start at the current folder; navigating elsewhere resets them.
    pub(in crate::browser) fn ensure_columns(&mut self) {
        if self.view.columns.first().map(|c| &c.path) != Some(&self.navigation.directory) {
            let mut column = Column::open(self.navigation.directory.clone());
            column.entries = self
                .listing
                .rows
                .iter()
                .filter(|r| r.depth == 0)
                .map(|r| r.entry.clone())
                .collect();
            self.view.columns = vec![column];
            self.view.col_focus = 0;
        }
    }
    pub(in crate::browser) fn column_view(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ensure_columns();
        let band = self.band_h();
        let mut strip = div()
            .id("columns")
            .flex_1()
            .min_h_0()
            .flex()
            .overflow_x_scroll()
            .track_scroll(&self.view.col_scroll);
        for k in 0..self.view.columns.len() {
            let n = self.view.columns[k].entries.len();
            let list = uniform_list(
                ("column", k),
                n,
                cx.processor(move |this, range: Range<usize>, window, cx| {
                    this.column_rows(k, range, window, cx)
                }),
            )
            .size_full()
            .pt(px(band + 4.))
            .pb(px(4.))
            .track_scroll(&self.view.columns[k].scroll);
            strip = strip.child(
                div()
                    .w(px(COLUMN_W))
                    .h_full()
                    .flex_none()
                    .border_r_1()
                    .border_color(t::separator())
                    .children(
                        self.view.columns[k]
                            .error
                            .clone()
                            .map(|e| div().pt(px(band)).p_2().child(e)),
                    )
                    .child(crate::ui::scrolling::vertical(
                        ("column-scroll", k),
                        &self.view.columns[k].scroll,
                        list,
                    )),
            );
        }
        if let Some(e) = self
            .view
            .columns
            .last()
            .and_then(|c| c.sel.map(|i| c.entries[i].clone()))
            .filter(|e| !e.is_dir)
        {
            strip = strip.child(self.column_preview(e, window, cx));
        }
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .overflow_hidden()
            .child(strip.lock_scroll_axis())
            .child(crate::ui::scrolling::bar(
                "columns-bar",
                &self.view.col_scroll,
                true,
            ))
            .into_any_element()
    }
    pub(in crate::browser) fn column_rows(
        &mut self,
        k: usize,
        range: Range<usize>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let active = k == self.view.col_focus && self.focus.is_focused(window);
        range
            .map(|ix| {
                let e = self.view.columns[k].entries[ix].clone();
                let src = self.icon_source(&e, 32, cx);
                let sel = self.view.columns[k].sel == Some(ix);
                let drag = self.drag_value(&e, false, cx);
                let (fg, fg2): (gpui_kit::Hsla, gpui_kit::Hsla) = if sel && active {
                    (gpui_kit::white(), gpui_kit::white().opacity(0.85))
                } else {
                    (t::text().into(), t::text_secondary().into())
                };
                let row = div()
                    .id(("crow", ix))
                    .flex_1()
                    .min_w_0()
                    .h(px(t::ROW_H))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(5.))
                    .when(sel, |d| {
                        d.bg(if active {
                            t::selection()
                        } else {
                            t::selection_inactive()
                        })
                    })
                    .text_color(fg)
                    .child(img(src).size(px(t::ROW_ICON)).flex_none())
                    .child(div().flex_1().min_w_0().truncate().child(e.name.clone()))
                    .when(e.is_dir, |d| {
                        d.child(sym("chevron.right@semibold", 8., fg2))
                    })
                    .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                        window.focus(&this.focus, cx);
                        this.col_select(k, ix, cx);
                        if ev.click_count() >= 2 {
                            this.col_open(cx);
                        }
                    }))
                    .map(|el| dnd::drag_source(el, drag))
                    .when(e.is_dir, |el| dnd::folder_target(el, e.path.clone(), cx));
                div()
                    .w_full()
                    .px(px(6.))
                    .flex()
                    .child(row)
                    .into_any_element()
            })
            .collect()
    }
    pub(in crate::browser) fn col_select(&mut self, k: usize, ix: usize, cx: &mut Context<Self>) {
        self.view.columns.truncate(k + 1);
        let c = &mut self.view.columns[k];
        c.sel = Some(ix);
        c.scroll.scroll_to_item(ix, ScrollStrategy::Nearest);
        self.view.col_focus = k;
        let e = &c.entries[ix];
        if e.is_dir {
            let p = e.path.clone();
            self.view.columns.push(Column::open(p));
            self.refresh(&[], cx);
        }
        // Keep the newest column (or the preview) in view.
        self.view.col_scroll.scroll_to_item(self.view.columns.len());
        cx.notify();
    }
    /// Up and down within the focused column.
    pub(in crate::browser) fn col_step(&mut self, delta: isize, cx: &mut Context<Self>) {
        self.ensure_columns();
        let k = self.view.col_focus.min(self.view.columns.len() - 1);
        let n = self.view.columns[k].entries.len() as isize;
        if n == 0 {
            return;
        }
        let ix = match self.view.columns[k].sel {
            Some(i) => (i as isize + delta).clamp(0, n - 1),
            None => 0,
        };
        self.col_select(k, ix as usize, cx);
    }
    /// Right: into the selected folder's column.
    pub(in crate::browser) fn col_right(&mut self, cx: &mut Context<Self>) {
        let k = self.view.col_focus;
        let into_dir = self
            .view
            .columns
            .get(k)
            .and_then(|c| c.sel.map(|i| c.entries[i].is_dir))
            .unwrap_or(false);
        if into_dir
            && self
                .view
                .columns
                .get(k + 1)
                .is_some_and(|c| !c.entries.is_empty())
        {
            self.col_select(k + 1, 0, cx);
        }
    }
    /// Left: back to the parent column, keeping its selection.
    pub(in crate::browser) fn col_left(&mut self, cx: &mut Context<Self>) {
        let k = self.view.col_focus;
        if k == 0 {
            return;
        }
        self.view.columns[k].sel = None;
        self.view.columns.truncate(k + 1);
        self.view.col_focus = k - 1;
        cx.notify();
    }
    /// Open what's selected in the focused column.
    pub(in crate::browser) fn col_open(&mut self, cx: &mut Context<Self>) {
        let Some(e) = self
            .view
            .columns
            .get(self.view.col_focus)
            .and_then(|c| c.sel.map(|i| c.entries[i].clone()))
        else {
            return;
        };
        if e.is_dir {
            self.navigate(e.path, cx);
        } else {
            let _ = crate::services::xdg::open(&e.path);
        }
    }
    pub(in crate::browser) fn column_preview(
        &mut self,
        e: Entry,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pictured = crate::services::icons::has_thumbnail(&e.path);
        let src = self.icon_source(&e, 512, cx);
        div()
            .w(px(PREVIEW_W))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .p(px(16.))
            .pt(px(self.band_h() + 16.))
            .gap(px(4.))
            .child(
                div()
                    .h(px(200.))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        img(src)
                            .when(pictured, |i| i.w(px(220.)).h(px(190.)))
                            .when(!pictured, |i| i.size(px(128.)))
                            .object_fit(gpui_kit::ObjectFit::Contain),
                    ),
            )
            .child(
                div()
                    .mt(px(8.))
                    .text_size(px(t::TEXT_SIZE))
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .text_center()
                    .child(e.name.clone()),
            )
            .child(
                div()
                    .text_size(px(t::SMALL_SIZE))
                    .text_color(t::text_secondary())
                    .child(format!("{} - {}", e.kind, files::format_size(&e))),
            )
            .into_any_element()
    }
}

/// One column of the column view: a folder's entries, by name.
pub(in crate::browser) struct Column {
    pub(in crate::browser) error: Option<String>,
    pub(in crate::browser) path: PathBuf,
    pub(in crate::browser) entries: Vec<Entry>,
    pub(in crate::browser) sel: Option<usize>,
    pub(in crate::browser) scroll: UniformListScrollHandle,
}

impl Column {
    pub fn open(path: PathBuf) -> Column {
        let entries = Vec::new();
        Column {
            error: None,
            path,
            entries,
            sel: None,
            scroll: UniformListScrollHandle::new(),
        }
    }
}

const COLUMN_W: f32 = 235.;
const PREVIEW_W: f32 = 260.;
