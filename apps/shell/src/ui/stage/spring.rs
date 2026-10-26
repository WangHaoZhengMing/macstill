//! Independent reconstruction, NOT Apple's source code.
//! Closed-form scalar damped spring with continuity-preserving retargeting.
//! Units: seconds, position units, position units/second; caller chooses geometry units.

#[derive(Clone, Copy, Debug)]
pub struct Spring {
    pub position: f64,
    pub velocity: f64,
    pub target: f64,
    mass: f64,
    stiffness: f64,
    damping: f64,
}

impl Spring {
    pub fn new(position: f64, mass: f64, stiffness: f64, damping: f64) -> Self {
        assert!(position.is_finite());
        assert!(mass.is_finite() && mass > 0.0);
        assert!(stiffness.is_finite() && stiffness > 0.0);
        assert!(damping.is_finite() && damping >= 0.0);
        Self {
            position,
            velocity: 0.0,
            target: position,
            mass,
            stiffness,
            damping,
        }
    }

    pub fn retarget(&mut self, target: f64) {
        assert!(target.is_finite());
        self.target = target; // Keep current presentation position AND velocity.
    }

    pub fn step(&mut self, dt: f64) {
        assert!(dt.is_finite() && dt >= 0.0);
        if dt == 0.0 {
            return;
        }
        let y = self.position - self.target;
        let v = self.velocity;
        let a = self.damping / (2.0 * self.mass);
        let w2 = self.stiffness / self.mass;
        let delta = w2 - a * a;
        let eps = 1e-8 * w2.max(a * a);
        let (next_y, next_v) = if delta.abs() <= eps {
            let b = v + a * y;
            let e = (-a * dt).exp();
            (e * (y + b * dt), e * (v - a * b * dt))
        } else if delta > 0.0 {
            let w = delta.sqrt();
            let (sin, cos) = (w * dt).sin_cos();
            let e = (-a * dt).exp();
            (
                e * (y * cos + (v + a * y) / w * sin),
                e * (v * cos - (a * v + w2 * y) / w * sin),
            )
        } else {
            let d = (-delta).sqrt();
            let r1 = -a + d;
            let r2 = -a - d;
            let c1 = (v - r2 * y) / (r1 - r2);
            let c2 = y - c1;
            let e1 = (r1 * dt).exp();
            let e2 = (r2 * dt).exp();
            (c1 * e1 + c2 * e2, r1 * c1 * e1 + r2 * c2 * e2)
        };
        self.position = self.target + next_y;
        self.velocity = next_v;
    }

    pub fn settled(&self, position_epsilon: f64, velocity_epsilon: f64) -> bool {
        (self.position - self.target).abs() <= position_epsilon
            && self.velocity.abs() <= velocity_epsilon
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converges_for_all_damping_regimes() {
        for damping in [8.0, 20.0, 40.0] {
            let mut s = Spring::new(0.0, 1.0, 100.0, damping);
            s.retarget(100.0);
            for _ in 0..2400 {
                s.step(1.0 / 240.0);
            }
            assert!(s.settled(0.001, 0.001), "{s:?}");
        }
    }

    #[test]
    fn cadence_does_not_change_constant_target_result() {
        for damping in [8.0, 20.0, 40.0] {
            let mut a = Spring::new(0.0, 1.0, 100.0, damping);
            a.retarget(1.0);
            let mut b = a;
            a.step(0.7);
            for _ in 0..84 {
                b.step(1.0 / 120.0);
            }
            assert!((a.position - b.position).abs() < 1e-10);
            assert!((a.velocity - b.velocity).abs() < 1e-10);
        }
    }

    #[test]
    fn retarget_preserves_presentation_state() {
        let mut s = Spring::new(0.0, 2.0, 300.0, 50.0);
        s.retarget(100.0);
        s.step(0.12);
        let before = (s.position, s.velocity);
        s.retarget(-20.0);
        assert_eq!((s.position, s.velocity), before);
        for _ in 0..600 {
            s.step(1.0 / 120.0);
        }
        assert!(s.settled(1e-6, 1e-6));
    }

    #[test]
    fn very_stiff_tutorial_reset_stays_finite() {
        let mut s = Spring::new(0.0, 1.0, 852559.0, 1846.68);
        s.retarget(1.0);
        for _ in 0..120 {
            s.step(1.0 / 60.0);
        }
        assert!(s.position.is_finite() && s.velocity.is_finite());
        assert!(s.settled(1e-6, 1e-6));
    }
}
