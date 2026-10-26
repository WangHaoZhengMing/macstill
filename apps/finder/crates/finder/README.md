# Finder application architecture

Finder remains one application crate. `fsearch` is the separate search engine crate. The executable entry point delegates to `app::run`; action definitions and keyboard bindings live in `actions.rs`.

```text
src/
  app.rs
  actions.rs
  browser/
    mod.rs           window composition and state construction
    state.rs         navigation, listing, selection, search, sidebar, preview,
                     view and command state groups
    navigation.rs    location activation, loading, history and refresh
    selection.rs     selection and keyboard navigation
    commands.rs      asynchronous file operations, inline rename and undo
    search.rs        queries, generation checks and tag views
    sidebar.rs       places and external-drive monitoring/mounting
    toolbar.rs       toolbar, search controls, breadcrumbs and status bar
    inspector.rs     file information and thumbnail cache
    menu.rs          GPUI Kit menus backed by application actions
    quicklook.rs     image/text preview overlay
    dnd.rs           internal and external drag and drop
    views/
      mod.rs         mode dispatch and scroll/keyboard coordination
      list.rs        common file list for directories, Trash and search
      icons.rs
      columns.rs
      gallery.rs
  model/
    entry.rs         ordinary Rust file records, sorting and display formatting
    location.rs      Folder, Trash, Search and Tag destinations and capabilities
    tag.rs           tag identities and names
  services/
    filesystem.rs    metadata/enumeration and filesystem operations
    trash.rs         Freedesktop Trash discovery and exact-item restore
    drives.rs        external-volume discovery and mounting
    clipboard.rs     Linux file clipboard integration
    icons.rs         thumbnail decoding, without GPUI renderer types
    tags.rs          persistent tag storage
    xdg.rs           XDG directories and desktop file opening
  ui/
    assets.rs
    components.rs    shared controls and renderer image conversion
    theme.rs         layout/color tokens and tag colors
    window.rs        rounded window surfaces and native dragging
    scrolling.rs     shared scrollbars and edge bounce
    text_input.rs    application-event adapter around Kit Input
  tests/
    navigation.rs    actual browser transitions across virtual locations
    rendering.rs     pane rendering, rounded corners and sidebar scroll isolation
```

`model` does not depend on GPUI or call filesystem services. `services` uses model data and Linux APIs/commands, and does not depend on browser widgets or GPUI. `ui` owns GPUI presentation. `browser` composes these layers; its private submodules implement one window rather than introducing a separate controller framework or an Entity per small component.

`Location` is the navigation destination, and history stores complete locations. Trash has no filesystem directory. `NavigationState::directory` keeps the working folder context used by searches and tags; it is not the identity of the current location. The optional `FINDER_DIR=trash:` startup string is parsed at the application boundary into `Location::Trash`.

Location capabilities describe available operations, not Unix access permissions. Commands and menus consult the same policy. Filesystem permissions and restore collisions are still checked by the services. Trash metadata stays associated with physical item identity, while its rows reuse the normal file list and inspector.

Keep asynchronous task ownership and UI notifications in the browser layer. File work runs on the background executor. A new feature should extend its existing state group and module; create another crate or independent view Entity only when its lifecycle or reuse warrants that boundary.

From the workspace root:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo run -p finder
```

Rendering tests require a working Linux GPU renderer. `FINDER_UI_PREVIEW=/tmp/finder.png` saves a rendering-test screenshot; `FINDER_UI_SIZE=1056x620` selects its test viewport.

### Window frame

`ui/frame.rs` procedurally paints a subtle rounded edge and a top-contour
highlight that fades down the upper corners. It follows the light/dark theme
and active/inactive window state, and uses a one-device-pixel stroke at the
current display scale. The surface and frame share `ui/window.rs::radius`;
maximized, fullscreen and tiled windows suppress the frame.

Reference: `macos-extract/macos-window-frame/FOUNDATION.md` (Sequoia 15.0).
The 12-point radius and ink values are adjustable implementation choices;
no extracted Apple assets are bundled. Existing pane backgrounds remain solid.
Outside shadow rendering is left to the Linux compositor, with no clipped
client-side shadow or duplicate shadow added inside the window.

### Context menus

`browser/menu.rs` supplies commands; `ui/menu.rs` owns their presentation and
interaction. `MenuMetrics`, `MenuMaterial` and `MenuState` keep geometry,
color/shadow choices and navigation separate. Reference:
`macos-extract/macos-context-menu-foundation/FOUNDATION.md` (Sequoia 15.0).

The renderer uses an explicit opaque fallback, not translucent sharp content
presented as blur. It does not embed the extracted Apple resources or claim
private Core Animation blur units are equivalent to GPUI blur parameters.
The menu has independent surface/selection radii, measured shared columns,
device-aligned separators, inset selection and outside shadows. Popup placement
leaves room for the shadow; submenus flip left at the window edge. Menus are
confined to the application window, rather than separate native popup windows.

Keyboard navigation skips disabled items and separators, wraps, opens/closes
submenus with Right/Left, confirms with Enter/Space and dismisses with Escape.
Browser shortcuts are excluded while the menu owns focus. Outside clicks dismiss
without activating files underneath. Pointer travel into adjacent submenus keeps
them open; long menus scroll keyboard selection into view. Commands close the
menu and restore browser focus before dispatch. Accessible menu roles, labels,
selection, checkmarks and shortcut descriptions accompany the custom rendering.

`tests/menu.rs` checks real key/pointer input, disabled commands, submenu travel,
edge placement and rendering at 1×/2×. Set `FINDER_MENU_PREVIEW=/tmp/menu.png`
when running `rendered_menu_routes_input` to save the submenu preview; use
`FINDER_APPEARANCE=light` or `dark` for the theme. `FINDER_UI_MENU=1` includes a
context menu in the main rendering-test screenshot.
