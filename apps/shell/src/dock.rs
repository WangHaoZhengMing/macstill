//! Geometry and motion are independent of the renderer, so pointer stability and
//! persistence can be checked without a compositor.
use std::f32::consts::PI;

pub const GAP: f32 = 4.;
pub const PADDING: f32 = 10.;
pub const SEPARATOR: f32 = 14.;

#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub x: f32,
    pub width: f32,
    pub icon: bool,
}
#[derive(Debug)]
pub struct Layout {
    pub left: f32,
    pub width: f32,
    pub base: f32,
    pub cells: Vec<Cell>,
}
/// Compute the influence from *resting* centers; growing items must never feed
/// their new center back into the magnification curve and oscillate at rest.
pub fn layout(slots: &[bool], requested: f32, viewport: f32, mouse: f32, progress: f32) -> Layout {
    let icons = slots.iter().filter(|&&v| v).count() as f32;
    let separators = slots.len() as f32 - icons;
    let overhead =
        PADDING * 2. + GAP * slots.len().saturating_sub(1) as f32 + SEPARATOR * separators;
    // Leave headroom for the largest possible magnification on narrow displays.
    let base = requested.min(((viewport - overhead - 150.).max(icons * 12.)) / icons.max(1.));
    let resting_width = icons * base + overhead;
    let mut resting_x = (viewport - resting_width) / 2. + PADDING;
    let mut widths = Vec::with_capacity(slots.len());
    for &icon in slots {
        let rest = if icon { base } else { SEPARATOR };
        let distance = (mouse - (resting_x + rest / 2.)).abs();
        let radius = base * (180. / 42.);
        let influence = if icon && distance < radius {
            ((distance / radius * PI).cos() + 1.) / 2.
        } else {
            0.
        };
        widths.push(rest + base * (30. / 42.) * influence * progress.clamp(0., 1.));
        resting_x += rest + GAP;
    }
    let mut width = widths.iter().sum::<f32>() + overhead - separators * SEPARATOR;
    let scale = ((viewport - 20.).max(1.) / width.max(1.)).min(1.);
    width *= scale;
    let left = (viewport - width) / 2.;
    let mut x = left + PADDING * scale;
    let cells = widths
        .into_iter()
        .zip(slots)
        .map(|(w, &icon)| {
            let cell = Cell {
                x,
                width: w * scale,
                icon,
            };
            x += (w + GAP) * scale;
            cell
        })
        .collect();
    Layout {
        left,
        width,
        base: base * scale,
        cells,
    }
}
pub fn ease_out_cubic(t: f32) -> f32 {
    1. - (1. - t.clamp(0., 1.)).powi(3)
}
/// SwiftDE's 160 ms out / 200 ms back launch bounce, anchored above the baseline.
pub fn bounce(seconds: f32) -> f32 {
    if !(0. ..0.36).contains(&seconds) {
        return 0.;
    }
    if seconds < 0.16 {
        let t = seconds / 0.16;
        16. * (1. - (1. - t).powi(2))
    } else {
        let t = (seconds - 0.16) / 0.20;
        16. * (1. - t.powi(2))
    }
}
/// Reorder only active keys. Hidden or temporarily unavailable entries retain
/// their place and come back when restored or reinstalled.
pub fn move_to(order: &mut Vec<String>, active: &[String], source: &str, target: &str) {
    if source == target
        || !active.iter().any(|k| k == source)
        || !active.iter().any(|k| k == target)
    {
        return;
    }
    for key in active {
        if !order.contains(key) {
            order.push(key.clone());
        }
    }
    let index = order.iter().position(|k| k == target).unwrap();
    order.retain(|k| k != source);
    order.insert(index.min(order.len()), source.to_owned());
}
