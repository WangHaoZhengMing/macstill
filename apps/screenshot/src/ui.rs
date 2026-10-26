use crate::{
    capture::{self, Desktop},
    control::Server,
    model::{Drag, Mode, Preferences, Rect, Request},
};
use gpui_kit::{prelude::*, *};
use std::{path::PathBuf, sync::Arc, time::Duration};
use swiftui_gpui::Fonts;

#[cfg(test)]
mod tests;
struct Session {
    desktop: Option<Arc<Desktop>>,
    request: Request,
    prefs: Preferences,
    mode: Mode,
    area: Option<Rect>,
    drag: Option<Drag>,
    hovered: Option<usize>,
    screen: usize,
    options: bool,
    error: Option<String>,
    busy: bool,
    windows: Vec<AnyWindowHandle>,
}
impl Session {
    fn save_options(&mut self) {
        if let Err(e) = self.prefs.save() {
            self.error = Some(format!("Cannot save options: {e}"));
        }
    }
}
struct Runtime {
    _poll: Task<()>,
    _server: Option<Server>,
}
impl Global for Runtime {}
struct Overlay {
    session: Entity<Session>,
    index: usize,
    background: Arc<Image>,
    focus: FocusHandle,
    _observe: Subscription,
}
fn image(pixels: &image::RgbaImage) -> Arc<Image> {
    Arc::new(Image::from_bytes(
        ImageFormat::Png,
        capture::png(pixels).unwrap_or_default(),
    ))
}
fn close_surfaces(s: &mut Session, cx: &mut App) {
    for handle in s.windows.drain(..) {
        let _ = handle.update(cx, |_, w, _| w.remove_window());
    }
}
pub fn run(request: Request, server: Option<Server>) {
    gpui_kit::application().run(move |cx| {
        macstill_ui::init(cx);
        let prefs = Preferences::load();
        let request = request.with_preferences(&prefs);
        let mode = request.mode;
        let session = cx.new(|_| Session {
            desktop: None,
            request,
            prefs,
            mode,
            area: None,
            drag: None,
            hovered: None,
            screen: 0,
            options: false,
            error: None,
            busy: false,
            windows: vec![],
        });
        let closing = session.clone();
        cx.on_window_closed(move |cx, _| {
            if cx.windows().is_empty() && !closing.read(cx).busy {
                cx.quit();
            }
        })
        .detach();
        start(session.clone(), cx);
        let poll = cx.spawn(async move |cx| {
            loop {
                if let Some(request) = server.as_ref().and_then(|s| s.receive()) {
                    let restart = session.update(cx, |s, cx| {
                        if s.busy {
                            return false;
                        }
                        s.prefs = Preferences::load();
                        s.request = request.with_preferences(&s.prefs);
                        s.mode = s.request.mode;
                        s.options = false;
                        let completed = s.windows.is_empty();
                        if completed {
                            s.busy = true;
                            for w in cx.windows() {
                                let _ = w.update(cx, |_, w, _| w.remove_window());
                            }
                        }
                        cx.notify();
                        completed
                    });
                    if restart {
                        cx.background_executor()
                            .timer(Duration::from_millis(180))
                            .await;
                        cx.update(|cx| start(session.clone(), cx));
                    }
                }
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
            }
        });
        cx.set_global(Runtime {
            _poll: poll,
            _server: None,
        });
    });
}
fn start(session: Entity<Session>, cx: &mut App) {
    session.update(cx, |s, _| s.busy = true);
    cx.spawn(async move |cx| {
        let desktop = cx
            .background_executor()
            .spawn(async { Desktop::load(false) })
            .await;
        cx.update(|cx| match desktop {
            Ok(desktop) => {
                session.update(cx, |s, _| {
                    s.screen = desktop.screens.iter().position(|s| s.focused).unwrap_or(0);
                    let r = desktop.screens[s.screen].rect;
                    s.area = s
                        .prefs
                        .region
                        .filter(|_| s.prefs.remember)
                        .filter(|a| {
                            desktop
                                .screens
                                .iter()
                                .any(|s| s.rect.intersection(*a).is_some())
                        })
                        .or(Some(Rect {
                            x: r.x + r.w * 0.2,
                            y: r.y + r.h * 0.2,
                            w: r.w * 0.6,
                            h: r.h * 0.5,
                        }));
                    if s.request.immediate && s.mode == Mode::Region {
                        s.area = None;
                    }
                    s.desktop = Some(Arc::new(desktop));
                    s.busy = false;
                });
                if session.read(cx).request.immediate && session.read(cx).mode == Mode::Screen {
                    commit(session.clone(), true, cx);
                } else if let Err(e) = open_overlays(session.clone(), cx) {
                    show_error(format!("{e:#}"), cx);
                }
            }
            Err(e) => {
                session.update(cx, |s, _| s.busy = false);
                show_error(format!("{e:#}"), cx)
            }
        });
    })
    .detach();
}
fn open_overlays(session: Entity<Session>, cx: &mut App) -> anyhow::Result<()> {
    let desktop = session.read(cx).desktop.clone().unwrap();
    for (index, screen) in desktop.screens.iter().enumerate() {
        let uuid = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, screen.name.as_bytes());
        let display = cx
            .displays()
            .into_iter()
            .find(|d| d.uuid().ok() == Some(uuid))
            .ok_or_else(|| anyhow::anyhow!("Display {} disappeared", screen.name))?;
        let background = image(&screen.pixels);
        use layer_shell::*;
        let handle = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(screen.rect.x as f32), px(screen.rect.y as f32)),
                    size(px(screen.rect.w as f32), px(screen.rect.h as f32)),
                ))),
                display_id: Some(display.id()),
                titlebar: None,
                app_id: Some("org.macstill.Screenshot".into()),
                kind: WindowKind::LayerShell(LayerShellOptions {
                    namespace: "macstill-screenshot".into(),
                    layer: Layer::Overlay,
                    anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                    exclusive_zone: Some(px(-1.)),
                    keyboard_interactivity: KeyboardInteractivity::Exclusive,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |w, cx| {
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    w.focus(&focus, cx);
                    let observe = cx.observe(&session, |_, _, cx| cx.notify());
                    Overlay {
                        session: session.clone(),
                        index,
                        background,
                        focus,
                        _observe: observe,
                    }
                })
            },
        )?;
        session.update(cx, |s, _| s.windows.push(handle.into()));
    }
    Ok(())
}
#[derive(Clone)]
enum Source {
    Area(Rect),
    Window(String),
    All,
}
fn commit(session: Entity<Session>, all: bool, cx: &mut App) {
    let job = session.update(cx, |s, cx| {
        if s.busy {
            return None;
        }
        let desktop = s.desktop.clone()?;
        let source = if all {
            Source::All
        } else {
            match s.mode {
                Mode::Screen => Source::Area(desktop.screens[s.screen].rect),
                Mode::Region => match s.area {
                    Some(r) if r.w >= 1. && r.h >= 1. => Source::Area(r),
                    _ => return None,
                },
                Mode::Window => match s.hovered.and_then(|i| desktop.windows.get(i)) {
                    Some(w) => Source::Window(w.address.clone()),
                    None => return None,
                },
            }
        };
        s.prefs.mode = s.mode;
        s.prefs.region = s.area;
        if let Err(e) = s.prefs.save() {
            s.error = Some(format!("Cannot save options: {e}"));
            cx.notify();
            return None;
        }
        s.busy = true;
        let prefs = s.prefs.clone();
        let request = s.request.clone();
        close_surfaces(s, cx);
        Some((desktop, source, prefs, request))
    });
    let Some((desktop, source, prefs, request)) = job else {
        return;
    };
    let countdown = if prefs.delay > 0 {
        open_countdown(prefs.delay, cx)
    } else {
        None
    };
    cx.spawn(async move |cx| {
        // All selection layers are destroyed before any fresh capture (timer or pointer).
        cx.background_executor()
            .timer(Duration::from_millis(180))
            .await;
        for remaining in (0..prefs.delay).rev() {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            if let Some(handle) = countdown {
                let _ = handle.update(cx, |s, _, cx| {
                    s.0 = remaining;
                    cx.notify();
                });
            }
        }
        if let Some(handle) = countdown {
            let _ = handle.update(cx, |_, w, _| w.remove_window());
            cx.background_executor()
                .timer(Duration::from_millis(180))
                .await;
        }
        let thumbnail = prefs.thumbnail;
        let result = cx
            .background_executor()
            .spawn(async move || -> anyhow::Result<_> {
                let fresh;
                let desktop = if prefs.delay > 0 || prefs.cursor {
                    fresh = Desktop::load(prefs.cursor)?;
                    &fresh
                } else {
                    &desktop
                };
                let images = match source {
                    Source::Area(area) => vec![desktop.crop(area)?],
                    Source::All if request.clipboard => {
                        let x = desktop
                            .screens
                            .iter()
                            .map(|s| s.rect.x)
                            .fold(f64::INFINITY, f64::min);
                        let y = desktop
                            .screens
                            .iter()
                            .map(|s| s.rect.y)
                            .fold(f64::INFINITY, f64::min);
                        let right = desktop
                            .screens
                            .iter()
                            .map(|s| s.rect.x + s.rect.w)
                            .fold(f64::NEG_INFINITY, f64::max);
                        let bottom = desktop
                            .screens
                            .iter()
                            .map(|s| s.rect.y + s.rect.h)
                            .fold(f64::NEG_INFINITY, f64::max);
                        vec![desktop.crop(Rect {
                            x,
                            y,
                            w: right - x,
                            h: bottom - y,
                        })?]
                    }
                    Source::All => desktop
                        .screens
                        .iter()
                        .map(|s| s.pixels.as_ref().clone())
                        .collect(),
                    Source::Window(address) => {
                        vec![macstill_capture::Capture::connect()?.capture_with_cursor(
                            &address,
                            prefs.cursor,
                            Duration::from_secs(3),
                        )?]
                    }
                };
                let mut results = Vec::new();
                for pixels in images {
                    let bytes = capture::png(&pixels)?;
                    let path = if request.clipboard {
                        capture::clipboard(&bytes)?;
                        None
                    } else {
                        Some(capture::save(
                            &bytes,
                            request.output.as_deref().unwrap_or(&prefs.directory),
                        )?)
                    };
                    results.push((bytes, path));
                }
                Ok(results)
            }())
            .await;
        cx.update(|cx| match result {
            Ok(results) => {
                session.update(cx, |s, _| s.busy = false);
                if thumbnail {
                    for (bytes, path) in results {
                        open_preview(bytes, path, true, cx);
                    }
                } else {
                    cx.quit();
                }
            }
            Err(e) => {
                session.update(cx, |s, _| s.busy = false);
                show_error(format!("{e:#}"), cx)
            }
        });
    })
    .detach();
}
fn control(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(12.))
        .h(px(38.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .bg(if selected {
            rgba(0xffffff38)
        } else {
            rgba(0xffffff00)
        })
        .hover(|s| s.bg(rgba(0xffffff26)))
        .cursor_pointer()
        .child(label.into())
}
impl Overlay {
    fn global_point(&self, p: Point<Pixels>, cx: &App) -> (f64, f64) {
        let r = self.session.read(cx).desktop.as_ref().unwrap().screens[self.index].rect;
        (r.x + f32::from(p.x) as f64, r.y + f32::from(p.y) as f64)
    }
    fn toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let s = self.session.read(cx);
        let mode = s.mode;
        let session = self.session.clone();
        let mut bar = div()
            .flex()
            .items_center()
            .gap(px(3.))
            .p(px(5.))
            .rounded(px(11.))
            .bg(rgba(0x303033f2))
            .border_1()
            .border_color(rgba(0xffffff55))
            .shadow_lg()
            .child(
                control("cancel", "×", false)
                    .text_size(px(24.))
                    .on_click(|_, _, cx| cx.quit()),
            )
            .child(div().w(px(1.)).h(px(30.)).mx(px(4.)).bg(rgba(0xffffff30)));
        for (m, name, asset) in [
            (
                Mode::Screen,
                "Capture Entire Screen",
                include_bytes!("../assets/screen.svg").as_slice(),
            ),
            (
                Mode::Window,
                "Capture Selected Window",
                include_bytes!("../assets/window.svg").as_slice(),
            ),
            (
                Mode::Region,
                "Capture Selected Portion",
                include_bytes!("../assets/region.svg").as_slice(),
            ),
        ] {
            let session = session.clone();
            bar = bar.child(
                control(name, "", mode == m)
                    .w(px(52.))
                    .child(
                        img(Arc::new(Image::from_bytes(
                            ImageFormat::Svg,
                            asset.to_vec(),
                        )))
                        .w(px(30.))
                        .h(px(24.)),
                    )
                    .on_click(move |_, _, cx| {
                        session.update(cx, |s, cx| {
                            s.mode = m;
                            s.request.immediate = false;
                            s.options = false;
                            cx.notify();
                        })
                    }),
            );
        }
        let options = self.session.clone();
        let capture = self.session.clone();
        bar = bar
            .child(div().w(px(1.)).h(px(30.)).mx(px(4.)).bg(rgba(0xffffff30)))
            .child(
                control("options", "Options ⌄", s.options).on_click(move |_, _, cx| {
                    options.update(cx, |s, cx| {
                        s.options = !s.options;
                        cx.notify();
                    })
                }),
            )
            .child(
                control("capture", "Capture", false)
                    .bg(rgba(0xffffff20))
                    .on_click(move |_, _, cx| commit(capture.clone(), false, cx)),
            );
        bar.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .into_any_element()
    }
    fn options(&self, cx: &mut Context<Self>) -> AnyElement {
        let s = self.session.read(cx);
        let p = s.prefs.clone();
        let clipboard = s.request.clipboard;
        let mut menu = div()
            .w(px(252.))
            .p(px(6.))
            .rounded(px(8.))
            .bg(rgba(0x303033fa))
            .border_1()
            .border_color(rgba(0xffffff40))
            .shadow_lg()
            .flex()
            .flex_col();
        menu = menu.child(
            div()
                .px(px(10.))
                .py(px(4.))
                .text_color(rgb(0xaaaaaf))
                .child("Save to"),
        );
        for (label, kind) in [
            ("Desktop", 0),
            ("Documents", 1),
            ("Clipboard", 2),
            ("Other Location…", 3),
        ] {
            let session = self.session.clone();
            menu = menu.child(
                control(
                    label,
                    format!(
                        "{}  {label}",
                        if (kind == 2 && clipboard)
                            || (!clipboard && kind == 0 && p.directory.ends_with("Desktop"))
                            || (!clipboard && kind == 1 && p.directory.ends_with("Documents"))
                        {
                            "✓"
                        } else {
                            " "
                        }
                    ),
                    false,
                )
                .h(px(27.))
                .justify_start()
                .on_click(move |_, _, cx| {
                    if kind == 3 {
                        let paths = cx.prompt_for_paths(PathPromptOptions {
                            files: false,
                            directories: true,
                            multiple: false,
                            prompt: Some("Save Screenshots To".into()),
                        });
                        let session = session.clone();
                        cx.spawn(async move |cx| {
                            if let Ok(Ok(Some(paths))) = paths.await {
                                if let Some(path) = paths.first() {
                                    session.update(cx, |s, cx| {
                                        s.prefs.directory = path.clone();
                                        s.request.clipboard = false;
                                        s.request.output = None;
                                        s.prefs.clipboard = false;
                                        s.save_options();
                                        cx.notify();
                                    });
                                }
                            }
                        })
                        .detach();
                    } else {
                        session.update(cx, |s, cx| {
                            s.request.clipboard = kind == 2;
                            s.request.output = None;
                            s.prefs.clipboard = kind == 2;
                            if kind < 2 {
                                s.prefs.directory = std::process::Command::new("xdg-user-dir")
                                    .arg(if kind == 0 { "DESKTOP" } else { "DOCUMENTS" })
                                    .output()
                                    .ok()
                                    .filter(|o| o.status.success())
                                    .and_then(|o| String::from_utf8(o.stdout).ok())
                                    .map(|v| PathBuf::from(v.trim()))
                                    .filter(|p| p.is_absolute())
                                    .unwrap_or_else(|| {
                                        PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                                            .join(label)
                                    });
                            }
                            s.save_options();
                            cx.notify();
                        });
                    }
                }),
            );
        }
        menu = menu
            .child(div().h(px(1.)).my(px(5.)).bg(rgba(0xffffff25)))
            .child(
                div()
                    .px(px(10.))
                    .py(px(4.))
                    .text_color(rgb(0xaaaaaf))
                    .child("Timer"),
            );
        for delay in [0, 5, 10] {
            let session = self.session.clone();
            menu = menu.child(
                control(
                    format!("delay-{delay}"),
                    format!(
                        "{}  {}",
                        if p.delay == delay { "✓" } else { " " },
                        if delay == 0 {
                            "None".into()
                        } else {
                            format!("{delay} Seconds")
                        }
                    ),
                    false,
                )
                .h(px(27.))
                .justify_start()
                .on_click(move |_, _, cx| {
                    session.update(cx, |s, cx| {
                        s.prefs.delay = delay;
                        s.save_options();
                        cx.notify();
                    })
                }),
            );
        }
        menu = menu.child(div().h(px(1.)).my(px(5.)).bg(rgba(0xffffff25)));
        for (label, value, kind) in [
            ("Show Floating Thumbnail", p.thumbnail, 0),
            ("Show Mouse Pointer", p.cursor, 1),
            ("Remember Last Selection", p.remember, 2),
        ] {
            let session = self.session.clone();
            menu = menu.child(
                control(
                    label,
                    format!("{}  {label}", if value { "✓" } else { " " }),
                    false,
                )
                .h(px(27.))
                .justify_start()
                .on_click(move |_, _, cx| {
                    session.update(cx, |s, cx| {
                        if kind == 0 {
                            s.prefs.thumbnail = !s.prefs.thumbnail
                        } else if kind == 1 {
                            s.prefs.cursor = !s.prefs.cursor
                        } else {
                            s.prefs.remember = !s.prefs.remember
                        }
                        s.save_options();
                        cx.notify();
                    })
                }),
            );
        }
        menu.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .into_any_element()
    }
}
impl Render for Overlay {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.session.read(cx);
        let desktop = s.desktop.as_ref().unwrap();
        let screen = desktop.screens[self.index].rect;
        let area = match s.mode {
            Mode::Region => s.area,
            Mode::Window => s.hovered.map(|i| desktop.windows[i].rect),
            Mode::Screen => Some(desktop.screens[s.screen].rect),
        };
        let is_toolbar = self.index == s.screen && !s.request.immediate;
        let options = s.options && is_toolbar;
        let error = s.error.clone();
        let mode = s.mode;
        let mut root = div()
            .id("screenshot-overlay")
            .track_focus(&self.focus)
            .cursor(CursorStyle::Crosshair)
            .size_full()
            .relative()
            .overflow_hidden()
            .font_family(cx.global::<Fonts>().text.clone())
            .text_size(px(13.))
            .text_color(rgb(0xffffff))
            .child(
                img(self.background.clone())
                    .absolute()
                    .size_full()
                    .object_fit(ObjectFit::Fill),
            );
        if let Some(local) = area.and_then(|r| r.intersection(screen)) {
            let x = (local.x - screen.x) as f32;
            let y = (local.y - screen.y) as f32;
            let w = local.w as f32;
            let h = local.h as f32;
            for (x, y, w, h) in [
                (0., 0., screen.w as f32, y),
                (0., y, x, h),
                (x + w, y, screen.w as f32 - x - w, h),
                (0., y + h, screen.w as f32, screen.h as f32 - y - h),
            ] {
                root = root.child(
                    div()
                        .absolute()
                        .left(px(x))
                        .top(px(y))
                        .w(px(w.max(0.)))
                        .h(px(h.max(0.)))
                        .bg(rgba(0x00000065)),
                );
            }
            root = root.child(
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(w))
                    .h(px(h))
                    .border_1()
                    .border_color(rgb(0xffffff))
                    .when(mode == Mode::Window, |d| d.bg(rgba(0x007aff45))),
            );
            if mode == Mode::Region {
                for (hx, hy) in [
                    (x, y),
                    (x + w / 2., y),
                    (x + w, y),
                    (x, y + h / 2.),
                    (x + w, y + h / 2.),
                    (x, y + h),
                    (x + w / 2., y + h),
                    (x + w, y + h),
                ] {
                    root = root.child(
                        div()
                            .absolute()
                            .left(px(hx - 3.))
                            .top(px(hy - 3.))
                            .size(px(6.))
                            .rounded(px(1.))
                            .bg(rgb(0xffffff))
                            .border_1()
                            .border_color(rgba(0x00000080)),
                    );
                }
                root = root.child(
                    div()
                        .absolute()
                        .left(px(x.max(10.)))
                        .top(px((y - 29.).max(10.)))
                        .px(px(8.))
                        .py(px(3.))
                        .rounded(px(5.))
                        .bg(rgba(0x202024d9))
                        .child(format!(
                            "{} × {}",
                            area.unwrap().w.round(),
                            area.unwrap().h.round()
                        )),
                );
            }
        } else {
            root = root.child(div().absolute().size_full().bg(rgba(0x00000065)));
        }
        if mode == Mode::Window {
            if let Some(i) = s.hovered {
                root = root.child(
                    div()
                        .absolute()
                        .top(px(24.))
                        .left(px(24.))
                        .px(px(10.))
                        .py(px(5.))
                        .rounded(px(6.))
                        .bg(rgba(0x202024dd))
                        .child(desktop.windows[i].title.clone()),
                );
            }
        }
        if is_toolbar {
            root = root.child(
                div()
                    .absolute()
                    .bottom(px(42.))
                    .w_full()
                    .flex()
                    .justify_center()
                    .child(self.toolbar(cx)),
            );
            if options {
                root = root.child(
                    div()
                        .absolute()
                        .bottom(px(106.))
                        .right(px((screen.w as f32 / 2. - 180.).max(12.)))
                        .child(self.options(cx)),
                );
            }
        }
        if let Some(error) = error {
            root = root.child(
                div()
                    .absolute()
                    .top(px(40.))
                    .left(px(40.))
                    .p(px(12.))
                    .bg(rgb(0x802020))
                    .child(error),
            );
        }
        root.on_key_down(cx.listener(
            |this, e: &KeyDownEvent, _, cx| match e.keystroke.key.as_str() {
                "escape" => cx.quit(),
                "space" => this.session.update(cx, |s, cx| {
                    s.mode = if s.mode == Mode::Window {
                        Mode::Region
                    } else {
                        Mode::Window
                    };
                    s.options = false;
                    cx.notify();
                }),
                "enter" => {
                    this.session.update(cx, |s, _| {
                        s.request.clipboard |= e.keystroke.modifiers.control
                    });
                    commit(this.session.clone(), false, cx);
                }
                "left" | "right" | "up" | "down" => this.session.update(cx, |s, cx| {
                    if let Some(r) = &mut s.area {
                        let step = if e.keystroke.modifiers.shift { 10. } else { 1. };
                        match e.keystroke.key.as_str() {
                            "left" => r.x -= step,
                            "right" => r.x += step,
                            "up" => r.y -= step,
                            _ => r.y += step,
                        };
                        cx.notify();
                    }
                }),
                _ => {}
            },
        ))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, e: &MouseDownEvent, _, cx| {
                let point = this.global_point(e.position, cx);
                let index = this.index;
                this.session.update(cx, |s, cx| {
                    s.screen = index;
                    s.options = false;
                    s.request.clipboard |= e.modifiers.control;
                    if s.mode == Mode::Screen {
                        s.screen = this.index;
                    }
                    if s.mode == Mode::Window {
                        s.hovered = s
                            .desktop
                            .as_ref()
                            .unwrap()
                            .windows
                            .iter()
                            .position(|w| w.rect.contains(point.0, point.1));
                    }
                    if s.mode == Mode::Region {
                        let drag = Drag::new(point, s.area);
                        s.area = Some(drag.area(point));
                        s.drag = Some(drag);
                    }
                    cx.notify();
                });
            }),
        )
        .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
            let point = this.global_point(e.position, cx);
            this.session.update(cx, |s, cx| {
                if let Some(drag) = s.drag {
                    s.area = Some(drag.area(point));
                }
                if s.mode == Mode::Screen {
                    s.screen = this.index;
                }
                if s.mode == Mode::Window {
                    s.hovered = s
                        .desktop
                        .as_ref()
                        .unwrap()
                        .windows
                        .iter()
                        .position(|w| w.rect.contains(point.0, point.1));
                }
                cx.notify();
            });
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, e: &MouseUpEvent, _, cx| {
                let should = this.session.update(cx, |s, cx| {
                    s.drag = None;
                    s.request.clipboard |= e.modifiers.control;
                    cx.notify();
                    s.request.immediate || s.mode != Mode::Region
                });
                if should {
                    commit(this.session.clone(), false, cx);
                }
            }),
        )
    }
}
struct Preview {
    bytes: Vec<u8>,
    image: Arc<Image>,
    path: Option<PathBuf>,
    error: Option<String>,
    thumbnail: bool,
    focus: FocusHandle,
}
fn open_preview(bytes: Vec<u8>, path: Option<PathBuf>, thumbnail: bool, cx: &mut App) {
    let image = Arc::new(Image::from_bytes(ImageFormat::Png, bytes.clone()));
    let mut options = WindowOptions {
        titlebar: None,
        app_id: Some("org.macstill.Screenshot".into()),
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(
                px(if thumbnail { 320. } else { 900. }),
                px(if thumbnail { 228. } else { 660. }),
            ),
            cx,
        ))),
        ..Default::default()
    };
    if thumbnail {
        use layer_shell::*;
        options.kind = WindowKind::LayerShell(LayerShellOptions {
            namespace: "macstill-screenshot-preview".into(),
            layer: Layer::Overlay,
            anchor: Anchor::BOTTOM | Anchor::RIGHT,
            exclusive_zone: Some(px(-1.)),
            margin: Some((px(0.), px(24.), px(24.), px(0.))),
            keyboard_interactivity: KeyboardInteractivity::OnDemand,
            ..Default::default()
        });
    }
    match cx.open_window(options, |w, cx| {
        w.set_window_title("Screenshot");
        cx.new(|cx| {
            let focus = cx.focus_handle();
            w.focus(&focus, cx);
            Preview {
                bytes,
                image,
                path,
                error: None,
                thumbnail,
                focus,
            }
        })
    }) {
        Ok(handle) => {
            if thumbnail {
                cx.spawn(async move |cx| {
                    cx.background_executor().timer(Duration::from_secs(8)).await;
                    let _ = handle.update(cx, |_, w, cx| {
                        w.remove_window();
                        if cx.windows().len() <= 1 {
                            cx.quit();
                        }
                    });
                })
                .detach();
            }
        }
        Err(e) => {
            eprintln!("Cannot show screenshot preview: {e:#}");
            cx.quit();
        }
    }
}
impl Render for Preview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let thumbnail = self.thumbnail;
        div()
            .id("screenshot-preview")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .p(px(8.))
            .gap(px(6.))
            .rounded(px(10.))
            .bg(rgb(0x303033))
            .text_color(rgb(0xffffff))
            .font_family(cx.global::<Fonts>().text.clone())
            .text_size(px(12.))
            .child(
                div()
                    .id("preview-image")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_hidden()
                    .cursor_pointer()
                    .child(
                        img(self.image.clone())
                            .size_full()
                            .object_fit(ObjectFit::Contain),
                    )
                    .on_click(cx.listener(move |s, _, w, cx| {
                        if thumbnail {
                            open_preview(s.bytes.clone(), s.path.clone(), false, cx);
                            w.remove_window();
                        }
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        control("copy", "Copy", false)
                            .h(px(28.))
                            .on_click(cx.listener(|s, _, _, cx| {
                                let bytes = s.bytes.clone();
                                cx.spawn(async move |this, cx| {
                                    let r = cx
                                        .background_executor()
                                        .spawn(async move { capture::clipboard(&bytes) })
                                        .await;
                                    let _ = this.update(cx, |s, cx| {
                                        s.error = Some(match r {
                                            Ok(()) => "Copied to Clipboard".into(),
                                            Err(e) => e.to_string(),
                                        });
                                        cx.notify();
                                    });
                                })
                                .detach();
                            })),
                    )
                    .when(self.path.is_some(), |d| {
                        d.child(
                            control("reveal", "Show in Files", false)
                                .h(px(28.))
                                .on_click(cx.listener(|s, _, _, _| {
                                    if let Some(path) = &s.path {
                                        let _ = std::process::Command::new("xdg-open")
                                            .arg(path.parent().unwrap())
                                            .spawn();
                                    }
                                })),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        control("done", "Done", false)
                            .h(px(28.))
                            .on_click(|_, w, cx| {
                                w.remove_window();
                                if cx.windows().len() <= 1 {
                                    cx.quit();
                                }
                            }),
                    ),
            )
            .when(self.error.is_some(), |d| {
                d.child(self.error.clone().unwrap())
            })
            .on_key_down(|e, _, cx| {
                if e.keystroke.key == "escape" {
                    cx.quit()
                }
            })
    }
}
struct ErrorView(String);
fn show_error(message: String, cx: &mut App) {
    eprintln!("Screenshot: {message}");
    if cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(480.), px(220.)),
                    cx,
                ))),
                app_id: Some("org.macstill.Screenshot".into()),
                ..Default::default()
            },
            |w, cx| {
                w.set_window_title("Screenshot");
                cx.new(|_| ErrorView(message))
            },
        )
        .is_err()
    {
        cx.quit();
    }
}
impl Render for ErrorView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p(px(24.))
            .flex()
            .flex_col()
            .gap(px(18.))
            .bg(rgb(0x303033))
            .text_color(rgb(0xffffff))
            .child("Couldn’t Capture Screenshot")
            .child(self.0.clone())
            .child(control("close", "Close", false).on_click(|_, _, cx| cx.quit()))
    }
}

struct Countdown(u64, FocusHandle);
fn open_countdown(seconds: u64, cx: &mut App) -> Option<WindowHandle<Countdown>> {
    use layer_shell::*;
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(290.), px(64.)),
                cx,
            ))),
            titlebar: None,
            app_id: Some("org.macstill.Screenshot".into()),
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: "macstill-screenshot-timer".into(),
                layer: Layer::Overlay,
                anchor: Anchor::BOTTOM,
                exclusive_zone: Some(px(-1.)),
                margin: Some((px(0.), px(0.), px(42.), px(0.))),
                keyboard_interactivity: KeyboardInteractivity::Exclusive,
                ..Default::default()
            }),
            ..Default::default()
        },
        |w, cx| {
            cx.new(|cx| {
                let f = cx.focus_handle();
                w.focus(&f, cx);
                Countdown(seconds, f)
            })
        },
    )
    .ok()
}
impl Render for Countdown {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.1)
            .size_full()
            .flex()
            .items_center()
            .justify_between()
            .p(px(12.))
            .rounded(px(10.))
            .bg(rgb(0x303033))
            .text_color(rgb(0xffffff))
            .child(format!("Capture in {}s", self.0))
            .child(control("cancel-timer", "Cancel", false).on_click(|_, _, cx| cx.quit()))
            .on_key_down(|e, _, cx| {
                if e.keystroke.key == "escape" {
                    cx.quit()
                }
            })
    }
}
