//! Quick Look: Space shows the selected item in a floating panel over the
//! window, as macOS does. Arrow keys keep moving the selection underneath,
//! and the panel follows it.

use super::*;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::deferred;

/// Shown as text: these extensions, or any small file without NUL bytes.
const TEXT: &[&str] = &[
    "txt",
    "md",
    "markdown",
    "rs",
    "toml",
    "json",
    "jsonc",
    "yaml",
    "yml",
    "xml",
    "html",
    "htm",
    "css",
    "scss",
    "js",
    "mjs",
    "cjs",
    "ts",
    "tsx",
    "jsx",
    "py",
    "rb",
    "go",
    "java",
    "kt",
    "c",
    "h",
    "cc",
    "cpp",
    "hpp",
    "cs",
    "swift",
    "sh",
    "ps1",
    "bat",
    "cmd",
    "ini",
    "cfg",
    "conf",
    "log",
    "csv",
    "tsv",
    "sql",
    "lua",
    "vue",
    "svelte",
    "gitignore",
    "env",
    "lock",
];
/// Decoded directly (full resolution) rather than through the shell.
const IMAGE: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "tif", "tiff",
];
const MAX_TEXT: usize = 256 * 1024;

pub(super) enum Peek {
    Loading(PathBuf),
    Text(PathBuf, SharedString),
    /// Not text after all.
    Binary(PathBuf),
}

fn ext(p: &Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// Read the start of a file as text, if it is text.
fn read_text(p: &Path) -> Option<String> {
    use std::io::Read;
    let mut b = Vec::new();
    std::fs::File::open(p)
        .ok()?
        .take(MAX_TEXT as u64)
        .read_to_end(&mut b)
        .ok()?;
    if b.iter().take(8192).any(|&c| c == 0) {
        return None;
    }
    let mut s = String::from_utf8_lossy(&b).into_owned();
    if b.len() == MAX_TEXT {
        s.push_str("\n\u{2026}");
    }
    Some(s)
}

impl Browser {
    pub(super) fn toggle_quick_look(
        &mut self,
        _: &QuickLook,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Space types a space in the search and rename fields.
        if !self.focus.is_focused(window) {
            cx.propagate();
            return;
        }
        self.preview.quick_look = !self.preview.quick_look && self.selected_entry().is_some();
        cx.notify();
    }

    pub(super) fn render_quick_look(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.preview.quick_look {
            return None;
        }
        let Some(e) = self.selected_entry() else {
            self.preview.quick_look = false;
            return None;
        };
        let vw = window.viewport_size().width / px(1.);
        let vh = window.viewport_size().height / px(1.);
        let (w, h) = (
            (vw * 0.62).clamp(420., 1100.),
            (vh * 0.78).clamp(320., 900.),
        );
        let x = &e.path;
        let body: AnyElement = if e.is_dir {
            self.ql_card(&e, cx)
        } else if IMAGE.contains(&ext(x).as_str()) {
            div()
                .size_full()
                .p(px(16.))
                .flex()
                .child(
                    img(x.clone())
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .object_fit(gpui_kit::ObjectFit::Contain),
                )
                .into_any_element()
        } else if icons::has_thumbnail(x) {
            let src = self.icon_source(&e, 1024, cx);
            div()
                .size_full()
                .p(px(16.))
                .flex()
                .child(
                    img(src)
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .object_fit(gpui_kit::ObjectFit::Contain),
                )
                .into_any_element()
        } else if TEXT.contains(&ext(x).as_str()) || e.size < 64 * 1024 {
            self.ql_text(&e, cx)
        } else {
            self.ql_card(&e, cx)
        };

        let app = (!e.is_dir).then(|| default_app(x)).flatten();
        let header = div()
            .h(px(44.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(12.))
            .border_b_1()
            .border_color(t::separator())
            .child(
                div()
                    .id("ql-close")
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .hover(|s| s.bg(t::glass_pressed()))
                    .child(sym("xmark@semibold", 11., t::text_secondary()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.preview.quick_look = false;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_center()
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .truncate()
                    .child(e.name.clone()),
            )
            .child(
                div()
                    .id("ql-open")
                    .flex_none()
                    .px(px(12.))
                    .py(px(4.))
                    .rounded_full()
                    .bg(t::glass_pressed())
                    .hover(|s| s.bg(t::sidebar_selected()))
                    .text_size(px(12.))
                    .child(match &app {
                        Some(a) => format!("Open with {a}"),
                        None if e.is_dir => "Open".to_string(),
                        None => "Open".to_string(),
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.preview.quick_look = false;
                        if let Some(i) = this.selection.cursor {
                            this.open_row(i, cx);
                        }
                    })),
            );
        let panel = div()
            .id("quick-look")
            .w(px(w))
            .h(px(h))
            .flex()
            .flex_col()
            .rounded(px(14.))
            .overflow_hidden()
            .bg(t::menu_bg())
            .border(px(0.5))
            .border_color(t::glass_border())
            .shadow(t::menu_shadow())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(header)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(div().absolute().inset_0().child(body)),
            );
        Some(
            deferred(
                div()
                    .id("ql-backdrop")
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.preview.quick_look = false;
                            cx.notify();
                        }),
                    )
                    .child(panel),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// The text of `e`, read off the UI thread the first time it's shown.
    fn ql_text(&mut self, e: &Entry, cx: &mut Context<Self>) -> AnyElement {
        let text = match &self.preview.peek {
            Some(Peek::Text(p, t)) if *p == e.path => Some(t.clone()),
            Some(Peek::Binary(p)) if *p == e.path => return self.ql_card(e, cx),
            Some(Peek::Loading(p)) if *p == e.path => None,
            _ => {
                let path = e.path.clone();
                self.preview.peek = Some(Peek::Loading(path.clone()));
                cx.spawn(async move |this, cx| {
                    let p2 = path.clone();
                    let text = cx.background_spawn(async move { read_text(&p2) }).await;
                    let _ = this.update(cx, |b, cx| {
                        b.preview.peek = Some(match text {
                            Some(t) => Peek::Text(path, t.into()),
                            None => Peek::Binary(path),
                        });
                        cx.notify();
                    });
                })
                .detach();
                None
            }
        };
        div()
            .id("ql-text")
            .size_full()
            .p(px(16.))
            .font_family("Cascadia Mono")
            .text_size(px(12.))
            .line_height(px(17.))
            .text_color(t::text())
            .child(text.unwrap_or_else(|| "\u{2026}".into()))
            .overflow_y_scrollbar()
            .scroll_bounce(true)
            .into_any_element()
    }

    /// A big icon with the item's details (folders, and files with no preview).
    fn ql_card(&mut self, e: &Entry, cx: &mut Context<Self>) -> AnyElement {
        let src = self.icon_source(e, 256, cx);
        let detail = if e.is_dir {
            let n = std::fs::read_dir(&e.path).map(|d| d.count()).unwrap_or(0);
            format!("Folder \u{00b7} {n} items")
        } else {
            format!("{} \u{00b7} {}", e.kind, files::format_size(e))
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(10.))
            .child(img(src).size(px(160.)))
            .child(
                div()
                    .text_size(px(t::TITLE_SIZE))
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .child(e.name.clone()),
            )
            .child(div().text_color(t::text_secondary()).child(detail))
            .child(
                div()
                    .text_size(px(t::SMALL_SIZE))
                    .text_color(t::text_tertiary())
                    .child(format!("Modified {}", files::format_date_long(e.modified))),
            )
            .into_any_element()
    }
}

fn default_app(_: &Path) -> Option<String> {
    None
}
