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
//! ## The two halves of the roll moment
//!
//! `ΣK` is assembled from every load like any other generalised force: the
//! sail, the board, the rudder and the sheet all reach it through
//! `Generalized::add`, which is where the F6.4 heel geometry lives. Two terms
//! are not loads and are added directly:
//!
//! * slot 1 carries the F6.6 hull roll damping, `K_hull`;
//! * slot 6 carries the F6.7 hydrostatic righting, `K_restore = −Δ·g·GZ(φ)`.
//!
//! `stability::roll::roll_moments` reports those two together for the
//! free-decay tests and the debug panel. **Nothing sums both of its fields
//! into `ΣK`** — that would double the hull's roll damping.
//!
//! ## The mainsheet is a null force system on the hull — by construction
//!
//! Slot 5 sums **both** ends of the rope (F6.8): `F_b` on the boom at `P_b`
//! and its reaction `−F_b` on the hull at the block `P_k`. The two are equal,
//! opposite and **collinear** — `F_b` points along `P_k − P_b` — so their
//! resultant force and their resultant moment both vanish identically, and the
//! sheet's whole contribution to `ΣX, ΣY, ΣN, ΣK` is zero to rounding.
//!
//! That is the correct answer, not an accident. `P_b` and `P_k` are both fixed
//! to the boat for a given `β`, so a rigid motion of the boat cannot change
//! the rope path length: the rope can do no work on the hull degrees of
//! freedom, and its only generalised force is on `β`, where it is
//! `−T·dℓ/dβ = M_β`. Summing the boom end alone would leave a spurious force
//! and couple on the hull — that is the Newton's-third-law violation F6.8
//! warns about, and `sheet_does_no_negative_work` (invariants) is what catches
//! it: the orphaned couple pumps energy into roll. The sheet reaches heel the
//! way a real one does, through the boom angle it controls and the sail force
//! that follows.

use crate::aero::{apparent::apparent_wind_cg, sail::sail_load};
use crate::dynamics::{sheet_rate, ForceModel, Generalized, Load};
use crate::environment::WindField;
use crate::frames::world_to_body;
use crate::hydro::centerboard::centerboard_load;
use crate::hydro::hull::hull_loads;
use crate::hydro::rudder::rudder_load;
use crate::parameters::BoatParameters;
use crate::rigging::boom::{boom_passive_moments, BoomMoments};
use crate::rigging::mainsheet::sheet_output;
use crate::stability::hydrostatics::{righting_moment, GzCurve};
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
    /// Reaction on the hull at the block, `−F_b` applied at `P_k` (F6.8).
    pub sheet_hull: Load,
    pub sheet_tension: f64,
    /// m, `ℓ(β)` — the geometric rope path length. The renderer draws the rope
    /// from this and `l_sheet`; nothing is re-derived in TypeScript (F8).
    pub rope_length: f64,
    /// m, `e = ℓ − L`. Negative when the rope is slack.
    pub sheet_extension: f64,
    pub m_beta: f64,
    /// The four F6.9 boom moments, kept apart rather than only summed: brief
    /// §30 asks for the boom's loading to be inspectable, and `m_beta` is
    /// `boom.total()`.
    pub boom: BoomMoments,
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
    c: &Controls,
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

    // 5. mainsheet — both the pull on the boom and its reaction on the hull at
    //    the block (F6.8); see the module note on why their sum vanishes.
    //
    //    `L̇` comes from `dynamics::sheet_rate`, the same pure function the
    //    derivative integrates, so the damping term `ė` is consistent across
    //    every RK2 stage.
    //
    //    The pair is accumulated on its own before it joins the running total.
    //    `(0 + a) + (−a)` is exactly zero in IEEE arithmetic, while
    //    `(S + a) − a` is not `S` in general — summing the two ends straight
    //    into `total` would leave a heel- and yaw-dependent rounding residue
    //    on top of the hull terms, which `damping_survives_inversion` compares
    //    bit for bit. Nothing is cancelled by hand: both ends still go through
    //    `Generalized::add` with the F6.4 heel geometry.
    let sheet = sheet_output(st, sheet_rate(c, st, p), p);
    let mut sheet_total = Generalized::default();
    sheet_total.add(sheet.boom_load, st.phi);
    sheet_total.add(sheet.hull_load, st.phi);
    total.x += sheet_total.x;
    total.y += sheet_total.y;
    total.n += sheet_total.n;
    total.k += sheet_total.k;

    let (damping, limit) = boom_passive_moments(st, p);
    let boom = BoomMoments {
        aero: sail.m_beta,
        sheet: sheet.m_beta,
        damping,
        limit,
    };

    // 6. roll/hydrostatics — the F6.7 righting moment (section 07).
    //
    //    `GZ` is solved from the live `stability` parameters on every
    //    evaluation rather than cached, so a parameter edit (brief §31) takes
    //    effect on the next step with nothing to invalidate. It is two
    //    `sin_cos` calls and a 3×3 Cramer solve, and it keeps `evaluate` a
    //    pure function of `(state, controls, parameters, field, t)`.
    //
    //    Hull roll damping is **not** added here. It is an F6.6 term and slot
    //    1 has already contributed it; see the module note.
    let curve = GzCurve::from_params(p);
    let gz = curve.gz(st.phi);
    let k_restore = righting_moment(st.phi, &curve, p.total_mass());
    total.k += k_restore;

    ForceBreakdown {
        sail: sail.load,
        board: board.load,
        rudder: rudder.load,
        hull: hull.load,
        sheet: sheet.boom_load,
        sheet_hull: sheet.hull_load,
        total,
        alpha_sail: sail.alpha,
        alpha_board: board.alpha,
        alpha_rudder: rudder.alpha,
        cl_sail: sail.cl,
        cd_sail: sail.cd,
        sheet_tension: sheet.tension,
        rope_length: sheet.rope_length,
        sheet_extension: sheet.extension,
        m_beta: boom.total(),
        boom,
        k_restore,
        gz,
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

    /// `Δ·g·∫₀^φ GZ` — the potential energy stored in heel (F6.7).
    ///
    /// Live from section 07. Before it existed, roll had no restoring force
    /// and kinetic energy alone was a complete account of a still-air episode;
    /// it no longer is, because the boat now trades roll kinetic energy for
    /// roll potential every time it swings through upright.
    fn roll_potential(st: &BoatState, p: &BoatParameters) -> f64 {
        p.total_mass() * crate::constants::G * GzCurve::from_params(p).gz_integral(st.phi)
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
        // brief §35 dissipative behaviour. No wind, no external load. The
        // account includes the roll potential of F6.7: hydrostatic righting is
        // conservative, and a kinetic-only measure would read its return
        // stroke as energy appearing from nowhere.
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
            let mut prev = kinetic_energy(&st, &p) + roll_potential(&st, &p);
            for i in 0..(5.0 / p.sim.dt) as u32 {
                st = step(&st, &c, &p, &PhysicalForces, p.sim.dt, p.sim.integrator);
                let e = kinetic_energy(&st, &p) + roll_potential(&st, &p);
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
        // scales linearly with it — and the F6.7 righting arm by zeroing `GM`
        // and `GZ_max`, which makes every `GZ` coefficient exactly zero. What
        // is left is exactly the hull, which is what this test is about.
        // Slot 6 *is* heel-dependent, by construction; inversion is guarded
        // for it by `stability::capsize::passes_through_inversion` and by
        // `invariants::capsize_finite`.
        let mut p = params();
        p.board.section.area = 0.0;
        p.rudder.section.area = 0.0;
        p.sail.section.area = 0.0;
        p.stability.gm = 0.0;
        p.stability.gz_max = 0.0;

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

        // Hydrostatic righting is live from section 07. The initial state is
        // upright, so `GZ(0)` and the moment it produces are exactly zero —
        // and any heel at all makes both non-zero, with the F6.7 sign.
        assert_eq!(breezy.gz, 0.0);
        assert_eq!(breezy.k_restore, 0.0);
        let heeled = evaluate(
            &BoatState { phi: 0.3, ..st },
            &Controls::default(),
            &p,
            &uniform_wind(5.0, 180.0),
            0.0,
        );
        assert!(heeled.gz > 0.0);
        assert!(heeled.k_restore < 0.0);

        // The sheet is live from section 06. The default state is sheeted
        // hard in, so the rope is loaded — and its two ends cancel exactly in
        // the generalised sum (see the module note).
        assert!(breezy.sheet_tension > 0.0);
        assert_eq!(breezy.sheet.f, -breezy.sheet_hull.f);
        let mut sheet_total = Generalized::default();
        sheet_total.add(breezy.sheet, st.phi);
        sheet_total.add(breezy.sheet_hull, st.phi);
        assert_eq!(sheet_total, Generalized::default());
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

    /// A fully eased sheet: `L = l_sheet_max` is longer than `ℓ(β)` over the
    /// whole boom range, so the rope is slack and the element contributes
    /// nothing. Section 05's free-boom fixtures need it now that the sheet is
    /// live — `Simulation::initial_state` starts sheeted hard in.
    fn boom_free(p: &BoatParameters) -> BoatState {
        BoatState {
            l_sheet: p.sheet.l_sheet_max,
            ..Simulation::initial_state(p)
        }
    }

    #[test]
    fn boom_swings_free_without_sheet() {
        let p = BoatParameters::ilca7();
        let mut sim = beam_sim(p, 5.0);
        sim.reset(boom_free(&p), 5);
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
                ..boom_free(&p)
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

#[cfg(test)]
mod sheet_integrated {
    use super::*;
    use crate::dynamics::sheet_rate;
    use crate::environment::wind::{WindConfig, WindMode};
    use crate::rigging::mainsheet::sheet_output;
    use crate::simulation::Simulation;
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

    /// Wind on the starboard beam: `bearing_deg = 180` blows toward `+y`, and
    /// with `psi = 0` (`+y` to port) that puts the wind on the starboard side,
    /// so the boom blows out to port and `beta < 0` (F2.1).
    fn beam_sim(params: BoatParameters, speed: f64) -> Simulation {
        let mut sim = Simulation::new(params, 6);
        sim.set_wind(WindConfig {
            mode: WindMode::Uniform,
            speed,
            bearing_deg: 180.0,
            ..Default::default()
        });
        sim
    }

    /// The tension the derivative sees at this state and command, computed
    /// exactly the way `evaluate` computes it.
    fn tension(st: &BoatState, c: &Controls, p: &BoatParameters) -> f64 {
        sheet_output(st, sheet_rate(c, st, p), p).tension
    }

    fn haul() -> Controls {
        Controls {
            sheet_rate_cmd: -1.0,
            ..Controls::default()
        }
    }

    #[test]
    fn hauling_restrains_boom() {
        // brief §46 steps 4-6: haul, and the sheet holds the boom in.
        let p = BoatParameters::ilca7();
        let mut sim = beam_sim(p, 5.0);
        sim.reset(
            BoatState {
                beta: -1.4,
                l_sheet: p.sheet.l_sheet_max,
                ..Simulation::initial_state(&p)
            },
            6,
        );
        // Settle with the sheet fully eased: the boom blows out to leeward and
        // the rope is slack the whole time.
        for _ in 0..(2.0 / p.sim.dt) as u32 {
            sim.advance(1);
        }
        let eased = sim.state().beta;
        assert!(eased < -1.5, "boom did not blow out: {eased}");
        assert_eq!(tension(sim.state(), &Controls::default(), &p), 0.0);

        // Haul. The first half second is rope take-up, during which the rope
        // rings between taut and slack; after that it stays loaded and the
        // boom comes in without ever going back out.
        let c = haul();
        sim.set_controls(c);
        for _ in 0..(0.5 / p.sim.dt) as u32 {
            sim.advance(1);
        }
        let mut previous = sim.state().beta.abs();
        let mut steps = 0u32;
        let mut min_tension = f64::INFINITY;
        while sim.state().beta < -0.05 {
            sim.advance(1);
            let st = *sim.state();
            let t = tension(&st, &c, &p);
            min_tension = min_tension.min(t);
            assert!(t > 0.0, "step {steps}: rope went slack while hauling");
            assert!(
                st.beta.abs() <= previous,
                "step {steps}: |beta| rose {previous} -> {}",
                st.beta.abs()
            );
            previous = st.beta.abs();
            steps += 1;
            assert!(steps < 4000, "the boom never came in");
        }
        assert!(steps > 100, "the haul was over too fast to mean anything");
        assert!(min_tension > 0.0);
        assert!(
            sim.state().l_sheet < p.sheet.l_sheet_max,
            "the sheet was never hauled"
        );
    }

    #[test]
    fn release_depowers() {
        // brief §46 steps 11-14: release, tension drops, the boom swings out,
        // the sail depowers. No branch anywhere says "if released, depower".
        let p = BoatParameters::ilca7();
        let speed = 6.0;
        let wind = uniform_wind(speed, 180.0);
        let mut sim = beam_sim(p, speed);
        sim.reset(Simulation::initial_state(&p), 6);

        // Hauled and powered: sheeted hard in, the sail loaded, the rope taut
        // and a heeling moment on the boat.
        let hauled = haul();
        sim.set_controls(hauled);
        for _ in 0..(0.3 / p.sim.dt) as u32 {
            sim.advance(1);
        }
        let before = *sim.state();
        let powered = evaluate(&before, &hauled, &p, &wind, before.t);
        assert_eq!(before.l_sheet, p.sheet.l_sheet_min);
        assert!(tension(&before, &hauled, &p) > 0.0);
        assert!(
            before.beta.abs() < 0.1,
            "boom not sheeted in: {}",
            before.beta
        );
        // Heeling: the sail's own roll moment, and the boat already rolling.
        let mut heeling = Generalized::default();
        heeling.add(powered.sail, before.phi);
        assert!(
            heeling.k < -100.0,
            "sail is not heeling the boat: {heeling:?}"
        );
        assert!(before.p < 0.0, "boat is not rolling to port");

        let released = Controls {
            sheet_release: true,
            ..Controls::default()
        };
        sim.set_controls(released);
        let start = before.t;
        let mut slack_at = None;
        let mut out_at = None;
        for _ in 0..(3.0 / p.sim.dt) as u32 {
            sim.advance(1);
            let st = *sim.state();
            if slack_at.is_none() && tension(&st, &released, &p) == 0.0 {
                slack_at = Some(st.t - start);
            }
            if out_at.is_none() && st.beta.abs() > 1.0 {
                out_at = Some(st.t - start);
            }
        }
        assert!(slack_at.is_some(), "the sheet never went slack");
        assert!(out_at.is_some(), "the boom never swung out past 1 rad");

        let after = *sim.state();
        let depowered = evaluate(&after, &released, &p, &wind, after.t);
        assert!(
            depowered.sail.f.length() < 0.5 * powered.sail.f.length(),
            "sail did not depower: {} -> {}",
            powered.sail.f.length(),
            depowered.sail.f.length()
        );
    }

    #[test]
    fn clamp_respected() {
        let p = BoatParameters::ilca7();
        let mut rng = Lcg(0xC1A3_0000_2222_3333);
        for episode in 0..200 {
            let mut sim = beam_sim(p, rng.range(0.0, 8.0));
            sim.reset(
                BoatState {
                    beta: rng.range(-1.7, 1.7),
                    l_sheet: rng.range(p.sheet.l_sheet_min, p.sheet.l_sheet_max),
                    ..Simulation::initial_state(&p)
                },
                episode,
            );
            for chunk in 0..20 {
                sim.set_controls(Controls {
                    sheet_rate_cmd: rng.range(-1.5, 1.5),
                    sheet_release: rng.unit() < 0.25,
                    ..Controls::default()
                });
                for _ in 0..50 {
                    sim.advance(1);
                    let l = sim.state().l_sheet;
                    assert!(
                        l >= p.sheet.l_sheet_min && l <= p.sheet.l_sheet_max,
                        "episode {episode}, chunk {chunk}: l_sheet = {l}"
                    );
                }
            }
        }
    }

    #[test]
    fn rate_command_not_angle_command() {
        // The sheet commands `L̇`, never an angle (brief §12). Two runs with
        // the *identical* constant haul command: the commanded length history
        // is bit-identical, and the boom angle is not, because the boom
        // answers the wind.
        let p = BoatParameters::ilca7();
        let c = Controls {
            sheet_rate_cmd: -0.05,
            ..Controls::default()
        };
        let run = |speed: f64| {
            let mut sim = beam_sim(p, speed);
            sim.reset(
                BoatState {
                    l_sheet: p.sheet.l_sheet_max,
                    ..Simulation::initial_state(&p)
                },
                6,
            );
            // At rest the apparent wind is the true wind (F6.2), so doubling
            // one doubles the other.
            let apparent = apparent_wind_cg(
                sim.state(),
                uniform_wind(speed, 180.0).sample(0.0, 0.0, 0.0),
            )
            .length();
            sim.set_controls(c);
            for _ in 0..(2.0 / p.sim.dt) as u32 {
                sim.advance(1);
            }
            (apparent, *sim.state())
        };
        let (aw_slow, slow) = run(3.0);
        let (aw_fast, fast) = run(6.0);

        assert!(
            (aw_fast - 2.0 * aw_slow).abs() < 1e-12,
            "{aw_slow} -> {aw_fast}"
        );
        // A length rate knows nothing about the wind: same command, same L(t).
        assert_eq!(slow.l_sheet.to_bits(), fast.l_sheet.to_bits());
        assert!(slow.l_sheet < p.sheet.l_sheet_max);
        // The boom angle does. An angle command could not do this.
        assert!(
            (fast.beta - slow.beta).abs() > 0.1,
            "beta(2 s) barely moved with the wind: {} vs {}",
            slow.beta,
            fast.beta
        );
    }

    /// **R1 gate (F11).** 60 s of a hauled beam reach over the 3x3 grid of
    /// timestep and sheet stiffness the section PRD names. The measured table
    /// is in `docs/progress/06-handoff.md`; run with `--nocapture` to
    /// reproduce it.
    #[test]
    fn sheet_stiffness_stability() {
        let base = BoatParameters::ilca7();
        for dt in [0.01, 0.005, 0.0025] {
            for k_sheet in [1.0e4, 2.0e4, 3.0e4] {
                let mut p = base;
                p.sim.dt = dt;
                p.sheet.k_sheet = k_sheet;
                let mut sim = beam_sim(p, 5.0);
                sim.reset(Simulation::initial_state(&p), 6);
                sim.set_controls(haul());
                let mut peak_rate = 0.0_f64;
                let mut peak_tension = 0.0_f64;
                for _ in 0..(60.0 / dt) as u32 {
                    sim.advance(1);
                    let st = *sim.state();
                    assert!(
                        st.is_finite(),
                        "dt = {dt}, k_sheet = {k_sheet}: state left the reals at t = {}",
                        st.t
                    );
                    peak_rate = peak_rate.max(st.beta_dot.abs());
                    peak_tension = peak_tension.max(tension(&st, &haul(), &p));
                }
                eprintln!(
                    "R1 dt={dt} k_sheet={k_sheet:e}: peak |beta_dot| = {peak_rate:.3} rad/s, \
                     peak T = {peak_tension:.0} N, final beta = {:.4}",
                    sim.state().beta
                );
                assert!(
                    peak_rate < 50.0,
                    "dt = {dt}, k_sheet = {k_sheet}: peak |beta_dot| = {peak_rate}"
                );
            }
        }
    }
}

/// Roll, wired through the whole chain (section 07, task 7.3).
///
/// Every test here drives the **complete** `evaluate` → `derivative` →
/// `integrator` path. None of them reaches into `stability/` for a moment and
/// applies it by hand: the point is that heel emerges from loads that were
/// already being summed, through `Generalized::add` and the F6.4 geometry.
#[cfg(test)]
mod roll_integrated {
    use super::*;
    use crate::simulation::Simulation;
    use crate::state::{STATE_FIELDS, STATE_LEN};

    fn params() -> BoatParameters {
        BoatParameters::ilca7()
    }

    /// The roll moment one load contributes, through the same `add` the EOM
    /// uses. Nothing is recomputed: this is the F6.4 expression applied to a
    /// `Load` the breakdown already carries.
    fn roll_of(l: Load, phi: f64) -> f64 {
        let mut g = Generalized::default();
        g.add(l, phi);
        g.k
    }

    /// A simulation on a beam reach in a uniform wind, sheeted to `l_sheet`.
    ///
    /// Bearing 0 is a northerly; the boat starts heading east, so the wind is
    /// on the **port** beam and the boat heels to starboard. Bearing 180 puts
    /// it on the starboard beam and the heel goes the other way.
    fn beam_reach(speed: f64, bearing_deg: f64, l_sheet: f64) -> Simulation {
        let p = params();
        let mut sim = Simulation::new(p, 7);
        sim.set_wind(crate::environment::wind::WindConfig {
            mode: crate::environment::wind::WindMode::Uniform,
            speed,
            bearing_deg,
            ..Default::default()
        });
        sim.reset(
            BoatState {
                l_sheet,
                ..Simulation::initial_state(&p)
            },
            7,
        );
        sim
    }

    fn advance_seconds(sim: &mut Simulation, seconds: f64, p: &BoatParameters) {
        sim.advance((seconds / p.sim.dt) as u32);
    }

    #[test]
    fn sail_force_heels_boat() {
        // The full-chain sign test. Wind on the starboard beam pushes the boat
        // to port, so the port rail goes down: `φ < 0` (F2).
        let p = params();
        let mut sim = beam_reach(6.0, 180.0, p.sheet.l_sheet_min);
        advance_seconds(&mut sim, 6.0, &p);
        let st = *sim.state();
        assert!(st.phi < -0.1, "phi = {} rad", st.phi);

        // And the mirror: a northerly heels the boat the other way.
        let mut sim = beam_reach(6.0, 0.0, p.sheet.l_sheet_min);
        advance_seconds(&mut sim, 6.0, &p);
        assert!(sim.state().phi > 0.1, "phi = {} rad", sim.state().phi);
    }

    #[test]
    fn easing_reduces_heel() {
        // brief §46 steps 11–16 and §47, as a causal chain rather than an
        // outcome. A regression that produced the right heel for the wrong
        // reason — a rule that reads the sheet command, say — would still
        // fail here, because the three links have to happen **in order**:
        //
        //     sheet tension → 0   ⇒   |β| grows   ⇒   sail heeling moment falls
        //
        // and only then does the heel come off.
        let p = params();
        let mut sim = beam_reach(7.0, 0.0, 2.0);
        advance_seconds(&mut sim, 40.0, &p);

        let start = *sim.state();
        let f0 = evaluate(&start, sim.controls(), &p, sim.wind(), start.t);
        let k_sail_0 = roll_of(f0.sail, start.phi);
        assert!(start.phi > 0.3, "not heeled to begin with: {}", start.phi);
        assert!(f0.sheet_tension > 0.0, "the rope was not loaded");
        assert!(k_sail_0 > 0.0, "the sail was not heeling the boat");

        sim.set_controls(Controls {
            sheet_release: true,
            ..Default::default()
        });

        let (mut slack_at, mut boom_at, mut unloaded_at) = (None, None, None);
        for _ in 0..(20.0 / p.sim.dt) as u32 {
            sim.advance(1);
            let st = *sim.state();
            let f = evaluate(&st, sim.controls(), &p, sim.wind(), st.t);
            if slack_at.is_none() && f.sheet_tension == 0.0 {
                slack_at = Some(st.t);
            }
            if boom_at.is_none() && (st.beta - start.beta).abs() > 0.3 {
                boom_at = Some(st.t);
            }
            if unloaded_at.is_none() && roll_of(f.sail, st.phi).abs() < 0.5 * k_sail_0 {
                unloaded_at = Some(st.t);
            }
        }

        let slack = slack_at.expect("the sheet never went slack");
        let boom = boom_at.expect("the boom never moved out");
        let unloaded = unloaded_at.expect("the sail never depowered");
        assert!(
            slack < boom && boom < unloaded,
            "out of order: tension 0 at {slack}, boom out at {boom}, sail unloaded at {unloaded}"
        );
        assert!(
            sim.state().phi.abs() < 0.2 * start.phi,
            "heel did not come off: {} -> {}",
            start.phi,
            sim.state().phi
        );
    }

    #[test]
    fn couple_from_sail_and_board() {
        // A geometry check that catches a sign error in `board_pos_b.z`. The
        // sail force acts **above** the CG and the board's reaction **below**
        // it, and they point opposite ways, so their roll moments have the
        // same sign: together they are the heeling couple, not a pair that
        // cancels.
        let p = params();
        for bearing in [0.0, 180.0] {
            let mut sim = beam_reach(6.0, bearing, 2.0);
            advance_seconds(&mut sim, 30.0, &p);
            let st = *sim.state();
            let f = evaluate(&st, sim.controls(), &p, sim.wind(), st.t);
            let sail = roll_of(f.sail, st.phi);
            let board = roll_of(f.board, st.phi);
            assert!(sail.abs() > 1.0 && board.abs() > 1.0, "nothing loaded");
            assert!(
                sail * board > 0.0,
                "bearing {bearing}: sail K = {sail}, board K = {board}"
            );
        }
    }

    #[test]
    fn rest_equilibrium() {
        // `forces::tests::rest_equilibrium` (section 04) still holds with roll
        // live — `GZ(0)` is exactly zero, so slot 6 adds exactly `-0.0` and
        // rest stays rest, bit for bit.
        let p = params();
        let start = Simulation::initial_state(&p);
        let mut st = start;
        let c = Controls::default();
        for i in 0..10_000 {
            st = crate::integrator::step(&st, &c, &p, &PhysicalForces, p.sim.dt, p.sim.integrator);
            let (now, then) = (st.to_array(), start.to_array());
            for (k, name) in STATE_FIELDS.iter().enumerate().take(STATE_LEN - 1) {
                assert_eq!(
                    now[k].to_bits(),
                    then[k].to_bits(),
                    "step {i}, field {name}"
                );
            }
        }

        // And it is a *stable* equilibrium now: released from a heel with no
        // wind, the boat comes back to upright instead of staying put.
        let mut st = BoatState { phi: 0.35, ..start };
        for _ in 0..10_000 {
            st = crate::integrator::step(&st, &c, &p, &PhysicalForces, p.sim.dt, p.sim.integrator);
        }
        assert!(st.phi.abs() < 1e-3, "did not return upright: {}", st.phi);
    }
}
