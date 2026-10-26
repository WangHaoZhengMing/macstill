# Finder implementation checkpoint — 2026-10-26

Work stopped at the user's request. Do not treat the original requested scope as completed.

## Implemented in source

- Per-item operation journal: completed copies/moves/trash operations remain undoable after a later failure or cancellation. Trash undo uses the exact item identity. Failed undo retains the unprocessed entries.
- Copy to a temporary sibling and publish with no-replace rename. Replacement keeps a recoverable sibling backup, restored by Undo. Conflict choices: Skip, Keep Both, Replace, Merge, with Apply to All.
- Operation panel reports completed items and transferred bytes, with cancellation checkpoints during recursive copy. One file-operation job per window.
- GPUI external file-drag payload, preserving internal drag behavior. The installed Wayland backend supports native drag-out; the X11 backend does not implement it. Desktop interoperability remains untested.
- Background ordinary directory loading, generation guards against stale results, filesystem notifications plus periodic refresh, selection by path, expanded-directory snapshots and column refresh. Disk free-space queries moved off render.
- Hidden files toggle in the top View menu; Ctrl+H and Ctrl+Shift+period.
- Go > Go to Folder… in the system menu bar; Ctrl+Shift+G. Supports absolute/relative paths, `~`, and local file URLs.
- Recursive copy preserves timestamps, permission bits, xattrs/ACLs, symlinks and hardlink relationships within each copied tree. Ownership failures are reported when preservation is not permitted.

## Not completed

- P1-4: authenticated access to protected directories and privileged file operations. The unfinished helper was removed; Finder still uses ordinary user permissions.
- P1-7: safe unmount/eject. Existing drive discovery and mounting are unchanged.
- X11 native file drag-out and real desktop validation.

## Known limits / next review

- Operation history is in-memory. Replacement backups use `.finder-backup-*` siblings; successful Undo removes them. A persistent recovery/cleanup policy is still needed for backups surviving window closure or a crash.
- If cross-volume source removal partly fails after a completed copy, the copy is retained and its undo is blocked for manual reconciliation. It must never be trashed as though the source remained complete.
- Merge-move retains the source directory (including skipped items); hardlinks shared across separately selected top-level items are not coalesced.
- Cancellation is cooperative; source removal, rename and trash calls cannot be interrupted partway. Progress reports transferred bytes rather than an estimated total/ETA.
- New-folder and inline rename still use their existing synchronous handlers.
- Current listing refresh is monitored, but restricted-path retry/authentication and inaccessible expanded-directory error presentation still need work.
- No default-file-manager registration changes were requested in this implementation batch.

## Validation

- `cargo check -p finder`: passed.
- Formatting: `cargo fmt -p finder` applied.
- Full Finder test build: blocked by a `gpui-component 0.8.0` dependency compile error (`vertical_scrollbar` missing in its test feature configuration).
- A temporary minimal harness importing the actual filesystem/jobs/trash/XDG/model source files ran 16 tests: all passed. It excludes GPUI and does not validate UI behavior. Harness: `/tmp/macstill-finder-service-check`; output: `/tmp/finder-service-check.log`.
- Added regressions cover partial batch undo, failed copy staging, replacement undo, skip/cancel, metadata preservation and hidden-file filtering.
- No live desktop or cross-app drag test was performed.
