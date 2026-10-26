//! Logical output coordinates shared by the strip, hit regions and transitions.
//! These are reconstruction parameters, not recovered WindowManager defaults.
use super::super::*;

pub(super) const WIDTH: f32 = 156.;
pub(super) const TOP: f32 = 36.;
pub(super) const BOTTOM: f32 = 78.;
pub(super) const MAX_ROWS: usize = 6;

#[derive(Clone, Copy)]
pub(super) struct Strip {
    pub rows: usize,
    pub height: f32,
    top: f32,
    gap: f32,
}
impl Strip {
    pub fn new(height: f32, count: usize) -> Self {
        let available = (height - TOP - BOTTOM).max(0.);
        let rows = count
            .min(MAX_ROWS)
            .min(((available + 18.) / 94.).floor() as usize);
        let gap = 18.;
        let row_height = if rows == 0 {
            0.
        } else {
            ((available - gap * rows.saturating_sub(1) as f32) / rows as f32).min(112.)
        };
        let content = row_height * rows as f32 + gap * rows.saturating_sub(1) as f32;
        Self {
            rows,
            height: row_height,
            top: TOP + (available - content) * 0.5,
            gap,
        }
    }
    pub fn slot(&self, index: usize) -> Bounds<Pixels> {
        Bounds::new(
            point(
                px(8.),
                px(self.top + index as f32 * (self.height + self.gap)),
            ),
            size(px(WIDTH - 20.), px(self.height)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{BOTTOM, MAX_ROWS, Strip, TOP};
    #[test]
    fn recent_groups_fit_without_scrolling_at_every_output_height() {
        for height in [100., 380., 650., 800., 1080., 2160.] {
            for count in 0..30 {
                let layout = Strip::new(height, count);
                assert!(layout.rows <= MAX_ROWS && layout.rows <= count);
                for index in 0..layout.rows {
                    let rect = layout.slot(index);
                    assert!(f32::from(rect.top()) >= TOP);
                    assert!(f32::from(rect.bottom()) <= height - BOTTOM + 0.01);
                }
            }
        }
    }
}
