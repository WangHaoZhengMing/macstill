//! Transparent window corners and compositor-owned dragging for Linux.
use gpui_kit::{
    App, Context, ElementId, MouseButton, Pixels, Point, RenderOnce, Window, div, prelude::*, px,
};
pub const RADIUS: f32 = 12.;
pub fn radius(window: &Window) -> Pixels {
    let tiled = matches!(window.window_decorations(), gpui_kit::Decorations::Client { tiling } if tiling.is_tiled());
    px(
        if window.is_maximized() || window.is_fullscreen() || tiled {
            0.
        } else {
            RADIUS
        },
    )
}

struct TransparentSurface;
impl gpui_kit::Render for TransparentSurface {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
impl gpui_kit::base::RootPlugin for TransparentSurface {
    fn style(&self, surface: &mut gpui_kit::Stateful<gpui_kit::Div>, _: &mut Window, _: &mut App) {
        surface.style().background = Some(gpui_kit::transparent_black().into());
    }
}
pub fn init(cx: &mut App) {
    gpui_kit::base::Root::register_plugin(cx, |_, _| TransparentSurface);
}

#[derive(gpui_kit::IntoElement)]
pub struct DragArea {
    id: ElementId,
    child: gpui_kit::AnyElement,
}
pub fn drag_area(id: impl Into<ElementId>, child: impl IntoElement) -> DragArea {
    DragArea {
        id: id.into(),
        child: child.into_any_element(),
    }
}
impl RenderOnce for DragArea {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let pressed =
            window.use_keyed_state(
                (self.id.clone(), "pressed"),
                cx,
                |_, _| None::<Point<Pixels>>,
            );
        let down = pressed.clone();
        let moved = pressed.clone();
        div()
            .id(self.id)
            .size_full()
            .flex()
            .items_center()
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                // Input fields and Kit buttons prevent default when they own the press.
                if window.default_prevented() {
                    return;
                }
                if event.click_count == 2 {
                    window.zoom_window();
                    return;
                }
                down.update(cx, |point, _| *point = Some(event.position));
            })
            .on_mouse_move(move |event, window, cx| {
                let start = *moved.read(cx);
                if event.pressed_button != Some(MouseButton::Left) {
                    moved.update(cx, |point, _| *point = None);
                    return;
                }
                if let Some(start) = start {
                    let delta = event.position - start;
                    if delta.x.abs() + delta.y.abs() >= px(3.) {
                        moved.update(cx, |point, _| *point = None);
                        window.start_window_move();
                    }
                }
            })
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                pressed.update(cx, |point, _| *point = None)
            })
            .child(self.child)
    }
}
