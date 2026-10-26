//! Perspective projection of captured window textures using atlas-backed GPU strips.
//! Texture uploads occur only on capture changes; animation never reads the desktop back.
use super::super::*;
use std::sync::Arc;

pub(super) const TILT: f64 = 0.72;
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Pose {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub angle: f64,
}
impl Pose {
    pub fn rect(rect: &crate::hyprland::stage::Rect, origin: [i32; 2]) -> Self {
        Self {
            x: (rect.at[0] - origin[0]) as f64,
            y: (rect.at[1] - origin[1]) as f64,
            width: rect.size[0] as f64,
            height: rect.size[1] as f64,
            angle: 0.,
        }
    }
}
// A left-anchored Y rotation with perspective. Far/right edges shrink toward the center.
pub(super) fn column(pose: Pose, u: f64) -> (f64, f64, f64) {
    let distance = pose.width.max(1.) * 3.5;
    let z = u * pose.width * pose.angle.sin();
    let scale = distance / (distance + z);
    (
        pose.x + u * pose.width * pose.angle.cos() * scale,
        pose.y + pose.height * (1. - scale) * 0.5,
        pose.height * scale,
    )
}
pub(super) fn paint(image: Arc<RenderImage>, pose: Pose, window: &mut Window) {
    if pose.width < 1. || pose.height < 1. {
        return;
    }
    let (right, _, _) = column(pose, 1.);
    let radius = (pose.height * 0.045).clamp(2., 12.);
    window.paint_drop_shadows(
        Bounds::new(
            point(px(pose.x as f32), px(pose.y as f32)),
            size(px((right - pose.x) as f32), px(pose.height as f32)),
        ),
        Corners::all(px(radius as f32)),
        &[BoxShadow {
            color: rgba(0x00000038).into(),
            offset: point(px(0.), px(4.)),
            blur_radius: px(12.),
            spread_radius: px(-2.),
            inset: false,
        }],
    );
    // Subpixel columns at thumbnail scale keep the tilted top/bottom edges smooth.
    let steps = (pose.width * 1.5).ceil().clamp(48., 400.) as usize;
    for index in 0..steps {
        let u = index as f64 / steps as f64;
        let next = (index + 1) as f64 / steps as f64;
        let (left, top, height) = column(pose, u);
        let (right, _, _) = column(pose, next);
        let width = right - left;
        if width <= 0. {
            continue;
        }
        let full_width = width / (next - u);
        let local_x = (u + next) * 0.5 * pose.width;
        let edge = local_x.min(pose.width - local_x);
        let inset = if edge < radius {
            radius - (radius * radius - (radius - edge).powi(2)).max(0.).sqrt()
        } else {
            0.
        } * height
            / pose.height;
        let clip = Bounds::new(
            point(px(left as f32), px((top + inset) as f32)),
            size(px(width as f32 + 0.15), px((height - 2. * inset) as f32)),
        );
        let image_bounds = Bounds::new(
            point(px((left - u * full_width) as f32), px(top as f32)),
            size(px(full_width as f32), px(height as f32)),
        );
        let _ = window.paint_image(
            clip,
            image_bounds,
            Corners::all(px(0.)),
            image.clone(),
            0,
            false,
        );
        // Directional tilt shade is independent from window content.
        if pose.angle.abs() > 0.02 {
            let shade = (u * pose.angle.abs() * 38.).min(38.) as u32;
            window.paint_quad(fill(clip, rgba(shade)));
        }
    }
}
pub(super) fn pile_pose(
    image: &RenderImage,
    bounds: Bounds<Pixels>,
    stack: usize,
    angle: f64,
) -> Pose {
    let ratio = image.size(0).height.0 as f64 / image.size(0).width.0.max(1) as f64;
    let width = (f64::from(bounds.size.width) - 20.)
        .min((f64::from(bounds.size.height) - 18.) / ratio)
        .max(1.);
    Pose {
        x: f64::from(bounds.origin.x) + stack as f64 * 7.,
        y: f64::from(bounds.origin.y) + (f64::from(bounds.size.height) - width * ratio) * 0.5
            - stack as f64 * 3.,
        width,
        height: width * ratio,
        angle,
    }
}
pub(super) fn preview(images: Vec<Arc<RenderImage>>, angle: f64) -> AnyElement {
    canvas(
        move |_, _, _| (),
        move |bounds, _, window, _| {
            for (index, image) in images.into_iter().take(3).enumerate().rev() {
                let pose = pile_pose(&image, bounds, index, angle);
                paint(image, pose, window);
            }
        },
    )
    .size_full()
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{Pose, TILT, column};
    #[test]
    fn perspective_has_stable_anchor_monotonic_columns_and_a_shrunken_far_edge() {
        let p = Pose {
            x: 10.,
            y: 20.,
            width: 100.,
            height: 80.,
            angle: TILT,
        };
        assert_eq!(column(p, 0.), (10., 20., 80.));
        let mut last = 10.;
        for i in 1..=100 {
            let (x, y, h) = column(p, i as f64 / 100.);
            assert!(x > last && y >= 20. && h <= 80. && h > 0.);
            last = x;
        }
        let (_, y, h) = column(Pose { angle: 0., ..p }, 1.);
        assert_eq!((y, h), (20., 80.));
        assert!((column(Pose { angle: 0., ..p }, 1.).0 - 110.).abs() < 1e-8);
    }
}
