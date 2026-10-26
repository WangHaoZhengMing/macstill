//! Browser composition and top-level window layout.
mod airdrop;
mod global_menu;
mod state;
use crate::actions::*;
use crate::model::location::{Capabilities, Location};
use state::*;
mod commands;
mod dialogs;
mod inspector;
mod navigation;
mod search;
mod selection;
mod sidebar;
mod toolbar;
use crate::ui::components::{capsule, menu_point, sym, tag_dots, tool_button};
use search::chip_label;
use sidebar::sections;

use crate::model::entry::{self as files, Entry, SortKey};
use crate::model::tag::Tag;
use crate::services::clipboard;
use crate::services::filesystem as ops;
use crate::services::icons;
use crate::services::tags::Tags;
use crate::ui::assets::symbol;
use crate::ui::text_input::{self, TextInput};
use crate::ui::theme as t;

mod dnd;
mod menu;
mod quicklook;
mod views;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, FocusHandle, Focusable, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Render, ScrollStrategy, SharedString, StatefulInteractiveElement,
    Styled, UniformListScrollHandle, Window, WindowControlArea, div, img, prelude::*, px, svg,
    uniform_list,
};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

pub struct Browser {
    global_menu: global_menu::State,
    airdrop: airdrop::State,
    inspector_visible: bool,
    font: SharedString,
    focus: FocusHandle,
    tags: Tags,
    menu: Option<menu::Menu>,
    navigation: NavigationState,
    listing: ListingState,
    selection: SelectionState,
    search: SearchState,
    sidebar: SidebarState,
    preview: PreviewState,
    view: ViewState,
    commands: CommandState,
}

impl Focusable for Browser {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn home() -> PathBuf {
    crate::services::xdg::home()
}

impl Browser {
    pub fn new(font: SharedString, window: &mut Window, cx: &mut Context<Self>) -> Browser {
        let start = crate::services::xdg::user_dir("Desktop");
        let mut b = Browser {
            global_menu: Default::default(),
            airdrop: airdrop::State::default(),
            inspector_visible: true,
            font,
            focus: cx.focus_handle(),
            tags: Tags::load(),
            menu: None,
            navigation: NavigationState {
                directory: PathBuf::new(),
                location: Location::Folder(PathBuf::new()),
                back: Vec::new(),
                forward: Vec::new(),
            },
            listing: ListingState {
                generation: 0,
                loading: false,
                show_hidden: false,
                free_space: None,
                watcher: None,
                dirty: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                monitor: None,
                select_after: Vec::new(),
                select_name: None,
                rows: Vec::new(),
                sort: (SortKey::Kind, true),
                error: None,
                trash_items: HashMap::new(),
            },
            selection: SelectionState {
                cursor: None,
                marked: BTreeSet::new(),
                anchor: None,
            },
            search: SearchState {
                input: cx.new(|cx| TextInput::new("Search", window, cx)),
                engine: if cfg!(test) {
                    None
                } else {
                    fsearch::Engine::start(fsearch::Options {
                        dir: fsearch::sys::data_dir(),
                        home: fsearch::sys::home(),
                        skip: None,
                    })
                    .ok()
                },
                session: None,
                generation: 0,
                chips: Vec::new(),
            },
            sidebar: SidebarState {
                sections: sections(),
                drives: Vec::new(),
                error: None,
                mounting: None,
                monitor: None,
                scroll: gpui_kit::ScrollHandle::new(),
            },
            preview: PreviewState {
                quick_look: false,
                peek: None,
                info_collapsed: false,
                icons: HashMap::new(),
            },
            view: ViewState {
                mode: ViewMode::List,
                list_columns: t::ListColumns::for_width(
                    t::WINDOW_W - t::SIDEBAR_W - t::INSPECTOR_W,
                ),
                columns: Vec::new(),
                col_focus: 0,
                col_scroll: gpui_kit::ScrollHandle::new(),
                strip_scroll: gpui_kit::ScrollHandle::new(),
                strip_width: 600.,
                grid_cols: 1,
                scroll: UniformListScrollHandle::new(),
                pending_scroll: None,
            },
            commands: CommandState {
                go_to: None,
                job: None,
                apply_all: false,
                renaming: None,
                notice: None,
                undo: Vec::new(),
            },
        };
        cx.subscribe(&b.search.input, |this, _, _: &text_input::Changed, cx| {
            this.query_changed(cx)
        })
        .detach();
        cx.subscribe(&b.search.input, |this, _, _: &text_input::Cancelled, cx| {
            this.end_search(cx)
        })
        .detach();
        cx.subscribe(
            &b.search.input,
            |this, _, _: &text_input::BackspaceAtStart, cx| {
                if this.search.chips.pop().is_some() {
                    this.query_changed(cx);
                }
            },
        )
        .detach();
        // FINDER_DIR / FINDER_VIEW pick where and how it opens (testing).
        let start = std::env::var_os("FINDER_DIR")
            .map(PathBuf::from)
            .map(|path| {
                if path == Path::new("trash:") {
                    Location::Trash
                } else if path.is_dir() {
                    Location::Folder(path)
                } else {
                    Location::Folder(home())
                }
            })
            .unwrap_or(Location::Folder(start));
        let argument = std::env::args_os().nth(1);
        let reveal = (argument.as_deref() == Some(std::ffi::OsStr::new("--reveal")))
            .then(|| std::env::args_os().nth(2))
            .flatten()
            .map(PathBuf::from);
        let start = if argument.as_deref() == Some(std::ffi::OsStr::new("--airdrop")) {
            Location::AirDrop
        } else if argument.as_deref() == Some(std::ffi::OsStr::new("--trash")) {
            Location::Trash
        } else if let Some(parent) = reveal
            .as_ref()
            .and_then(|p| p.parent())
            .filter(|p| p.is_dir())
        {
            Location::Folder(parent.to_path_buf())
        } else if let Some(path) = argument.map(PathBuf::from).filter(|p| p.is_dir()) {
            Location::Folder(path)
        } else {
            start
        };
        b.load(start, cx);
        if let Some(name) = reveal
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|name| name.to_str())
        {
            b.select_by_name(name, cx);
        }
        b.view.mode = match std::env::var("FINDER_VIEW").as_deref() {
            Ok("icons") => ViewMode::Icons,
            Ok("columns") => ViewMode::Columns,
            Ok("gallery") => ViewMode::Gallery,
            _ => ViewMode::List,
        };
        if let Ok(name) = std::env::var("FINDER_SELECT") {
            b.select_by_name(&name, cx);
        }
        // FINDER_TAG=Red opens that tag's view, as its sidebar row does.
        if let Some(tag) = std::env::var("FINDER_TAG").ok().and_then(|n| {
            Tag::ALL
                .into_iter()
                .find(|t| t.name().eq_ignore_ascii_case(&n))
        }) {
            b.show_tag(tag, cx);
        }
        // FINDER_SCROLL_TO=n scrolls row n (of the list, or of the icon
        // grid) to just under the toolbar, so content runs beneath it, once
        // the window has its size.
        b.view.pending_scroll = std::env::var("FINDER_SCROLL_TO")
            .ok()
            .and_then(|n| n.parse().ok())
            .map(|n| (n, 0));
        // FINDER_SEARCH="..." types a search at startup (for testing
        // without driving the keyboard).
        if let Ok(q) = std::env::var("FINDER_SEARCH") {
            b.search.input.update(cx, |i, cx| i.set_text(q, cx));
        }
        b.start_listing_monitor(cx);
        b.start_drive_monitor(cx);
        b.start_airdrop_monitor(cx);
        b.start_global_menu(window, cx);
        b
    }

    #[cfg(test)]
    pub(crate) fn test_scroll_handles(&self) -> (gpui_kit::ScrollHandle, UniformListScrollHandle) {
        (self.sidebar.scroll.clone(), self.view.scroll.clone())
    }
}

impl Render for Browser {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::component::ActiveTheme;
        t::set_dark(cx.theme().mode.is_dark());
        if self.in_airdrop() {
            return div()
                .id("browser-airdrop")
                .key_context("Browser")
                .track_focus(&self.focus)
                .on_action(cx.listener(Self::go_to_folder))
                .on_action(cx.listener(Self::toggle_hidden))
                .on_action(cx.listener(Self::go_back))
                .on_action(cx.listener(Self::go_forward))
                .size_full()
                .relative()
                .flex()
                .font_family(self.font.clone())
                .text_color(t::text())
                .rounded(crate::ui::window::radius(window))
                .overflow_hidden()
                .child(self.sidebar(window, cx))
                .child(self.airdrop_page(cx))
                .children(self.render_go_to(cx))
                .child(crate::ui::frame::overlay(window))
                .into_any_element();
        }
        self.view.list_columns = t::ListColumns::for_width(
            window.viewport_size().width / px(1.)
                - t::SIDEBAR_W
                - if self.inspector_visible {
                    t::INSPECTOR_W
                } else {
                    0.
                },
        );
        if let Some((row, waited)) = self.view.pending_scroll.take() {
            if waited < 5 {
                self.view.pending_scroll = Some((row, waited + 1));
                cx.on_next_frame(window, |_, _, cx| cx.notify());
            } else {
                self.view
                    .scroll
                    .scroll_to_item_strict(row, ScrollStrategy::Top);
            }
        }

        div()
            .id("browser")
            .key_context("Browser")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::restore_trash))
            .on_action(cx.listener(Self::refresh_location))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_prev))
            .on_action(cx.listener(Self::expand_selected))
            .on_action(cx.listener(Self::collapse_selected))
            .on_action(cx.listener(Self::open_selected))
            .on_action(cx.listener(Self::go_to_folder))
            .on_action(cx.listener(Self::toggle_hidden))
            .on_action(cx.listener(Self::go_back))
            .on_action(cx.listener(Self::go_forward))
            .on_action(cx.listener(Self::go_up))
            .on_action(cx.listener(Self::focus_search))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::rename_action))
            .on_action(cx.listener(Self::new_folder))
            .on_action(cx.listener(Self::duplicate))
            .on_action(cx.listener(Self::trash))
            .on_action(cx.listener(Self::copy_files))
            .on_action(cx.listener(Self::cut_files))
            .on_action(cx.listener(Self::paste_files))
            .on_action(cx.listener(Self::copy_path))
            .on_action(cx.listener(Self::reveal_in_file_manager))
            .on_action(cx.listener(Self::show_icons))
            .on_action(cx.listener(Self::show_list))
            .on_action(cx.listener(Self::show_columns))
            .on_action(cx.listener(Self::show_gallery))
            .on_action(cx.listener(Self::context_menu_key))
            .on_action(cx.listener(Self::action_menu_key))
            .on_action(cx.listener(Self::sort_menu_key))
            .on_action(cx.listener(Self::tag_menu_key))
            .on_action(cx.listener(Self::undo_op))
            .on_action(cx.listener(Self::toggle_quick_look))
            .on_action(cx.listener(Self::escape))
            .on_mouse_down(
                MouseButton::Navigate(gpui_kit::NavigationDirection::Back),
                cx.listener(|this, _, w, cx| this.go_back(&GoBack, w, cx)),
            )
            .on_mouse_down(
                MouseButton::Navigate(gpui_kit::NavigationDirection::Forward),
                cx.listener(|this, _, w, cx| this.go_forward(&GoForward, w, cx)),
            )
            .size_full()
            .relative()
            .flex()
            .rounded(crate::ui::window::radius(window))
            .font_family(self.font.clone())
            .text_size(px(t::TEXT_SIZE))
            .text_color(t::text())
            .children(self.render_quick_look(window, cx))
            .children(self.render_menu(window, cx))
            .children(self.render_job(cx))
            .children(self.render_go_to(cx))
            .child(self.sidebar(window, cx))
            .child(
                div()
                    .bg(t::content_bg())
                    .rounded_r(crate::ui::window::radius(window))
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .relative()
                            .flex()
                            .flex_col()
                            // The view runs to the top; the toolbar band floats over it.
                            .child({
                                let view = self.main_view(window, cx);
                                let el = div()
                                    .id("view-drop")
                                    .flex_1()
                                    .min_h_0()
                                    .flex()
                                    .flex_col()
                                    .child(view);
                                if self.capabilities().paste {
                                    dnd::folder_target(el, self.navigation.directory.clone(), cx)
                                        .into_any_element()
                                } else {
                                    el.into_any_element()
                                }
                            })
                            .child(self.path_bar(cx))
                            .child(self.status_bar())
                            .child(self.toolbar_band(cx)),
                    )
                    .when(self.inspector_visible, |pane| {
                        pane.child(
                            div()
                                .w(px(t::INSPECTOR_W))
                                .h_full()
                                .flex_none()
                                .flex()
                                .flex_col()
                                .bg(t::inspector_bg())
                                .rounded_r(crate::ui::window::radius(window))
                                .border_l_1()
                                .border_color(t::separator())
                                .child(if self.in_trash() {
                                    div().h(px(t::TOOLBAR_H)).flex_none().into_any_element()
                                } else {
                                    self.search_bar(window, cx).into_any_element()
                                })
                                .child(self.inspector(cx)),
                        )
                    }),
            )
            .child(crate::ui::frame::overlay(window))
            .into_any_element()
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::*;
    #[test]
    fn home_has_exactly_one_sidebar_entry() {
        let home = home();
        let entries = sections()
            .into_iter()
            .flat_map(|section| section.places)
            .filter(|place| {
                place.location.as_ref().and_then(Location::directory) == Some(home.as_path())
            })
            .count();
        assert_eq!(
            entries, 1,
            "XDG aliases must not produce multiple active sidebar rows"
        );
    }
}

#[cfg(test)]
#[path = "../tests/navigation.rs"]
mod history_tests;

impl Drop for Browser {
    fn drop(&mut self) {
        if let Some(job) = &self.commands.job {
            job.cancel();
        }
    }
}
