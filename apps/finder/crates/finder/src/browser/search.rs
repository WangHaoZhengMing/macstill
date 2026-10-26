//! Query lifecycle and tag views.
use super::*;

impl Browser {
    // Search

    pub(super) fn query_changed(&mut self, cx: &mut Context<Self>) {
        if self.in_trash() || self.in_applications() {
            return;
        }
        // A filter followed by a space becomes a chip.
        let raw = self.search.input.read(cx).text().to_string();
        let mut words: Vec<&str> = raw.split(' ').collect();
        let open = words.pop().unwrap_or_default();
        let (filters, rest): (Vec<&str>, Vec<&str>) = words
            .into_iter()
            .filter(|w| !w.is_empty())
            .partition(|w| chip_of(w).is_some());
        if !filters.is_empty() {
            self.search
                .chips
                .extend(filters.iter().filter_map(|w| chip_of(w)));
            let left = rest.into_iter().chain([open]).collect::<Vec<_>>().join(" ");
            // Re-enters with the filters gone.
            self.search.input.update(cx, |i, cx| i.set_text(left, cx));
            return;
        }
        let typed = raw.trim();
        let text = self
            .search
            .chips
            .iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .chain((!typed.is_empty()).then(|| typed.to_string()))
            .collect::<Vec<_>>()
            .join(" ");
        if text.is_empty() {
            if self
                .search
                .session
                .as_ref()
                .is_some_and(|s| s.tag.is_some())
            {
                return;
            }
            if self.search.session.take().is_some() {
                let cur = self.navigation.directory.clone();
                self.load(cur, cx);
                cx.notify();
            }
            return;
        }
        if self
            .search
            .session
            .as_ref()
            .is_some_and(|s| s.tag.is_some())
        {
            self.search.session = None;
        }
        if self.search.session.is_none() {
            self.search.session = Some(Search {
                root: self.navigation.directory.clone(),
                scope: Scope::Everywhere,
                took: None,
                capped: false,
                error: None,
                sorted: false,
                tag: None,
            });
        }
        if let Some(session) = &self.search.session {
            self.navigation.location = Location::Search {
                root: session.root.clone(),
                query: text.clone(),
                folder_scope: session.scope == Scope::Folder,
            };
        }
        self.run_search(text, cx);
    }
    pub(super) fn run_search(&mut self, text: String, cx: &mut Context<Self>) {
        let (Some(engine), Some(s)) = (self.search.engine.clone(), self.search.session.as_mut())
        else {
            return;
        };
        let mut q = match fsearch::Query::parse(&text, &fsearch::sys::home()) {
            Ok(q) => q,
            Err(e) => {
                s.error = Some(e);
                cx.notify();
                return;
            }
        };
        q.limit = SEARCH_LIMIT;
        // Finder shows nothing rather than letters scattered across a name.
        q.min_quality(12);
        if s.scope == Scope::Folder {
            q.scope = Some(fsearch::sys::index_path(&s.root));
        }
        self.search.generation += 1;
        let generation = self.search.generation;
        cx.spawn(async move |this, cx| {
            let t = Instant::now();
            let res = cx.background_spawn(async move { engine.search(&q) }).await;
            let took = t.elapsed();
            let _ = this.update(cx, |b, cx| b.show_results(generation, text, res, took, cx));
        })
        .detach();
    }
    pub(super) fn show_results(
        &mut self,
        generation: u64,
        text: String,
        res: Result<Vec<fsearch::Found>, String>,
        took: Duration,
        cx: &mut Context<Self>,
    ) {
        if generation != self.search.generation {
            return;
        }
        let Some(s) = self.search.session.as_mut() else {
            return;
        };
        match res {
            Ok(found) => {
                s.error = None;
                s.took = Some(took);
                s.capped = found.len() >= SEARCH_LIMIT;
                let mut v: Vec<Entry> = found
                    .into_iter()
                    .map(|f| {
                        let name = f.path.file_name().map_or_else(
                            || f.path.to_string_lossy().into_owned(),
                            |n| n.to_string_lossy().into_owned(),
                        );
                        let modified = Some(UNIX_EPOCH + Duration::from_secs(f.mtime as u64));
                        Entry::from_parts(
                            name,
                            f.path,
                            f.kind & 3 == fsearch::walk::KIND_DIR,
                            f.size,
                            modified,
                        )
                    })
                    .collect();
                if s.sorted {
                    files::sort(&mut v, self.listing.sort.0, self.listing.sort.1);
                }
                self.listing.rows = v
                    .into_iter()
                    .map(|entry| Row {
                        entry,
                        depth: 0,
                        expanded: false,
                    })
                    .collect();
                self.selection.cursor = None;
                self.selection.marked.clear();
                self.view.scroll.scroll_to_item(0, ScrollStrategy::Top);
            }
            Err(e) => {
                // Still indexing: ask again shortly.
                let retry = e.starts_with("indexing");
                s.error = Some(e);
                if retry {
                    cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(Duration::from_secs(1)).await;
                        let _ = this.update(cx, |b, cx| {
                            if b.search.generation == generation && b.search.session.is_some() {
                                b.run_search(text, cx);
                            }
                        });
                    })
                    .detach();
                }
            }
        }
        cx.notify();
    }
    pub(super) fn set_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        if let Some(s) = &mut self.search.session
            && s.scope != scope
        {
            s.scope = scope;
            self.query_changed(cx);
        }
    }
    /// Everything tagged `tag`, as Finder's sidebar tags show it.
    pub(super) fn show_tag(&mut self, tag: Tag, cx: &mut Context<Self>) {
        let destination = Location::Tag {
            root: self.navigation.directory.clone(),
            tag,
        };
        if self.navigation.location != destination {
            self.navigation.back.push(self.navigation.location.clone());
            self.navigation.forward.clear();
        }
        self.activate_tag(tag, cx);
    }
    pub(super) fn activate_tag(&mut self, tag: Tag, cx: &mut Context<Self>) {
        self.navigation.location = Location::Tag {
            root: self.navigation.directory.clone(),
            tag,
        };
        self.search.chips.clear();
        self.search.generation += 1;
        self.search.session = Some(Search {
            root: self.navigation.directory.clone(),
            scope: Scope::Everywhere,
            took: None,
            capped: false,
            error: None,
            sorted: true,
            tag: Some(tag),
        });
        self.search.input.update(cx, |i, cx| i.set_text("", cx));
        let mut v: Vec<Entry> = self
            .tags
            .tagged(tag)
            .into_iter()
            .filter_map(ops::entry_from_path)
            .collect();
        files::sort(&mut v, self.listing.sort.0, self.listing.sort.1);
        self.listing.rows = v
            .into_iter()
            .map(|entry| Row {
                entry,
                depth: 0,
                expanded: false,
            })
            .collect();
        self.selection.cursor = None;
        self.selection.marked.clear();
        self.view.scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }
    /// Toggles `tag` on the selection.
    pub(super) fn toggle_tag(&mut self, tag: Tag, cx: &mut Context<Self>) {
        if self.in_trash() || self.in_applications() {
            return;
        }
        let paths = self.selection();
        self.tags.toggle(&paths, tag);
        // A tag view drops what no longer has its tag.
        if let Some(t) = self.search.session.as_ref().and_then(|s| s.tag) {
            self.show_tag(t, cx);
        }
        cx.notify();
    }
    /// Leave search: clear the field, back to the folder.
    pub(super) fn end_search(&mut self, cx: &mut Context<Self>) {
        self.search.chips.clear();
        if self.search.session.take().is_some() {
            self.search.generation += 1;
            self.search.input.update(cx, |i, cx| i.set_text("", cx));
            let cur = self.navigation.directory.clone();
            self.load(cur, cx);
            cx.notify();
        }
    }
    pub(super) fn focus_search(
        &mut self,
        _: &FocusSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.in_trash() || self.in_applications() {
            return;
        }
        self.inspector_visible = true;
        let h = self.search.input.focus_handle(cx);
        window.focus(&h, cx);
        cx.notify();
    }
    pub(super) fn escape(&mut self, _: &EndSearch, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview.quick_look {
            self.preview.quick_look = false;
            cx.notify();
            return;
        }
        self.end_search(cx);
        window.focus(&self.focus, cx);
    }
}

const SEARCH_LIMIT: usize = 1000;
/// `ext:pdf` and the like: a filter the field shows as a chip.
pub(super) fn chip_of(word: &str) -> Option<(String, String)> {
    let (k, v) = word.split_once(':')?;
    let k = k.to_ascii_lowercase();
    let known = ["ext", "kind", "type", "size", "mtime", "modified", "in"];
    (known.contains(&k.as_str()) && !v.is_empty()).then(|| (k, v.to_string()))
}
/// How a chip reads: Finder's attribute name, then the value in words.
pub(super) fn chip_label(k: &str, v: &str) -> (&'static str, String) {
    match k {
        "ext" => (
            "KIND",
            v.split(',')
                .map(|e| files::kind_of(&format!("x.{e}"), false))
                .collect::<Vec<_>>()
                .join(", "),
        ),
        "kind" => (
            "KIND",
            match v {
                "dir" | "folder" | "d" => "Folder".into(),
                "file" | "f" => "Document".into(),
                "link" | "symlink" | "l" => "Alias".into(),
                v => v.into(),
            },
        ),
        "type" => ("KIND", capitalize(v)),
        "size" => (
            "SIZE",
            v.replace('>', "> ").replace('<', "< ").to_uppercase(),
        ),
        "mtime" | "modified" => (
            "MODIFIED",
            v.replace('<', "within ").replace('>', "before "),
        ),
        "in" => ("IN", v.into()),
        _ => ("", v.into()),
    }
}
pub(super) fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
}
