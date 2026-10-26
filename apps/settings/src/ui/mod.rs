mod advanced_pages;
mod airdrop_page;
mod battery;
mod bluetooth_details;
mod control_center_page;
mod display_page;
mod editors;
mod general_pages;
mod material;
mod pages;
mod pointer_pages;
mod reference;
mod sidebar;
mod system_pages;
#[cfg(test)]
mod tests;
mod widgets;
mod wifi_page;
use crate::backend::{Action, Backend, Snapshot};
use crate::request::{Page, Request};
use gpui_kit::{base::input::InputState, prelude::*, *};
use macstill_apps::DesktopApp;
use macstill_config::{self as config, Preferences};
use std::{path::PathBuf, time::Duration};
use swiftui_gpui::{Fonts, Palette, caption, panel};
fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    p: Palette,
) -> swiftui_gpui::Button {
    swiftui_gpui::button(id, label, p)
        .px(px(10.))
        .py(px(3.))
        .rounded(px(5.))
}
// Neutral window materials, separate from the shell's glass popover palette.
fn settings_palette(dark: bool) -> Palette {
    let mut p = Palette::new(dark);
    if dark {
        p.background = material::SettingsMaterial::new(true).content;
        p.sidebar = 0x2b2b2d;
        p.surface = 0x303032;
        p.line = 0x414143;
        p.hover = 0x3a3a3d;
        p.muted = 0xa4a4aa;
    } else {
        p.background = material::SettingsMaterial::new(false).content;
        p.sidebar = 0xe9e9eb;
        p.surface = 0xffffff;
        p.line = 0xe0e0e3;
        p.hover = 0xe6e6e9;
        p.muted = 0x78787e;
    }
    p
}
#[derive(Clone)]
enum Picker {
    Bind(String),
    IconApp(String),
    IconShortcut(String),
}

struct Store {
    backend: Backend,
    snapshot: Snapshot,
    system: crate::system_backend::SystemBackend,
    system_snapshot: crate::system_backend::Snapshot,
    _refresh: Task<()>,
}
#[derive(Clone)]
struct SharedStore(Entity<Store>);
impl Global for SharedStore {}

pub struct SettingsView {
    show_login_apps: bool,
    request_sequence: u64,
    store: Entity<Store>,
    page: Page,
    display_selection: Option<String>,
    display_arrangement: Option<Entity<display_page::arrangement::Arrangement>>,
    pointer_tab: usize,
    battery_days: bool,
    sound_input: bool,
    picker: Option<Picker>,
    search: Entity<InputState>,
    navigation_search: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
    page_index: usize,
    scroll: ScrollHandle,
    page_scrolls: std::collections::HashMap<Page, Point<Pixels>>,
    navigation_scroll: ScrollHandle,
    reveal_navigation: u8,
    message: Option<String>,
    wallpaper_busy: bool,
    history: Vec<Page>,
    history_index: usize,
    form: Option<widgets::EditField>,
    form_input: Entity<InputState>,
    sliders: std::collections::HashMap<String, widgets::SliderBinding>,
    editor: Option<editors::Editor>,
    confirmation: Option<editors::Confirmation>,
    connection_details: Option<String>,
    wifi_details: Option<wifi_page::DetailsDraft>,
    bluetooth_details: Option<bluetooth_details::BluetoothTarget>,
    sheet_focus: FocusHandle,
    last_modal: u8,
    previous_focus: Option<FocusHandle>,
}
impl SettingsView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = cx.global::<SharedStore>().0.clone();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let navigation_search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let form_input = cx.new(|cx| InputState::new(window, cx));
        let navigation_keys = cx.subscribe_in(&navigation_search, window, |s, _, event, w, cx| {
            if matches!(event, gpui_kit::base::input::InputEvent::PressEnter { .. }) {
                let query = s.navigation_search.read(cx).value().trim().to_lowercase();
                if !query.is_empty()
                    && let Some(page) = Page::ALL
                        .into_iter()
                        .find(|page| sidebar::matches_search(*page, &query))
                {
                    s.set_page(page, w, cx);
                }
            }
        });
        let form_sub = cx.observe(&form_input, |_, _, cx| cx.notify());
        let activation_sub = cx.observe_window_activation(window, |_, _, cx| cx.notify());
        let navigation_sub = cx.observe(&navigation_search, |_, _, cx| cx.notify());
        let sub = cx.observe(&store, |_, _, cx| cx.notify());
        let search_sub = cx.observe(&search, |s, _, cx| {
            s.page_index = 0;
            cx.notify();
        });
        Self {
            request_sequence: 0,
            store,
            page: Page::General,
            display_selection: None,
            display_arrangement: None,
            pointer_tab: 0,
            battery_days: false,
            sound_input: false,
            picker: None,
            search,
            navigation_search,
            _subscriptions: vec![
                sub,
                search_sub,
                navigation_sub,
                navigation_keys,
                form_sub,
                activation_sub,
            ],
            page_index: 0,
            scroll: ScrollHandle::new(),
            page_scrolls: Default::default(),
            navigation_scroll: ScrollHandle::new(),
            reveal_navigation: 2,
            message: None,
            wallpaper_busy: false,
            history: vec![Page::General],
            history_index: 0,
            form: None,
            form_input,
            sliders: Default::default(),
            editor: None,
            confirmation: None,
            show_login_apps: false,
            connection_details: None,
            wifi_details: None,
            bluetooth_details: None,
            sheet_focus: cx.focus_handle(),
            last_modal: 0,
            previous_focus: None,
        }
    }
    fn snapshot(&self, cx: &App) -> Snapshot {
        self.store.read(cx).snapshot.clone()
    }
    fn send(&mut self, action: Action, cx: &mut Context<Self>) {
        if let Err(e) = self.store.read(cx).backend.send(action) {
            self.message = Some(e.to_string())
        }
        cx.notify();
    }
    fn edit(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut Preferences)) {
        let mut p = self.snapshot(cx).preferences;
        edit(&mut p);
        // Optimistic local state lets successive clicks compose while the worker persists it.
        self.store.update(cx, |s, cx| {
            s.snapshot.preferences = p.clone();
            cx.notify()
        });
        self.send(Action::Save(p), cx);
    }
    fn asset(name: &str) -> PathBuf {
        macstill_ui::asset(name)
    }
    fn app_icon(app: &DesktopApp, prefs: &Preferences) -> Option<PathBuf> {
        prefs
            .app_icons
            .get(&app.id)
            .cloned()
            .or_else(|| macstill_ui::application_icon_override(&app.id))
            .or_else(|| app.icon.clone())
    }
    fn set_page(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        if self.page != page {
            self.history.truncate(self.history_index + 1);
            self.history.push(page);
            self.history_index = self.history.len() - 1;
        }
        self.open_page(page, window, cx);
    }
    fn open_page(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        self.pointer_tab = 0;
        self.reveal_navigation = 2;
        self.page_scrolls.insert(self.page, self.scroll.offset());
        self.scroll.set_offset(
            self.page_scrolls
                .get(&page)
                .copied()
                .unwrap_or(point(px(0.), px(0.))),
        );
        self.page = page;
        self.store.read(cx).system.set_page(page);
        self.sliders.clear();
        self.form = None;
        self.editor = None;
        self.confirmation = None;
        self.connection_details = None;
        self.wifi_details = None;
        self.store.read(cx).system.clear_connection();
        self.bluetooth_details = None;
        self.picker = None;
        self.page_index = 0;
        self.search.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }
    fn history_move(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let next = if forward {
            (self.history_index + 1).min(self.history.len() - 1)
        } else {
            self.history_index.saturating_sub(1)
        };
        self.history_index = next;
        self.open_page(self.history[next], window, cx);
    }
    fn search_box(&self, p: Palette, cx: &App) -> Stateful<Div> {
        text_field("settings-search", &self.search, p, cx)
    }
    fn navigation_icon(page: Page, _color: u32) -> AnyElement {
        sidebar::icon(page, 22.)
    }
    fn title(cx: &App, title: &str, subtitle: &str, p: Palette) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .mb(px(22.))
            .child(
                div()
                    .font_family(cx.global::<Fonts>().display.clone())
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title.to_string()),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(rgb(p.muted))
                    .child(subtitle.to_string()),
            )
    }
    fn row(title: impl Into<SharedString>, subtitle: impl Into<SharedString>, p: Palette) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(3.))
            .flex_1()
            .min_w_0()
            .child(div().text_size(px(13.)).child(title.into()))
            .child(caption(subtitle, p))
    }
    fn applications(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let snap = self.snapshot(cx);
        let p = settings_palette(snap.preferences.dark);
        let query = self.search.read(cx).value().to_lowercase();
        let apps: Vec<_> = snap
            .apps
            .iter()
            .filter(|a| {
                query.is_empty()
                    || a.name.to_lowercase().contains(&query)
                    || a.id.to_lowercase().contains(&query)
            })
            .collect();
        let per_page = 10;
        let pages = apps.len().div_ceil(per_page).max(1);
        self.page_index = self.page_index.min(pages - 1);
        let mut body = div().flex().flex_col().gap(px(8.));
        let mut list = panel(p);
        for app in apps.iter().skip(self.page_index * per_page).take(per_page) {
            let id = app.id.clone();
            let reset = id.clone();
            let launch = id.clone();
            let pin = id.clone();
            let pinned = snap.preferences.dock_pinned_apps.contains(&id);
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(14.))
                    .py(px(9.))
                    .border_b_1()
                    .border_color(rgb(p.line))
                    .child(macstill_ui::icon(
                        Self::app_icon(app, &snap.preferences),
                        30.,
                    ))
                    .child(Self::row(app.name.clone(), app.id.clone(), p))
                    .child(button(eid("open", &id), "Open", p).on_click(
                        cx.listener(move |s, _, _, cx| s.send(Action::Launch(launch.clone()), cx)),
                    ))
                    .child(
                        button(eid("app-pin", &id), if pinned { "Unpin" } else { "Pin" }, p)
                            .on_click(cx.listener(move |s, _, _, cx| {
                                s.edit(cx, |p| {
                                    if pinned {
                                        p.dock_pinned_apps.retain(|id| id != &pin);
                                    } else if !p.dock_pinned_apps.contains(&pin) {
                                        p.dock_pinned_apps.push(pin.clone());
                                    }
                                })
                            })),
                    )
                    .child(
                        button(eid("app-icon", &id), "Icon…", p).on_click(cx.listener(
                            move |s, _, _, cx| {
                                s.picker = Some(Picker::IconApp(id.clone()));
                                cx.notify()
                            },
                        )),
                    )
                    .when(snap.preferences.app_icons.contains_key(&app.id), |d| {
                        d.child(button(eid("app-reset", &app.id), "Reset", p).on_click(
                            cx.listener(move |s, _, _, cx| {
                                s.edit(cx, |p| {
                                    p.app_icons.remove(&reset);
                                })
                            }),
                        ))
                    }),
            );
        }
        body = body.child(list);
        let mut content = div().flex().flex_col().gap(px(16.));
        content = content.child(self.search_box(p, cx)).child(body);
        if apps.is_empty() {
            content = content.child(
                caption(
                    if query.is_empty() {
                        "Scanning installed applications…"
                    } else {
                        "No applications match your search."
                    },
                    p,
                )
                .py(px(24.)),
            );
        }
        content
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(caption(
                        format!(
                            "{} applications · Page {} of {}",
                            apps.len(),
                            self.page_index + 1,
                            pages
                        ),
                        p,
                    ))
                    .child(div().flex_1())
                    .child(button("prev", "‹", p).on_click(cx.listener(|s, _, _, cx| {
                        s.page_index = s.page_index.saturating_sub(1);
                        cx.notify()
                    })))
                    .child(
                        button("next", "›", p).on_click(cx.listener(move |s, _, _, cx| {
                            s.page_index = (s.page_index + 1).min(pages - 1);
                            cx.notify()
                        })),
                    )
                    .child(
                        button("refresh", "Refresh", p)
                            .on_click(cx.listener(|s, _, _, cx| s.send(Action::Refresh, cx))),
                    ),
            )
            .into_any_element()
    }
    fn picker_page(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let snap = self.snapshot(cx);
        let p = settings_palette(snap.preferences.dark);
        let picker = self.picker.clone().unwrap();
        let mut content =
            div()
                .flex()
                .flex_col()
                .gap(px(14.))
                .child(
                    button("back", "‹ Back", p).on_click(cx.listener(|s, _, w, cx| {
                        s.picker = None;
                        s.search.update(cx, |v, cx| v.set_value("", w, cx));
                        cx.notify()
                    })),
                );
        if let Picker::Bind(shortcut) = picker {
            let query = self.search.read(cx).value().to_lowercase();
            content = content
                .child(Self::title(
                    cx,
                    "Choose an application",
                    "This shortcut will open or focus your chosen application.",
                    p,
                ))
                .child(self.search_box(p, cx));
            let mut list = panel(p).child(
                div()
                    .id("unbind")
                    .px(px(15.))
                    .py(px(12.))
                    .cursor_pointer()
                    .child("No application")
                    .on_click(cx.listener({
                        let shortcut = shortcut.clone();
                        move |s, _, w, cx| {
                            s.edit(cx, |p| {
                                if let Some(v) = p.shortcuts.iter_mut().find(|v| v.id == shortcut) {
                                    v.app_id = None
                                }
                            });
                            s.picker = None;
                            s.search.update(cx, |v, cx| v.set_value("", w, cx));
                        }
                    })),
            );
            let filtered: Vec<_> = snap
                .apps
                .iter()
                .filter(|a| {
                    a.name.to_lowercase().contains(&query) || a.id.to_lowercase().contains(&query)
                })
                .collect();
            let pages = filtered.len().div_ceil(10).max(1);
            self.page_index = self.page_index.min(pages - 1);
            for app in filtered.iter().skip(self.page_index * 10).take(10) {
                let app_id = app.id.clone();
                let shortcut = shortcut.clone();
                list = list.child(
                    div()
                        .id(eid("pick", &app.id))
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .px(px(15.))
                        .py(px(10.))
                        .border_t_1()
                        .border_color(rgb(p.line))
                        .cursor_pointer()
                        .hover(move |d| d.bg(rgb(p.hover)))
                        .child(macstill_ui::icon(
                            Self::app_icon(app, &snap.preferences),
                            28.,
                        ))
                        .child(app.name.clone())
                        .on_click(cx.listener(move |s, _, w, cx| {
                            s.edit(cx, |p| {
                                if let Some(v) = p.shortcuts.iter_mut().find(|v| v.id == shortcut) {
                                    v.app_id = Some(app_id.clone())
                                }
                            });
                            s.picker = None;
                            s.search.update(cx, |v, cx| v.set_value("", w, cx));
                            cx.notify()
                        })),
                );
            }
            content
                .child(list)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(caption(
                            format!("Page {} of {}", self.page_index + 1, pages),
                            p,
                        ))
                        .child(
                            button("pick-prev", "‹", p).on_click(cx.listener(|s, _, _, cx| {
                                s.page_index = s.page_index.saturating_sub(1);
                                cx.notify()
                            })),
                        )
                        .child(button("pick-next", "›", p).on_click(cx.listener(
                            move |s, _, _, cx| {
                                s.page_index = (s.page_index + 1).min(pages - 1);
                                cx.notify()
                            },
                        ))),
                )
                .into_any_element()
        } else {
            content = content.child(Self::title(
                cx,
                "Choose your icon",
                "Artwork is copied into Macstill, so moving the original won't break it.",
                p,
            ));
            let choose = picker.clone();
            content = content.child(
                panel(p)
                    .p(px(24.))
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .child(caption(
                        "PNG, JPEG, WebP or self-contained SVG · Maximum 16 MiB",
                        p,
                    ))
                    .child(button("choose-file", "Choose image…", p).on_click(
                        cx.listener(move |s, _, _, cx| s.choose_icon(choose.clone(), cx)),
                    )),
            );
            let _ = window;
            content.into_any_element()
        }
    }
    fn choose_wallpaper(&mut self, cx: &mut Context<Self>) {
        if self.wallpaper_busy {
            return;
        }
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose Wallpaper".into()),
        });
        self.wallpaper_busy = true;
        self.message = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = match prompt.await {
                Ok(Ok(Some(paths))) if !paths.is_empty() => {
                    let path = paths[0].clone();
                    Some(
                        cx.background_executor()
                            .spawn(async move {
                                config::import_wallpaper(&path).map_err(|e| e.to_string())
                            })
                            .await,
                    )
                }
                Ok(Err(error)) => Some(Err(error.to_string())),
                _ => None,
            };
            let _ = this.update(cx, |s, cx| {
                s.wallpaper_busy = false;
                if let Some(result) = result {
                    match result {
                        Ok(path) => s.edit(cx, |p| p.wallpaper = Some(path)),
                        Err(error) => s.message = Some(error),
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn choose_icon(&mut self, picker: Picker, cx: &mut Context<Self>) {
        let files = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        // The future uses a weak entity; closing Settings does not retain its page.
        cx.spawn(async move |this, cx| match files.await {
            Ok(Ok(Some(paths))) => {
                if let Some(path) = paths.first() {
                    let path = path.clone();
                    let imported = cx
                        .background_executor()
                        .spawn(async move { config::import_icon(&path) })
                        .await;
                    let _ = this.update(cx, |s, cx| match imported {
                        Ok(path) => {
                            s.edit(cx, |p| match &picker {
                                Picker::IconApp(id) => {
                                    p.app_icons.insert(id.clone(), path);
                                }
                                Picker::IconShortcut(id) => {
                                    if let Some(v) = p.shortcuts.iter_mut().find(|v| &v.id == id) {
                                        v.custom_icon = Some(path)
                                    }
                                }
                                _ => {}
                            });
                            s.picker = None;
                            s.message = None;
                            cx.notify()
                        }
                        Err(e) => {
                            s.message = Some(format!("{e:#}"));
                            cx.notify()
                        }
                    });
                }
            }
            Ok(Err(e)) => {
                let _ = this.update(cx, |s, cx| {
                    s.message = Some(e.to_string());
                    cx.notify()
                });
            }
            _ => {}
        })
        .detach();
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        swiftui_gpui::set_theme(settings_palette(self.snapshot(cx).preferences.dark), cx);
        self.settings(window, cx)
    }
}
fn eid(prefix: &str, key: &str) -> SharedString {
    format!("{prefix}:{key}").into()
}

pub fn run(request: Request, server: Option<crate::control::Server>) {
    gpui_kit::application().run(move |cx| {
        macstill_ui::init(cx);
        let store = cx.new(|cx: &mut Context<Store>| {
            let initial_page = match &request {
                Request::Show(page) => *page,
                Request::Bind(_) => Page::Shortcuts,
                Request::Icon(_) => Page::Applications,
            };
            let system = crate::system_backend::SystemBackend::new(initial_page);
            let system_snapshot = system.snapshot();
            let backend = Backend::new(request, server);
            let snapshot = backend.snapshot();
            let refresh = cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(250))
                        .await;
                    if this
                        .update(cx, |s, cx| {
                            let next = s.backend.snapshot();
                            let system = s.system.snapshot();
                            if system.revision != s.system_snapshot.revision {
                                s.system_snapshot = system;
                                cx.notify();
                            }
                            if next.revision != s.snapshot.revision {
                                s.snapshot = next;
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            });
            Store {
                backend,
                snapshot,
                system,
                system_snapshot,
                _refresh: refresh,
            }
        });
        cx.set_global(SharedStore(store.clone()));
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(960.), px(720.)),
                        cx,
                    ))),
                    titlebar: None,
                    window_background: WindowBackgroundAppearance::Blurred,
                    window_decorations: Some(WindowDecorations::Client),
                    app_id: Some("org.macstill.Settings".into()),
                    window_min_size: Some(size(px(760.), px(540.))),
                    ..Default::default()
                },
                |w, cx| {
                    w.set_window_title("System Settings — Macstill");
                    cx.new(|cx| {
                        let mut view = SettingsView::new(w, cx);
                        let request = view.snapshot(cx).open_request;
                        view.request_sequence = request.0;
                        view.apply_request(&request.1);
                        view.history = vec![view.page];
                        view
                    })
                },
            )
            .expect("Cannot open System Settings");
        cx.observe(&store, move |store, cx| {
            let request = store.read(cx).snapshot.open_request.clone();
            let _ = handle.update(cx, |s, w, cx| {
                if request.0 != s.last_request() {
                    s.apply_request(&request.1);
                    s.store.read(cx).system.set_page(s.page);
                    s.history.truncate(s.history_index + 1);
                    if s.history.last() != Some(&s.page) {
                        s.history.push(s.page);
                    }
                    s.history_index = s.history.len() - 1;
                    s.sliders.clear();
                    s.form = None;
                    s.scroll.set_offset(point(px(0.), px(0.)));
                    s.request_sequence = request.0;
                    s.page_index = 0;
                    s.search.update(cx, |v, cx| v.set_value("", w, cx));
                    w.activate_window();
                    cx.notify();
                }
            });
        })
        .detach();
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}
impl SettingsView {
    fn last_request(&self) -> u64 {
        self.request_sequence
    }
    fn apply_request(&mut self, request: &Request) {
        self.pointer_tab = 0;
        self.reveal_navigation = 2;
        self.picker = None;
        self.editor = None;
        self.confirmation = None;
        self.connection_details = None;
        self.bluetooth_details = None;
        match request {
            Request::Show(page) => self.page = *page,
            Request::Bind(id) => {
                self.page = Page::Shortcuts;
                self.picker = Some(Picker::Bind(id.clone()));
            }
            Request::Icon(id) => {
                self.page = Page::Applications;
                self.picker = Some(Picker::IconApp(id.clone()));
            }
        }
    }
}

use swiftui_gpui::{text_field, toggle, traffic_lights};
