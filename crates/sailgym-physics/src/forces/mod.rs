//! Generalised force assembly in a fixed summation order (F4.4, F9.4).
//!
//! [`evaluate`] is the pipeline of brief §7, written once:
//!
//! ```text
//! environment ─► local fluid velocities ─► component Loads ─► Generalized
//! ```
//!
//! Every force-producing module returns a [`Load`] in the boat-fixed frame
//! `B`; [`Generalized::add`] rotates it into `H` with the F6.4 heel geometry
//! and accumulates. No module reads or writes another module's state, and
//! there is no branch anywhere of the form `if sail_released { … }`
//! (brief §7, §46).
//!
//! The one exception is the hull (F6.6), whose four terms are functions of the
//! **horizontal-frame** velocities `u, v, r, p` and are therefore already in
//! `H`. They are summed directly; the reasoning, and why routing them through
//! `add` diverges, is at the call site.
//!
//! ## The summation order is part of the determinism contract
//!
//! F9.4 fixes it, and floating-point addition is not associative, so
//! reordering these six calls changes recorded trajectories in the last bits.
//! `tests::summation_order_documented` reads this file and asserts the order,
//! so an accidental reorder shows up in review rather than in a golden-file
//! mismatch three sections later.
//!
//! ## Deferred loads
//!
//! Mainsheet (section 06) and hydrostatic righting (section 07) contribute zero. They are summed anyway, at their
//! final position in the order, so that landing them changes a value and not
//! the arithmetic structure.

use crate::aero::{apparent::apparent_wind_cg, sail::sail_load};
use crate::dynamics::{ForceModel, Generalized, Load};
use crate::environment::WindField;
use crate::frames::world_to_body;
use crate::hydro::centerboard::centerboard_load;
use crate::hydro::hull::hull_loads;
use crate::hydro::rudder::rudder_load;
use crate::parameters::BoatParameters;
use crate::rigging::boom::{boom_passive_moments, BoomMoments};
use crate::state::{BoatState, Controls};
use crate::vec::{Vec2, Vec3};

/// One complete force evaluation, kept whole so that the accelerations and the
/// debug panel (section 08) are guaranteed to describe the same step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ForceBreakdown {
    pub sail: Load,
    pub board: Load,
    pub rudder: Load,
    pub hull: Load,
    pub sheet: Load,
    pub total: Generalized,
    pub alpha_sail: f64,
    pub alpha_board: f64,
    pub alpha_rudder: f64,
    pub cl_sail: f64,
    pub cd_sail: f64,
    pub sheet_tension: f64,
    pub m_beta: f64,
    pub k_restore: f64,
    pub gz: f64,
    /// Apparent wind at the CG, in `B`.
    pub aw_boat: Vec3,
    /// True wind at the boat, rotated into the horizontal body frame `H`.
    pub tw_boat: Vec2,
}

/// The real force model. R4 is closed here: this replaces the M1 placeholder
/// that section 02 shipped, which section 04 deleted outright.
///
/// Still-air adapter, used for explicit zero-wind tests. The simulation uses
/// `WindForces`, borrowing its actual environment.
#[derive(Clone, Copy, Debug, Default)]
pub struct PhysicalForces;

/// Zero wind for the explicit still-air adapter.
struct NoWind;

impl WindField for NoWind {
    fn sample(&self, _x: f64, _y: f64, _t: f64) -> Vec2 {
        Vec2::ZERO
    }

    fn sample_grid(
        &self,
        _x0: f64,
        _y0: f64,
        _dx: f64,
        _dy: f64,
        _nx: usize,
        _ny: usize,
        _t: f64,
        out: &mut [f32],
    ) {
        out.fill(0.0);
    }
}

impl ForceModel for PhysicalForces {
    fn generalized(&self, st: &BoatState, c: &Controls, p: &BoatParameters, t: f64) -> Generalized {
        evaluate(st, c, p, &NoWind, t).total
    }

    fn boom_moment(&self, st: &BoatState, c: &Controls, p: &BoatParameters, t: f64) -> f64 {
        evaluate(st, c, p, &NoWind, t).m_beta
    }
}

/// The physical model borrowing the simulation's environment. Each RK stage
/// samples at its own position and time; changing wind needs no cached copy.
pub struct WindForces<'a> {
    pub wind: &'a dyn WindField,
}
impl ForceModel for WindForces<'_> {
    fn generalized(&self, st: &BoatState, c: &Controls, p: &BoatParameters, t: f64) -> Generalized {
        evaluate(st, c, p, self.wind, t).total
    }
    fn boom_moment(&self, st: &BoatState, c: &Controls, p: &BoatParameters, t: f64) -> f64 {
        evaluate(st, c, p, self.wind, t).m_beta
    }
}

/// Compute the full breakdown once. `generalized` and the diagnostics both
/// read it, so they can never disagree.
///
/// The six additions below are in the F9.4 order and must not be reordered.
pub fn evaluate(
    st: &BoatState,
    _c: &Controls,
    p: &BoatParameters,
    w: &dyn WindField,
    t: f64,
) -> ForceBreakdown {
    let mut total = Generalized::default();
    let wind_world = w.sample(st.x, st.y, t);

    // 1. hull — all four F6.6 terms, added straight into `H`.
    //
    //    Hull resistance is **already a horizontal-frame quantity**: F6.6
    //    writes it as a function of `u, v, r, p`, which F3 defines in `H`, and
    //    F4.2 consumes the result as `ΣX, ΣY, ΣN, ΣK` in `H`. Passing it
    //    through `Generalized::add` would apply the F6.4 `cos φ` of a
    //    *boat-fixed* load to it, and past 90° of heel `cos φ` changes sign —
    //    the damping would then drive the motion it is meant to resist, and
    //    the sway DOF diverges within a second. (Observed, not theorised:
    //    `tests::damping_survives_inversion` is the guard, and brief §17
    //    requires the boat to pass dynamically through `|φ| > 90°`.) At
    //    `φ = π` the boat-fixed reading would also have hull drag pushing the
    //    boat forwards, which is nonsense. See the section 04 handoff note.
    let hull = hull_loads(st, p);
    total.x += hull.load.f.x;
    total.y += hull.load.f.y;
    total.n += hull.n_yaw;
    total.k += hull.k_roll;

    // 2. centerboard
    let board = centerboard_load(st, p);
    total.add(board.load, st.phi);

    // 3. rudder
    let rudder = rudder_load(st, p);
    total.add(rudder.load, st.phi);

    // 4. sail
    let sail = sail_load(st, wind_world, p);
    total.add(sail.load, st.phi);
    let (damping, limit) = boom_passive_moments(st, p);
    let boom = BoomMoments {
        aero: sail.m_beta,
        sheet: 0.0,
        damping,
        limit,
    };

    // 5. mainsheet — zero until section 06. Both the pull on the boom and its
    //    reaction on the hull at the block land here (F6.8).
    let sheet = Load::default();
    total.add(sheet, st.phi);

    // 6. roll/hydrostatics — zero until section 07 (F6.7).
    let k_restore = 0.0;
    total.k += k_restore;

    ForceBreakdown {
        sail: sail.load,
        board: board.load,
        rudder: rudder.load,
        hull: hull.load,
        sheet,
        total,
        alpha_sail: sail.alpha,
        alpha_board: board.alpha,
        alpha_rudder: rudder.alpha,
        cl_sail: sail.cl,
        cd_sail: sail.cd,
        sheet_tension: 0.0,
        m_beta: boom.total(),
        k_restore,
        gz: 0.0,
        aw_boat: apparent_wind_cg(st, wind_world),
        tw_boat: world_to_body(wind_world, st.psi),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrator::step;
    use crate::state::{STATE_FIELDS, STATE_LEN};
    use crate::testkit::uniform_wind;

    /// Deterministic test-local generator; see the note in `state.rs`.
    struct Lcg(u64);

    impl Lcg {
        fn unit(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
        }

        fn range(&mut self, lo: f64, hi: f64) -> f64 {
            lo + self.unit() * (hi - lo)
        }
    }

    fn params() -> BoatParameters {
        BoatParameters::ilca7()
    }

    /// `½ m_x u² + ½ m_y v² + ½ I_z r² + ½ I_x p²` (task 4.5).
    fn kinetic_energy(st: &BoatState, p: &BoatParameters) -> f64 {
        let m = p.total_mass();
        0.5 * (m + p.inertia.a_x) * st.u * st.u
            + 0.5 * (m + p.inertia.a_y) * st.v * st.v
            + 0.5 * (p.inertia.i_zz + p.inertia.a_psi) * st.r * st.r
            + 0.5 * (p.inertia.i_xx + p.inertia.a_phi) * st.p * st.p
    }

    #[test]
    fn summation_order_documented() {
        // Crude by design: it makes an accidental reorder visible in review.
        let src = include_str!("mod.rs");
        let order = [
            "let hull = hull_loads(",
            "let board = centerboard_load(",
            "let rudder = rudder_load(",
            "// 4. sail",
            "// 5. mainsheet",
            "// 6. roll/hydrostatics",
        ];
        let mut at = 0usize;
        for needle in order {
            let found = src[at..]
                .find(needle)
                .unwrap_or_else(|| panic!("`{needle}` missing or out of F9.4 order"));
            at += found + needle.len();
        }
    }

    #[test]
    fn rest_equilibrium() {
        // THE headline invariant of the section (brief §35): zero wind, zero
        // velocity, neutral controls, 10 000 steps, bit-identical.
        let p = params();
        let start = crate::simulation::Simulation::initial_state(&p);
        let mut st = start;
        let c = Controls::default();
        for i in 0..10_000 {
            st = step(&st, &c, &p, &PhysicalForces, p.sim.dt, p.sim.integrator);
            // `t` is the simulation clock and `ṫ = 1` by F3, so it is the one
            // field that must change; every field the boat actually has is
            // compared bit for bit. (Same treatment as section 02's
            // `integrator::zero_force_zero_motion`.)
            let (now, then) = (st.to_array(), start.to_array());
            for (k, name) in STATE_FIELDS.iter().enumerate().take(STATE_LEN - 1) {
                assert_eq!(
                    now[k].to_bits(),
                    then[k].to_bits(),
                    "step {i}, field {name}: {} vs {}",
                    now[k],
                    then[k]
                );
            }
        }
        assert!(st.t > 0.0);
    }

    #[test]
    fn coast_down_decelerates() {
        let p = params();
        let mut st = BoatState {
            u: 4.0,
            ..crate::simulation::Simulation::initial_state(&p)
        };
        let c = Controls::default();
        let mut prev = st.u;
        for i in 0..(60.0 / p.sim.dt) as u32 {
            st = step(&st, &c, &p, &PhysicalForces, p.sim.dt, p.sim.integrator);
            assert!(st.u < prev, "step {i}: u rose from {prev} to {}", st.u);
            assert!(st.u > 0.0, "step {i}: u crossed zero to {}", st.u);
            prev = st.u;
        }
        assert!(prev < 1.0, "the boat barely slowed: u = {prev}");
    }

    #[test]
    fn energy_not_created() {
        // brief §35 dissipative behaviour. No wind, no external load.
        let p = params();
        let c = Controls::default();
        let mut rng = Lcg(0xE0E0_1111_2222_3333);
        for k in 0..50 {
            let mut st = BoatState {
                u: rng.range(-6.0, 6.0),
                v: rng.range(-2.0, 2.0),
                r: rng.range(-1.5, 1.5),
                p: rng.range(-1.5, 1.5),
                delta_r: rng.range(-p.rudder.delta_r_max, p.rudder.delta_r_max),
                ..crate::simulation::Simulation::initial_state(&p)
            };
            let mut prev = kinetic_energy(&st, &p);
            for i in 0..(5.0 / p.sim.dt) as u32 {
                st = step(&st, &c, &p, &PhysicalForces, p.sim.dt, p.sim.integrator);
                let e = kinetic_energy(&st, &p);
                assert!(
                    e <= prev + 1e-9,
                    "episode {k}, step {i}: energy rose {prev} -> {e}"
                );
                prev = e;
            }
        }
    }

    #[test]
    fn damping_survives_inversion() {
        // brief §17 requires the boat to pass dynamically through |φ| > 90°.
        // The hull terms (F6.6) are functions of the horizontal-frame
        // velocities, so the hull's contribution to `ΣX, ΣY, ΣN, ΣK` must be
        // **independent of heel** and must always oppose its own velocity.
        //
        // This is a regression guard: routing the hull load through
        // `Generalized::add` gives it the `cos φ` of a boat-fixed load, which
        // reverses every damping term past 90° of heel. The sway DOF then
        // diverges to Inf inside a second, which is how the defect was found.
        //
        // The foils are switched off by zeroing their area — `foil_force`
        // scales linearly with it — so what is left is exactly the hull.
        let mut p = params();
        p.board.section.area = 0.0;
        p.rudder.section.area = 0.0;
        p.sail.section.area = 0.0;

        let mut rng = Lcg(0x1257_0000_4444_5555);
        for k in 0..200 {
            let st = BoatState {
                u: rng.range(-6.0, 6.0),
                v: rng.range(-3.0, 3.0),
                r: rng.range(-1.5, 1.5),
                p: rng.range(-1.5, 1.5),
                ..crate::simulation::Simulation::initial_state(&p)
            };
            let upright = evaluate(&st, &Controls::default(), &p, &NoWind, 0.0).total;
            for i in 0..=24 {
                let phi = -std::f64::consts::PI + std::f64::consts::TAU * (i as f64) / 24.0;
                let heeled = BoatState { phi, ..st };
                let g = evaluate(&heeled, &Controls::default(), &p, &NoWind, 0.0).total;
                assert_eq!(g, upright, "case {k}: hull terms moved with φ = {phi}");
                assert!(g.x * st.u <= 0.0, "case {k}: surge at φ = {phi}");
                assert!(g.y * st.v <= 0.0, "case {k}: sway at φ = {phi}");
                assert!(g.n * st.r <= 0.0, "case {k}: yaw at φ = {phi}");
                assert!(g.k * st.p <= 0.0, "case {k}: roll at φ = {phi}");
            }
        }
    }

    #[test]
    fn wind_reaches_the_sail_and_breakdown() {
        let p = params();
        let st = crate::simulation::Simulation::initial_state(&p);
        let calm = evaluate(&st, &Controls::default(), &p, &NoWind, 0.0);
        let breezy = evaluate(
            &st,
            &Controls::default(),
            &p,
            &uniform_wind(5.0, 180.0),
            0.0,
        );
        assert_eq!(calm.sail.f, Vec3::ZERO);
        assert!(breezy.sail.f.length() > 0.0);
        assert!(breezy.m_beta < 0.0);
        assert_ne!(calm.total, breezy.total);
        assert_eq!(breezy.sheet, Load::default());
        assert_eq!(breezy.k_restore, 0.0);
    }
}

#[cfg(test)]
mod sail_integrated {
    use super::*;
    use crate::environment::wind::{WindConfig, WindMode};
    use crate::rng::Pcg32;
    use crate::simulation::Simulation;

    fn beam_sim(params: BoatParameters, speed: f64) -> Simulation {
        let mut sim = Simulation::new(params, 5);
        sim.set_wind(WindConfig {
            mode: WindMode::Uniform,
            speed,
            bearing_deg: 180.0,
            ..Default::default()
        });
        sim
    }

    #[test]
    fn boom_swings_free_without_sheet() {
        let mut sim = beam_sim(BoatParameters::ilca7(), 5.0);
        let mut reached = false;
        for _ in 0..2000 {
            sim.advance(1);
            reached |= sim.state().beta < -1.0;
        }
        assert!(reached, "boom never swung to leeward: {:?}", sim.state());
    }

    #[test]
    fn boat_accelerates_from_rest() {
        let mut p = BoatParameters::ilca7();
        // Test-only drag at the gooseneck slows easing; no holding spring.
        p.sail.c_beta = 1000.0;
        let mut sim = beam_sim(p, 8.0);
        sim.reset(
            BoatState {
                beta: -1.0,
                ..Simulation::initial_state(&p)
            },
            5,
        );
        let mut peak_u = 0.0_f64;
        for _ in 0..4000 {
            sim.advance(1);
            peak_u = peak_u.max(sim.state().u);
        }
        assert!(
            peak_u > 0.5,
            "peak u={peak_u}, final heel={}",
            sim.state().phi
        );
    }

    #[test]
    fn energy_bounded() {
        let p = BoatParameters::ilca7();
        let mut rng = Pcg32::seed_from_u64(504);
        for episode in 0..20 {
            let mut sim = beam_sim(p, rng.range(2.0, 10.0));
            sim.reset(
                BoatState {
                    u: rng.range(-3.0, 3.0),
                    v: rng.range(-1.0, 1.0),
                    psi: rng.range(-3.0, 3.0),
                    phi: rng.range(-1.0, 1.0),
                    r: rng.range(-0.5, 0.5),
                    p: rng.range(-0.5, 0.5),
                    beta: rng.range(-1.7, 1.7),
                    beta_dot: rng.range(-1.0, 1.0),
                    ..Simulation::initial_state(&p)
                },
                episode,
            );
            for _ in 0..12000 {
                sim.advance(1);
                let st = sim.state();
                let energy = 0.5
                    * ((p.total_mass() + p.inertia.a_x) * st.u.powi(2)
                        + (p.total_mass() + p.inertia.a_y) * st.v.powi(2)
                        + (p.inertia.i_zz + p.inertia.a_psi) * st.r.powi(2)
                        + (p.inertia.i_xx + p.inertia.a_phi) * st.p.powi(2)
                        + p.sail.i_boom * st.beta_dot.powi(2)
                        + p.sail.k_lim * (st.beta.abs() - p.sail.beta_max).max(0.0).powi(2));
                assert!(
                    st.is_finite() && energy.is_finite() && energy < 100_000.0,
                    "episode {episode}: energy={energy}, {st:?}"
                );
            }
        }
    }
}
