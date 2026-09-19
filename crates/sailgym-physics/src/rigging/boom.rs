//! Passive boom moments (F6.9). No sheet restraint until section 06.

use crate::parameters::BoatParameters;
use crate::state::BoatState;

#[derive(Clone, Copy, Debug, Default)]
pub struct BoomMoments {
    pub aero: f64,
    pub sheet: f64,
    pub damping: f64,
    pub limit: f64,
}
impl BoomMoments {
    pub fn total(&self) -> f64 {
        self.aero + self.sheet + self.damping + self.limit
    }
}

/// Returns (gooseneck damping, mechanical limit). The elastic limit is
/// continuous; F6.9 switches limit damping on at the boundary discontinuously.
pub fn boom_passive_moments(st: &BoatState, p: &BoatParameters) -> (f64, f64) {
    let limit = if st.beta.abs() > p.sail.beta_max {
        -p.sail.k_lim * (st.beta.abs() - p.sail.beta_max) * st.beta.signum()
            - p.sail.c_lim * st.beta_dot
    } else {
        0.0
    };
    (-p.sail.c_beta * st.beta_dot, limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn moments(beta: f64, beta_dot: f64) -> (f64, f64) {
        boom_passive_moments(
            &BoatState {
                beta,
                beta_dot,
                ..BoatState::ZERO
            },
            &BoatParameters::ilca7(),
        )
    }
    #[test]
    fn damping_opposes_rotation() {
        for rate in [-2.0, -0.1, 0.1, 2.0] {
            assert!(moments(0.0, rate).0 * rate < 0.0);
        }
    }
    #[test]
    fn limit_inactive_inside_range() {
        let max = BoatParameters::ilca7().sail.beta_max;
        for beta in [-max, -0.4, 0.0, 0.4, max] {
            assert_eq!(moments(beta, 1.0).1, 0.0);
        }
    }
    #[test]
    fn limit_restores_outside_range() {
        let beta = BoatParameters::ilca7().sail.beta_max + 0.1;
        assert!(moments(beta, 0.0).1 < 0.0);
        assert!(moments(-beta, 0.0).1 > 0.0);
    }
    #[test]
    fn limit_is_continuous() {
        let max = BoatParameters::ilca7().sail.beta_max;
        for sign in [-1.0, 1.0] {
            let mut previous = 0.0;
            for i in -100..=100 {
                let limit = moments(sign * (max + i as f64 * 1e-6), 0.0).1;
                assert!((limit - previous).abs() < 1e-3);
                previous = limit;
            }
        }
    }
    #[test]
    fn limit_damping_opposes_rotation() {
        let max = BoatParameters::ilca7().sail.beta_max;
        for beta in [-max - 0.1, max + 0.1] {
            for rate in [-1.0, 1.0] {
                assert!((moments(beta, rate).1 - moments(beta, 0.0).1) * rate < 0.0);
            }
        }
    }
    // The source guard lives in the integration test so it cannot match itself.
}
