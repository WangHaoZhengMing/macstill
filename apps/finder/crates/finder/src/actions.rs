//! Application actions and their keyboard bindings.
use gpui_kit::{App, KeyBinding, actions};

actions!(
    finder,
    [
        GoToFolder,
        ToggleHidden,
        RestoreTrash,
        RefreshLocation,
        SelectNext,
        SelectPrev,
        Expand,
        Collapse,
        Open,
        GoBack,
        GoForward,
        GoUp,
        FocusSearch,
        EndSearch,
        SelectAll,
        Rename,
        NewFolder,
        Duplicate,
        Trash,
        CopyFiles,
        CutFiles,
        PasteFiles,
        CopyPath,
        Reveal,
        ShowIcons,
        ShowList,
        ShowColumns,
        ShowGallery,
        ContextMenu,
        UndoOp,
        QuickLook,
        ShowActionMenu,
        ShowSortMenu,
        ShowTagMenu
    ]
);

pub fn bind(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new(
            "ctrl-shift-g",
            GoToFolder,
            Some("Browser && !FinderContextMenu"),
        ),
        KeyBinding::new(
            "ctrl-h",
            ToggleHidden,
            Some("Browser && !FinderContextMenu"),
        ),
        KeyBinding::new(
            "ctrl-shift-.",
            ToggleHidden,
            Some("Browser && !FinderContextMenu"),
        ),
        KeyBinding::new("f5", RefreshLocation, Some("Browser && !FinderContextMenu")),
        KeyBinding::new(
            "ctrl-shift-r",
            RestoreTrash,
            Some("Browser && !FinderContextMenu"),
        ),
        KeyBinding::new("down", SelectNext, Some("Browser && !FinderContextMenu")),
        KeyBinding::new("up", SelectPrev, Some("Browser && !FinderContextMenu")),
        KeyBinding::new("right", Expand, Some("Browser && !FinderContextMenu")),
        KeyBinding::new("left", Collapse, Some("Browser && !FinderContextMenu")),
        // Linux navigation and file operation shortcuts.
        KeyBinding::new("ctrl-down", Open, Some("Browser && !FinderContextMenu")),
        KeyBinding::new("enter", Open, Some("Browser && !FinderContextMenu")),
        KeyBinding::new("ctrl-up", GoUp, Some("Browser && !FinderContextMenu")),
        KeyBinding::new("alt-up", GoUp, Some("Browser && !FinderContextMenu")),
        KeyBinding::new("ctrl-[", GoBack, Some("Browser && !FinderContextMenu")),
        KeyBinding::new("ctrl-]", GoForward, Some("Browser && !FinderContextMenu")),
        KeyBinding::new("alt-left", GoBack, Some("Browser && !FinderContextMenu")),
        KeyBinding::new(
            "alt-right",
            GoForward,
            Some("Browser && !FinderContextMenu"),
        ),
        KeyBinding::new("backspace", GoBack, Some("Browser && !FinderContextMenu")),
        KeyBinding::new("ctrl-f", FocusSearch, Some("Browser && !FinderContextMenu")),
        KeyBinding::new("escape", EndSearch, Some("Browser && !FinderContextMenu")),
        KeyBinding::new("ctrl-a", SelectAll, Some("Browser && !FinderContextMenu")),
        KeyBinding::new("f2", Rename, Some("Browser && !FinderContextMenu")),
        KeyBinding::new(
            "ctrl-shift-n",
            NewFolder,
            Some("Browser && !FinderContextMenu"),
        ),
        KeyBinding::new("ctrl-d", Duplicate, Some("Browser && !FinderContextMenu")),
        KeyBinding::new("delete", Trash, Some("Browser && !FinderContextMenu")),
        KeyBinding::new(
            "ctrl-backspace",
            Trash,
            Some("Browser && !FinderContextMenu"),
        ),
        KeyBinding::new("ctrl-c", CopyFiles, Some("Browser && !FinderContextMenu")),
        KeyBinding::new("ctrl-x", CutFiles, Some("Browser && !FinderContextMenu")),
        KeyBinding::new("ctrl-v", PasteFiles, Some("Browser && !FinderContextMenu")),
        KeyBinding::new(
            "ctrl-alt-c",
            CopyPath,
            Some("Browser && !FinderContextMenu"),
        ),
        KeyBinding::new(
            "shift-f10",
            ContextMenu,
            Some("Browser && !FinderContextMenu"),
        ),
        KeyBinding::new("ctrl-z", UndoOp, Some("Browser && !FinderContextMenu")),
        KeyBinding::new("space", QuickLook, Some("Browser && !FinderContextMenu")),
        KeyBinding::new("ctrl-y", QuickLook, Some("Browser && !FinderContextMenu")),
    ]);
}
