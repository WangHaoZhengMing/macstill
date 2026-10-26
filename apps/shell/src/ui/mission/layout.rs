#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub fn mix(self, to: Self, t: f32) -> Self {
        Self {
            x: self.x + (to.x - self.x) * t,
            y: self.y + (to.y - self.y) * t,
            w: self.w + (to.w - self.w) * t,
            h: self.h + (to.h - self.h) * t,
        }
    }
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}

#[derive(Default)]
pub(super) struct Cache {
    sizes: Vec<[i32; 2]>,
    viewport: (f32, f32),
    rects: Vec<Rect>,
}
impl Cache {
    pub fn arrange(&mut self, sizes: Vec<[i32; 2]>, width: f32, height: f32) -> &[Rect] {
        if self.sizes != sizes || self.viewport != (width, height) {
            self.rects = arrange(&sizes, width, height);
            self.sizes = sizes;
            self.viewport = (width, height);
        }
        &self.rects
    }
}
/// Choose the grid that gives the actual aspect ratios the greatest visible area.
/// Every tile retains its window aspect ratio and leaves room for its title.
pub fn arrange(sizes: &[[i32; 2]], width: f32, height: f32) -> Vec<Rect> {
    if sizes.is_empty() {
        return vec![];
    }
    let n = sizes.len();
    let gap = 24.;
    let available_w = (width - 64.).max(100.);
    let available_h = (height - 220.).max(100.);
    let mut best = (0., 1);
    for columns in 1..=n {
        let rows = n.div_ceil(columns);
        let cw = (available_w - gap * (columns - 1) as f32) / columns as f32;
        let ch = (available_h - gap * (rows - 1) as f32) / rows as f32 - 28.;
        if cw <= 0. || ch <= 0. {
            continue;
        }
        let area = sizes
            .iter()
            .map(|s| {
                let scale = (cw / s[0].max(1) as f32)
                    .min(ch / s[1].max(1) as f32)
                    .min(1.);
                s[0].max(1) as f32 * s[1].max(1) as f32 * scale * scale
            })
            .sum::<f32>();
        if area > best.0 {
            best = (area, columns);
        }
    }
    let cols = best.1;
    let rows = n.div_ceil(cols);
    let cw = (available_w - gap * (cols - 1) as f32) / cols as f32;
    let ch = (available_h - gap * (rows - 1) as f32) / rows as f32;
    sizes
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let row = i / cols;
            let in_row = (n - row * cols).min(cols);
            let scale = (cw / s[0].max(1) as f32)
                .min((ch - 28.).max(1.) / s[1].max(1) as f32)
                .min(1.);
            let w = s[0].max(1) as f32 * scale;
            let h = s[1].max(1) as f32 * scale;
            Rect {
                x: (width - in_row as f32 * cw - (in_row - 1) as f32 * gap) / 2.
                    + (i % cols) as f32 * (cw + gap)
                    + (cw - w) / 2.,
                y: 170. + row as f32 * (ch + gap) + (ch - 28. - h) / 2.,
                w,
                h,
            }
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_layout_tracks_resize_reordering_and_window_removal() {
        let mut cache = Cache::default();
        let sizes = vec![[1200, 800], [500, 900]];
        let first = cache.arrange(sizes.clone(), 1440., 900.).to_vec();
        assert_eq!(cache.arrange(sizes.clone(), 1440., 900.), first);
        for (sizes, width, height) in [
            (sizes, 900., 700.),
            (vec![[500, 900], [1200, 800]], 900., 700.),
            (vec![[500, 900]], 900., 700.),
            (vec![], 900., 700.),
        ] {
            assert_eq!(
                cache.arrange(sizes.clone(), width, height),
                arrange(&sizes, width, height)
            );
        }
    }
    #[test]
    fn windows_fit_without_overlap_at_varied_counts() {
        for n in 1..=48 {
            let sizes: Vec<_> = (0..n)
                .map(|i| if i % 2 == 0 { [1200, 800] } else { [500, 900] })
                .collect();
            let rects = arrange(&sizes, 1440., 900.);
            for (i, a) in rects.iter().enumerate() {
                assert!(a.x >= 0. && a.y >= 140. && a.x + a.w <= 1440.1 && a.y + a.h <= 900.1);
                for b in rects.iter().skip(i + 1) {
                    assert!(
                        a.x + a.w <= b.x + 0.1
                            || b.x + b.w <= a.x + 0.1
                            || a.y + a.h <= b.y + 0.1
                            || b.y + b.h <= a.y + 0.1
                    );
                }
            }
        }
    }
}
