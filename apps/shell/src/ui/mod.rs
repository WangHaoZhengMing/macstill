mod apple_menu;
mod app_menu;
mod window_menu;
mod bar;
#[path = "../control_center/mod.rs"]
mod control_center;
mod desktop;
mod displays;
mod dock;
mod launchpad;
mod mission;
mod notifications;
mod stage;
mod tray;
use crate::{
    backend::{Action, Backend, Snapshot, matches_app},
    catalog::DesktopApp,
    config::Preferences,
};
use gpui_kit::{
    base::input::{Input, InputEvent, InputState},
    prelude::*,
    *,
};
use std::{path::PathBuf, time::Duration};
use swiftui_gpui::{Fonts, Palette, button};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Preview,
    Bar,
    Dock,
    Launchpad,
    ControlCenter,
    NotificationCenter,
    NotificationBanner,
    Desktop,
    AppleMenu,
    Tray,
    Stage,
}
struct Store {
    backend: Backend,
    tray: crate::tray::Tray,
    menus: macstill_menu::host::Host,
    menu_snapshot: macstill_menu::host::Snapshot,
    notifications: crate::notifications::Notifications,
    notification_snapshot: crate::notifications::Snapshot,
    _notification_refresh: Task<()>,
    control_status: Entity<control_center::StatusCache>,
    snapshot: Snapshot,
    _refresh: Task<()>,
    clock_minute: String,
    _control: Task<()>,
}
#[derive(Clone)]
struct SharedStore(Entity<Store>);
impl Global for SharedStore {}
#[derive(Default)]
struct OpenWindows {
    geometry: std::collections::HashMap<DisplayId, Bounds<Pixels>>,
    surfaces: std::collections::HashMap<DisplayId, Vec<WindowHandle<DesktopView>>>,
    tray_x: f32,
    launchpad: Option<WindowHandle<DesktopView>>,
    control_center: Option<WindowHandle<DesktopView>>,
    notification_center: Option<WindowHandle<DesktopView>>,
    notification_banner: Option<WindowHandle<DesktopView>>,
    apple_menu: Option<WindowHandle<DesktopView>>,
    tray: Option<WindowHandle<DesktopView>>,
    stage: std::collections::HashMap<DisplayId, WindowHandle<DesktopView>>,
}
impl Global for OpenWindows {}

impl Store {
    fn new(cx: &mut Context<Self>, serve_control: bool) -> Self {
        let notifications = crate::notifications::Notifications::new(serve_control);
        let notification_snapshot = notifications.snapshot();
        let notification_refresh = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let Ok((changed, has_banners)) = this.update(cx, |s, cx| {
                    let next = s.notifications.snapshot();
                    let changed = next.revision != s.notification_snapshot.revision;
                    if changed {
                        s.notification_snapshot = next;
                        cx.notify();
                    }
                    (changed, !s.notification_snapshot.banners().is_empty())
                }) else {
                    break;
                };
                if serve_control && changed {
                    let output = if has_banners {
                        cx.background_executor()
                            .spawn(async { displays::focused_output_name() })
                            .await
                    } else {
                        None
                    };
                    let _ = this.update(cx, |_, cx| {
                        cx.defer(move |cx| notifications::sync_banner(output, cx))
                    });
                }
            }
        });
        let control_status = cx.new(control_center::StatusCache::new);
        let backend = Backend::new(serve_control);
        let requests = backend.open_requests.clone();
        let control = cx.spawn(async move |this, cx| {
            while let Ok(command) = requests.recv().await {
                if this
                    .update(cx, |_, cx| {
                        // Opening a prepared window renders synchronously and reads Store.
                        // Release this entity's update lease before dispatching UI commands.
                        cx.defer(move |cx| {
                            if command.starts_with("mission-") {
                                mission::command(&command, cx);
                            } else {
                                let _ = open(Mode::Launchpad, cx);
                            }
                        });
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let snapshot = backend.snapshot();
        let refresh = cx.spawn(async move |this, cx| {
            let mut geometry_tick = 0;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                geometry_tick += 1;
                let geometry = if serve_control && geometry_tick % 10 == 0 {
                    cx.background_executor()
                        .spawn(async { crate::hyprland::displays::monitors().ok() })
                        .await
                } else {
                    None
                };
                if this
                    .update(cx, |s, cx| {
                        if let Some(monitors) = geometry {
                            cx.defer(move |cx| displays::update_geometry(monitors, cx));
                        }
                        let next = s.backend.snapshot();
                        s.menus.select(next.clients.iter().find(|c| c.focus_history_id == Some(0))
                            .filter(|c| !matches!(c.class.as_str(), "org.macstill.Desktop" | "org.macstill.MissionControl"))
                            .map(|c| c.address.clone()));
                        let menu = s.menus.snapshot();
                        if menu != s.menu_snapshot { s.menu_snapshot = menu; cx.notify(); }
                        if serve_control {
                            cx.defer(stage::sync);
                        }
                        if next.revision != s.snapshot.revision
                            || chrono::Local::now().format("%Y%m%d%H%M").to_string()
                                != s.clock_minute
                        {
                            s.snapshot = next;
                            s.clock_minute = chrono::Local::now().format("%Y%m%d%H%M").to_string();
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            backend,
            notifications,
            notification_snapshot,
            _notification_refresh: notification_refresh,
            tray: crate::tray::Tray::new(),
            menus: if serve_control { macstill_menu::host::Host::new(app_menu::windows) }
                else { macstill_menu::host::Host::disabled() },
            menu_snapshot: Default::default(),
            control_status,
            snapshot,
            _refresh: refresh,
            clock_minute: String::new(),
            _control: control,
        }
    }
}

pub struct DesktopView {
    store: Entity<Store>,
    mode: Mode,
    target_display: Option<DisplayId>,
    tray_expanded: Vec<i32>,
    search: Entity<InputState>,
    search_value: SharedString,
    _subscriptions: Vec<Subscription>,
    page_index: usize,
    message: Option<String>,
    apple_menu: apple_menu::AppleMenuState,
    window_menu: Option<window_menu::State>,
    app_menu: Option<app_menu::State>,
    desktop: desktop::DesktopState,
    control_center: control_center::ControlCenter,
    notification_focus: FocusHandle,
    notification_had_focus: bool,
    notification_expanded: Vec<u32>,
    notification_groups: Vec<String>,
    dock_state: dock::DockState,
    launchpad_state: launchpad::LaunchpadState,
    stage_state: stage::State,
}
impl DesktopView {
    fn new(mode: Mode, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = cx.global::<SharedStore>().0.clone();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        if mode == Mode::Launchpad {
            search.update(cx, |s, cx| s.focus(window, cx));
        }
        let control_center = control_center::ControlCenter::new(window, cx);
        let enter_sub = cx.subscribe_in(&search, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) && this.mode == Mode::Launchpad {
                this.launch_selected(window, cx);
            }
        });
        let sub = cx.observe(&store, |_, _, cx| cx.notify());
        let search_sub = cx.observe(&search, |s, _, cx| {
            let value = s.search.read(cx).value();
            if value != s.search_value {
                s.search_value = value;
                s.page_index = 0;
                s.launchpad_state.reset_search();
            }
            cx.notify()
        });
        let mut notification_subscriptions = vec![sub, search_sub, enter_sub];
        if mode == Mode::AppleMenu {
            notification_subscriptions.push(cx.on_release(|s, cx| s.close_app_menu(cx)));
        }
        if mode == Mode::NotificationCenter {
            store
                .read(cx)
                .notifications
                .send(crate::notifications::Command::Center(true));
            let center_id = window.window_handle().window_id();
            let visible = std::rc::Rc::new(std::cell::Cell::new(true));
            let released = visible.clone();
            notification_subscriptions.push(cx.on_release(move |s, cx| {
                if released.replace(false) {
                    s.store
                        .read(cx)
                        .notifications
                        .send(crate::notifications::Command::Center(false));
                }
            }));
            let notification_store = store.clone();
            notification_subscriptions.push(cx.on_window_closed(move |cx, closed_id| {
                if closed_id == center_id && visible.replace(false) {
                    notification_store
                        .read(cx)
                        .notifications
                        .send(crate::notifications::Command::Center(false));
                }
            }));
            notification_subscriptions.push(cx.observe_window_activation(window, |s, w, _| {
                if w.is_window_active() {
                    s.notification_had_focus = true;
                } else if s.notification_had_focus {
                    w.remove_window();
                }
            }));
        }
        Self {
            store,
            mode,
            target_display: window.display(cx).map(|d| d.id()),
            tray_expanded: Vec::new(),
            search,
            search_value: SharedString::default(),
            _subscriptions: notification_subscriptions,
            page_index: 0,
            message: None,
            apple_menu: apple_menu::AppleMenuState::new(cx),
            window_menu: None,
            app_menu: None,
            desktop: desktop::DesktopState::new(window, cx),
            control_center,
            notification_focus: cx.focus_handle(),
            notification_had_focus: window.is_window_active(),
            notification_expanded: Vec::new(),
            notification_groups: Vec::new(),
            dock_state: dock::DockState::default(),
            launchpad_state: launchpad::LaunchpadState::default(),
            stage_state: stage::State::default(),
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
    fn launch_from_grid(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let target = if let Some(shortcut) = id.strip_prefix(crate::launchpad::SHORTCUT_PREFIX) {
            let snapshot = self.snapshot(cx);
            let target = snapshot
                .preferences
                .shortcuts
                .iter()
                .find(|s| s.id == shortcut && s.visible)
                .and_then(|s| s.effective_app_id())
                .filter(|id| snapshot.apps.iter().any(|a| a.id == *id))
                .map(str::to_owned);
            let Some(target) = target else {
                match open_settings(&["--bind", shortcut], cx) {
                    Ok(_) if self.mode == Mode::Launchpad => self.close_launchpad(window, cx),
                    Err(e) => self.message = Some(e.to_string()),
                    _ => {}
                }
                cx.notify();
                return;
            };
            target
        } else {
            id.clone()
        };

        if self.mode == Mode::Launchpad {
            if self.launchpad_state.pending_launch.is_some() || self.launchpad_state.is_closing() {
                return;
            }
            self.launchpad_state.pending_launch = Some(id.clone());
        }
        self.message = None;
        let before = self.snapshot(cx).launch_sequence;
        self.send(Action::Launch(target), cx);
        if self.message.is_some() {
            self.launchpad_state.pending_launch = None;
            return;
        }
        if self.mode != Mode::Launchpad {
            return;
        }
        let handle = window
            .window_handle()
            .downcast::<Self>()
            .expect("DesktopView root");
        cx.spawn(async move |this, cx| {
            for _ in 0..100 {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                let Ok(done) = this.update(cx, |s, cx| {
                    let snap = s.store.read(cx).backend.snapshot();
                    (snap.launch_sequence > before).then_some(snap.error.is_none())
                }) else {
                    break;
                };
                if let Some(success) = done {
                    if success {
                        let _ = handle.update(cx, |s, w, cx| s.close_launchpad(w, cx));
                    } else {
                        let _ = this.update(cx, |s, cx| {
                            s.launchpad_state.pending_launch = None;
                            cx.notify();
                        });
                    }
                    return;
                }
            }
            let _ = this.update(cx, |s, cx| {
                s.launchpad_state.pending_launch = None;
                s.message = Some("The application did not respond. Please try again.".into());
                cx.notify();
            });
        })
        .detach();
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
}
impl Render for DesktopView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        swiftui_gpui::set_theme(Palette::new(self.snapshot(cx).preferences.dark), cx);
        if matches!(self.mode, Mode::Stage)
            && let Some(bounds) = self.target_display.and_then(|id| displays::bounds(id, cx))
            && window.bounds().size != bounds.size
        {
            window.resize(bounds.size);
        }
        match self.mode {
            Mode::Bar => {
                if let Some(bounds) = displays::window_bounds(window, cx) {
                    let wanted = size(bounds.size.width, px(30.));
                    if window.bounds().size != wanted {
                        window.resize(wanted);
                    }
                }
                self.bar(cx)
            }
            Mode::ControlCenter => self.control_center_view(window, cx),
            Mode::NotificationCenter => self.notification_center_view(window, cx),
            Mode::NotificationBanner => self.notification_banners(window, cx),
            Mode::Desktop => self.desktop_surface(window, cx),
            Mode::AppleMenu => if self.app_menu.is_some() { self.app_menu_view(window, cx) } else if self.window_menu.is_some() { self.window_menu_view(window, cx) } else { self.apple_menu_view(window, cx) },
            Mode::Tray => self.tray_menu(window, cx),
            Mode::Stage => self.stage_view(window, cx),
            Mode::Dock => self.dock(window, cx),
            Mode::Launchpad => self.launchpad(window, cx),
            Mode::Preview => div()
                .size_full()
                .font_family(cx.global::<Fonts>().text.clone())
                .bg(rgb(0xb7c9e6))
                .flex()
                .flex_col()
                .child(self.bar(cx))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(10.))
                        .text_color(rgb(0xffffff))
                        .child(
                            div()
                                .font_family(cx.global::<Fonts>().display.clone())
                                .text_size(px(46.))
                                .font_weight(FontWeight::LIGHT)
                                .child("Make yourself at home."),
                        )
                        .child("Macstill · Desktop preview")
                        .child(
                            div().mt(px(20.)).child(
                                button(
                                    "preview-settings",
                                    "Open System Settings",
                                    Palette::new(false),
                                )
                                .on_click(|_, _, cx| {
                                    let _ = open_settings(&[], cx);
                                }),
                            ),
                        ),
                )
                .child(
                    div()
                        .h(px(360.))
                        .flex_shrink_0()
                        .child(self.dock(window, cx)),
                )
                .into_any_element(),
        }
    }
}
pub fn open(mode: Mode, cx: &mut App) -> anyhow::Result<WindowHandle<DesktopView>> {
    open_on(mode, displays::focused(cx), cx)
}
fn open_on(
    mode: Mode,
    display_id: Option<DisplayId>,
    cx: &mut App,
) -> anyhow::Result<WindowHandle<DesktopView>> {
    let existing = if cx.has_global::<OpenWindows>() {
        match mode {
            Mode::Launchpad => cx.global::<OpenWindows>().launchpad,
            Mode::ControlCenter => cx.global::<OpenWindows>().control_center,
            Mode::NotificationCenter => cx.global::<OpenWindows>().notification_center,
            Mode::NotificationBanner => cx.global::<OpenWindows>().notification_banner,
            Mode::AppleMenu => cx.global::<OpenWindows>().apple_menu,
            Mode::Tray => cx.global::<OpenWindows>().tray,
            Mode::Stage => {
                display_id.and_then(|id| cx.global::<OpenWindows>().stage.get(&id).copied())
            }
            _ => None,
        }
    } else {
        None
    };
    if let Some(handle) = existing {
        let same = handle
            .update(cx, |v, w, _| {
                if v.target_display == display_id {
                    if !matches!(mode, Mode::Stage) {
                        w.activate_window();
                    }
                    true
                } else {
                    w.remove_window();
                    false
                }
            })
            .unwrap_or(false);
        if same {
            return Ok(handle);
        }
    }
    let display = display_id
        .and_then(|id| cx.displays().into_iter().find(|d| d.id() == id))
        .or_else(|| cx.primary_display());
    let logical_bounds = display
        .as_ref()
        .map(|d| displays::bounds(d.id(), cx).unwrap_or(d.bounds()));
    let size = match mode {
        Mode::Preview => size(px(1180.), px(760.)),
        Mode::Bar => size(
            logical_bounds
                .as_ref()
                .map(|d| d.size.width)
                .unwrap_or(px(1200.)),
            px(30.),
        ),
        Mode::Dock => size(
            logical_bounds
                .as_ref()
                .map(|d| d.size.width)
                .unwrap_or(px(1000.))
                .min(px(1000.)),
            px(360.),
        ),
        Mode::Launchpad => logical_bounds
            .as_ref()
            .map(|d| d.size)
            .unwrap_or(size(px(1180.), px(760.))),
        Mode::NotificationBanner => notifications::banner_size(
            logical_bounds
                .as_ref()
                .map(|b| b.size)
                .unwrap_or(size(px(1180.), px(760.))),
        ),
        Mode::ControlCenter
        | Mode::NotificationCenter
        | Mode::Desktop
        | Mode::AppleMenu
        | Mode::Stage
        | Mode::Tray => logical_bounds
            .as_ref()
            .map(|d| d.size)
            .unwrap_or(size(px(1180.), px(760.))),
    };
    // Bounds::centered clamps to GPUI's integer-scaled display bounds. On a
    // fractional output this can shrink the layer, which Hyprland then centers.
    // Keep both Stage surfaces in the compositor's full logical coordinates.
    let bounds = if matches!(mode, Mode::Stage) {
        Bounds::new(logical_bounds.map(|b| b.origin).unwrap_or_default(), size)
    } else {
        Bounds::centered(display_id, size, cx)
    };
    let mut options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        display_id,
        titlebar: None,
        app_id: Some("org.macstill.Desktop".into()),
        ..Default::default()
    };
    if matches!(
        mode,
        Mode::Bar
            | Mode::Stage
            | Mode::Dock
            | Mode::ControlCenter
            | Mode::NotificationCenter
            | Mode::NotificationBanner
            | Mode::AppleMenu
            | Mode::Tray
    ) || (mode == Mode::Desktop && std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some())
        || (mode == Mode::Launchpad && std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some())
    {
        use layer_shell::*;
        options.kind = WindowKind::LayerShell(LayerShellOptions {
            namespace: match mode {
                Mode::Bar => "macstill-bar",
                Mode::Dock => "macstill-dock",
                Mode::ControlCenter => "macstill-control-center",
                Mode::NotificationCenter => "macstill-notification-center",
                Mode::NotificationBanner => "macstill-notification-banner",
                Mode::Desktop => "macstill-desktop",
                Mode::AppleMenu => "macstill-apple-menu",
                Mode::Tray => "macstill-tray",
                Mode::Stage => "macstill-stage-manager",
                _ => "macstill-launchpad",
            }
            .into(),
            layer: if mode == Mode::Desktop {
                Layer::Background
            } else if matches!(
                mode,
                Mode::Launchpad
                    | Mode::ControlCenter
                    | Mode::NotificationCenter
                    | Mode::NotificationBanner
                    | Mode::AppleMenu
                    | Mode::Tray
            ) {
                Layer::Overlay
            } else {
                Layer::Top
            },
            anchor: match mode {
                Mode::Bar => Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
                Mode::Dock => Anchor::BOTTOM,
                Mode::NotificationBanner => Anchor::TOP | Anchor::RIGHT,
                Mode::Stage => Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                _ => Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            },
            exclusive_zone: Some(px(
                if matches!(
                    mode,
                    Mode::ControlCenter
                        | Mode::NotificationCenter
                        | Mode::NotificationBanner
                        | Mode::Desktop
                        | Mode::AppleMenu
                        | Mode::Tray
                        | Mode::Stage
                ) {
                    -1.
                } else if mode == Mode::Bar {
                    30.
                } else if mode == Mode::Dock {
                    70.
                } else {
                    -1. // Launchpad covers the output, including bar/Dock reservations.
                },
            )),
            keyboard_interactivity: if matches!(
                mode,
                Mode::Launchpad
                    | Mode::ControlCenter
                    | Mode::NotificationCenter
                    | Mode::AppleMenu
                    | Mode::Tray
            ) {
                KeyboardInteractivity::Exclusive
            } else if mode == Mode::Desktop {
                // The pinned Wayland backend gates macstill-desktop's OnDemand
                // focus on clicks; idle backgrounds advertise None to avoid
                // Hyprland transferring keyboard focus on pointer motion.
                KeyboardInteractivity::OnDemand
            } else {
                KeyboardInteractivity::None
            },
            margin: match mode {
                Mode::NotificationBanner => Some((px(40.), px(16.), px(0.), px(0.))),
                _ => None,
            },
            ..Default::default()
        });
        options.window_background = WindowBackgroundAppearance::Transparent;
    }
    let handle = cx.open_window(options, |w, cx| {
        w.set_window_title(match mode {
            Mode::Launchpad => "Launchpad — Macstill",
            _ => "Macstill",
        });
        cx.new(|cx| {
            let mut view = DesktopView::new(mode, w, cx);
            view.target_display = display_id;
            if matches!(mode, Mode::AppleMenu | Mode::Tray) {
                view.apple_menu.focus.focus(w, cx);
            }
            if mode == Mode::Desktop {
                view.watch_desktop(cx);
            }
            if mode == Mode::NotificationCenter {
                view.notification_focus.focus(w, cx);
            }
            if mode == Mode::ControlCenter {
                view.control_center.focus.focus(w, cx);
            }
            view
        })
    })?;
    if !cx.has_global::<OpenWindows>() {
        cx.set_global(OpenWindows::default());
    }
    if mode == Mode::Launchpad {
        cx.global_mut::<OpenWindows>().launchpad = Some(handle);
    }
    if mode == Mode::NotificationCenter {
        cx.global_mut::<OpenWindows>().notification_center = Some(handle);
    }
    if mode == Mode::NotificationBanner {
        cx.global_mut::<OpenWindows>().notification_banner = Some(handle);
    }
    if mode == Mode::ControlCenter {
        cx.global_mut::<OpenWindows>().control_center = Some(handle);
    }
    if mode == Mode::Tray {
        cx.global_mut::<OpenWindows>().tray = Some(handle);
    }
    if mode == Mode::AppleMenu {
        cx.global_mut::<OpenWindows>().apple_menu = Some(handle);
    }
    if mode == Mode::Stage {
        if let Some(id) = display_id {
            cx.global_mut::<OpenWindows>().stage.insert(id, handle);
        }
    }
    Ok(handle)
}
pub fn run(mode: Mode) {
    crate::diagnostics::event("ui.run");
    gpui_kit::application().run(move |cx| {
        let _startup = crate::diagnostics::span("ui.initialize");
        macstill_ui::init(cx);
        let store = cx.new(|cx| Store::new(cx, mode == Mode::Bar));
        cx.set_global(SharedStore(store));
        cx.set_global(OpenWindows::default());
        cx.on_window_closed(move |cx, _| {
            if mode != Mode::Bar && cx.windows().is_empty() {
                cx.quit()
            }
        })
        .detach();
        if mode == Mode::Bar {
            displays::sync(cx);
            cx.observe_displays(|_, cx| displays::sync(cx)).detach();
        } else if let Err(e) = open(mode, cx) {
            eprintln!("Cannot open Macstill: {e:#}");
            cx.quit();
        }
        cx.activate(true);
        crate::diagnostics::event("ui.windows_requested");
    });
}

fn eid(prefix: &str, key: &str) -> SharedString {
    format!("{prefix}:{key}").into()
}

fn open_settings(args: &[&str], cx: &mut App) -> anyhow::Result<()> {
    macstill_apps::spawn_companion("macstill-settings", args)?;
    // Release Launchpad's exclusive keyboard grab before activating another app.
    if cx.has_global::<OpenWindows>()
        && let Some(handle) = cx.global::<OpenWindows>().launchpad
    {
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, w, _| w.remove_window());
        });
    }
    Ok(())
}

fn toggle_popover(mode: Mode, window: &mut Window, cx: &mut App) {
    let desired = match mode {
        Mode::AppleMenu => cx.global::<OpenWindows>().apple_menu,
        Mode::ControlCenter => cx.global::<OpenWindows>().control_center,
        Mode::NotificationCenter => cx.global::<OpenWindows>().notification_center,
        _ => return,
    };
    if let Some(handle) = desired {
        let target = window.display(cx).map(|d| d.id());
        if handle
            .update(cx, |v, w, _| {
                let same = v.target_display == target;
                w.remove_window();
                same
            })
            .unwrap_or(false)
        {
            return;
        }
    }
    let state = cx.global::<OpenWindows>();
    let others = [
        (Mode::AppleMenu, state.apple_menu),
        (Mode::ControlCenter, state.control_center),
        (Mode::NotificationCenter, state.notification_center),
        (Mode::Tray, state.tray),
    ];
    for (other_mode, handle) in others {
        if other_mode != mode
            && let Some(handle) = handle
        {
            let _ = handle.update(cx, |_, w, _| w.remove_window());
        }
    }
    if let Err(error) = open_on(mode, window.display(cx).map(|d| d.id()), cx) {
        eprintln!("Cannot open menu: {error:#}");
    }
}
