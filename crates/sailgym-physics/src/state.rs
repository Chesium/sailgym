//! Boat state, its time derivative, and the player controls (F3).
//!
//! The field order below is **normative** (F3, F8.3): the snapshot buffer
//! crossing the WASM boundary, the recording schema of section 09 and the
//! golden regression files all index into it. [`STATE_FIELDS`] is the single
//! shared name list; `web/src/sim/snapshot.ts` mirrors it.

use serde::{Deserialize, Serialize};

use crate::frames::wrap_pi;

/// Number of scalars in the flat state buffer (F3).
pub const STATE_LEN: usize = 13;

/// Field names in [`BoatState::to_array`] order (F8.3).
///
/// `web/src/sim/snapshot.ts` mirrors this list in camel case; a test in
/// `simulation.rs` asserts the two agree in length and order.
pub const STATE_FIELDS: [&str; STATE_LEN] = [
    "x", "y", "psi", "phi", "u", "v", "r", "p", "beta", "beta_dot", "delta_r", "l_sheet", "t",
];

/// The complete simulation state (F3). 13 scalars, verbatim from foundations.
///
/// `phi` is deliberately **not** wrapped: brief §17 requires the boat to pass
/// dynamically through `|φ| > 90°`, and wrapping would break roll-rate
/// continuity and the energy invariant.
///
/// Accelerations are derived, never integrated as independent state (brief §5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BoatState {
    // pose, world frame
    /// m, world east
    pub x: f64,
    /// m, world north
    pub y: f64,
    /// rad, yaw, CCW from world +x, wrapped to (−π, π]
    pub psi: f64,
    /// rad, roll, +stbd down, NOT wrapped (capsize may exceed ±π)
    pub phi: f64,

    // velocities, horizontal body frame H
    /// m/s, surge, +forward
    pub u: f64,
    /// m/s, sway, +to port
    pub v: f64,
    /// rad/s, yaw rate, +to port
    pub r: f64,
    /// rad/s, roll rate, +toward starboard
    pub p: f64,

    // rig
    /// rad, boom angle, +to starboard, wrapped to (−π, π]
    pub beta: f64,
    /// rad/s
    pub beta_dot: f64,

    // actuators
    /// rad, rudder angle, +bow to starboard
    pub delta_r: f64,
    /// m, available mainsheet length at the boom attachment
    pub l_sheet: f64,

    // simulation time
    /// s, seconds since reset
    pub t: f64,
}

impl BoatState {
    /// All thirteen fields zero — the same value as `BoatState::default()`,
    /// available in `const` context. Scenarios (section 09) override it
    /// wholesale.
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        psi: 0.0,
        phi: 0.0,
        u: 0.0,
        v: 0.0,
        r: 0.0,
        p: 0.0,
        beta: 0.0,
        beta_dot: 0.0,
        delta_r: 0.0,
        l_sheet: 0.0,
        t: 0.0,
    };

    /// Flat buffer in F8.3 order. The WASM `snapshot()` returns exactly this.
    pub fn to_array(&self) -> [f64; STATE_LEN] {
        [
            self.x,
            self.y,
            self.psi,
            self.phi,
            self.u,
            self.v,
            self.r,
            self.p,
            self.beta,
            self.beta_dot,
            self.delta_r,
            self.l_sheet,
            self.t,
        ]
    }

    /// Inverse of [`BoatState::to_array`]; the round trip is the identity.
    pub fn from_array(a: &[f64; STATE_LEN]) -> Self {
        Self {
            x: a[0],
            y: a[1],
            psi: a[2],
            phi: a[3],
            u: a[4],
            v: a[5],
            r: a[6],
            p: a[7],
            beta: a[8],
            beta_dot: a[9],
            delta_r: a[10],
            l_sheet: a[11],
            t: a[12],
        }
    }

    /// `self + h * d`, field-wise. The one primitive every integrator stage in
    /// `integrator.rs` is built from.
    pub fn axpy(&self, h: f64, d: &StateDot) -> Self {
        Self {
            x: self.x + h * d.x,
            y: self.y + h * d.y,
            psi: self.psi + h * d.psi,
            phi: self.phi + h * d.phi,
            u: self.u + h * d.u,
            v: self.v + h * d.v,
            r: self.r + h * d.r,
            p: self.p + h * d.p,
            beta: self.beta + h * d.beta,
            beta_dot: self.beta_dot + h * d.beta_dot,
            delta_r: self.delta_r + h * d.delta_r,
            l_sheet: self.l_sheet + h * d.l_sheet,
            t: self.t + h * d.t,
        }
    }

    /// Wrap `psi` and `beta` to (−π, π]. **`phi` is left alone** (F3).
    pub fn wrap_angles(&mut self) {
        self.psi = wrap_pi(self.psi);
        self.beta = wrap_pi(self.beta);
    }

    /// True when no field is `NaN` or infinite (brief §35).
    pub fn is_finite(&self) -> bool {
        self.to_array().iter().all(|v| v.is_finite())
    }
}

/// The time derivative of [`BoatState`]: the same thirteen fields, each `d/dt`
/// of its namesake. `beta_dot` here is therefore `β̈`, and `t` is always 1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StateDot {
    /// m/s
    pub x: f64,
    /// m/s
    pub y: f64,
    /// rad/s
    pub psi: f64,
    /// rad/s
    pub phi: f64,
    /// m/s²
    pub u: f64,
    /// m/s²
    pub v: f64,
    /// rad/s²
    pub r: f64,
    /// rad/s²
    pub p: f64,
    /// rad/s
    pub beta: f64,
    /// rad/s²
    pub beta_dot: f64,
    /// rad/s
    pub delta_r: f64,
    /// m/s
    pub l_sheet: f64,
    /// s/s, always 1
    pub t: f64,
}

/// Player input. Controls are **rates**, never absolute angles
/// (brief §12, §13).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Controls {
    /// normalised [−1, 1]; +1 = steer bow to starboard
    pub rudder_rate_cmd: f64,
    /// normalised [−1, 1]; +1 = ease (pay out), −1 = haul
    pub sheet_rate_cmd: f64,
    /// Space: ease at the release rate, overrides `sheet_rate_cmd`
    pub sheet_release: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic test-local generator. The simulation's own RNG is
    /// `rng.rs` (F9.2, section 03); nothing here feeds the physics.
    struct Lcg(u64);

    impl Lcg {
        fn next_f64(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            // Top 53 bits → [0, 1), then mapped to [−10, 10).
            (((self.0 >> 11) as f64) / ((1u64 << 53) as f64)) * 20.0 - 10.0
        }

        fn state(&mut self) -> BoatState {
            let mut a = [0.0; STATE_LEN];
            for v in a.iter_mut() {
                *v = self.next_f64();
            }
            BoatState::from_array(&a)
        }
    }

    #[test]
    fn to_array_from_array_round_trip_is_the_identity() {
        let mut rng = Lcg(0x5EED_1234_ABCD_0001);
        for _ in 0..100 {
            let s = rng.state();
            // Exact equality: the snapshot path must not perturb a single bit.
            assert_eq!(BoatState::from_array(&s.to_array()), s);
            assert_eq!(
                BoatState::from_array(&s.to_array()).to_array(),
                s.to_array()
            );
        }
    }

    #[test]
    fn state_fields_match_the_f8_3_layout() {
        assert_eq!(STATE_FIELDS.len(), STATE_LEN);
        assert_eq!(STATE_FIELDS[3], "phi");
        assert_eq!(STATE_FIELDS[10], "delta_r");
        assert_eq!(STATE_FIELDS[STATE_LEN - 1], "t");
    }

    #[test]
    fn axpy_is_field_wise() {
        let s = BoatState {
            x: 1.0,
            u: 2.0,
            t: 3.0,
            ..BoatState::ZERO
        };
        let d = StateDot {
            x: 10.0,
            u: 20.0,
            t: 1.0,
            ..StateDot::default()
        };
        let out = s.axpy(0.5, &d);
        assert_eq!(out.x, 6.0);
        assert_eq!(out.u, 12.0);
        assert_eq!(out.t, 3.5);
        assert_eq!(out.y, 0.0);
    }

    #[test]
    fn wrap_angles_leaves_phi_untouched() {
        // Section acceptance criterion 5: phi is unwrapped (F3, brief §17).
        let mut s = BoatState {
            psi: std::f64::consts::PI * 3.0,
            phi: 4.0,
            beta: -std::f64::consts::PI * 3.0,
            ..BoatState::ZERO
        };
        s.wrap_angles();
        assert_eq!(s.phi, 4.0);
        assert!((s.psi - std::f64::consts::PI).abs() < 1e-12);
        assert!((s.beta - std::f64::consts::PI).abs() < 1e-12);
    }

    #[test]
    fn is_finite_detects_nan_and_inf() {
        assert!(BoatState::ZERO.is_finite());
        let nan = BoatState {
            v: f64::NAN,
            ..BoatState::ZERO
        };
        assert!(!nan.is_finite());
        let inf = BoatState {
            phi: f64::INFINITY,
            ..BoatState::ZERO
        };
        assert!(!inf.is_finite());
    }
}
