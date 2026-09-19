//! The roll degree of freedom: hydrostatic restoring plus hull roll damping.
//!
//! ## What this module is, and what it is not
//!
//! [`roll_moments`] is the roll-DOF **summary**: the two moments that act on
//! `φ` when no sail, foil or rigging load is present. It is what the free-decay
//! tests integrate, and what section 08 will report.
//!
//! It is deliberately *not* a second force path. In `forces::evaluate`:
//!
//! * `restore` is what slot 6 adds to `ΣK` (through
//!   [`hydrostatics::righting_moment`], the same expression this module uses);
//! * `damping` is what slot 1 has **already** added, as `hull_loads(…).k_roll`.
//!
//! Nothing sums both fields of a [`RollMoments`] into `ΣK`, and nothing may.
//! Sail, foil and sheet heeling moments arrive separately through
//! `Generalized::add` (F6.4); they are not recomputed here.
//!
//! ## There is exactly one roll damping coefficient pair
//!
//! `K_p` and `K_pp` are F6.6 hull quantities and live in `hydro::hull`. This
//! module *calls* that model rather than restating it, so there is no second
//! place for the pair to drift out of step, and no numeric coefficient appears
//! in this file at all (`no_duplicate_damping`).

use crate::hydro::hull::hull_loads;
use crate::parameters::BoatParameters;
use crate::stability::hydrostatics::{righting_moment, GzCurve};
use crate::state::BoatState;

/// The two moments about `+x_H` that act on roll in still water with no rig
/// load. N·m.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RollMoments {
    /// `K_restore = −Δ·g·GZ(φ)` (F6.7).
    pub restore: f64,
    /// `K_hull = −(K_p·p + K_pp·p·|p|)` (F6.6), read from `hydro::hull`.
    pub damping: f64,
}

impl RollMoments {
    /// The sum, for a caller integrating roll on its own. `forces::evaluate`
    /// does **not** use this — see the module note.
    pub fn total(&self) -> f64 {
        self.restore + self.damping
    }
}

/// Hydrostatic restoring and hull roll damping at this state.
pub fn roll_moments(st: &BoatState, p: &BoatParameters, curve: &GzCurve) -> RollMoments {
    RollMoments {
        restore: righting_moment(st.phi, curve, p.total_mass()),
        damping: hull_loads(st, p).k_roll,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parameters::BoatParameters;

    fn params() -> BoatParameters {
        BoatParameters::ilca7()
    }

    fn curve(p: &BoatParameters) -> GzCurve {
        GzCurve::from_params(p)
    }

    fn rolling(phi: f64, rate: f64) -> BoatState {
        BoatState {
            phi,
            p: rate,
            ..BoatState::ZERO
        }
    }

    #[test]
    fn damping_opposes_roll_rate() {
        let p = params();
        let g = curve(&p);
        assert!(roll_moments(&rolling(0.0, 0.8), &p, &g).damping < 0.0);
        assert!(roll_moments(&rolling(0.0, -0.8), &p, &g).damping > 0.0);
        // And the mirror is exact, because F6.6 is odd in `p`.
        assert_eq!(
            roll_moments(&rolling(0.4, 0.8), &p, &g).damping,
            -roll_moments(&rolling(-0.4, -0.8), &p, &g).damping
        );
    }

    #[test]
    fn no_duplicate_damping() {
        // Two halves. The grep proves this file states no coefficient of its
        // own; the numeric half proves the value it returns really is the F6.6
        // pair read from the catalogue, which a grep alone cannot show.
        let src = include_str!("roll.rs");
        let body = src.split("#[cfg(test)]").next().expect("module body");
        for (n, line) in body.lines().enumerate() {
            let code: Vec<char> = line.split("//").next().unwrap_or("").chars().collect();
            // A digit that *starts* a token is a literal; one inside an
            // identifier is part of a name such as `f64` or `i_xx`.
            let literal = code.iter().enumerate().any(|(i, c)| {
                c.is_ascii_digit()
                    && !(i > 0 && (code[i - 1].is_ascii_alphanumeric() || code[i - 1] == '_'))
            });
            assert!(
                !literal,
                "roll.rs:{}: a numeric literal in the roll model — K_p/K_pp live in hydro::hull (F6.6): {line}",
                n + 1
            );
        }

        let mut p = params();
        let g = curve(&p);
        let rate = 0.7;
        let expected = -(p.resistance.k_p * rate + p.resistance.k_pp * rate * rate.abs());
        assert_eq!(roll_moments(&rolling(0.2, rate), &p, &g).damping, expected);

        // Editing the catalogue moves the result; a hard-coded copy would not.
        p.resistance.k_p *= 2.0;
        p.resistance.k_pp = 0.0;
        assert_eq!(
            roll_moments(&rolling(0.2, rate), &p, &g).damping,
            -(p.resistance.k_p * rate)
        );
    }

    #[test]
    fn upright_equilibrium() {
        let p = params();
        let m = roll_moments(&rolling(0.0, 0.0), &p, &curve(&p));
        assert_eq!(m.restore, 0.0);
        assert_eq!(m.damping, 0.0);
        assert_eq!(m.total(), 0.0);
    }

    /// One RK2-midpoint step of the isolated roll DOF `I_x φ̈ = K(φ, p)`.
    ///
    /// The 1-DOF system is integrated here rather than through `Simulation`
    /// precisely because the criterion says *"with no external moment"*: a
    /// full simulation would put the sail, the board and the rudder into the
    /// flow that a rolling mast and hull generate, and measure their damping
    /// as well as this module's.
    fn decay_step(phi: f64, rate: f64, dt: f64, p: &BoatParameters, g: &GzCurve) -> (f64, f64) {
        let i_x = p.inertia.i_xx + p.inertia.a_phi;
        let accel = |phi: f64, rate: f64| roll_moments(&rolling(phi, rate), p, g).total() / i_x;
        let (k1_phi, k1_p) = (rate, accel(phi, rate));
        let (mid_phi, mid_p) = (phi + 0.5 * dt * k1_phi, rate + 0.5 * dt * k1_p);
        let (k2_phi, k2_p) = (mid_p, accel(mid_phi, mid_p));
        (phi + dt * k2_phi, rate + dt * k2_p)
    }

    /// `(times of the positive peaks, their amplitudes)` of a free decay.
    fn free_decay(phi0: f64, p: &BoatParameters, g: &GzCurve, seconds: f64) -> Vec<(f64, f64)> {
        let dt = p.sim.dt;
        let mut state = (phi0, 0.0f64);
        let mut previous = state;
        let mut peaks = Vec::new();
        let steps = (seconds / dt) as usize;
        for i in 1..=steps {
            let next = decay_step(state.0, state.1, dt, p, g);
            // A positive peak is where the roll rate crosses from + to −.
            if previous.1 > 0.0 && state.1 <= 0.0 {
                peaks.push(((i as f64) * dt, state.0));
            }
            previous = state;
            state = next;
        }
        peaks
    }

    #[test]
    fn free_decay_period() {
        // The physical sanity check on `i_xx`, `a_phi` and `gm` together
        // (task 7.2). Small amplitude, so the linearised period applies.
        let p = params();
        let g = curve(&p);
        let i_x = p.inertia.i_xx + p.inertia.a_phi;
        let expected = std::f64::consts::TAU
            * (i_x / (p.total_mass() * crate::constants::G * p.stability.gm)).sqrt();

        let peaks = free_decay(0.1, &p, &g, 20.0);
        assert!(peaks.len() >= 3, "only {} peaks", peaks.len());
        let measured = (peaks[peaks.len() - 1].0 - peaks[0].0) / ((peaks.len() - 1) as f64);
        let error = (measured - expected).abs() / expected;
        assert!(
            error < 0.05,
            "measured roll period {measured:.4} s vs 2*pi*sqrt(I_x/(D g GM)) = {expected:.4} s ({:.2} %)",
            error * 100.0
        );
    }

    #[test]
    fn free_decay_decays() {
        let p = params();
        let peaks = free_decay(0.1, &p, &curve(&p), 20.0);
        assert!(peaks.len() >= 4, "only {} peaks", peaks.len());
        for pair in peaks.windows(2) {
            assert!(
                pair[1].1 < pair[0].1,
                "amplitude rose from {} to {}",
                pair[0].1,
                pair[1].1
            );
        }
    }
}
