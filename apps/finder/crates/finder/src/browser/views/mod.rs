//! Shared file-view dispatch and keyboard/scroll coordination.

use super::*;
mod columns;
mod gallery;
mod icons;
mod list;
pub(super) use columns::Column;
use gallery::TILE;
use gpui_kit::component::InteractiveElementExt;
use std::ops::Range;

impl Browser {
    /// Width left for the view between the sidebar and the inspector.
    fn content_width(&self, window: &Window) -> f32 {
        window.viewport_size().width / px(1.)
            - t::SIDEBAR_W
            - if self.inspector_visible {
                t::INSPECTOR_W
            } else {
                0.
            }
    }

    /// Move the selection by `delta` (rows in the list, cells in the grid
    /// and filmstrip).
    pub(super) fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.listing.rows.is_empty() {
            return;
        }
        let last = self.listing.rows.len() as isize - 1;
        let ix = match self.selection.cursor {
            Some(i) => (i as isize + delta).clamp(0, last),
            None if delta > 0 => 0,
            None => last,
        };
        self.select(ix as usize, cx);
    }

    /// Keep the selection in view, however the current view lays it out.
    pub(super) fn reveal(&mut self, ix: usize) {
        match self.shown_view() {
            ViewMode::Icons => self
                .view
                .scroll
                .scroll_to_item(ix / self.view.grid_cols.max(1), ScrollStrategy::Nearest),
            ViewMode::Gallery => {
                let x = ix as f32 * TILE - (self.view.strip_width - TILE) / 2.;
                self.view
                    .strip_scroll
                    .set_offset(gpui_kit::point(px(-x.max(0.)), px(0.)));
            }
            _ => self.view.scroll.scroll_to_item(ix, ScrollStrategy::Nearest),
        }
    }

    /// The view on screen: search results always show as a list.
    pub(super) fn shown_view(&self) -> ViewMode {
        if self.search.session.is_some() || self.in_trash() {
            ViewMode::List
        } else if self.in_applications() && self.view.mode == ViewMode::Columns {
            ViewMode::Icons
        } else {
            self.view.mode
        }
    }

    pub(super) fn set_view(&mut self, mode: ViewMode, cx: &mut Context<Self>) {
        self.view.mode = mode;
        // Only the list shows folders opened in place.
        if mode != ViewMode::List
            && self.search.session.is_none()
            && self.listing.rows.iter().any(|r| r.depth > 0)
        {
            let sel = self.selected_entry().map(|e| e.path);
            let cur = self.navigation.directory.clone();
            self.load(cur, cx);
            self.selection.cursor =
                sel.and_then(|p| self.listing.rows.iter().position(|r| r.entry.path == p));
        }
        if mode == ViewMode::Gallery
            && self.selection.cursor.is_none()
            && !self.listing.rows.is_empty()
        {
            self.selection.cursor = Some(0);
        }
        if let Some(i) = self.selection.cursor {
            self.reveal(i);
        }
        cx.notify();
    }

    pub(super) fn main_view(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        if self.listing.error.is_some() || self.shown_view() == ViewMode::List {
            return self.list(window, cx).into_any_element();
        }
        match self.view.mode {
            ViewMode::Icons => self.icon_grid(window, cx),
            ViewMode::Columns => self.column_view(window, cx),
            _ => self.gallery(window, cx),
        }
    }

    /// FINDER_SELECT=name selects an entry at startup (testing).
    pub(super) fn select_by_name(&mut self, name: &str, cx: &mut Context<Self>) {
        if self.listing.loading {
            self.listing.select_name = Some(name.into());
            return;
        }
        if self.shown_view() == ViewMode::Columns {
            self.ensure_columns();
            if let Some(ix) = self.view.columns[0]
                .entries
                .iter()
                .position(|e| *e.name == *name)
            {
                self.col_select(0, ix, cx);
            }
        } else if let Some(ix) = self
            .listing
            .rows
            .iter()
            .position(|r| *r.entry.name == *name)
        {
            self.select(ix, cx);
        }
    }
}
