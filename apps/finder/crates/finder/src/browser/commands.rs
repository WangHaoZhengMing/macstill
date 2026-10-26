//! Asynchronous file commands, inline rename and undo.
use super::*;

impl Browser {
    pub(super) fn restore_trash(
        &mut self,
        _: &RestoreTrash,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.capabilities().restore {
            return;
        }
        let items = self
            .selection()
            .iter()
            .filter_map(|path| self.listing.trash_items.get(path).cloned())
            .collect::<Vec<_>>();
        if items.is_empty() {
            return;
        }
        self.run_op(
            move || crate::services::trash::restore(items),
            Vec::new(),
            None,
            cx,
        );
    }
    pub(super) fn refresh_location(
        &mut self,
        _: &RefreshLocation,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh(&[], cx);
    }
    /// Run `op` off the UI thread (shell operations show their own
    /// progress and prompts), then refresh selecting `after`.
    pub(super) fn run_op(
        &mut self,
        op: impl FnOnce() -> Result<(), String> + Send + 'static,
        after: Vec<PathBuf>,
        undo: Option<Undo>,
        cx: &mut Context<Self>,
    ) {
        if self.commands.job.is_some() {
            self.commands.notice =
                Some("Wait for the current file operation or cancel it first.".into());
            cx.notify();
            return;
        }
        self.commands.notice = None;
        self.commands.apply_all = false;
        let job = Arc::new(crate::services::jobs::Job::default());
        self.commands.job = Some(job.clone());
        self.poll_job(cx);
        cx.spawn(async move |this, cx| {
            let worker = job.clone();
            let res = cx
                .background_spawn(async move { crate::services::jobs::scoped(worker, op) })
                .await;
            let _ = this.update(cx, |b, cx| {
                b.commands.job = None;
                let changes = std::mem::take(&mut *job.changes.lock().unwrap());
                for change in &changes {
                    if let crate::services::jobs::Change::Moved { from, to } = change {
                        b.tags.moved(from, to);
                    }
                }
                if !changes.is_empty() {
                    b.commands.undo.push(Undo::Batch(changes));
                } else if res.is_ok()
                    && !matches!(
                        undo,
                        Some(Undo::Created(_) | Undo::Moved(_) | Undo::Trashed(_))
                    )
                {
                    b.commands.undo.extend(undo);
                }
                let progress = job.progress.lock().unwrap();
                b.commands.notice = match res {
                    Err(e) => Some(format!("{} completed. {e}", progress.completed)),
                    Ok(()) if !progress.warnings.is_empty() => Some(progress.warnings.join("; ")),
                    _ => None,
                };
                b.refresh(&after, cx);
            });
        })
        .detach();
        cx.notify();
    }
    fn poll_job(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let active = this
                    .update(cx, |b, cx| {
                        cx.notify();
                        b.commands.job.is_some()
                    })
                    .unwrap_or(false);
                if !active {
                    break;
                }
            }
        })
        .detach();
    }
    /// Reverse the last operation.
    pub(super) fn undo_op(&mut self, _: &UndoOp, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) {
            return;
        }
        if self.commands.job.is_some() {
            return;
        }
        let Some(u) = self.commands.undo.pop() else {
            return;
        };
        use crate::services::jobs::{Change, Job};
        let changes = match u {
            Undo::Batch(changes) => changes,
            Undo::Rename { from, to } => vec![Change::Moved { from, to }],
            Undo::Created(paths) => paths.into_iter().map(Change::Created).collect(),
            Undo::Moved(pairs) => pairs
                .into_iter()
                .map(|(to, from)| Change::Moved { from, to })
                .collect(),
            Undo::Trashed(paths) => {
                // Older callers are converted to exact Trash identities by run_op.
                self.commands.undo.push(Undo::Trashed(paths));
                self.commands.notice =
                    Some("This legacy Trash operation cannot be safely undone.".into());
                return;
            }
        };
        let job = Arc::new(Job::default());
        self.commands.job = Some(job.clone());
        self.commands.notice = None;
        self.poll_job(cx);
        cx.spawn(async move |this, cx| {
            let (remaining, undone, error) = cx
                .background_spawn(async move {
                    crate::services::jobs::scoped(job, || {
                        let mut remaining = changes;
                        let mut undone = Vec::new();
                        while let Some(change) = remaining.last() {
                            if let Err(e) = ops::undo_change(change) {
                                return (remaining, undone, Some(e));
                            }
                            undone.push(remaining.pop().unwrap());
                        }
                        (remaining, undone, None)
                    })
                })
                .await;
            let _ = this.update(cx, |b, cx| {
                b.commands.job = None;
                for change in undone {
                    if let Change::Moved { from, to } = change {
                        b.tags.moved(&to, &from);
                    }
                }
                if !remaining.is_empty() {
                    b.commands.undo.push(Undo::Batch(remaining));
                }
                b.commands.notice = error;
                b.refresh(&[], cx);
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn copy_files(
        &mut self,
        _: &CopyFiles,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.put_on_clipboard(false, window, cx);
    }
    pub(super) fn cut_files(&mut self, _: &CutFiles, window: &mut Window, cx: &mut Context<Self>) {
        self.put_on_clipboard(true, window, cx);
    }
    pub(super) fn put_on_clipboard(
        &mut self,
        cut: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.capabilities().copy {
            return;
        }
        let paths = self.selection();
        if !self.focus.is_focused(window) || paths.is_empty() {
            return;
        }
        self.commands.notice = clipboard::put_files(&paths, cut).err();
        cx.notify();
    }
    /// Paste into the current folder: copies, or moves what was cut.
    /// Copies into the folder they came from are named like Finder's
    /// ("name copy"), not Explorer's ("name - Copy").
    pub(super) fn paste_files(
        &mut self,
        _: &PasteFiles,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.capabilities().paste {
            return;
        }
        if !self.focus.is_focused(window) || self.search.session.is_some() {
            return;
        }
        let Some((from, cut)) = clipboard::get_files() else {
            return;
        };
        let dir = self.navigation.directory.clone();
        let (same, other): (Vec<PathBuf>, Vec<PathBuf>) = from
            .into_iter()
            .partition(|p| p.parent() == Some(dir.as_path()));
        if cut && other.is_empty() {
            return; // already here
        }
        let copies: Vec<PathBuf> = if cut {
            Vec::new()
        } else {
            same.iter().map(|p| ops::copy_name(p)).collect()
        };
        let mut after: Vec<PathBuf> = other
            .iter()
            .filter_map(|p| p.file_name().map(|n| dir.join(n)))
            .collect();
        let undo = if cut {
            Undo::Moved(after.iter().cloned().zip(other.iter().cloned()).collect())
        } else {
            Undo::Created(after.iter().chain(&copies).cloned().collect())
        };
        after.extend(copies.iter().cloned());
        self.run_op(
            move || {
                if cut {
                    ops::run(ops::Op::Move, &other, Some(&dir))?;
                    clipboard::clear();
                } else {
                    ops::run(ops::Op::Copy, &other, Some(&dir))?;
                    ops::copy_as(&same, &copies)?;
                }
                Ok(())
            },
            after,
            Some(undo),
            cx,
        );
    }
    /// The selected paths as text (Finder's Option-Command-C).
    pub(super) fn copy_path(&mut self, _: &CopyPath, window: &mut Window, cx: &mut Context<Self>) {
        let paths = self.selection();
        if !self.focus.is_focused(window) || paths.is_empty() {
            return;
        }
        let text = paths
            .iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>()
            .join("\r\n");
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(text));
    }
    pub(super) fn trash(&mut self, _: &Trash, window: &mut Window, cx: &mut Context<Self>) {
        if !self.capabilities().trash {
            return;
        }
        if !self.focus.is_focused(window) {
            return;
        }
        let paths = self.selection();
        if paths.is_empty() {
            return;
        }
        // Land on the row after the last one removed.
        let next = self
            .selection
            .cursor
            .and_then(|i| {
                self.listing
                    .rows
                    .iter()
                    .skip(i)
                    .find(|r| !paths.contains(&r.entry.path))
            })
            .map(|r| r.entry.path.clone());
        let undo = Undo::Trashed(paths.clone());
        self.run_op(
            move || ops::run(ops::Op::Trash, &paths, None),
            next.into_iter().collect(),
            Some(undo),
            cx,
        );
    }
    pub(super) fn duplicate(&mut self, _: &Duplicate, window: &mut Window, cx: &mut Context<Self>) {
        if !self.capabilities().duplicate {
            return;
        }
        if !self.focus.is_focused(window) {
            return;
        }
        let from = self.selection();
        let to: Vec<PathBuf> = from.iter().map(|p| ops::copy_name(p)).collect();
        if from.is_empty() {
            return;
        }
        let after = to.clone();
        let undo = Undo::Created(to.clone());
        self.run_op(move || ops::copy_as(&from, &to), after, Some(undo), cx);
    }
    pub(super) fn new_folder(
        &mut self,
        _: &NewFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.commands.job.is_some() || !self.capabilities().create {
            return;
        }
        if self.search.session.is_some() {
            return;
        }
        match ops::new_folder(&self.navigation.directory) {
            Ok(p) => {
                self.commands.undo.push(Undo::Created(vec![p.clone()]));
                self.refresh(std::slice::from_ref(&p), cx);
                self.start_rename(p, window, cx);
            }
            Err(e) => {
                self.commands.notice = Some(e);
                cx.notify();
            }
        }
    }
    pub(super) fn rename_action(
        &mut self,
        _: &Rename,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.commands.job.is_some() || !self.capabilities().rename {
            return;
        }
        if !self.focus.is_focused(window)
            || !matches!(self.shown_view(), ViewMode::List | ViewMode::Icons)
        {
            return;
        }
        if let Some(e) = self.selected_entry() {
            self.start_rename(e.path, window, cx);
        }
    }
    /// Edit a name in place: Enter or clicking away saves, Escape doesn't.
    pub(super) fn start_rename(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // Finder selects the name without its extension.
        let stem = match name.rfind('.') {
            Some(i) if i > 0 && !path.is_dir() => i,
            _ => name.len(),
        };
        let input = cx.new(|cx| {
            let mut i = TextInput::new("", window, cx);
            i.set_text(name, cx);
            i.select_range(0..stem, cx);
            i
        });
        let subs = vec![
            cx.subscribe_in(
                &input,
                window,
                |this, _, _: &text_input::Confirmed, window, cx| {
                    this.finish_rename(true, window, cx)
                },
            ),
            cx.subscribe_in(
                &input,
                window,
                |this, _, _: &text_input::Cancelled, window, cx| {
                    this.finish_rename(false, window, cx)
                },
            ),
            cx.on_blur(&input.focus_handle(cx), window, |this, window, cx| {
                this.finish_rename(true, window, cx)
            }),
        ];
        window.focus(&input.focus_handle(cx), cx);
        self.commands.renaming = Some(Renaming {
            path,
            input,
            _subs: subs,
        });
        cx.notify();
    }
    pub(super) fn finish_rename(
        &mut self,
        save: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(r) = self.commands.renaming.take() else {
            return;
        };
        window.focus(&self.focus, cx);
        let name = r.input.read(cx).text().to_string();
        let unchanged = r
            .path
            .file_name()
            .is_some_and(|n| n.to_string_lossy() == name);
        if !save || unchanged {
            cx.notify();
            return;
        }
        match ops::rename(&r.path, &name) {
            Ok(to) => {
                self.commands.notice = None;
                self.tags.moved(&r.path, &to);
                self.commands.undo.push(Undo::Rename {
                    from: r.path.clone(),
                    to: to.clone(),
                });
                self.refresh(&[to], cx);
            }
            Err(e) => {
                self.commands.notice = Some(e);
                cx.notify();
            }
        }
    }
    /// The box a name is edited in, in place of the name.
    pub(super) fn rename_box(&self, path: &Path) -> Option<AnyElement> {
        let r = self.commands.renaming.as_ref().filter(|r| r.path == path)?;
        Some(
            div()
                .flex_1()
                .min_w_0()
                .h(px(18.))
                .px(px(3.))
                .flex()
                .items_center()
                .rounded(px(3.))
                .bg(t::content_bg())
                .border_1()
                .border_color(t::focus_ring())
                .text_color(t::text())
                .child(r.input.clone())
                .into_any_element(),
        )
    }
}
