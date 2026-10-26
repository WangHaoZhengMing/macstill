//! Location activation, history and directory loading.
use super::*;

impl Browser {
    pub(super) fn in_applications(&self) -> bool {
        self.navigation.location == Location::Applications
    }
    // Navigation

    pub(super) fn in_trash(&self) -> bool {
        self.navigation.location.is_trash()
    }
    pub(super) fn capabilities(&self) -> Capabilities {
        self.navigation.location.capabilities()
    }
    pub(super) fn load(&mut self, destination: impl Into<Location>, cx: &mut Context<Self>) {
        let destination = destination.into();
        if let Some(dir) = destination.directory() {
            self.navigation.directory = dir.into();
        }
        self.navigation.location = destination;
        self.listing.rows.clear();
        self.listing.trash_items.clear();
        self.listing.error = None;
        self.listing.select_after.clear();
        self.selection.cursor = None;
        self.selection.marked.clear();
        self.selection.anchor = None;
        self.view.columns.clear();
        self.view.scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.request_listing(Vec::new(), cx);
    }
    fn request_listing(&mut self, expanded: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.listing.generation += 1;
        let generation = self.listing.generation;
        let location = self.navigation.location.clone();
        if matches!(
            location,
            Location::AirDrop | Location::Search { .. } | Location::Tag { .. }
        ) {
            self.listing.loading = false;
            return;
        }
        self.listing.loading = true;
        let directory = self.navigation.directory.clone();
        let hidden = self.listing.show_hidden;
        let sort = self.listing.sort;
        let dirty = self.listing.dirty.clone();
        let columns: Vec<_> = self.view.columns.iter().map(|c| c.path.clone()).collect();
        cx.spawn(async move |this, cx| {
            let expected = location.clone();
            let result = cx
                .background_spawn(async move {
                    use notify::Watcher;
                    let mut watcher =
                        notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                            if event.is_ok_and(|e| !matches!(e.kind, notify::EventKind::Access(_)))
                            {
                                dirty.store(true, std::sync::atomic::Ordering::Relaxed);
                            }
                        })
                        .ok();
                    if let Some(w) = &mut watcher {
                        if location == Location::Applications {
                            for root in macstill_apps::roots() {
                                let _ = w.watch(&root, notify::RecursiveMode::Recursive);
                            }
                        }
                        for path in std::iter::once(&directory)
                            .chain(expanded.iter())
                            .chain(columns.iter())
                        {
                            let _ = w.watch(path, notify::RecursiveMode::NonRecursive);
                        }
                        if let Some(parent) = directory.parent() {
                            let _ = w.watch(parent, notify::RecursiveMode::NonRecursive);
                        }
                    }
                    let free = if location == Location::Applications {
                        None
                    } else {
                        ops::free_space(&directory)
                    };
                    let (entries, trash) = if location == Location::Applications {
                        (crate::services::applications::list(), HashMap::new())
                    } else if location.is_trash() {
                        crate::services::trash::list()?
                    } else {
                        (
                            ops::list_hidden(&directory, hidden).map_err(|e| e.to_string())?,
                            HashMap::new(),
                        )
                    };
                    fn append(
                        mut entries: Vec<Entry>,
                        depth: u16,
                        expanded: &[PathBuf],
                        hidden: bool,
                        sort: (SortKey, bool),
                        rows: &mut Vec<Row>,
                    ) {
                        files::sort(&mut entries, sort.0, sort.1);
                        for entry in entries {
                            let open = entry.is_dir && expanded.contains(&entry.path);
                            let children = if open {
                                ops::list_hidden(&entry.path, hidden).ok()
                            } else {
                                None
                            };
                            rows.push(Row {
                                entry,
                                depth,
                                expanded: children.is_some(),
                            });
                            if let Some(children) = children {
                                append(children, depth + 1, expanded, hidden, sort, rows);
                            }
                        }
                    }
                    let mut rows = Vec::new();
                    append(entries, 0, &expanded, hidden, sort, &mut rows);
                    let columns = columns
                        .into_iter()
                        .map(|p| {
                            let entries = ops::list_hidden(&p, hidden).map_err(|e| e.to_string());
                            (p, entries)
                        })
                        .collect::<Vec<_>>();
                    Ok::<_, String>((rows, trash, free, watcher, columns))
                })
                .await;
            let _ = this.update(cx, |b, cx| {
                if b.listing.generation != generation || b.navigation.location != expected {
                    return;
                }
                b.listing.loading = false;
                let selected = if b.listing.select_after.is_empty() {
                    b.selection()
                } else {
                    std::mem::take(&mut b.listing.select_after)
                };
                match result {
                    Ok((rows, trash, free, watcher, columns)) => {
                        b.listing.rows = rows;
                        b.listing.trash_items = trash;
                        b.listing.free_space = free;
                        b.listing.watcher = watcher;
                        b.listing.error = None;
                        for (path, entries) in columns {
                            if let Some(c) = b.view.columns.iter_mut().find(|c| c.path == path) {
                                let selected =
                                    c.sel.and_then(|i| c.entries.get(i)).map(|e| e.path.clone());
                                match entries {
                                    Ok(mut entries) => {
                                        files::sort(&mut entries, SortKey::Name, true);
                                        c.entries = entries;
                                        c.error = None;
                                    }
                                    Err(e) => c.error = Some(e),
                                }
                                c.sel = selected
                                    .and_then(|p| c.entries.iter().position(|e| e.path == p));
                            }
                        }
                        b.selection.cursor = None;
                        b.selection.marked.clear();
                        for (i, row) in b.listing.rows.iter().enumerate() {
                            if selected.contains(&row.entry.path) {
                                b.selection.marked.extend(b.selection.cursor);
                                b.selection.cursor = Some(i);
                            }
                        }
                        if let Some(c) = b.view.columns.first_mut() {
                            if c.path == b.navigation.directory {
                                let selection =
                                    c.sel.and_then(|i| c.entries.get(i)).map(|e| e.path.clone());
                                c.entries = b
                                    .listing
                                    .rows
                                    .iter()
                                    .filter(|r| r.depth == 0)
                                    .map(|r| r.entry.clone())
                                    .collect();
                                c.sel = selection
                                    .and_then(|p| c.entries.iter().position(|e| e.path == p));
                            }
                        }
                        b.preview.icons.clear();
                        if let Some(name) = b.listing.select_name.take() {
                            b.select_by_name(&name, cx);
                        }
                    }
                    Err(e) => {
                        b.listing.rows.clear();
                        b.listing.error = Some(e);
                        b.selection.cursor = None;
                        b.selection.marked.clear();
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn start_listing_monitor(&mut self, cx: &mut Context<Self>) {
        if cfg!(test) {
            return;
        }
        self.listing.monitor = Some(cx.spawn(async move |this, cx| {
            let mut ticks = 0;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                ticks += 1;
                if this
                    .update(cx, |b, cx| {
                        if !b.listing.loading
                            && b.commands.renaming.is_none()
                            && b.commands.job.is_none()
                            && (b
                                .listing
                                .dirty
                                .swap(false, std::sync::atomic::Ordering::Relaxed)
                                || ticks % 20 == 0)
                        {
                            b.refresh(&[], cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }
    pub(super) fn navigate(&mut self, destination: impl Into<Location>, cx: &mut Context<Self>) {
        let destination = destination.into();
        if destination == self.navigation.location {
            self.refresh(&[], cx);
            return;
        }
        let previous = self.navigation.location.clone();
        self.end_search(cx);
        self.navigation.back.push(previous);
        self.navigation.forward.clear();
        self.activate_location(destination, cx);
    }
    pub(super) fn activate_location(&mut self, destination: Location, cx: &mut Context<Self>) {
        match destination {
            Location::Search {
                root,
                query,
                folder_scope,
            } => {
                self.load(root.clone(), cx);
                self.search.session = Some(Search {
                    root: root.clone(),
                    scope: if folder_scope {
                        Scope::Folder
                    } else {
                        Scope::Everywhere
                    },
                    took: None,
                    capped: false,
                    error: None,
                    sorted: false,
                    tag: None,
                });
                self.navigation.location = Location::Search {
                    root,
                    query: query.clone(),
                    folder_scope,
                };
                self.search
                    .input
                    .update(cx, |input, cx| input.set_text(query, cx));
            }
            Location::Tag { root, tag } => {
                self.load(root, cx);
                self.activate_tag(tag, cx);
            }
            destination => self.load(destination, cx),
        }
        cx.notify();
    }
    pub(super) fn go_back(&mut self, _: &GoBack, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(destination) = self.navigation.back.pop() {
            self.navigation
                .forward
                .push(self.navigation.location.clone());
            self.end_search(cx);
            self.activate_location(destination, cx);
        }
    }
    pub(super) fn go_forward(&mut self, _: &GoForward, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(destination) = self.navigation.forward.pop() {
            self.navigation.back.push(self.navigation.location.clone());
            self.end_search(cx);
            self.activate_location(destination, cx);
        }
    }
    pub(super) fn go_up(&mut self, _: &GoUp, _: &mut Window, cx: &mut Context<Self>) {
        if self.in_trash() || self.in_applications() {
            return;
        }
        if let Some(p) = self.navigation.directory.parent() {
            let (from, to) = (self.navigation.directory.clone(), p.to_path_buf());
            self.navigate(to, cx);
            // Land on the folder we came out of.
            self.listing.select_after = vec![from];
            if let Some(i) = self.selection.cursor {
                self.view.scroll.scroll_to_item(i, ScrollStrategy::Center);
            }
        }
    }
    /// Open a row: folders in place, files with their default app.
    pub(super) fn open_row(&mut self, ix: usize, cx: &mut Context<Self>) {
        if !self.capabilities().open {
            return;
        }
        let Some(r) = self.listing.rows.get(ix) else {
            return;
        };
        if self.in_applications() {
            self.listing.error = crate::services::applications::launch(&r.entry.path).err();
            cx.notify();
        } else if r.entry.is_dir {
            let p = r.entry.path.clone();
            self.navigate(p, cx);
        } else {
            let _ = crate::services::xdg::open(&r.entry.path);
        }
    }
    /// A column header click: that key, or the other direction of it.
    pub(super) fn set_sort(&mut self, key: SortKey, cx: &mut Context<Self>) {
        let ascending = if self.listing.sort.0 == key {
            !self.listing.sort.1
        } else {
            true
        };
        self.sort_by(key, ascending, cx);
    }
    pub(super) fn sort_by(&mut self, key: SortKey, ascending: bool, cx: &mut Context<Self>) {
        self.listing.sort = (key, ascending);
        if let Some(s) = &mut self.search.session {
            s.sorted = true;
            let sel = self
                .selection
                .cursor
                .map(|i| self.listing.rows[i].entry.path.clone());
            let mut v: Vec<Entry> = self.listing.rows.drain(..).map(|r| r.entry).collect();
            files::sort(&mut v, self.listing.sort.0, self.listing.sort.1);
            self.listing.rows = v
                .into_iter()
                .map(|entry| Row {
                    entry,
                    depth: 0,
                    expanded: false,
                })
                .collect();
            self.selection.cursor =
                sel.and_then(|p| self.listing.rows.iter().position(|r| r.entry.path == p));
            cx.notify();
            return;
        }
        self.refresh(&[], cx);
    }
    // File operations

    /// Re-read the folder, keeping folders opened in place, then select
    /// `select` (if they're there).
    pub(super) fn refresh(&mut self, select: &[PathBuf], cx: &mut Context<Self>) {
        if self.search.session.is_some() {
            return;
        }
        if self.in_airdrop() {
            return;
        }
        if !select.is_empty() {
            self.listing.select_after = select.to_vec();
        }
        let expanded = self
            .listing
            .rows
            .iter()
            .filter(|r| r.expanded)
            .map(|r| r.entry.path.clone())
            .collect();
        self.request_listing(expanded, cx);
    }
}
