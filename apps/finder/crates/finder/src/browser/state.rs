//! Window-owned browser state, grouped by responsibility.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ViewMode {
    Icons,
    List,
    Columns,
    Gallery,
}

/// One visible row of the (expandable) list.
pub(super) struct Row {
    pub(super) entry: Entry,
    pub(super) depth: u16,
    pub(super) expanded: bool,
}

pub(super) struct Place {
    pub(super) name: SharedString,
    pub(super) icon: &'static str,
    pub(super) location: Option<Location>,
}

pub(super) struct Section {
    pub(super) title: &'static str,
    pub(super) places: Vec<Place>,
}

/// How to reverse a file operation.
#[derive(Clone)]
pub(super) enum Undo {
    Batch(Vec<crate::services::jobs::Change>),
    Rename {
        from: PathBuf,
        to: PathBuf,
    },
    /// Made by us: back to the Trash.
    Created(Vec<PathBuf>),
    /// (where it is now, where it was).
    Moved(Vec<(PathBuf, PathBuf)>),
    /// Back out of the Trash.
    Trashed(Vec<PathBuf>),
}

pub(super) struct Renaming {
    pub(super) path: PathBuf,
    pub(super) input: gpui_kit::Entity<TextInput>,
    pub(super) _subs: Vec<gpui_kit::Subscription>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Scope {
    Everywhere,
    Folder,
}

pub(super) struct Search {
    /// The folder the search began in (the "Folder" scope).
    pub(super) root: PathBuf,
    pub(super) scope: Scope,
    pub(super) took: Option<Duration>,
    pub(super) capped: bool,
    /// Why there are no results (still indexing, a bad query).
    pub(super) error: Option<String>,
    /// Sorted by a column; else by relevance.
    pub(super) sorted: bool,
    /// Showing everything with this tag (from the sidebar), not a query.
    pub(super) tag: Option<Tag>,
}

pub(super) enum Icon {
    Loading,
    Ready(Arc<gpui_kit::RenderImage>),
    Failed,
}

pub(super) struct NavigationState {
    pub(super) directory: PathBuf,
    pub(super) location: Location,
    pub(super) back: Vec<Location>,
    pub(super) forward: Vec<Location>,
}

pub(super) struct ListingState {
    pub(super) generation: u64,
    pub(super) loading: bool,
    pub(super) show_hidden: bool,
    pub(super) free_space: Option<u64>,
    pub(super) watcher: Option<notify::RecommendedWatcher>,
    pub(super) dirty: Arc<std::sync::atomic::AtomicBool>,
    pub(super) monitor: Option<gpui_kit::Task<()>>,
    pub(super) select_after: Vec<PathBuf>,
    pub(super) select_name: Option<String>,
    pub(super) rows: Vec<Row>,
    pub(super) sort: (SortKey, bool),
    pub(super) error: Option<String>,
    pub(super) trash_items: HashMap<PathBuf, trash::TrashItem>,
}

pub(super) struct SelectionState {
    pub(super) cursor: Option<usize>,
    pub(super) marked: BTreeSet<usize>,
    pub(super) anchor: Option<usize>,
}

pub(super) struct SearchState {
    pub(super) input: gpui_kit::Entity<TextInput>,
    pub(super) engine: Option<fsearch::Engine>,
    pub(super) session: Option<Search>,
    pub(super) generation: u64,
    pub(super) chips: Vec<(String, String)>,
}

pub(super) struct SidebarState {
    pub(super) sections: Vec<Section>,
    pub(super) drives: Vec<crate::services::drives::Drive>,
    pub(super) error: Option<String>,
    pub(super) mounting: Option<PathBuf>,
    pub(super) monitor: Option<gpui_kit::Task<()>>,
    pub(super) scroll: gpui_kit::ScrollHandle,
}

pub(super) struct PreviewState {
    pub(super) quick_look: bool,
    pub(super) peek: Option<quicklook::Peek>,
    pub(super) info_collapsed: bool,
    pub(super) icons: HashMap<icons::Key, Icon>,
}

pub(super) struct ViewState {
    pub(super) mode: ViewMode,
    pub(super) list_columns: t::ListColumns,
    pub(super) columns: Vec<views::Column>,
    pub(super) col_focus: usize,
    pub(super) col_scroll: gpui_kit::ScrollHandle,
    pub(super) strip_scroll: gpui_kit::ScrollHandle,
    pub(super) strip_width: f32,
    pub(super) grid_cols: usize,
    pub(super) scroll: UniformListScrollHandle,
    pub(super) pending_scroll: Option<(usize, u8)>,
}

pub(super) struct GoToDialog {
    pub(super) input: gpui_kit::Entity<TextInput>,
    pub(super) _subs: Vec<gpui_kit::Subscription>,
    pub(super) error: Option<String>,
}
pub(super) struct CommandState {
    pub(super) go_to: Option<GoToDialog>,
    pub(super) job: Option<Arc<crate::services::jobs::Job>>,
    pub(super) apply_all: bool,
    pub(super) renaming: Option<Renaming>,
    pub(super) notice: Option<String>,
    pub(super) undo: Vec<Undo>,
}
