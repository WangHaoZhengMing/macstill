//! Application initialization and window creation.
use crate::{
    browser::Browser,
    services::xdg as platform,
    ui::{assets, theme, window as window_chrome},
};
use gpui_kit::{
    App, Bounds, Focusable, SharedString, TitlebarOptions, WindowBackgroundAppearance,
    WindowBounds, WindowOptions, prelude::*, px, size,
};

pub fn run() {
    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);
            window_chrome::init(cx);
            crate::actions::bind(cx);
            load_local_fonts(cx);
            let font: SharedString = theme::ui_font(&cx.text_system().all_font_names()).into();
            let bounds = Bounds::centered(None, size(px(theme::WINDOW_W), px(theme::WINDOW_H)), cx);
            let (window, _) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Finder".into()),
                        appears_transparent: true,
                        traffic_light_position: None,
                    }),
                    // Regional alpha: compositor backdrop blur under the sidebar.
                    window_background: WindowBackgroundAppearance::Blurred,
                    app_id: Some("org.macstill.Finder".into()),
                    app_owns_titlebar_drag: true,
                    window_min_size: Some(size(px(theme::WINDOW_MIN_W), px(theme::WINDOW_MIN_H))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    sync_window_theme(window, cx);
                    let view = cx.new(|cx| {
                        // Follow desktop appearance changes.
                        cx.observe_window_appearance(window, |_, window, cx| {
                            sync_window_theme(window, cx);
                            window.refresh();
                        })
                        .detach();
                        // Refresh frame ink and traffic lights on focus changes.
                        cx.observe_window_activation(window, |_, _, cx| cx.notify())
                            .detach();
                        // Refresh previews when the window changes size.
                        cx.observe_window_bounds(window, |_, window, _| window.refresh())
                            .detach();
                        Browser::new(font, window, cx)
                    });
                    window.focus(&view.focus_handle(cx), cx);
                    view
                },
            )
            .expect("open window");
            // FINDER_ACTIONS="finder::Trash,wait:500,finder::UndoOp" dispatches
            // actions at startup, with pauses (testing without the keyboard).
            if let Ok(list) = std::env::var("FINDER_ACTIONS") {
                let steps: Vec<String> = list
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                cx.spawn(async move |cx| {
                    for step in steps {
                        if let Some(ms) = step.strip_prefix("wait:").and_then(|m| m.parse().ok()) {
                            cx.background_executor()
                                .timer(std::time::Duration::from_millis(ms))
                                .await;
                            continue;
                        }
                        let _ = cx.update(|cx| match cx.build_action(&step, None) {
                            Ok(action) => {
                                let _ = window
                                    .update(cx, |_, window, cx| window.dispatch_action(action, cx));
                            }
                            Err(e) => eprintln!("FINDER_ACTIONS: {step}: {e}"),
                        });
                    }
                })
                .detach();
            }
            cx.activate(true);
        });
}

/// Keep component colors and custom file views on the same appearance.
fn sync_window_theme(window: &mut gpui_kit::Window, cx: &mut App) {
    use gpui_kit::component::{ActiveTheme, Theme, ThemeMode};
    let dark = match std::env::var("FINDER_APPEARANCE").as_deref() {
        Ok("dark") => true,
        Ok("light") => false,
        _ => matches!(
            window.appearance(),
            gpui_kit::WindowAppearance::Dark | gpui_kit::WindowAppearance::VibrantDark
        ),
    };
    let mode = if dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    if cx.theme().mode != mode {
        Theme::change(mode, Some(window), cx);
    }
    theme::set_dark(dark);
}

/// Load optional fonts from the application XDG data directory.
fn load_local_fonts(cx: &mut App) {
    let dir = platform::data_dir().join("fonts");
    let fonts: Vec<std::borrow::Cow<'static, [u8]>> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("otf") || x.eq_ignore_ascii_case("ttf"))
        })
        .filter_map(|e| std::fs::read(e.path()).ok().map(Into::into))
        .collect();
    if !fonts.is_empty() {
        let _ = cx.text_system().add_fonts(fonts);
    }
}

#[cfg(test)]
#[path = "tests/rendering.rs"]
mod rendering_tests;
