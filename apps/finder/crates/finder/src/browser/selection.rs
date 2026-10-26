//! Selection and keyboard navigation across views.
use super::*;

impl Browser {
    // Selection

    pub(super) fn is_selected(&self, ix: usize) -> bool {
        self.selection.cursor == Some(ix) || self.selection.marked.contains(&ix)
    }
    /// Selected paths, in row order.
    pub(super) fn selection(&self) -> Vec<PathBuf> {
        if self.shown_view() == ViewMode::Columns {
            return self.selected_entry().map(|e| e.path).into_iter().collect();
        }
        let mut ix: BTreeSet<usize> = self.selection.marked.clone();
        ix.extend(self.selection.cursor);
        ix.into_iter()
            .filter_map(|i| self.listing.rows.get(i))
            .map(|r| r.entry.path.clone())
            .collect()
    }
    /// A click: plain selects, Ctrl toggles, Shift selects a range.
    pub(super) fn click_select(
        &mut self,
        ix: usize,
        m: &gpui_kit::Modifiers,
        cx: &mut Context<Self>,
    ) {
        if m.control {
            if self.is_selected(ix) {
                self.selection.marked.remove(&ix);
                if self.selection.cursor == Some(ix) {
                    self.selection.cursor = self.selection.marked.pop_last();
                }
            } else {
                self.selection.marked.extend(self.selection.cursor);
                self.selection.cursor = Some(ix);
            }
            self.selection.anchor = Some(ix);
            cx.notify();
        } else if m.shift {
            let a = self.selection.anchor.unwrap_or(ix);
            self.selection.marked = (a.min(ix)..=a.max(ix)).filter(|&i| i != ix).collect();
            self.selection.cursor = Some(ix);
            cx.notify();
        } else {
            self.select(ix, cx);
        }
    }
    pub(super) fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        if self.listing.rows.is_empty() {
            return;
        }
        let cur = self.selection.cursor.unwrap_or(0);
        self.selection.cursor = Some(cur);
        self.selection.marked = (0..self.listing.rows.len()).filter(|&i| i != cur).collect();
        cx.notify();
    }
    // Keyboard

    pub(super) fn select(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.selection.cursor = Some(ix);
        self.selection.marked.clear();
        self.selection.anchor = Some(ix);
        self.reveal(ix);
        cx.notify();
    }
    // Arrow keys mean what they do in each view: rows in the list, cells
    // in the grid and filmstrip, columns in the column view.

    pub(super) fn select_next(
        &mut self,
        _: &SelectNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // From the search field too: the arrow moves into the results.
        window.focus(&self.focus, cx);
        match self.shown_view() {
            ViewMode::Columns => self.col_step(1, cx),
            ViewMode::Icons => self.step(self.view.grid_cols as isize, cx),
            _ => self.step(1, cx),
        }
    }
    pub(super) fn select_prev(
        &mut self,
        _: &SelectPrev,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        match self.shown_view() {
            ViewMode::Columns => self.col_step(-1, cx),
            ViewMode::Icons => self.step(-(self.view.grid_cols as isize), cx),
            _ => self.step(-1, cx),
        }
    }
    pub(super) fn expand_selected(&mut self, _: &Expand, _: &mut Window, cx: &mut Context<Self>) {
        match self.shown_view() {
            ViewMode::Columns => return self.col_right(cx),
            ViewMode::Icons | ViewMode::Gallery => return self.step(1, cx),
            ViewMode::List => {}
        }
        if let Some(i) = self.selection.cursor {
            self.expand(i, cx);
            cx.notify();
        }
    }
    /// Left arrow: fold the folder, or else go to its parent row.
    pub(super) fn collapse_selected(
        &mut self,
        _: &Collapse,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.shown_view() {
            ViewMode::Columns => return self.col_left(cx),
            ViewMode::Icons | ViewMode::Gallery => return self.step(-1, cx),
            ViewMode::List => {}
        }
        let Some(i) = self.selection.cursor else {
            return;
        };
        if self.listing.rows[i].expanded {
            self.collapse(i);
        } else if self.listing.rows[i].depth > 0 {
            let d = self.listing.rows[i].depth;
            if let Some(p) = (0..i).rev().find(|&j| self.listing.rows[j].depth < d) {
                self.selection.cursor = Some(p);
                self.view.scroll.scroll_to_item(p, ScrollStrategy::Nearest);
            }
        }
        cx.notify();
    }
    pub(super) fn open_selected(&mut self, _: &Open, _: &mut Window, cx: &mut Context<Self>) {
        if self.shown_view() == ViewMode::Columns {
            return self.col_open(cx);
        }
        if let Some(i) = self.selection.cursor {
            self.open_row(i, cx);
        }
    }
}

impl Browser {
    /// What the inspector, path bar and status bar describe.
    pub(super) fn selected_entry(&self) -> Option<Entry> {
        if self.shown_view() == ViewMode::Columns {
            return self
                .view
                .columns
                .iter()
                .rev()
                .find_map(|c| c.sel.map(|i| c.entries[i].clone()));
        }
        self.selection
            .cursor
            .and_then(|i| self.listing.rows.get(i))
            .map(|r| r.entry.clone())
    }
}
