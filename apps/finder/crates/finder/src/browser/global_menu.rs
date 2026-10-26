//! A real per-window menu. Commands use the same handlers as Finder's in-window UI.
use super::*;
use macstill_menu::{Entry, Publisher, wayland::Binding};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
#[derive(Default)]
pub(super) struct State {
    // Destroy the Wayland association before the D-Bus service.
    binding: Option<Binding>,
    publisher: Option<Publisher>,
    task: Option<gpui_kit::Task<()>>,
}
fn item(id: i32, label: &str, enabled: bool, shortcut: &[&str]) -> Entry {
    Entry {
        enabled,
        shortcut: if shortcut.is_empty() {
            vec![]
        } else {
            vec![shortcut.iter().map(|s| s.to_string()).collect()]
        },
        ..Entry::item(id, label)
    }
}
impl Browser {
    fn global_menu_model(&self) -> Entry {
        let caps = self.capabilities();
        let n = self.selection().len();
        let idle = self.commands.renaming.is_none()
            && self.commands.go_to.is_none()
            && self.menu.is_none();
        let mut root = Entry::menu(
            0,
            "",
            vec![
                Entry::menu(
                    100,
                    "File",
                    vec![
                        item(1, "New Window", true, &[]),
                        item(
                            10,
                            "New Folder",
                            caps.create && self.search.session.is_none(),
                            &["Control", "Shift", "n"],
                        ),
                        item(11, "Open", n > 0 && caps.open, &["Control", "Down"]),
                        item(12, "Rename", n == 1 && caps.rename, &["F2"]),
                        item(13, "Move to Trash", n > 0 && caps.trash, &["Delete"]),
                        item(
                            14,
                            "Restore from Trash",
                            n > 0 && caps.restore,
                            &["Control", "Shift", "r"],
                        ),
                        item(2, "Close Window", true, &[]),
                    ],
                ),
                Entry::menu(
                    200,
                    "Edit",
                    vec![
                        item(
                            20,
                            "Undo File Operation",
                            !self.commands.undo.is_empty(),
                            &["Control", "z"],
                        ),
                        item(21, "Cut Files", n > 0 && caps.copy, &["Control", "x"]),
                        item(22, "Copy Files", n > 0 && caps.copy, &["Control", "c"]),
                        // Clipboard validity is checked by paste_files at execution time. Do not
                        // launch wl-paste synchronously whenever the selection changes.
                        item(23, "Paste Files", caps.paste, &["Control", "v"]),
                        item(
                            24,
                            "Select All Files",
                            !self.listing.rows.is_empty(),
                            &["Control", "a"],
                        ),
                        item(25, "Duplicate", n > 0 && caps.duplicate, &["Control", "d"]),
                    ],
                ),
                Entry::menu(
                    300,
                    "View",
                    vec![
                        Entry::menu(
                            310,
                            "View As",
                            [
                                (31, "Icons", matches!(self.view.mode, ViewMode::Icons)),
                                (32, "List", matches!(self.view.mode, ViewMode::List)),
                                (33, "Columns", matches!(self.view.mode, ViewMode::Columns)),
                                (34, "Gallery", matches!(self.view.mode, ViewMode::Gallery)),
                            ]
                            .into_iter()
                            .map(|(id, label, checked)| Entry {
                                checked: Some(checked),
                                radio: true,
                                enabled: !self.in_airdrop(),
                                ..Entry::item(id, label)
                            })
                            .collect(),
                        ),
                        item(35, "Quick Look", n > 0 && !self.in_airdrop(), &["space"]),
                        item(36, "Refresh", !self.in_airdrop(), &["F5"]),
                        Entry {
                            checked: Some(self.listing.show_hidden),
                            ..item(
                                37,
                                "Show Hidden Files",
                                !self.in_airdrop(),
                                &["Control", "Shift", "period"],
                            )
                        },
                    ],
                ),
                Entry::menu(
                    400,
                    "Go",
                    vec![
                        item(
                            41,
                            "Back",
                            !self.navigation.back.is_empty(),
                            &["Alt", "Left"],
                        ),
                        item(
                            42,
                            "Forward",
                            !self.navigation.forward.is_empty(),
                            &["Alt", "Right"],
                        ),
                        item(
                            43,
                            "Enclosing Folder",
                            !self.in_trash()
                                && !self.in_airdrop()
                                && self.navigation.directory.parent().is_some(),
                            &["Alt", "Up"],
                        ),
                        item(44, "Search", !self.in_airdrop(), &["Control", "f"]),
                        item(45, "Go to Folder…", true, &["Control", "Shift", "g"]),
                    ],
                ),
            ],
        );
        // File commands must not steal a rename or context-menu interaction.
        if !idle {
            for menu in &mut root.children {
                menu.enabled = false;
            }
        }
        root
    }
    pub(super) fn start_global_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if cfg!(test) {
            return;
        }
        let raw = HasWindowHandle::window_handle(window)
            .ok()
            .map(|h| h.as_raw());
        let xid = match raw {
            Some(RawWindowHandle::Xlib(h)) => u32::try_from(h.window).ok(),
            Some(RawWindowHandle::Xcb(h)) => Some(h.window.get()),
            _ => None,
        };
        let model = self.global_menu_model();
        let handle = Window::window_handle(window);
        let id = handle.window_id();
        let weak = cx.weak_entity();
        cx.on_window_closed(move |cx, closed| {
            if closed == id {
                let _ = weak.update(cx, |s, _| s.global_menu = State::default());
            }
        })
        .detach();
        cx.observe_self(|s, _| {
            if let Some(publisher) = &s.global_menu.publisher {
                let _ = publisher.update(s.global_menu_model());
            }
        })
        .detach();
        self.global_menu.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { Publisher::new(model, xid) })
                .await;
            let publisher = match result {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Finder menu: {e:#}");
                    return;
                }
            };
            if handle
                .update(cx, |_, window, cx| {
                    this.update(cx, |s, _| {
                        if let (Ok(display), Ok(surface)) = (
                            window.display_handle(),
                            HasWindowHandle::window_handle(window),
                        ) {
                            if let (
                                RawDisplayHandle::Wayland(display),
                                RawWindowHandle::Wayland(surface),
                            ) = (display.as_raw(), surface.as_raw())
                            {
                                // All foreign Wayland access is on this window's UI thread. Binding
                                // is removed by on_window_closed before the display is torn down.
                                match unsafe { Binding::new(display, surface, &publisher.endpoint) }
                                {
                                    Ok(binding) => s.global_menu.binding = Some(binding),
                                    Err(e) => eprintln!(
                                        "Finder global menu association unavailable: {e:#}"
                                    ),
                                }
                            }
                        }
                        let _ = publisher.update(s.global_menu_model());
                        s.global_menu.publisher = Some(publisher);
                    })
                })
                .is_err()
            {
                return;
            }
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(25))
                    .await;
                if handle
                    .update(cx, |_, window, cx| {
                        this.update(cx, |s, cx| {
                            let id = s
                                .global_menu
                                .publisher
                                .as_ref()
                                .and_then(Publisher::try_event);
                            if let Some(id) = id {
                                // Revalidate against current UI state, not only the last exported tree.
                                if !s.global_menu_model().actionable(id) {
                                    return;
                                }
                                window.focus(&s.focus, cx);
                                s.global_menu_action(id, window, cx);
                            }
                        })
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }
    fn global_menu_action(&mut self, id: i32, w: &mut Window, cx: &mut Context<Self>) {
        match id {
            1 => {
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe)
                        .arg(&self.navigation.directory)
                        .spawn();
                }
            }
            2 => w.remove_window(),
            10 => self.new_folder(&NewFolder, w, cx),
            11 => self.open_selected(&Open, w, cx),
            12 => self.rename_action(&Rename, w, cx),
            13 => self.trash(&Trash, w, cx),
            14 => self.restore_trash(&RestoreTrash, w, cx),
            20 => self.undo_op(&UndoOp, w, cx),
            21 => self.cut_files(&CutFiles, w, cx),
            22 => self.copy_files(&CopyFiles, w, cx),
            23 => self.paste_files(&PasteFiles, w, cx),
            24 => self.select_all(&SelectAll, w, cx),
            25 => self.duplicate(&Duplicate, w, cx),
            31 => self.show_icons(&ShowIcons, w, cx),
            32 => self.show_list(&ShowList, w, cx),
            33 => self.show_columns(&ShowColumns, w, cx),
            34 => self.show_gallery(&ShowGallery, w, cx),
            35 => self.toggle_quick_look(&QuickLook, w, cx),
            37 => self.toggle_hidden(&ToggleHidden, w, cx),
            45 => self.go_to_folder(&GoToFolder, w, cx),
            36 => self.refresh_location(&RefreshLocation, w, cx),
            41 => self.go_back(&GoBack, w, cx),
            42 => self.go_forward(&GoForward, w, cx),
            43 => self.go_up(&GoUp, w, cx),
            44 => self.focus_search(&FocusSearch, w, cx),
            _ => {}
        }
    }
}
