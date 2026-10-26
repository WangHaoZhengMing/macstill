use super::*;
#[cfg(test)]
mod tests;
use crate::desktop as files;
use gio::prelude::*;
use swiftui_gpui::{menu as context_menu, menu_item, menu_separator};

pub(super) struct DesktopState {
    root: PathBuf,
    positions: files::Positions,
    layout_path: PathBuf,
    drag: Option<(PathBuf, Point<Pixels>, [f32; 2], bool)>,
    rename: Option<(PathBuf, Entity<InputState>)>,
    entries: Vec<files::Entry>,
    selected: Option<PathBuf>,
    menu: Option<Point<Pixels>>,
    focus: FocusHandle,
    page: usize,
    busy: bool,
    _watch: Option<Task<()>>,
}
impl DesktopState {
    pub fn new(window: &Window, cx: &mut Context<DesktopView>) -> Self {
        let output = window
            .display(cx)
            .and_then(|d| d.uuid().ok())
            .map(|u| u.to_string())
            .unwrap_or_else(|| "default".into());
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| gio::glib::home_dir().join(".config"));
        let layout_path = config
            .join("macstill")
            .join(format!("desktop-{output}.json"));
        Self {
            positions: files::load_positions(&layout_path),
            layout_path,
            drag: None,
            rename: None,
            root: files::directory(),
            entries: Vec::new(),
            selected: None,
            menu: None,
            focus: cx.focus_handle(),
            page: 0,
            busy: false,
            _watch: None,
        }
    }
}
impl DesktopView {
    fn desktop_save_layout(&mut self, cx: &mut Context<Self>) {
        // Tiny per-output state; serialize synchronously so successive drops cannot reorder writes.
        if let Err(e) = files::save_positions(&self.desktop.layout_path, &self.desktop.positions) {
            self.message = Some(e.to_string());
        }
        cx.notify();
    }
    fn desktop_copy(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.desktop.selected.clone() {
            cx.write_to_clipboard(ClipboardItem {
                entries: vec![ClipboardEntry::ExternalPaths(ExternalPaths(
                    vec![path].into(),
                ))],
            });
        }
        self.desktop.menu = None;
        cx.notify();
    }
    fn desktop_rename(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.desktop.selected.clone() else {
            return;
        };
        let input = cx.new(|cx| {
            let mut input = InputState::new(w, cx);
            input.set_value(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                w,
                cx,
            );
            input.focus(w, cx);
            input.select_all(w, cx);
            input
        });
        self.desktop.rename = Some((path, input));
        self.desktop.menu = None;
        cx.notify();
    }
    fn desktop_finish_rename(&mut self, cx: &mut Context<Self>) {
        let Some((path, input)) = self.desktop.rename.as_ref() else {
            return;
        };
        let path = path.clone();
        let name = input.read(cx).value().to_string();
        self.desktop_operation(path, Some(name), false, cx);
    }
    fn desktop_operation(
        &mut self,
        path: PathBuf,
        name: Option<String>,
        trash: bool,
        cx: &mut Context<Self>,
    ) {
        if self.desktop.busy {
            return;
        }
        self.desktop.busy = true;
        self.desktop.menu = None;
        self.message = None;
        let original = path.clone();
        let is_rename = name.is_some();
        let root = self.desktop.root.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let new = if let Some(name) = name {
                        Some(files::rename(&path, &name)?)
                    } else if trash {
                        files::trash(&path)?;
                        None
                    } else {
                        Some(files::duplicate(&path)?)
                    };
                    anyhow::Ok((new, files::entries(&root)?))
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.desktop.busy = false;
                match result {
                    Ok((new, entries)) => {
                        if is_rename || trash {
                            if let Some(position) = s.desktop.positions.remove(&original) {
                                if let Some(path) = &new {
                                    s.desktop.positions.insert(path.clone(), position);
                                }
                            }
                            s.desktop_save_layout(cx);
                        }
                        s.desktop.rename = None;
                        s.desktop.selected = new;
                        s.desktop.entries = entries;
                    }
                    Err(e) => s.message = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn watch_desktop(&mut self, cx: &mut Context<Self>) {
        let root = self.desktop.root.clone();
        self.desktop._watch = Some(cx.spawn(async move |this, cx| {
            loop {
                let root = root.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { files::entries(&root) })
                    .await;
                if this
                    .update(cx, |s, cx| match result {
                        Ok(entries) if entries != s.desktop.entries => {
                            s.desktop.entries = entries;
                            if s.desktop.selected.as_ref().is_some_and(|path| {
                                !s.desktop.entries.iter().any(|e| &e.path == path)
                            }) {
                                s.desktop.selected = None;
                                s.desktop.menu = None;
                            }
                            cx.notify();
                        }
                        Err(e) => {
                            s.message = Some(e.to_string());
                            cx.notify();
                        }
                        _ => {}
                    })
                    .is_err()
                {
                    break;
                }
                cx.background_executor().timer(Duration::from_secs(1)).await;
            }
        }));
    }
    fn desktop_import(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if self.desktop.busy || paths.is_empty() {
            return;
        }
        self.desktop.busy = true;
        self.desktop.menu = None;
        self.message = None;
        let root = self.desktop.root.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { files::import(&root, &paths) })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.desktop.busy = false;
                if let Err(e) = result {
                    s.message = Some(e.to_string());
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn desktop_paste(&mut self, cx: &mut Context<Self>) {
        let paths = cx
            .read_from_clipboard()
            .map(|item| {
                item.entries()
                    .iter()
                    .filter_map(|entry| {
                        if let ClipboardEntry::ExternalPaths(paths) = entry {
                            Some(paths.paths().to_vec())
                        } else {
                            None
                        }
                    })
                    .flatten()
                    .collect()
            })
            .unwrap_or_default();
        self.desktop_import(paths, cx);
    }
    fn desktop_open(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.desktop.menu = None;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    gio::AppInfo::launch_default_for_uri(
                        gio::File::for_path(path).uri().as_str(),
                        gio::AppLaunchContext::NONE,
                    )
                    .map_err(|e| e.to_string())
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                if let Err(e) = result {
                    s.message = Some(e);
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn desktop_new_folder(&mut self, cx: &mut Context<Self>) {
        let root = self.desktop.root.clone();
        self.desktop.menu = None;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { files::new_folder(&root) })
                .await;
            let _ = this.update(cx, |s, cx| {
                match result {
                    Ok(path) => s.desktop.selected = Some(path),
                    Err(e) => s.message = Some(e.to_string()),
                };
                cx.notify();
            });
        })
        .detach();
    }
    fn desktop_add(&mut self, cx: &mut Context<Self>) {
        self.desktop.menu = None;
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: true,
            prompt: Some("Copy to Desktop".into()),
        });
        cx.spawn(async move |this, cx| match prompt.await {
            Ok(Ok(Some(paths))) => {
                let _ = this.update(cx, |s, cx| s.desktop_import(paths, cx));
            }
            Ok(Err(e)) => {
                let _ = this.update(cx, |s, cx| {
                    s.message = Some(e.to_string());
                    cx.notify();
                });
            }
            _ => {}
        })
        .detach();
    }
    pub(super) fn desktop_surface(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.mode == Mode::Desktop
            && let Some(bounds) = super::displays::window_bounds(window, cx)
        {
            if window.bounds().size != bounds.size {
                window.resize(bounds.size);
            }
        }
        let bounds = window.bounds().size;
        let width = f32::from(bounds.width);
        let height = f32::from(bounds.height);
        let rows = ((height - 150.) / 106.).floor().max(1.) as usize;
        let columns = ((width - 20.) / 100.).floor().max(1.) as usize;
        let capacity = rows * columns;
        let pages = self.desktop.entries.len().div_ceil(capacity).max(1);
        let page = self.desktop.page.min(pages - 1);
        let p = Palette::new(self.snapshot(cx).preferences.dark);
        let wallpaper = macstill_ui::wallpaper(self.snapshot(cx).preferences.wallpaper.as_deref());
        let mut root = div()
            .id("desktop-files")
            .relative()
            .size_full()
            .track_focus(&self.desktop.focus)
            .font_family(cx.global::<Fonts>().text.clone())
            .text_size(px(12.))
            .child(
                img(wallpaper)
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|s, _, w, cx| {
                    s.desktop.menu = None;
                    s.desktop.selected = None;
                    s.desktop.rename = None;
                    s.desktop.focus.focus(w, cx);
                    let snapshot = s.snapshot(cx).stage;
                    let stage = snapshot
                        .outputs
                        .iter()
                        .find(|stage| {
                            displays::output_named(stage.output.as_deref(), cx) == s.target_display
                        })
                        .unwrap_or(&snapshot);
                    if stage.enabled && stage.visible {
                        s.stage_state.output = stage.output.clone();
                        s.stage_command(crate::hyprland::stage::Command::ShowDesktop, cx);
                    }
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |s, e: &MouseDownEvent, w, cx| {
                    s.desktop.selected = None;
                    s.desktop.rename = None;
                    s.desktop.drag = None;
                    s.desktop.menu = Some(point(
                        e.position.x.min(px((width - 234.).max(0.))),
                        e.position.y.min(px((height - 245.).max(0.))),
                    ));
                    s.desktop.focus.focus(w, cx);
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(move |s, e: &MouseMoveEvent, _, cx| {
                if let Some((path, start, origin, moved)) = &mut s.desktop.drag {
                    let dx = f32::from(e.position.x - start.x);
                    let dy = f32::from(e.position.y - start.y);
                    if *moved || dx * dx + dy * dy > 16. {
                        *moved = true;
                        s.desktop.positions.insert(
                            path.clone(),
                            files::clamp_position([origin[0] + dx, origin[1] + dy], width, height),
                        );
                        cx.notify();
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| {
                    if s.desktop.drag.take().is_some_and(|(_, _, _, moved)| moved) {
                        s.desktop_save_layout(cx);
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| {
                    if s.desktop.drag.take().is_some_and(|(_, _, _, moved)| moved) {
                        s.desktop_save_layout(cx);
                    }
                }),
            )
            .on_drop(cx.listener(|s, paths: &ExternalPaths, _, cx| {
                s.desktop_import(paths.paths().to_vec(), cx)
            }))
            .on_key_down(cx.listener(move |s, e: &KeyDownEvent, w, cx| {
                if s.desktop.rename.is_some() {
                    return;
                }
                match e.keystroke.key.as_str() {
                    "v" if e.keystroke.modifiers.control || e.keystroke.modifiers.platform => {
                        s.desktop_paste(cx)
                    }
                    "c" if e.keystroke.modifiers.control || e.keystroke.modifiers.platform => {
                        s.desktop_copy(cx)
                    }
                    "f2" => s.desktop_rename(w, cx),
                    "backspace" if e.keystroke.modifiers.platform => {
                        if let Some(path) = s.desktop.selected.clone() {
                            s.desktop_operation(path, None, true, cx);
                        }
                    }
                    "enter" => {
                        if let Some(path) = s.desktop.selected.clone() {
                            s.desktop_open(path, cx);
                        }
                    }
                    "escape" => {
                        if let Some((path, _, origin, true)) = s.desktop.drag.take() {
                            s.desktop.positions.insert(path, origin);
                        }
                        s.desktop.menu = None;
                        s.desktop.selected = None;
                        cx.notify();
                    }
                    "pagedown" => {
                        s.desktop.page = (page + 1).min(pages - 1);
                        cx.notify();
                    }
                    "pageup" => {
                        s.desktop.page = page.saturating_sub(1);
                        cx.notify();
                    }
                    _ => {}
                }
            }));
        for (i, entry) in self
            .desktop
            .entries
            .iter()
            .skip(page * capacity)
            .take(capacity)
            .enumerate()
        {
            let path = entry.path.clone();
            let right_path = path.clone();
            let position = files::clamp_position(
                self.desktop.positions.get(&path).copied().unwrap_or([
                    (width - 110. - (i / rows) as f32 * 100.).max(0.),
                    44. + (i % rows) as f32 * 106.,
                ]),
                width,
                height,
            );
            let selected = self.desktop.selected.as_ref() == Some(&path);
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let icon = if entry.directory {
                macstill_ui::icon(Some(Self::asset("folder.png")), 56.)
            } else if path.extension().is_some_and(|s| {
                ["png", "jpg", "jpeg", "webp"]
                    .iter()
                    .any(|ext| s.eq_ignore_ascii_case(ext))
            }) {
                macstill_ui::icon(Some(path.clone()), 56.)
            } else {
                div()
                    .w(px(42.))
                    .h(px(54.))
                    .rounded(px(5.))
                    .bg(rgb(0xffffff))
                    .border_1()
                    .border_color(rgba(0x00000030))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(0x697489))
                    .text_size(px(10.))
                    .child(
                        path.extension()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_uppercase(),
                    )
                    .into_any_element()
            };
            root = root.child(
                div()
                    .id(("desktop-file", i))
                    .absolute()
                    .left(px(position[0]))
                    .top(px(position[1]))
                    .w(px(92.))
                    .h(px(98.))
                    .p(px(4.))
                    .rounded(px(6.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(5.))
                    .bg(if selected {
                        rgba(0x3478f650)
                    } else {
                        rgba(0x00000000)
                    })
                    .child(
                        div()
                            .h(px(58.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_center()
                            .max_h(px(30.))
                            .overflow_hidden()
                            .rounded(px(3.))
                            .bg(if selected {
                                rgba(0x3478f6dd)
                            } else {
                                rgba(0x00000055)
                            })
                            .text_color(rgb(0xffffff))
                            .text_size(px(11.))
                            .line_height(relative(1.15))
                            .child(name),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |s, e: &MouseDownEvent, w, cx| {
                            cx.stop_propagation();
                            s.desktop.selected = Some(path.clone());
                            s.desktop.menu = None;
                            s.desktop.focus.focus(w, cx);
                            s.desktop.rename = None;
                            if e.click_count >= 2 {
                                s.desktop.drag = None;
                                s.desktop_open(path.clone(), cx);
                            } else {
                                s.desktop.drag = Some((path.clone(), e.position, position, false));
                            }
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |s, e: &MouseDownEvent, w, cx| {
                            cx.stop_propagation();
                            s.desktop.selected = Some(right_path.clone());
                            s.desktop.drag = None;
                            s.desktop.rename = None;
                            s.desktop.menu = Some(point(
                                e.position.x.min(px((width - 234.).max(0.))),
                                e.position.y.min(px((height - 245.).max(0.))),
                            ));
                            s.desktop.focus.focus(w, cx);
                            cx.notify();
                        }),
                    ),
            );
        }
        if let Some(position) = self.desktop.menu {
            if self.desktop.selected.is_some() {
                root = root.child(
                    context_menu("desktop-file-menu", p)
                        .absolute()
                        .left(position.x)
                        .top(position.y)
                        .on_mouse_down_out(cx.listener(|s, _, _, cx| {
                            s.desktop.menu = None;
                            cx.notify();
                        }))
                        .child(menu_item("file-open", "Open", p).on_click(cx.listener(
                            |s, _, _, cx| {
                                if let Some(path) = s.desktop.selected.clone() {
                                    s.desktop_open(path, cx);
                                }
                            },
                        )))
                        .child(
                            menu_item("file-reveal", "Open Containing Folder", p).on_click(
                                cx.listener(|s, _, _, cx| {
                                    s.desktop_open(s.desktop.root.clone(), cx)
                                }),
                            ),
                        )
                        .child(menu_separator(p))
                        .child(
                            menu_item("file-rename", "Rename", p)
                                .disabled(self.desktop.busy)
                                .on_click(cx.listener(|s, _, w, cx| s.desktop_rename(w, cx))),
                        )
                        .child(
                            menu_item("file-copy", "Copy", p)
                                .on_click(cx.listener(|s, _, _, cx| s.desktop_copy(cx))),
                        )
                        .child(
                            menu_item("file-duplicate", "Duplicate", p)
                                .disabled(self.desktop.busy)
                                .on_click(cx.listener(|s, _, _, cx| {
                                    if let Some(path) = s.desktop.selected.clone() {
                                        s.desktop_operation(path, None, false, cx);
                                    }
                                })),
                        )
                        .child(menu_separator(p))
                        .child(
                            menu_item("file-trash", "Move to Trash", p)
                                .disabled(self.desktop.busy)
                                .on_click(cx.listener(|s, _, _, cx| {
                                    if let Some(path) = s.desktop.selected.clone() {
                                        s.desktop_operation(path, None, true, cx);
                                    }
                                })),
                        ),
                );
            } else {
                root =
                    root.child(
                        context_menu("desktop-menu", p)
                            .absolute()
                            .left(position.x)
                            .top(position.y)
                            .on_mouse_down_out(cx.listener(|s, _, _, cx| {
                                s.desktop.menu = None;
                                cx.notify();
                            }))
                            .child(
                                menu_item("desktop-new-folder", "New Folder", p)
                                    .on_click(cx.listener(|s, _, _, cx| s.desktop_new_folder(cx))),
                            )
                            .child(
                                menu_item("desktop-import", "Add Files or Folders…", p)
                                    .on_click(cx.listener(|s, _, _, cx| s.desktop_add(cx))),
                            )
                            .child(menu_item("desktop-clean-up", "Clean Up", p).on_click(
                                cx.listener(|s, _, _, cx| {
                                    s.desktop.positions.clear();
                                    s.desktop.menu = None;
                                    s.desktop_save_layout(cx);
                                }),
                            ))
                            .child(
                                menu_item("desktop-paste", "Paste", p)
                                    .on_click(cx.listener(|s, _, _, cx| s.desktop_paste(cx))),
                            )
                            .child(menu_separator(p))
                            .child(
                                menu_item("desktop-wallpaper", "Change Wallpaper…", p).on_click(
                                    cx.listener(|s, _, _, cx| {
                                        s.desktop.menu = None;
                                        if let Err(error) =
                                            open_settings(&["--page", "wallpaper"], cx)
                                        {
                                            s.message = Some(error.to_string());
                                        }
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                menu_item("desktop-show-folder", "Open Desktop Folder", p)
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        s.desktop_open(s.desktop.root.clone(), cx)
                                    })),
                            ),
                    );
            }
        }
        if let Some((_, input)) = &self.desktop.rename {
            root = root.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .w(px(360.))
                            .p(px(20.))
                            .rounded(px(12.))
                            .bg(rgb(p.surface))
                            .shadow_lg()
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .child(div().text_color(rgb(p.text)).child("Rename"))
                            .child(swiftui_gpui::text_field("desktop-rename", input, p, cx))
                            .child(
                                div()
                                    .flex()
                                    .justify_end()
                                    .gap(px(8.))
                                    .child(button("rename-cancel", "Cancel", p).on_click(
                                        cx.listener(|s, _, w, cx| {
                                            s.desktop.rename = None;
                                            s.desktop.focus.focus(w, cx);
                                            cx.notify();
                                        }),
                                    ))
                                    .child(
                                        button("rename-save", "Rename", p)
                                            .disabled(self.desktop.busy)
                                            .on_click(cx.listener(|s, _, _, cx| {
                                                s.desktop_finish_rename(cx)
                                            })),
                                    ),
                            ),
                    )
                    .capture_key_down(cx.listener(|s, e: &KeyDownEvent, w, cx| {
                        match e.keystroke.key.as_str() {
                            "enter" => {
                                cx.stop_propagation();
                                s.desktop_finish_rename(cx);
                            }
                            "escape" => {
                                cx.stop_propagation();
                                s.desktop.rename = None;
                                s.desktop.focus.focus(w, cx);
                                cx.notify();
                            }
                            _ => {}
                        }
                    })),
            );
        }
        if pages > 1 {
            root = root.child(
                div()
                    .absolute()
                    .bottom(px(88.))
                    .left(px(20.))
                    .flex()
                    .gap(px(8.))
                    .child(button("desktop-prev", "‹", p).on_click(cx.listener(
                        move |s, _, _, cx| {
                            s.desktop.page = page.saturating_sub(1);
                            cx.notify();
                        },
                    )))
                    .child(div().text_color(rgb(0xffffff)).child(format!(
                        "{} / {}",
                        page + 1,
                        pages
                    )))
                    .child(button("desktop-next", "›", p).on_click(cx.listener(
                        move |s, _, _, cx| {
                            s.desktop.page = (page + 1).min(pages - 1);
                            cx.notify();
                        },
                    ))),
            );
        }
        if self.desktop.busy || self.message.is_some() {
            root = root.child(
                div()
                    .absolute()
                    .bottom(px(90.))
                    .left(px(20.))
                    .max_w(px(460.))
                    .p(px(12.))
                    .rounded(px(8.))
                    .bg(rgb(p.surface))
                    .text_color(rgb(p.text))
                    .child(self.message.clone().unwrap_or("Updating Desktop…".into())),
            );
        }
        root.into_any_element()
    }
}
