//! One persistent desktop/bar/Dock set per output; shared application state.
use super::*;
use std::collections::HashSet;
pub(super) fn sync(cx: &mut App) {
    if let Ok(monitors) = crate::hyprland::displays::monitors() {
        update_geometry(monitors, cx);
    }
    let displays = cx.displays();
    let ids: HashSet<_> = displays.iter().map(|d| d.id()).collect();
    // Create replacement surfaces first so a transient output change cannot quit the shell.
    for display in displays {
        let id = display.id();
        if cx.global::<OpenWindows>().surfaces.contains_key(&id) {
            continue;
        }
        let mut surfaces = Vec::new();
        for mode in [Mode::Desktop, Mode::Bar, Mode::Dock] {
            match open_on(mode, Some(id), cx) {
                Ok(handle) => surfaces.push(handle),
                Err(e) => eprintln!("Cannot open {mode:?} on {id:?}: {e:#}"),
            }
        }
        cx.global_mut::<OpenWindows>().surfaces.insert(id, surfaces);
    }
    let removed: Vec<_> = cx
        .global::<OpenWindows>()
        .surfaces
        .keys()
        .copied()
        .filter(|id| !ids.contains(id))
        .collect();
    for id in removed {
        if let Some(windows) = cx.global_mut::<OpenWindows>().surfaces.remove(&id) {
            for handle in windows {
                let _ = handle.update(cx, |_, w, _| w.remove_window());
            }
        }
    }
    let global = cx.global::<OpenWindows>();
    let popovers = [
        global.launchpad,
        global.control_center,
        global.notification_center,
        global.notification_banner,
        global.apple_menu,
        global.tray,
    ];
    for handle in popovers.into_iter().flatten() {
        let _ = handle.update(cx, |v, w, _| {
            if v.target_display.is_some_and(|id| !ids.contains(&id)) {
                w.remove_window();
            }
        });
    }
    stage::sync(cx);
}
pub(super) fn focused(cx: &App) -> Option<DisplayId> {
    // Hyprland knows the focused output even when keyboard focus belongs to another app.
    let focused_point = crate::hyprland::request("j/monitors")
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| {
            let m = v
                .as_array()?
                .iter()
                .find(|m| m["focused"].as_bool() == Some(true))?;
            let scale = m["scale"].as_f64().unwrap_or(1.).max(0.1);
            Some((
                m["x"].as_f64()? + m["width"].as_f64()? / scale / 2.,
                m["y"].as_f64()? + m["height"].as_f64()? / scale / 2.,
            ))
        });
    if let Some((x, y)) = focused_point {
        if let Some(d) = cx.displays().into_iter().find(|d| {
            bounds(d.id(), cx)
                .unwrap_or(d.bounds())
                .contains(&point(px(x as f32), px(y as f32)))
        }) {
            return Some(d.id());
        }
    }
    cx.primary_display().map(|d| d.id())
}

pub(super) fn bounds(id: DisplayId, cx: &App) -> Option<Bounds<Pixels>> {
    cx.has_global::<OpenWindows>()
        .then(|| cx.global::<OpenWindows>().geometry.get(&id).copied())
        .flatten()
}
pub(super) fn window_bounds(window: &Window, cx: &App) -> Option<Bounds<Pixels>> {
    let display = window.display(cx)?;
    bounds(display.id(), cx).or_else(|| Some(display.bounds()))
}
pub(super) fn update_geometry(monitors: Vec<crate::hyprland::displays::Monitor>, cx: &mut App) {
    if !cx.has_global::<OpenWindows>() {
        return;
    }
    let mut geometry = std::collections::HashMap::new();
    for d in cx.displays() {
        let Ok(id) = d.uuid() else { continue };
        if let Some(m) = monitors
            .iter()
            .find(|m| uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, m.name.as_bytes()) == id)
        {
            let (w, h) = m.logical_size();
            geometry.insert(
                d.id(),
                Bounds::new(
                    point(px(m.x as f32), px(m.y as f32)),
                    size(px(w as f32), px(h as f32)),
                ),
            );
        }
    }
    if cx.global::<OpenWindows>().geometry == geometry {
        return;
    }
    cx.global_mut::<OpenWindows>().geometry = geometry;
    let state = cx.global::<OpenWindows>();
    let mut windows: Vec<_> = state.surfaces.values().flatten().copied().collect();
    windows.extend(
        [
            state.launchpad,
            state.control_center,
            state.notification_center,
            state.notification_banner,
            state.apple_menu,
            state.tray,
        ]
        .into_iter()
        .flatten(),
    );
    windows.extend(state.stage.values().copied());
    for handle in windows {
        let _ = handle.update(cx, |_, _, cx| cx.notify());
    }
}

// This query is called by the notification background task, never by its render path.
pub(super) fn focused_output_name() -> Option<String> {
    let response = crate::hyprland::request("j/monitors").ok()?;
    let monitors: serde_json::Value = serde_json::from_str(&response).ok()?;
    monitors
        .as_array()?
        .iter()
        .find(|m| m["focused"].as_bool() == Some(true))?["name"]
        .as_str()
        .map(str::to_string)
}
pub(super) fn output_named(name: Option<&str>, cx: &App) -> Option<DisplayId> {
    name.and_then(|name| {
        let uuid = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, name.as_bytes());
        cx.displays()
            .into_iter()
            .find(|display| display.uuid().ok() == Some(uuid))
            .map(|d| d.id())
    })
    .or_else(|| cx.primary_display().map(|d| d.id()))
}
