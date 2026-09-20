//! The systematic symmetry and rotation harness (section 10, task 10.3).
//!
//! brief §35 asks for two things this file proves together:
//!
//! * **Coordinate-frame consistency** — "rotating the complete physical setup
//!   in world coordinates should rotate the resulting trajectory without
//!   changing intrinsic dynamics";
//! * **Port/starboard mirror symmetry** — "mirroring initial conditions and
//!   inputs should produce an appropriately mirrored trajectory".
//!
//! `tests/invariants.rs` already asserts both on one hand-picked fixture each.
//! This promotes them to a sweep: **6 shipped scenarios × 8 world rotations ×
//! {unmirrored, mirrored} = 96 cases**, each run for 20 s under a scripted
//! control sequence and compared at **every** step.
//!
//! ## Rotating the *complete* setup, including the wind
//!
//! The wind field is the part that makes this non-trivial. `ProceduralWind` in
//! `Spatial` or `Gust` mode draws its wavenumbers, phases and frequencies from
//! the seed in **world coordinates** (F6.1), so it is not rotationally
//! symmetric: turning the boat while leaving the field alone is not rotating
//! the setup, it is putting the boat somewhere else in the same field.
//!
//! The honest fix is to rotate the field too, which [`Transformed`] does by
//! wrapping any `WindField`:
//!
//! ```text
//! W_T(p, t) = R_θ · M · W_inner( M · R_(−θ) · p , t )
//! ```
//!
//! That is the same field seen from a rotated (and optionally reflected) set of
//! world axes — the transformation brief §35 describes, applied to the
//! environment as well as to the boat. It means `gybe`, the one shipped
//! scenario with a gusty field, is swept on its own wind rather than on a
//! uniform stand-in.
//!
//! Gate step 4 runs this target.

use sailgym_physics::environment::wind::ProceduralWind;
use sailgym_physics::environment::WindField;
use sailgym_physics::forces::WindForces;
use sailgym_physics::frames::wrap_pi;
use sailgym_physics::integrator::step;
use sailgym_physics::parameters::BoatParameters;
use sailgym_physics::scenario::{load_shipped, shipped_names};
use sailgym_physics::state::{BoatState, Controls};
use sailgym_physics::testkit::{mirror_controls, mirror_state};
use sailgym_physics::vec::Vec2;

/// Simulated seconds per case.
const HORIZON_S: f64 = 20.0;

/// The eight world rotations, in degrees. Multiples of 45° so the sweep
/// includes the axis-aligned cases (where `sin`/`cos` are exact) and the
/// diagonal ones (where they are not), rather than only one kind.
const ROTATIONS_DEG: [f64; 8] = [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0];

/// Body-frame quantities must be **unchanged** by a world rotation, to here,
/// over the **early** window — every step before the script first releases the
/// sheet, which is the first instant any case crosses the mainsheet's take-up
/// boundary.
///
/// This is **v1's bound, unchanged**, and it is the one that carries the content
/// of the invariant: nothing in this window has crossed the take-up boundary
/// yet, so a genuine frame error cannot hide behind amplified round-off.
/// Measured worst residual over the window, across all 96 cases: **7.3e-13**, a
/// margin of ≈ 14.
const TOL_BODY_EARLY: f64 = 1e-11;

/// Body-frame quantities must be unchanged by a world rotation to here over the
/// **whole** 20 s horizon.
///
/// v1 asserted 1e-11 over the whole horizon and measured 1.34e-12
/// (`docs/v1/progress/10-handoff.md`). **v2 section 08 raised it to 1e-10, and
/// the reason is recorded rather than assumed.** F18.1b made the mainsheet's tension discontinuous at take-up, so
/// a trajectory that works the sheet is no longer Lipschitz in its state: two
/// runs that differ only in the last bits — which is what rotating the world by
/// 45° does — can land on opposite sides of `e = 0` at a step and diverge from
/// there. The measurement says exactly that and nothing more:
///
/// | window | worst `|Δ|` |
/// |---|---|
/// | before the release cue (11 s, all 96 cases) | 7.3e-13 |
/// | whole horizon, the four scenarios that settle | ≤ 5.1e-12 |
/// | whole horizon, `beam_reach_capsize` / `sheet_release_recovery` | **2.6e-11** |
///
/// The two that exceed 1e-11 are the two that start two-blocked at
/// `l_sheet_min`, i.e. **on** the take-up boundary, and their excursions are
/// confined to the steps after the release and haul cues. 1e-10 is four times
/// the worst measurement. `TOL_BODY_EARLY` is what keeps the file honest: v1's
/// bound is still asserted, unchanged, over the first 11 s of all 96 cases,
/// which is where a frame error would show.
const TOL_BODY: f64 = 1e-10;

/// World position must rotate by exactly the applied angle, to here.
const TOL_WORLD: f64 = 1e-9;

/// The end of the early window, in simulated seconds: the first cue that
/// releases the sheet, which is the first thing in the script that can take the
/// rope across `e = 0`. Derived from [`SCRIPT`] rather than written down, so
/// moving a cue moves the window with it.
fn early_horizon_s() -> f64 {
    SCRIPT
        .iter()
        .find(|c| c.release)
        .map(|c| c.t)
        .expect("the symmetry script must release the sheet at least once")
}

/// Every quantity of a mirrored case must match the F2 mirror map, to here.
const TOL_MIRROR: f64 = 1e-9;

/// A control change, applied at the start of the step at time `t`.
struct Cue {
    t: f64,
    rudder_rate: f64,
    sheet_rate: f64,
    release: bool,
}

/// The scripted 20 s input, identical for every scenario.
///
/// It steers both ways, hauls, eases and uses the release command, so each case
/// exercises the rudder, the mainsheet and the boom — a sweep driven by neutral
/// controls would prove symmetry of a boat that was barely doing anything.
const SCRIPT: [Cue; 6] = [
    Cue {
        t: 0.0,
        rudder_rate: 0.0,
        sheet_rate: -0.6,
        release: false,
    },
    Cue {
        t: 3.0,
        rudder_rate: 0.5,
        sheet_rate: 0.0,
        release: false,
    },
    Cue {
        t: 7.0,
        rudder_rate: -0.7,
        sheet_rate: 0.0,
        release: false,
    },
    Cue {
        t: 11.0,
        rudder_rate: 0.0,
        sheet_rate: 0.0,
        release: true,
    },
    Cue {
        t: 14.0,
        rudder_rate: 0.3,
        sheet_rate: -0.4,
        release: false,
    },
    Cue {
        t: 18.0,
        rudder_rate: 0.0,
        sheet_rate: 0.0,
        release: false,
    },
];

/// A `WindField` seen from rotated, optionally reflected, world axes.
///
/// `mirror` reflects about the world `x` axis, which is the world-frame image
/// of `testkit::mirror_state`'s `y → −y`. The reflection is applied **before**
/// the rotation, so the composed map is `R_θ ∘ M` on both the sample point and
/// the returned velocity.
struct Transformed<'a> {
    inner: &'a dyn WindField,
    sin: f64,
    cos: f64,
    mirror: bool,
}

impl<'a> Transformed<'a> {
    fn new(inner: &'a dyn WindField, theta: f64, mirror: bool) -> Self {
        let (sin, cos) = theta.sin_cos();
        Self {
            inner,
            sin,
            cos,
            mirror,
        }
    }

    /// The forward map `R_θ ∘ M`, applied to a point or to a vector.
    fn forward(&self, x: f64, y: f64) -> Vec2 {
        let y = if self.mirror { -y } else { y };
        Vec2::new(x * self.cos - y * self.sin, x * self.sin + y * self.cos)
    }

    /// Its inverse, `M ∘ R_(−θ)` (`M` is its own inverse).
    fn inverse(&self, x: f64, y: f64) -> Vec2 {
        let (rx, ry) = (x * self.cos + y * self.sin, -x * self.sin + y * self.cos);
        Vec2::new(rx, if self.mirror { -ry } else { ry })
    }
}

impl WindField for Transformed<'_> {
    fn sample(&self, x: f64, y: f64, t: f64) -> Vec2 {
        let q = self.inverse(x, y);
        let w = self.inner.sample(q.x, q.y, t);
        self.forward(w.x, w.y)
    }

    fn sample_grid(
        &self,
        x0: f64,
        y0: f64,
        dx: f64,
        dy: f64,
        nx: usize,
        ny: usize,
        t: f64,
        out: &mut [f32],
    ) {
        // Node by node through `sample`, so the transformed field cannot
        // disagree with itself. Nothing in this test uses it; it exists
        // because the F6.1 trait requires it.
        for j in 0..ny {
            for i in 0..nx {
                let w = self.sample(x0 + i as f64 * dx, y0 + j as f64 * dy, t);
                let k = 2 * (j * nx + i);
                out[k] = w.x as f32;
                out[k + 1] = w.y as f32;
            }
        }
    }
}

/// The transformed image of a state: mirror first, then rotate.
fn transform_state(st: &BoatState, sin: f64, cos: f64, mirror: bool) -> BoatState {
    let m = if mirror { mirror_state(st) } else { *st };
    BoatState {
        x: m.x * cos - m.y * sin,
        y: m.x * sin + m.y * cos,
        psi: wrap_pi(m.psi + sin.atan2(cos)),
        ..m
    }
}

/// One case's whole trajectory, sampled at every step.
fn run_case(
    scenario: &str,
    params: &BoatParameters,
    start: BoatState,
    field: &dyn WindField,
    mirror: bool,
) -> Vec<BoatState> {
    let model = WindForces { wind: field };
    let dt = params.sim.dt;
    let steps = (HORIZON_S / dt).round() as u64;
    let cue_steps: Vec<u64> = SCRIPT.iter().map(|c| (c.t / dt).round() as u64).collect();

    let mut st = start;
    let mut controls = Controls::default();
    let mut next = 0usize;
    let mut out = Vec::with_capacity(steps as usize);
    for i in 0..steps {
        while next < SCRIPT.len() && cue_steps[next] == i {
            let c = &SCRIPT[next];
            let base = Controls {
                rudder_rate_cmd: c.rudder_rate,
                sheet_rate_cmd: c.sheet_rate,
                sheet_release: c.release,
            };
            controls = if mirror { mirror_controls(&base) } else { base };
            next += 1;
        }
        st = step(&st, &controls, params, &model, dt, params.sim.integrator);
        assert!(
            st.is_finite(),
            "{scenario}: non-finite state at step {i} of a symmetry case"
        );
        out.push(st);
    }
    out
}

#[test]
fn rotation_and_mirror_sweep() {
    let mut cases = 0usize;
    let mut worst_body = 0.0f64;
    let mut worst_early = 0.0f64;
    let mut worst_world = 0.0f64;
    let mut worst_mirror = 0.0f64;
    let early_horizon = early_horizon_s();
    // Every case must be a case: a scenario that never moved would pass any
    // symmetry test at all.
    let mut moved = 0usize;

    for scenario in shipped_names() {
        let sc = load_shipped(scenario).expect("a shipped scenario");
        let params = sc.to_parameters().expect("valid scenario parameters");
        let inner = ProceduralWind::new(sc.wind, sc.seed);
        let start = sc.to_boat_state();

        let baseline = run_case(scenario, &params, start, &inner, false);
        let last = baseline.last().expect("a non-empty trajectory");
        if (last.x - start.x).hypot(last.y - start.y) > 1.0 {
            moved += 1;
        }

        for theta_deg in ROTATIONS_DEG {
            let theta = theta_deg.to_radians();
            let (sin, cos) = theta.sin_cos();
            for mirror in [false, true] {
                cases += 1;
                let field = Transformed::new(&inner, theta, mirror);
                let got = run_case(
                    scenario,
                    &params,
                    transform_state(&start, sin, cos, mirror),
                    &field,
                    mirror,
                );
                assert_eq!(got.len(), baseline.len());

                for (i, (a, b)) in baseline.iter().zip(got.iter()).enumerate() {
                    // The case identifies itself in every message: a bare
                    // assertion failure in a 96-case sweep is not debuggable
                    // (task 10.3).
                    let who = format!(
                        "{scenario} @ {theta_deg:.0}° {}, step {i}",
                        if mirror { "mirrored" } else { "upright" }
                    );
                    let want = transform_state(a, sin, cos, mirror);

                    // 1. Body-frame quantities: unchanged by a rotation,
                    //    negated (or not) by a mirror. Never rotated.
                    //
                    //    Two bounds, not one: the tight `TOL_BODY_EARLY` over
                    //    the window before the script first releases the sheet,
                    //    and the measured `TOL_BODY` over the whole horizon.
                    //    See the constants for why.
                    let early = (i as f64) * params.sim.dt < early_horizon;
                    let body_tol = if mirror {
                        TOL_MIRROR
                    } else if early {
                        TOL_BODY_EARLY
                    } else {
                        TOL_BODY
                    };
                    for (got_v, want_v, name) in [
                        (b.u, want.u, "u"),
                        (b.v, want.v, "v"),
                        (b.r, want.r, "r"),
                        (b.p, want.p, "p"),
                        (b.beta, want.beta, "beta"),
                        (b.beta_dot, want.beta_dot, "beta_dot"),
                        (b.delta_r, want.delta_r, "delta_r"),
                        (b.phi, want.phi, "phi"),
                        (b.l_sheet, want.l_sheet, "l_sheet"),
                    ] {
                        let e = (got_v - want_v).abs();
                        if mirror {
                            worst_mirror = worst_mirror.max(e);
                        } else {
                            worst_body = worst_body.max(e);
                            if early {
                                worst_early = worst_early.max(e);
                            }
                        }
                        assert!(
                            e < body_tol,
                            "{who}: {name} = {got_v}, expected {want_v} (|Δ| = {e:.3e} > {body_tol:.0e})"
                        );
                    }

                    // 2. World position: rotated by exactly the applied angle.
                    let e = (b.x - want.x).abs().max((b.y - want.y).abs());
                    if mirror {
                        worst_mirror = worst_mirror.max(e);
                    } else {
                        worst_world = worst_world.max(e);
                    }
                    assert!(
                        e < TOL_WORLD,
                        "{who}: position ({}, {}), expected ({}, {}) (|Δ| = {e:.3e})",
                        b.x,
                        b.y,
                        want.x,
                        want.y
                    );

                    // 3. Heading: `wrap_pi` because a rotation can push `psi`
                    //    across the branch cut, which is a change in the
                    //    stored number and not in the boat.
                    let e = wrap_pi(b.psi - want.psi).abs();
                    if mirror {
                        worst_mirror = worst_mirror.max(e);
                    } else {
                        worst_world = worst_world.max(e);
                    }
                    assert!(
                        e < TOL_WORLD,
                        "{who}: psi = {}, expected {} (|Δ| = {e:.3e})",
                        b.psi,
                        want.psi
                    );
                }
            }
        }
    }

    assert_eq!(cases, 96, "the sweep must be 6 scenarios × 8 rotations × 2");
    assert_eq!(
        moved, 6,
        "only {moved} of 6 scenarios travelled more than a metre"
    );
    // The early window has to be a window, or the tight bound is asserting
    // nothing; and it has to have produced a measurable residual, or the
    // comparison is between two identical runs.
    assert!(early_horizon > 0.0 && early_horizon < HORIZON_S);
    assert!(
        worst_early > 0.0,
        "no rotated case differed from its baseline at all in the early window; \
         the sweep is comparing a trajectory with itself"
    );
    eprintln!(
        "symmetry: {cases} cases, worst |Δ| — body {worst_body:.3e} (tol {TOL_BODY:.0e}), \
         body over the first {early_horizon} s {worst_early:.3e} (tol {TOL_BODY_EARLY:.0e}), \
         world {worst_world:.3e} (tol {TOL_WORLD:.0e}), mirror {worst_mirror:.3e} \
         (tol {TOL_MIRROR:.0e})"
    );
}
