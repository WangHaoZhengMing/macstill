//! Window toolbar, search controls and footer bars.
use super::*;

impl Browser {
    pub(super) fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::component::Disableable;
        let title = match &self.search.session {
            Some(Search { tag: Some(tag), .. }) => tag.name().to_string(),
            Some(s) if s.scope == Scope::Folder => {
                format!("Searching \u{201c}{}\u{201d}", files::display_name(&s.root))
            }
            Some(_) => "Searching \u{201c}File System\u{201d}".to_string(),
            None if self.in_applications() => "Applications".into(),
            None if self.in_trash() => "Trash".into(),
            None => files::display_name(&self.navigation.directory),
        };
        let view_btn =
            |id: &'static str, name: &'static str, mode: ViewMode, cx: &mut Context<Self>| {
                tool_button(id, name, self.shown_view() == mode)
                    .disabled(
                        (self.in_trash() && mode != ViewMode::List)
                            || (self.in_applications() && mode == ViewMode::Columns),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_view(mode, cx);
                    }))
            };
        div()
            .w_full()
            .h(px(t::TOOLBAR_H))
            .flex_none()
            .flex()
            .items_center()
            .pl(px(12.))
            .pr(px(12.))
            .gap(px(t::CAPSULE_GAP))
            .window_control_area(WindowControlArea::Drag)
            .child(
                capsule("nav")
                    .child(
                        tool_button("back", "chevron.left@medium", false)
                            .when(self.navigation.back.is_empty(), |d| d.opacity(0.3))
                            .on_click(cx.listener(|this, _, w, cx| this.go_back(&GoBack, w, cx))),
                    )
                    .child(div().w(px(1.)).h(px(16.)).bg(t::separator()))
                    .child(
                        tool_button("forward", "chevron.right@medium", false)
                            .when(self.navigation.forward.is_empty(), |d| d.opacity(0.3))
                            .on_click(
                                cx.listener(|this, _, w, cx| this.go_forward(&GoForward, w, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .px(px(4.))
                    .text_size(px(t::TITLE_SIZE))
                    .font_weight(gpui_kit::FontWeight::BOLD)
                    .truncate()
                    .child(title),
            )
            .child(
                gpui_kit::component::toolbar::ToolbarGroup::new("views")
                    .flex_none()
                    .rounded_full()
                    .bg(t::bar_bg())
                    .px(px(4.))
                    .child(view_btn("v-icons", "square.grid.2x2", ViewMode::Icons, cx))
                    .child(view_btn("v-list", "list.bullet", ViewMode::List, cx))
                    .child(view_btn(
                        "v-columns",
                        "rectangle.split.3x1",
                        ViewMode::Columns,
                        cx,
                    ))
                    .child(view_btn(
                        "v-gallery",
                        "squares.below.rectangle",
                        ViewMode::Gallery,
                        cx,
                    )),
            )
            .child(
                tool_button("toggle-inspector", "sidebar.right", self.inspector_visible).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.inspector_visible = !this.inspector_visible;
                        cx.notify();
                    }),
                ),
            )
            .child(
                capsule("group-cap").child(
                    tool_button("group", "square.grid.3x1.below.line.grid.1x2", false)
                        .on_click(cx.listener(|this, e: &ClickEvent, _, cx| {
                            this.open_sort_menu(menu_point(e), cx)
                        }))
                        .gap(px(1.))
                        .child(sym("chevron.down@semibold", 8., t::glass_icon())),
                ),
            )
            .child(
                capsule("actions")
                    .child(if self.in_trash() {
                        use gpui_kit::component::button::{Button, ButtonVariants};
                        Button::new("restore-trash")
                            .ghost()
                            .label("Restore")
                            .disabled(self.selection().is_empty())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.restore_trash(&RestoreTrash, window, cx)
                            }))
                            .into_any_element()
                    } else {
                        tool_button("tags", "tag", false)
                            .on_click(cx.listener(|this, e: &ClickEvent, _, cx| {
                                this.open_tag_menu(menu_point(e), cx)
                            }))
                            .into_any_element()
                    })
                    .child(tool_button("more", "ellipsis", false).on_click(cx.listener(
                        |this, e: &ClickEvent, _, cx| this.open_action_menu(menu_point(e), cx),
                    ))),
            )
    }
    /// Everything above the content, floating over it: the toolbar, the
    /// search scope bar and the list's column header, on a veil that
    /// content scrolling beneath them shows through.
    pub(super) fn toolbar_band(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let header = self.listing.error.is_none() && self.shown_view() == ViewMode::List;
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .bg(t::content_bg())
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
            .child(
                gpui_kit::component::toolbar::Toolbar::new("file-toolbar")
                    .w_full()
                    .h(px(t::TOOLBAR_H))
                    .p_0()
                    .content(crate::ui::window::drag_area(
                        "toolbar-drag",
                        self.toolbar(cx),
                    )),
            )
            .when(self.searching_query(), |d| d.child(self.scope_bar(cx)))
            // The column header draws its own hairline beneath it.
            .when(header, |d| d.child(self.column_header(cx)))
            .when(!header, |d| {
                d.child(div().h(px(1.)).flex_none().bg(t::separator()))
            })
    }
    /// A typed search (with its scope bar), rather than a tag view.
    pub(super) fn searching_query(&self) -> bool {
        self.search
            .session
            .as_ref()
            .is_some_and(|s| s.tag.is_none())
    }
    /// How far the toolbar band reaches down over the content, so views
    /// start their content below it.
    pub(super) fn band_h(&self) -> f32 {
        let header = self.listing.error.is_none() && self.shown_view() == ViewMode::List;
        t::TOOLBAR_H
            + if self.searching_query() {
                t::SCOPE_BAR_H
            } else {
                0.
            }
            + if header { t::HEADER_H } else { 1. }
    }
    /// The inspector column's own strip of toolbar: the search field.
    pub(super) fn search_bar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.search.input.focus_handle(cx).is_focused(window);
        let has_text =
            !self.search.input.read(cx).text().is_empty() || !self.search.chips.is_empty();
        div()
            .h(px(t::TOOLBAR_H))
            .flex_none()
            .flex()
            .items_center()
            .px(px(12.))
            .window_control_area(WindowControlArea::Drag)
            .child(
                tool_button("close-inspector", "xmark@semibold", false).on_click(cx.listener(
                    |this, _, w, cx| {
                        this.inspector_visible = false;
                        w.focus(&this.focus, cx);
                        cx.notify();
                    },
                )),
            )
            .child(
                capsule("search")
                    .flex_1()
                    .px(px(10.))
                    .gap(px(5.))
                    .when(focused, |d| d.border_2().border_color(t::focus_ring()))
                    .child(sym("magnifyingglass", 13., t::text_secondary()))
                    .children(self.search.chips.iter().enumerate().map(|(i, (k, v))| {
                        let (label, value) = chip_label(k, v);
                        div()
                            .id(("chip", i))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .px(px(6.))
                            .py(px(1.))
                            .rounded(px(5.))
                            .bg(t::glass_pressed())
                            .text_size(px(t::SMALL_SIZE))
                            .child(
                                div()
                                    .text_size(px(9.))
                                    .font_weight(gpui_kit::FontWeight::BOLD)
                                    .text_color(t::text_secondary())
                                    .child(label),
                            )
                            .child(div().text_color(t::text()).child(value))
                    }))
                    .child(self.search.input.clone())
                    .when(has_text, |d| {
                        d.child(
                            div()
                                .id("clear-search")
                                .child(sym("xmark.circle.fill", 13., t::text_tertiary()))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.end_search(cx);
                                    let h = this.search.input.focus_handle(cx);
                                    window.focus(&h, cx);
                                })),
                        )
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.focus_search(&FocusSearch, window, cx)
                    })),
            )
    }
    /// "Search: File System | "Folder"", under the toolbar while searching.
    pub(super) fn scope_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.search.session.as_ref().expect("searching");
        let button =
            |id: &'static str, label: String, on: bool, scope: Scope, cx: &mut Context<Self>| {
                div()
                    .id(id)
                    .px(px(9.))
                    .py(px(3.))
                    .rounded(px(6.))
                    .when(on, |d| {
                        d.bg(t::sidebar_selected())
                            .text_color(t::text())
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    })
                    .hover(|d| d.bg(t::glass_pressed()))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_scope(scope, cx)))
            };
        let plain = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .px(px(9.))
                .py(px(3.))
                .rounded(px(6.))
                .hover(|d| d.bg(t::glass_pressed()))
                .child(label)
        };
        div()
            .h(px(t::SCOPE_BAR_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(2.))
            .px(px(14.))
            .text_size(px(t::HEADER_SIZE))
            .text_color(t::text_secondary())
            .child(div().mr(px(4.)).child("Search:"))
            .child(button(
                "scope-pc",
                "File System".into(),
                s.scope == Scope::Everywhere,
                Scope::Everywhere,
                cx,
            ))
            .child(button(
                "scope-folder",
                format!("\u{201c}{}\u{201d}", files::display_name(&s.root)),
                s.scope == Scope::Folder,
                Scope::Folder,
                cx,
            ))
            .child(div().flex_1())
            .child(plain("save-search", "Save"))
            .child(plain("add-criteria", "+"))
    }
    pub(super) fn path_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.in_trash() || self.in_applications() {
            return div()
                .h(px(t::PATH_BAR_H))
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .bg(t::bar_bg())
                .child(sym(
                    if self.in_applications() {
                        "applications"
                    } else {
                        "trash"
                    },
                    12.,
                    t::text_secondary(),
                ))
                .child(if self.in_applications() {
                    "Applications"
                } else {
                    "Trash"
                })
                .into_any_element();
        }
        // While searching it shows where the selected result is.
        let target = match (&self.search.session, self.view.mode) {
            (Some(_), _) => self.selected_entry().map(|e| e.path),
            (None, ViewMode::Columns) => Some(
                self.selected_entry()
                    .map_or_else(|| self.navigation.directory.clone(), |e| e.path),
            ),
            (None, _) => Some(self.navigation.directory.clone()),
        };
        let mut crumbs: Vec<PathBuf> = target
            .iter()
            .flat_map(|p| p.ancestors())
            .map(Path::to_path_buf)
            .collect();
        crumbs.reverse();
        let n = crumbs.len();
        let mut bar = div()
            .id("path-bar")
            .h(px(t::PATH_BAR_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(3.))
            .px(px(14.))
            .bg(t::bar_bg())
            .border_t_1()
            .border_color(t::separator())
            .text_size(px(t::SMALL_SIZE))
            .text_color(t::text_secondary())
            .overflow_x_hidden();
        for (i, p) in crumbs.into_iter().enumerate() {
            let is_root = p.parent().is_none();
            let name = files::display_name(&p);
            bar = bar
                .child(
                    div()
                        .id(("crumb", i))
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(4.))
                        .child(
                            img(if is_root {
                                "icons/disk.svg"
                            } else if p.is_dir() {
                                "icons/folder.svg"
                            } else {
                                "icons/file.svg"
                            })
                            .size(px(14.)),
                        )
                        .child(name)
                        .when(p.is_dir(), |el| dnd::folder_target(el, p.clone(), cx))
                        .on_click(cx.listener(move |this, _, _, cx| this.navigate(p.clone(), cx))),
                )
                .when(i + 1 < n, |d| {
                    d.child(sym("chevron.right@semibold", 7., t::text_tertiary()))
                });
        }
        bar.into_any_element()
    }
    /// "24 items, 58.9 GB available".
    pub(super) fn status_bar(&self) -> impl IntoElement {
        let n = self.listing.rows.iter().filter(|r| r.depth == 0).count();
        if let Some(e) = &self.commands.notice {
            return gpui_kit::component::status_bar::StatusBar::new()
                .h(px(t::STATUS_BAR_H))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .bg(t::bar_bg())
                .text_size(px(t::SMALL_SIZE))
                .text_color(gpui_kit::rgb(0xE5484D))
                .child(e.clone());
        }
        let picked = self.selection().len();
        let items = if self.listing.loading {
            "Loading…".to_string()
        } else {
            match (n, self.selected_entry().map(|_| ())) {
                (n, Some(_)) => format!("{picked} of {n} selected"),
                (1, None) => "1 item".to_string(),
                (n, None) => format!("{n} items"),
            }
        };
        // A tag view isn't a search: it counts like a folder.
        if let Some(s) = self.search.session.as_ref().filter(|s| s.tag.is_none()) {
            let text = match (&s.error, s.took) {
                (Some(e), _) => e.clone(),
                (None, Some(took)) => {
                    let n = self.listing.rows.len();
                    let plus = if s.capped { "+" } else { "" };
                    let sel = if self.selection.cursor.is_some() {
                        format!("1 of {n}{plus} selected")
                    } else {
                        format!("{n}{plus} items")
                    };
                    format!("{sel} \u{00b7} {:.1} ms", took.as_secs_f64() * 1e3)
                }
                (None, None) => "Searching\u{2026}".into(),
            };
            return gpui_kit::component::status_bar::StatusBar::new()
                .h(px(t::STATUS_BAR_H))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .bg(t::bar_bg())
                .text_size(px(t::SMALL_SIZE))
                .text_color(t::text_secondary())
                .child(text);
        }
        let free = (!self.in_trash() && !self.in_applications())
            .then_some(self.listing.free_space)
            .flatten()
            .map(|b| format!(", {} available", files::format_bytes(b)))
            .unwrap_or_default();
        gpui_kit::component::status_bar::StatusBar::new()
            .h(px(t::STATUS_BAR_H))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .bg(t::bar_bg())
            .text_size(px(t::SMALL_SIZE))
            .text_color(t::text_secondary())
            .child(format!("{items}{free}"))
    }
}
