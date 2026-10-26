//! Browser commands mapped to the procedural context-menu component.

use super::*;
use gpui_kit::{Action, Pixels, Point, deferred};
use std::rc::Rc;

type Run = Rc<dyn Fn(&mut Browser, &mut Window, &mut Context<Browser>)>;

pub(super) enum Item {
    Action {
        label: SharedString,
        shortcut: Option<&'static str>,
        action: Box<dyn Action>,
        enabled: bool,
        checked: bool,
    },
    /// A choice that carries data, like "sort by Size", with an optional
    /// color dot (a tag's).
    Call {
        label: SharedString,
        run: Run,
        enabled: bool,
        checked: bool,
        dot: Option<gpui_kit::Hsla>,
    },
    /// Tag choices, presented in a color-coded submenu.
    Swatches(Vec<(Tag, bool)>),
    /// A section title, like "Sort By".
    Header(SharedString),
    Separator,
    Submenu {
        label: SharedString,
        items: Vec<Item>,
    },
}

pub(super) struct Menu {
    pub at: Point<Pixels>,
    pub items: Vec<Item>,
    popup: Option<gpui_kit::Entity<crate::ui::menu::ContextMenu>>,
    _dismiss: Option<gpui_kit::Subscription>,
}

fn item(label: &str, shortcut: Option<&'static str>, action: impl Action, enabled: bool) -> Item {
    Item::Action {
        label: label.to_string().into(),
        shortcut,
        action: Box::new(action),
        enabled,
        checked: false,
    }
}

fn call(
    label: &str,
    run: impl Fn(&mut Browser, &mut Window, &mut Context<Browser>) + 'static,
) -> Item {
    Item::Call {
        label: label.to_string().into(),
        run: Rc::new(run),
        enabled: true,
        checked: false,
        dot: None,
    }
}

fn check(i: Item, on: bool) -> Item {
    match i {
        Item::Action {
            label,
            shortcut,
            action,
            enabled,
            ..
        } => Item::Action {
            label,
            shortcut,
            action,
            enabled,
            checked: on,
        },
        Item::Call {
            label,
            run,
            enabled,
            dot,
            ..
        } => Item::Call {
            label,
            run,
            enabled,
            checked: on,
            dot,
        },
        other => other,
    }
}

/// Translate commands without coupling menu painting to browser state.
fn popup_rows(
    items: Vec<Item>,
    owner: gpui_kit::WeakEntity<Browser>,
) -> Vec<crate::ui::menu::MenuRow> {
    use crate::ui::menu::MenuRow;
    let mut rows = Vec::new();
    for item in items {
        let row = match item {
            Item::Separator => MenuRow::Separator,
            Item::Header(label) => MenuRow::Header(label),
            Item::Submenu { label, items } => MenuRow::Submenu {
                label,
                rows: popup_rows(items, owner.clone()),
            },
            Item::Action {
                label,
                shortcut,
                action,
                enabled,
                checked,
            } => {
                let owner = owner.clone();
                MenuRow::Command {
                    label,
                    shortcut: shortcut.map(Into::into),
                    enabled,
                    checked,
                    dot: None,
                    run: Rc::new(move |window, cx| {
                        let restored = owner.update(cx, |this, cx| {
                            this.menu = None;
                            window.focus(&this.focus, cx);
                            cx.notify();
                        });
                        if restored.is_ok() {
                            window.dispatch_action(action.boxed_clone(), cx);
                        }
                    }),
                }
            }
            Item::Call {
                label,
                run,
                enabled,
                checked,
                dot,
            } => {
                let owner = owner.clone();
                MenuRow::Command {
                    label,
                    shortcut: None,
                    enabled,
                    checked,
                    dot,
                    run: Rc::new(move |window, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            this.menu = None;
                            window.focus(&this.focus, cx);
                            run(this, window, cx);
                            cx.notify();
                        });
                    }),
                }
            }
            Item::Swatches(states) => {
                let items = states
                    .into_iter()
                    .map(|(tag, checked)| Item::Call {
                        label: tag.name().into(),
                        run: Rc::new(move |this, _, cx| this.toggle_tag(tag, cx)),
                        enabled: true,
                        checked,
                        dot: Some(tag.color()),
                    })
                    .collect();
                MenuRow::Submenu {
                    label: "Tags".into(),
                    rows: popup_rows(items, owner.clone()),
                }
            }
        };
        rows.push(row);
    }
    rows
}

impl Browser {
    #[cfg(test)]
    pub(crate) fn test_open_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ix = (!self.listing.rows.is_empty()).then_some(0);
        self.open_menu(ix, gpui_kit::point(px(360.), px(100.)), window, cx);
    }

    /// Right-click on row `ix` (None: on the background).
    pub(super) fn open_menu(
        &mut self,
        ix: Option<usize>,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        // Right-clicking an item outside the selection selects just it.
        if let Some(ix) = ix
            && !self.is_selected(ix)
        {
            self.select(ix, cx);
        }
        let items = match ix {
            Some(_) => self.selection_items(),
            None => self.folder_items(),
        };
        self.show_menu(at, items, cx);
    }

    pub(super) fn show_menu(
        &mut self,
        at: Point<Pixels>,
        items: Vec<Item>,
        cx: &mut Context<Self>,
    ) {
        self.menu = Some(Menu {
            at,
            items,
            popup: None,
            _dismiss: None,
        });
        cx.notify();
    }

    /// What can be done to the selection.
    fn selection_items(&self) -> Vec<Item> {
        if self.in_applications() {
            return vec![
                item("Open", Some("Ctrl+↓"), Open, true),
                item("Show in File Manager", None, Reveal, true),
                item("Refresh", Some("F5"), RefreshLocation, true),
            ];
        }
        if self.in_trash() {
            return vec![
                item(
                    "Restore to Original Location",
                    Some("Ctrl+Shift+R"),
                    RestoreTrash,
                    !self.selection().is_empty(),
                ),
                item(
                    "Quick Look",
                    Some("Space"),
                    QuickLook,
                    self.selection.cursor.is_some(),
                ),
                Item::Separator,
                item("Refresh", Some("F5"), RefreshLocation, true),
            ];
        }
        let n = self.selection().len();
        let what = if n == 1 {
            String::new()
        } else {
            format!(" {n} Items")
        };
        let can_paste = self.capabilities().paste && clipboard::get_files().is_some();
        vec![
            item("Open", Some("Ctrl+\u{2193}"), Open, true),
            item("Quick Look", Some("Space"), QuickLook, true),
            item("Show in File Manager", None, Reveal, true),
            call("Send via AirDrop…", |this, _, cx| {
                this.airdrop.files = this.selection();
                this.navigate(Location::AirDrop, cx);
            }),
            Item::Separator,
            item("Move to Trash", Some("Del"), Trash, true),
            Item::Separator,
            item(
                "Rename",
                Some("F2"),
                Rename,
                n == 1 && self.capabilities().rename,
            ),
            item(
                "Duplicate",
                Some("Ctrl+D"),
                Duplicate,
                self.capabilities().duplicate,
            ),
            Item::Separator,
            item(&format!("Copy{what}"), Some("Ctrl+C"), CopyFiles, true),
            item("Copy as Pathname", Some("Ctrl+Alt+C"), CopyPath, true),
            item("Paste Item", Some("Ctrl+V"), PasteFiles, can_paste),
            Item::Separator,
            Item::Swatches(self.tag_states()),
        ]
    }

    /// Each tag, and whether everything selected has it.
    fn tag_states(&self) -> Vec<(Tag, bool)> {
        let sel = self.selection();
        Tag::ALL
            .into_iter()
            .map(|t| {
                (
                    t,
                    !sel.is_empty() && sel.iter().all(|p| self.tags.of(p).contains(&t)),
                )
            })
            .collect()
    }

    /// The toolbar's tag button (and the inspector's tag field): each tag,
    /// checked if everything selected has it.
    pub(super) fn open_tag_menu(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        if self.in_trash() || self.in_applications() {
            return;
        }
        let any = !self.selection().is_empty();
        let mut items = vec![Item::Header(if any {
            "Tags".into()
        } else {
            "Select items to tag".into()
        })];
        for (tag, on) in self.tag_states() {
            items.push(Item::Call {
                label: tag.name().into(),
                run: Rc::new(
                    move |this: &mut Browser, _: &mut Window, cx: &mut Context<Browser>| {
                        this.toggle_tag(tag, cx)
                    },
                ),
                enabled: any,
                checked: on,
                dot: Some(tag.color()),
            });
        }
        self.show_menu(at, items, cx);
    }

    /// What can be done in the folder itself.
    fn folder_items(&self) -> Vec<Item> {
        if self.in_trash() || self.in_applications() {
            return vec![item("Refresh", Some("F5"), RefreshLocation, true)];
        }
        let can_paste = self.capabilities().paste && clipboard::get_files().is_some();
        vec![
            item(
                "New Folder",
                Some("Ctrl+Shift+N"),
                NewFolder,
                self.capabilities().create,
            ),
            Item::Separator,
            item("Paste Item", Some("Ctrl+V"), PasteFiles, can_paste),
            Item::Separator,
            Item::Submenu {
                label: "View".into(),
                items: vec![
                    check(
                        item("as Icons", None, ShowIcons, true),
                        self.view.mode == ViewMode::Icons,
                    ),
                    check(
                        item("as List", None, ShowList, true),
                        self.view.mode == ViewMode::List,
                    ),
                    check(
                        item("as Columns", None, ShowColumns, true),
                        self.view.mode == ViewMode::Columns,
                    ),
                    check(
                        item("as Gallery", None, ShowGallery, true),
                        self.view.mode == ViewMode::Gallery,
                    ),
                ],
            },
        ]
    }

    /// The toolbar's "…" (and the inspector's "More…"): Finder's action menu,
    /// for the selection if there is one, else for the folder.
    pub(super) fn open_action_menu(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        if self.in_trash() {
            let items = self.selection_items();
            self.show_menu(at, items, cx);
            return;
        }
        let mut items = vec![item(
            "New Folder",
            Some("Ctrl+Shift+N"),
            NewFolder,
            self.capabilities().create,
        )];
        if self.selection.cursor.is_some() {
            items.push(Item::Separator);
            items.extend(self.selection_items());
        } else {
            items.push(item(
                "Paste Item",
                Some("Ctrl+V"),
                PasteFiles,
                self.capabilities().paste && clipboard::get_files().is_some(),
            ));
            items.push(Item::Separator);
            items.push(call("Show in File Manager", |this, _, _| {
                let _ = crate::services::xdg::open(&this.navigation.directory);
            }));
            items.push(call("Copy Folder Pathname", |this, _, cx| {
                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
                    this.navigation.directory.display().to_string(),
                ));
            }));
        }
        self.show_menu(at, items, cx);
    }

    /// The toolbar's group button: how the view is ordered.
    pub(super) fn open_sort_menu(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let (key, ascending) = self.listing.sort;
        let by = |label: &str, k: SortKey| {
            check(
                call(label, move |this, _, cx| {
                    this.sort_by(k, this.listing.sort.1, cx)
                }),
                key == k,
            )
        };
        let items = vec![
            Item::Header("Sort By".into()),
            by("Name", SortKey::Name),
            by("Kind", SortKey::Kind),
            by(
                if self.in_trash() {
                    "Date Deleted"
                } else {
                    "Date Modified"
                },
                SortKey::Modified,
            ),
            by("Size", SortKey::Size),
            Item::Separator,
            check(
                call("Ascending", |this, _, cx| {
                    this.sort_by(this.listing.sort.0, true, cx)
                }),
                ascending,
            ),
            check(
                call("Descending", |this, _, cx| {
                    this.sort_by(this.listing.sort.0, false, cx)
                }),
                !ascending,
            ),
        ];
        self.show_menu(at, items, cx);
    }

    pub(super) fn render_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.menu.as_mut()?;
        if menu.popup.is_none() {
            let items = std::mem::take(&mut menu.items);
            let rows = popup_rows(items, cx.entity().downgrade());
            let at = menu.at;
            let font = self.font.clone();
            let popup = cx.new(|cx| crate::ui::menu::ContextMenu::new(rows, at, font, cx));
            menu._dismiss = Some(cx.subscribe_in(
                &popup,
                window,
                |this, _, _: &gpui_kit::DismissEvent, window, cx| {
                    this.menu = None;
                    window.focus(&this.focus, cx);
                    cx.notify();
                },
            ));
            window.focus(&popup.focus_handle(cx), cx);
            menu.popup = Some(popup);
        }
        let popup = menu.popup.clone()?;
        Some(deferred(popup).with_priority(3).into_any_element())
    }
    /// The action menu without a click: under the toolbar's right end.
    pub(super) fn action_menu_key(
        &mut self,
        _: &ShowActionMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at = gpui_kit::point(
            window.viewport_size().width
                - px(if self.inspector_visible {
                    t::INSPECTOR_W
                } else {
                    0.
                } + 240.),
            px(t::TOOLBAR_H - 6.),
        );
        self.open_action_menu(at, cx);
    }

    /// The sort menu without a click: under the toolbar's group button.
    pub(super) fn sort_menu_key(
        &mut self,
        _: &ShowSortMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at = gpui_kit::point(
            window.viewport_size().width
                - px(if self.inspector_visible {
                    t::INSPECTOR_W
                } else {
                    0.
                } + 300.),
            px(t::TOOLBAR_H - 6.),
        );
        self.open_sort_menu(at, cx);
    }

    /// The tag menu without a click: under the toolbar's tag button.
    pub(super) fn tag_menu_key(
        &mut self,
        _: &ShowTagMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at = gpui_kit::point(
            window.viewport_size().width
                - px(if self.inspector_visible {
                    t::INSPECTOR_W
                } else {
                    0.
                } + 200.),
            px(t::TOOLBAR_H - 6.),
        );
        self.open_tag_menu(at, cx);
    }

    /// Shift+F10 / the menu key: the menu for the selection, by the list.
    pub(super) fn context_menu_key(
        &mut self,
        _: &ContextMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ix = self.selection.cursor;
        let row = ix.map_or(0., |i| i as f32);
        let at = gpui_kit::point(
            px(t::SIDEBAR_W + 160.),
            px(t::TOOLBAR_H + t::HEADER_H + (row + 1.) * t::ROW_H),
        );
        self.open_menu(ix, at, window, cx);
    }

    pub(super) fn reveal_in_file_manager(
        &mut self,
        _: &Reveal,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        if let Some(e) = self.selected_entry() {
            let _ = crate::services::xdg::reveal(&e.path);
        }
    }

    pub(super) fn show_icons(&mut self, _: &ShowIcons, _: &mut Window, cx: &mut Context<Self>) {
        self.set_view(ViewMode::Icons, cx);
    }

    pub(super) fn show_list(&mut self, _: &ShowList, _: &mut Window, cx: &mut Context<Self>) {
        self.set_view(ViewMode::List, cx);
    }

    pub(super) fn show_columns(&mut self, _: &ShowColumns, _: &mut Window, cx: &mut Context<Self>) {
        self.set_view(ViewMode::Columns, cx);
    }

    pub(super) fn show_gallery(&mut self, _: &ShowGallery, _: &mut Window, cx: &mut Context<Self>) {
        self.set_view(ViewMode::Gallery, cx);
    }
}
