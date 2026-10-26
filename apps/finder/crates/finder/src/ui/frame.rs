//! Procedural frame inspired by macos-window-frame/FOUNDATION.md (Sequoia 15).
//! No extracted Apple artwork is embedded. Radius and ink are application
//! parameters, not recovered Apple constants. Outside shadows remain owned
//! by the Linux compositor; painting them inside the surface would clip them.
use gpui_kit::{
    Bounds, Hsla, PathBuilder, Pixels, Window, canvas, linear_color_stop, linear_gradient, point,
    prelude::*, px, rgba,
};

#[derive(Clone, Copy)]
struct FrameGeometry {
    bounds: Bounds<Pixels>,
    radius: Pixels,
    stroke: Pixels,
}

impl FrameGeometry {
    fn new(bounds: Bounds<Pixels>, radius: Pixels, scale: f32) -> Self {
        let stroke = px(1. / scale.max(1.));
        let bounds = bounds.inset(stroke / 2.);
        Self {
            bounds,
            radius: (radius - stroke / 2.)
                .max(px(0.))
                .min(bounds.size.width / 2.)
                .min(bounds.size.height / 2.),
            stroke,
        }
    }

    fn upper_contour(&self) -> PathBuilder {
        let b = self.bounds;
        let r = self.radius;
        let k = 0.552_284_8;
        let mut path = PathBuilder::stroke(self.stroke);
        path.move_to(point(b.left(), b.top() + r + px(8.)));
        path.line_to(point(b.left(), b.top() + r));
        path.cubic_bezier_to(
            point(b.left() + r, b.top()),
            point(b.left(), b.top() + r * (1. - k)),
            point(b.left() + r * (1. - k), b.top()),
        );
        path.line_to(point(b.right() - r, b.top()));
        path.cubic_bezier_to(
            point(b.right(), b.top() + r),
            point(b.right() - r * (1. - k), b.top()),
            point(b.right(), b.top() + r * (1. - k)),
        );
        path.line_to(point(b.right(), b.top() + r + px(8.)));
        path
    }
}

#[derive(Clone, Copy)]
struct FrameAppearance {
    edge: Hsla,
    highlight: Hsla,
}

impl FrameAppearance {
    fn new(dark: bool, active: bool) -> Self {
        Self {
            edge: rgba(if dark { 0x00000090 } else { 0x00000028 }).into(),
            highlight: rgba(match (dark, active) {
                (false, true) => 0xFFFFFF40,
                (false, false) => 0xFFFFFF24,
                (true, true) => 0xFFFFFF1A,
                (true, false) => 0xFFFFFF0D,
            })
            .into(),
        }
    }
}

/// Paint-only overlay: it does not insert a hitbox or intercept pointer input.
/// The contour uses the same radius as the window surface and corner panes.
pub fn overlay(window: &Window) -> impl IntoElement {
    let radius = super::window::radius(window);
    let appearance = FrameAppearance::new(super::theme::dark(), window.is_window_active());
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            // Maximized, tiled and fullscreen windows meet their screen edges.
            if radius == px(0.) {
                return;
            }
            let geometry = FrameGeometry::new(bounds, radius, window.scale_factor());
            window.paint_quad(gpui_kit::quad(
                geometry.bounds,
                geometry.radius,
                gpui_kit::transparent_black(),
                geometry.stroke,
                appearance.edge,
                gpui_kit::BorderStyle::Solid,
            ));
            // Place the light just inside the darker outer contour, rather
            // than blending both strokes into a single uniform outline.
            let highlight = FrameGeometry {
                bounds: geometry.bounds.inset(geometry.stroke),
                radius: (geometry.radius - geometry.stroke).max(px(0.)),
                ..geometry
            };
            if let Ok(path) = highlight.upper_contour().build() {
                window.paint_path(
                    path,
                    linear_gradient(
                        180.,
                        linear_color_stop(appearance.highlight, 0.),
                        linear_color_stop(appearance.highlight.alpha(0.), 1.),
                    ),
                );
            }
        },
    )
    .absolute()
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hairline_tracks_device_pixels_and_radius_is_bounded() {
        for scale in [1., 1.25, 1.5, 2.] {
            let geometry = FrameGeometry::new(
                Bounds::new(gpui_kit::Point::default(), gpui_kit::size(px(20.), px(10.))),
                px(12.),
                scale,
            );
            assert!((geometry.stroke / px(1.) * scale - 1.).abs() < 0.0001);
            assert!(geometry.radius <= px(5.));
            assert!(geometry.upper_contour().build().is_ok());
        }
    }
}
