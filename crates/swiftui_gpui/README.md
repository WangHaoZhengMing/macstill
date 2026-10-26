# swiftui_gpui

Apple-style presentation for GPUI Kit 0.8.0, built on its unstyled `gpui-base`
controls. This is a Rust component library, not an implementation of SwiftUI's
language or state system. It has no Macstill assets, configuration, or platform
service dependencies.

## Included

- `button`: bordered push button backed by `base::Button`, including disabled,
  hover, pressed, and keyboard focus presentation.
- `toggle`: controlled switch backed by `base::Switch`; supports an accessible
  label, disabled state, an external focus handle, and `on_change`.
- `text_field`: styled `base::Input` using the application's existing `InputState`.
- `panel`, `caption`, `form`, `section`, and `sidebar`: composable containers.
- `traffic_lights`: scoped close, minimize, and zoom buttons for custom title bars.
- `Palette`, `set_theme`, and `Fonts`: light/dark colors, an overridable accent,
  semantic tokens projected into gpui-base, and installed-font fallback selection.

Slider, menu, and dialog styling can be added here when an application needs them.

## Use

Add `swiftui_gpui.workspace = true` to an application in this workspace. Call
`swiftui_gpui::init(cx)` once in the GPUI application startup callback.
Macstill applications can instead use `macstill_ui::init(cx)`, which delegates to
this initialization. Do not initialize both layers separately.

Inside a view, compose controls with GPUI's normal builders:

```rust,ignore
let palette = Palette::new(self.dark);
swiftui_gpui::set_theme(palette, cx);

form()
    .child(button("save", "Save", palette)
        .disabled(self.saving)
        .on_click(cx.listener(|view, _, _, cx| view.save(cx))))
    .child(toggle("enabled", self.enabled, palette)
        .accessibility_label("Enable notifications")
        .on_change(cx.listener(|view, checked: &bool, _, cx| {
            view.enabled = *checked;
            cx.notify();
        })))
    .child(text_field("name", &self.input, palette, cx))
```

The application owns state. A toggle reports its next value; render that value
back on the next frame. Configure placeholders and observe edits through the
existing `InputState`. Put a form inside a GPUI tab group for focus traversal.
Use stable, distinct element IDs for controls and traffic-light groups.

For a custom accent, override `palette.accent` and `palette.accent_text` before
calling `set_theme`. Render controls with that same palette. Fonts are detected
at startup: SF Pro Text/Display and SF Mono are preferred, followed by installed
system alternatives. Font files are not bundled.

## Gallery and validation

```sh
cargo run -p swiftui_gpui --example gallery
cargo test -p swiftui_gpui
```

The gallery is independent of the shell and settings applications. It demonstrates
live light/dark switching, enabled/disabled buttons, a controlled toggle, and a
text field. Tests exercise pointer and keyboard activation, controlled state,
disabled input suppression, theme projection, and font fallback.
