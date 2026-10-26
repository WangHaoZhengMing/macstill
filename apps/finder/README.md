# Finder for Linux

A Linux file browser built with [GPUI Kit](https://gpui-kit.com/component/), with an in-process filename and text search engine. Supports X11 and Wayland.

Features include list, icon, column and gallery views; image and text previews; tags; file clipboard, drag and drop, rename, duplicate, trash and undo. GPUI Kit provides the window root, input editing and IME, buttons, toolbar, sidebar navigation buttons, status bar and keyboard-accessible popup menus. File icons and thumbnails use the Linux desktop theme and thumbnail cache.

## Build and run

Install a current stable Rust toolchain and the Linux GPUI build prerequisites. On Debian/Ubuntu:

```sh
sudo apt install build-essential pkg-config clang cmake libwayland-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libx11-xcb-dev libxcb1-dev libfontconfig1-dev \
  libfreetype6-dev libvulkan-dev libssl-dev libzstd-dev xdg-utils xdg-user-dirs
```

A Vulkan-capable driver is required. File clipboard integration additionally needs `wl-clipboard` on Wayland or `xclip` on X11. GPUI Kit's upstream [installation guide](https://gpui-kit.com/docs/installation) covers distribution-specific requirements.

```sh
cargo run -p finder
cargo build --release -p finder
cargo run -p fsearch -- --help
```

Finder is a built-in Macstill component in the repository's root Cargo workspace.
It shares the root `Cargo.lock`, build profiles and GPUI backend with Shell.
`cargo build --workspace --locked` builds both into the root `target` directory;
the release installer installs them together. Use `cargo run -p finder` for a
standalone development window, or `target/debug/macstill finder [PATH]` after
building the workspace. Finder remains a separate process so file operations
and window failures do not stop the desktop.

The shared entry point also supports `--reveal PATH`, `--trash` and `--airdrop`.
Dock, Launchpad and AirDrop use the Finder binary beside the current Macstill
release; old copies named `finder` on PATH are not used. An unbound Finder Dock
shortcut uses the built-in component automatically; explicit custom bindings
are preserved.

## Workspace

These two crates are members of the root Macstill workspace:

```text
crates/
  finder/
    src/main.rs       executable entry point
    src/app.rs        application initialization and window creation
    src/actions.rs    action definitions and keyboard bindings
    src/browser/      window state, commands and file views
    src/model/        GPUI-independent entries, tags and typed locations
    src/services/     filesystem, Trash, drives, clipboard and XDG integration
    src/ui/           GPUI Kit controls, theme, assets, scrolling and window chrome
    src/tests/        rendering and navigation regression tests
  fsearch/            search library and optional CLI/daemon
```

See [Finder architecture](crates/finder/README.md) for module responsibilities and dependency boundaries.

The application directly embeds the search engine. The `fsearch` daemon is optional and serves CLI clients through a Unix socket. Indexing skips `/proc`, `/sys`, `/dev` and `/run`; unreadable files remain subject to normal Linux permissions. Windows and macOS backends, Windows preview hosting, platform probes and platform-specific benchmark scripts have been removed. Cross-platform packages may still appear as transitive dependencies in `Cargo.lock` because GPUI Kit supports other systems.

## Window

The normal window uses 12 logical-pixel rounded corners with opaque content panes. Drag the toolbar's empty space or the sidebar's top strip to move it; double-click those areas to maximize or restore. Buttons and text inputs keep their own mouse interaction. Maximized, fullscreen and tiled windows use square corners.

## Scrolling

The sidebar, file list, icon grid, columns, inspector and text previews have independent scroll regions. Overlay scrollbars appear during scrolling and fade when idle. Precise touchpad input uses GPUI Kit's edge bounce; ordinary mouse-wheel input stays clamped at the boundaries. Headers, window controls and the inspector footer stay fixed. The app keeps the system's scroll direction and input deltas.

## External drives

The sidebar's External Drives section discovers USB disks, removable cards and hot-plug volumes, including unmounted filesystems. Device and mount changes refresh every three seconds in a background task. Clicking a mounted volume opens it in Finder; clicking an unmounted volume asks UDisks to mount it. A disconnected current volume returns the browser to Home.

Discovery requires `lsblk` from `util-linux`; mounting requires `udisksctl` from `udisks2` and the desktop's normal permissions. Encrypted volumes must be unlocked in the system disk utility first. Mount paths are read from the system, supporting both `/media/...` and `/run/media/...` as well as custom mount points. Merely detecting a drive does not mount it.

## Trash

The sidebar Trash location uses the same Finder file list and inspector. It aggregates accessible Freedesktop trash bins across mounted volumes, shows deletion dates, and displays the original path in the inspector. Select items and click Restore (or press Ctrl+Shift+R); F5 refreshes the location. Restore refuses to overwrite an existing destination.

The standard home bin is `$XDG_DATA_HOME/Trash` (normally `~/.local/share/Trash`). Other volumes may use `<mount>/.Trash/<uid>` or `<mount>/.Trash-<uid>`; discovery is handled by the `trash` crate rather than a distribution-specific hardcoded path. See the [Freedesktop Trash specification](https://specifications.freedesktop.org/trash/1.0/).

## Configuration

- `FINDER_DIR`: initial directory.
- `FINDER_VIEW`: `list`, `icons`, `columns` or `gallery`.
- `FINDER_APPEARANCE`: `light` or `dark`; otherwise follows desktop appearance.
- `FSEARCH_DIR`: search index directory; defaults to `$XDG_DATA_HOME/fsearch` or `~/.local/share/fsearch`.
- Finder tags and optional fonts live under `$XDG_DATA_HOME/finder` or `~/.local/share/finder`.

Use Ctrl+F to search, F2 to rename, Ctrl+Shift+N to create a folder, Ctrl+C/X/V for file clipboard, Delete to trash, Ctrl+Z to undo, Space for preview and Shift+F10 for the context menu.

## Checks

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test -p fsearch
cargo test -p finder
```

Finder's rendering test requires an available Linux GPU renderer. To save its screenshot, set `FINDER_UI_PREVIEW=/tmp/finder.png`.

Search engine originally based on Noah Dunnagan's FSearch. See [LICENSE](LICENSE).

## Background material

Finder requests a blurred window surface with app ID `org.macstill.Finder`.
The sidebar paints a translucent neutral tint, with stronger coverage when
inactive; the file area and inspector retain opaque backgrounds. Text and icons
retain their own opacity. This follows the regional material approach used in
Macstill Settings, informed by macOS 15 appearance resources; it is not a
recovered Finder-specific filter recipe.

On Hyprland 0.56+, load `config/material.lua` after general window rules and
enable `decoration.blur`. Macstill's shared profile includes the matching rule.
The compositor filters the actual windows behind Finder (`xray=false`); there
is no wallpaper capture or CPU blur. On unsupported compositors the sidebar
will be transparent without blur. Do not reduce whole-window opacity.
