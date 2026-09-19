//! Equations of motion (F4).
//!
//! **This is the only place the equations of motion appear.** Force-producing
//! modules return [`Load`]s; [`Generalized`] accumulates them; [`derivative`]
//! turns them into `StateDot`. Nothing here mutates state, and no module reads
//! or writes another module's state (F4.4, brief §7).

use crate::parameters::BoatParameters;
use crate::state::{BoatState, Controls, StateDot};
use crate::vec::Vec3;

/// A force and its application point, expressed in the boat-fixed frame `B`
/// (F4.4).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Load {
    /// N, force in `B`.
    pub f: Vec3,
    /// m, application point relative to the CG, in `B`.
    pub r: Vec3,
}

/// Accumulated generalised forces in the horizontal frame `H` (F4.4).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Generalized {
    /// N, surge force along `+x_H`.
    pub x: f64,
    /// N, sway force along `+y_H` (to port).
    pub y: f64,
    /// N·m, yaw moment about `+z_H`.
    pub n: f64,
    /// N·m, roll moment about `+x_H ≡ +x_B`.
    pub k: f64,
}

impl Generalized {
    /// Rotate a boat-fixed [`Load`] into `H` and accumulate, verbatim from
    /// F6.4.
    ///
    /// `K` (roll moment) is about `+x`, shared by both frames, so it is not
    /// rotated. The `cos φ` factors here **are** the classical heel
    /// correction, derived geometrically; do not add another one anywhere.
    pub fn add(&mut self, l: Load, phi: f64) {
        let (s, c) = phi.sin_cos();
        // roll moment about +x, computed in B; +x is shared between B and H
        self.k += l.r.y * l.f.z - l.r.z * l.f.y;
        // in-plane force rotated B -> H, then projected onto the horizontal plane
        self.x += l.f.x;
        self.y += l.f.y * c - l.f.z * s;
        // yaw moment about +z_B rotated into H
        let n_b = l.r.x * l.f.y - l.r.y * l.f.x;
        self.n += n_b * c;
    }
}

/// A pure source of generalised forces and boom torque.
pub trait ForceModel {
    /// Pure: must not mutate anything. Returns generalised forces in `H`
    /// (F4.4).
    fn generalized(&self, st: &BoatState, c: &Controls, p: &BoatParameters, t: f64) -> Generalized;

    /// Boom moment about `+z_B` at the mast: aerodynamic and passive moments.
    fn boom_moment(&self, st: &BoatState, c: &Controls, p: &BoatParameters, t: f64) -> f64;
}

/// Rate limiter for an actuator state confined to `[lo, hi]` (F4.3).
///
/// The rate is zeroed only when the value is **strictly** outside the box and
/// the rate would push it further out. The strictness matters: a rate cut at
/// the boundary itself would make every RK stage inside the boundary layer
/// return zero, freezing the actuator a timestep-dependent distance short of
/// its limit. With the strict test, the stage saturation applied by
/// `integrator::step` lands the actuator exactly on the limit for any `dt`,
/// which is what `integrator::clamp_in_derivative` asserts.
fn limit_rate(value: f64, rate: f64, lo: f64, hi: f64) -> f64 {
    if (rate > 0.0 && value > hi) || (rate < 0.0 && value < lo) {
        0.0
    } else {
        rate
    }
}

/// `δ̇r` (F4.3). Self-centring lives here, in Rust, not in the browser.
///
/// With no steering command and `delta_r_self_centre` set, the tiller returns
/// to neutral at a constant rate. A constant-rate return cannot land exactly
/// on zero, so it limit-cycles within `±delta_r_return_rate·dt` (≈ 0.008 rad
/// at the default timestep). That is deterministic and shrinks with `dt`;
/// a proportional return would need a gain that F7 does not define.
fn rudder_rate(st: &BoatState, c: &Controls, p: &BoatParameters) -> f64 {
    let cmd = c.rudder_rate_cmd.clamp(-1.0, 1.0);
    let raw = if cmd != 0.0 {
        cmd * p.rudder.delta_r_rate_max
    } else if p.rudder.delta_r_self_centre {
        match st.delta_r.partial_cmp(&0.0) {
            Some(std::cmp::Ordering::Greater) => -p.rudder.delta_r_return_rate,
            Some(std::cmp::Ordering::Less) => p.rudder.delta_r_return_rate,
            _ => 0.0,
        }
    } else {
        0.0
    };
    limit_rate(st.delta_r, raw, -p.rudder.delta_r_max, p.rudder.delta_r_max)
}

/// `L̇` (F4.3), the commanded payout rate in m/s. `+` eases (pays out), `−`
/// hauls; `sheet_release` (Space) overrides the analogue command with
/// `sheet_release_rate` (brief §12). The rate is zeroed when `L` is already
/// outside `[l_sheet_min, l_sheet_max]` and the command would push it further
/// out, so the clamp lives **inside** the derivative (F4.3).
///
/// Public because `forces::evaluate` must feed the mainsheet element exactly
/// the `L̇` this derivative evaluation integrates; recomputing it anywhere
/// else is how the damping term drifts between RK2 stages.
pub fn sheet_rate(c: &Controls, st: &BoatState, p: &BoatParameters) -> f64 {
    let raw = if c.sheet_release {
        p.sheet.sheet_release_rate
    } else {
        let cmd = c.sheet_rate_cmd.clamp(-1.0, 1.0);
        if cmd >= 0.0 {
            cmd * p.sheet.sheet_ease_rate
        } else {
            cmd * p.sheet.sheet_haul_rate
        }
    };
    limit_rate(st.l_sheet, raw, p.sheet.l_sheet_min, p.sheet.l_sheet_max)
}

/// F4.1–F4.3 assembled: the state derivative at `(st, c, t)`.
///
/// Takes `&BoatState` and returns a value — there is no `&mut` anywhere in
/// this signature or in [`ForceModel`], which is what makes multi-stage
/// integrators and the determinism guarantee possible.
pub fn derivative(
    st: &BoatState,
    c: &Controls,
    p: &BoatParameters,
    fm: &dyn ForceModel,
    t: f64,
) -> StateDot {
    let g = fm.generalized(st, c, p, t);
    let m_beta = fm.boom_moment(st, c, p, t);

    // F4.2 effective inertias.
    let m = p.total_mass();
    let m_x = m + p.inertia.a_x;
    let m_y = m + p.inertia.a_y;
    let i_z = p.inertia.i_zz + p.inertia.a_psi;
    let i_x = p.inertia.i_xx + p.inertia.a_phi;
    let i_b = p.sail.i_boom;

    let (sin_psi, cos_psi) = st.psi.sin_cos();

    StateDot {
        // F4.1 kinematics
        x: st.u * cos_psi - st.v * sin_psi,
        y: st.u * sin_psi + st.v * cos_psi,
        psi: st.r,
        phi: st.p,

        // F4.2 rigid-body dynamics with added mass
        u: (g.x + m_y * st.v * st.r) / m_x,
        v: (g.y - m_x * st.u * st.r) / m_y,
        r: g.n / i_z,
        p: g.k / i_x,

        // rig
        beta: st.beta_dot,
        beta_dot: m_beta / i_b,

        // F4.3 actuators
        delta_r: rudder_rate(st, c, p),
        l_sheet: sheet_rate(c, st, p),

        t: 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Zero;

    impl ForceModel for Zero {
        fn generalized(
            &self,
            _: &BoatState,
            _: &Controls,
            _: &BoatParameters,
            _: f64,
        ) -> Generalized {
            Generalized::default()
        }
        fn boom_moment(&self, _: &BoatState, _: &Controls, _: &BoatParameters, _: f64) -> f64 {
            0.0
        }
    }

    fn params() -> BoatParameters {
        BoatParameters::ilca7()
    }

    #[test]
    fn kinematics_rotate_body_velocity_into_the_world() {
        // Heading north (ψ = π/2), moving forward: the boat goes north.
        let st = BoatState {
            psi: std::f64::consts::FRAC_PI_2,
            u: 2.0,
            ..BoatState::ZERO
        };
        let d = derivative(&st, &Controls::default(), &params(), &Zero, 0.0);
        assert!(d.x.abs() < 1e-15);
        assert!((d.y - 2.0).abs() < 1e-15);
        assert_eq!(d.t, 1.0);
    }

    #[test]
    fn turning_to_port_drifts_sway_to_starboard() {
        // F4.2 verification note: with ΣY = 0, u > 0 and r > 0, v̇ = −u r < 0.
        let st = BoatState {
            u: 3.0,
            r: 0.4,
            ..BoatState::ZERO
        };
        let d = derivative(&st, &Controls::default(), &params(), &Zero, 0.0);
        let p = params();
        let m_x = p.total_mass() + p.inertia.a_x;
        let m_y = p.total_mass() + p.inertia.a_y;
        assert!(d.v < 0.0);
        assert!((d.v + m_x * 3.0 * 0.4 / m_y).abs() < 1e-12);
    }

    #[test]
    fn generalized_add_matches_f6_4() {
        // A pure side force at the CE: heel moment, no yaw moment at φ = 0.
        let mut g = Generalized::default();
        g.add(
            Load {
                f: Vec3::new(0.0, 100.0, 0.0),
                r: Vec3::new(0.0, 0.0, 2.4),
            },
            0.0,
        );
        assert_eq!(g.y, 100.0);
        assert_eq!(g.k, -2.4 * 100.0);
        assert_eq!(g.n, 0.0);

        // At φ = π/2 the same boat-fixed side force is vertical in H.
        let mut g = Generalized::default();
        g.add(
            Load {
                f: Vec3::new(0.0, 100.0, 0.0),
                r: Vec3::new(0.0, 0.0, 2.4),
            },
            std::f64::consts::FRAC_PI_2,
        );
        assert!(g.y.abs() < 1e-12);
    }

    #[test]
    fn rudder_command_maps_to_the_rate_limit() {
        let p = params();
        let st = BoatState::ZERO;
        let c = Controls {
            rudder_rate_cmd: 1.0,
            ..Controls::default()
        };
        let d = derivative(&st, &c, &p, &Zero, 0.0);
        assert_eq!(d.delta_r, p.rudder.delta_r_rate_max);

        // Commands outside [−1, 1] cannot beat the rate limit.
        let c = Controls {
            rudder_rate_cmd: 12.0,
            ..Controls::default()
        };
        let d = derivative(&st, &c, &p, &Zero, 0.0);
        assert_eq!(d.delta_r, p.rudder.delta_r_rate_max);
    }

    #[test]
    fn rudder_self_centres_only_when_no_command_is_held() {
        let p = params();
        let st = BoatState {
            delta_r: 0.3,
            ..BoatState::ZERO
        };
        let d = derivative(&st, &Controls::default(), &p, &Zero, 0.0);
        assert_eq!(d.delta_r, -p.rudder.delta_r_return_rate);

        let st = BoatState {
            delta_r: -0.3,
            ..BoatState::ZERO
        };
        let d = derivative(&st, &Controls::default(), &p, &Zero, 0.0);
        assert_eq!(d.delta_r, p.rudder.delta_r_return_rate);

        // Exactly neutral: no rate at all, so rest stays rest.
        let d = derivative(&BoatState::ZERO, &Controls::default(), &p, &Zero, 0.0);
        assert_eq!(d.delta_r, 0.0);
    }

    #[test]
    fn sheet_release_overrides_the_analogue_command() {
        let p = params();
        let st = BoatState {
            l_sheet: 2.0,
            ..BoatState::ZERO
        };
        let c = Controls {
            sheet_rate_cmd: -1.0,
            sheet_release: true,
            ..Controls::default()
        };
        let d = derivative(&st, &c, &p, &Zero, 0.0);
        assert_eq!(d.l_sheet, p.sheet.sheet_release_rate);

        let c = Controls {
            sheet_rate_cmd: -1.0,
            ..Controls::default()
        };
        let d = derivative(&st, &c, &p, &Zero, 0.0);
        assert_eq!(d.l_sheet, -p.sheet.sheet_haul_rate);
    }

    #[test]
    fn rates_are_zeroed_outside_the_actuator_box() {
        let p = params();
        let beyond = p.rudder.delta_r_max * 1.5;
        let st = BoatState {
            delta_r: beyond,
            l_sheet: p.sheet.l_sheet_max + 1.0,
            ..BoatState::ZERO
        };
        let c = Controls {
            rudder_rate_cmd: 1.0,
            sheet_rate_cmd: 1.0,
            ..Controls::default()
        };
        let d = derivative(&st, &c, &p, &Zero, 0.0);
        assert_eq!(d.delta_r, 0.0);
        assert_eq!(d.l_sheet, 0.0);

        // Pushing back into the box is still allowed.
        let c = Controls {
            rudder_rate_cmd: -1.0,
            sheet_rate_cmd: -1.0,
            ..Controls::default()
        };
        let d = derivative(&st, &c, &p, &Zero, 0.0);
        assert!(d.delta_r < 0.0);
        assert!(d.l_sheet < 0.0);
    }
}
