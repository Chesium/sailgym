#![allow(dead_code)]
//! The time-step convergence study: scripts, runner, error norms and the
//! order estimate (section 10, task 10.2).
//!
//! brief §35 asks for the same scenario at `dt`, `dt/2` and `dt/4` and a
//! demonstration of numerical convergence. This file is the one definition of
//! how that is measured, `#[path]`-included by its two consumers exactly as
//! section 09 shares the golden control scripts:
//!
//! * `crates/sailgym-physics/tests/convergence.rs` — asserts the order and the
//!   absolute error at the default timestep;
//! * `crates/sailgym-bench/src/bin/convergence.rs` — prints the full table and
//!   writes `docs/convergence.md`.
//!
//! ## What the script had to avoid, and why that is not tuning
//!
//! Two parts of the model are deliberately `dt`-dependent, and both would cap
//! the observed order at **one** if a convergence script let them run:
//!
//! 1. **Rudder self-centring.** With no steering command the tiller returns at
//!    a constant rate and therefore cannot land on zero; it limit-cycles
//!    within `±delta_r_return_rate·dt` (`dynamics::rudder_rate` says so in as
//!    many words). That is an `O(dt)` oscillation injected straight into the
//!    yaw moment. The script keeps a non-zero rudder command at **every**
//!    instant, so self-centring never engages.
//! 2. **Actuator saturation events.** A clamp is exact once reached — section
//!    02 proved `delta_r` lands on `delta_r_max` to 1e-12 at any `dt` — but the
//!    *moment* of arrival is only `O(dt)` accurate. Arriving is unavoidable and
//!    costs `O(dt²)` in the state, which is second order and therefore fine;
//!    what is avoided is a script that sits on a clamp boundary.
//!
//! Neither choice touches a coefficient (brief §43). They are choices about
//! what the *input signal* is, and a convergence study that measured the
//! self-centring limit cycle would be measuring a known first-order artefact
//! rather than the integrator.

use sailgym_physics::frames::wrap_pi;
use sailgym_physics::integrator::Integrator;
use sailgym_physics::parameters::BoatParameters;
use sailgym_physics::scenario::load_shipped;
use sailgym_physics::simulation::Simulation;
use sailgym_physics::state::{BoatState, Controls};

/// The three scenarios task 10.2 names.
pub const SCENARIOS: [&str; 3] = ["close_hauled", "beam_reach_capsize", "gybe"];

/// Simulated seconds each run covers.
pub const HORIZON_S: f64 = 20.0;

/// The `dt` sweep. Each is half the one before, and every one divides every
/// cue time exactly, so the control input is identical in all five runs.
pub const TEST_DTS: [f64; 4] = [0.01, 0.005, 0.0025, 0.00125];

/// The reference timestep task 10.2 pins: `Rk4` at a quarter of the finest
/// `dt` in the sweep.
///
/// A fourth-order method at a quarter of the step carries roughly `4⁻⁴ ≈ 0.4 %`
/// of the finest RK2 run's error, so the comparison measures RK2 and not the
/// reference. That was checked rather than assumed: re-running the whole study
/// against `Rk4` at `dt = 7.8125e-5` (a quarter of this again) moves every
/// observed order by less than 0.01 and every error by less than 1 %.
pub const REF_DT: f64 = 0.000_312_5;

/// The shipped default (F7), the one the acceptance bound is stated at.
pub const DEFAULT_DT: f64 = 0.005;

/// A control change, applied at the start of the step at time `t`.
#[derive(Clone, Copy, Debug)]
pub struct Cue {
    pub t: f64,
    pub rudder_rate: f64,
    pub sheet_rate: f64,
}

/// The fixed 20 s script, used unchanged for all three scenarios.
///
/// The rudder command alternates sign and is **never zero**, so the tiller
/// sweeps under command rather than self-centring (see the module note). The
/// magnitude is chosen so the sweep stays inside `±0.334 rad`, comfortably
/// short of `delta_r_max = 0.698`: a boat pinned on the rudder stop for twenty
/// seconds spins in place at half a knot, which exercises the actuator clamp
/// and very little else.
///
/// The sheet is hauled for four seconds and eased for three, gently enough
/// that a scenario starting mid-range never reaches a stop. One that starts
/// *on* a stop — `beam_reach_capsize` begins at `l_sheet_min` — has its length
/// held there exactly by F4.3's clamp at every `dt` in the sweep.
///
/// Cue times are whole seconds, so `t/dt` is an integer for every timestep in
/// the sweep including the reference, and the five runs see an identical input.
pub const SCRIPT: [Cue; 6] = [
    Cue {
        t: 0.0,
        rudder_rate: -0.08,
        sheet_rate: 0.0,
    },
    Cue {
        t: 2.0,
        rudder_rate: 0.08,
        sheet_rate: 0.0,
    },
    Cue {
        t: 6.0,
        rudder_rate: -0.08,
        sheet_rate: -0.1,
    },
    Cue {
        t: 10.0,
        rudder_rate: 0.08,
        sheet_rate: 0.0,
    },
    Cue {
        t: 14.0,
        rudder_rate: -0.08,
        sheet_rate: 0.1,
    },
    Cue {
        t: 17.0,
        rudder_rate: 0.08,
        sheet_rate: 0.0,
    },
];

/// One `(dt, error)` row of the study (task 10.2's published struct).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConvergenceResult {
    pub dt: f64,
    pub err_pos: f64,
    pub err_psi: f64,
    pub err_phi: f64,
}

/// Run one scenario to `horizon` at `dt` with `method`, and return the final
/// state.
///
/// The scenario supplies the parameters, so `dt` and the integrator are
/// installed **after** it loads. `advance` is called in whole-cue chunks rather
/// than one step at a time: `Simulation::advance(n)` refreshes its cached force
/// breakdown once per call, and per-step calls would double the work for a
/// value nothing here reads. F9.7 makes the two identical in the state.
pub fn run(scenario: &str, dt: f64, method: Integrator, horizon: f64) -> BoatState {
    let sc = load_shipped(scenario).expect("a shipped scenario");
    let base = sc.to_parameters().expect("valid scenario parameters");
    let mut sim = Simulation::new(base, sc.seed);
    sim.load_scenario(&sc).expect("the scenario loads");
    sim.set_parameters(BoatParameters {
        sim: sailgym_physics::parameters::SimParams {
            dt,
            integrator: method,
        },
        ..base
    })
    .expect("dt and integrator are a valid catalogue");

    let steps_to = |t: f64| (t / dt).round() as u64;
    let end = steps_to(horizon);
    let mut done = 0u64;
    for (i, cue) in SCRIPT.iter().enumerate() {
        let at = steps_to(cue.t);
        if at >= end {
            break;
        }
        debug_assert!(at >= done, "cue {i} is out of order");
        advance_to(&mut sim, &mut done, at);
        sim.set_controls(Controls {
            rudder_rate_cmd: cue.rudder_rate,
            sheet_rate_cmd: cue.sheet_rate,
            sheet_release: false,
        });
    }
    advance_to(&mut sim, &mut done, end);
    *sim.state()
}

fn advance_to(sim: &mut Simulation, done: &mut u64, target: u64) {
    while *done < target {
        let chunk = (target - *done).min(u32::MAX as u64) as u32;
        sim.advance(chunk);
        *done += chunk as u64;
    }
}

/// The `Rk4` reference trajectory's final state at `horizon`.
pub fn reference(scenario: &str, horizon: f64) -> BoatState {
    run(scenario, REF_DT, Integrator::Rk4, horizon)
}

/// Error of one RK2 run against a reference state.
///
/// `psi` and `beta` are wrapped to `(−π, π]` by F3, so their difference is
/// taken through `wrap_pi`: a run either side of the branch cut differs by
/// `2π` in the stored number and by nothing at all in the boat. `phi` is
/// deliberately **not** wrapped (F3) and is subtracted plainly, which is what
/// lets the heel error stay meaningful through a capsize.
pub fn error_against(state: &BoatState, reference: &BoatState, dt: f64) -> ConvergenceResult {
    ConvergenceResult {
        dt,
        err_pos: ((state.x - reference.x).powi(2) + (state.y - reference.y).powi(2)).sqrt(),
        err_psi: wrap_pi(state.psi - reference.psi).abs(),
        err_phi: (state.phi - reference.phi).abs(),
    }
}

/// The whole sweep for one scenario, against its own `Rk4` reference.
pub fn study(scenario: &str, horizon: f64) -> Vec<ConvergenceResult> {
    let reference = reference(scenario, horizon);
    TEST_DTS
        .iter()
        .map(|&dt| {
            error_against(
                &run(scenario, dt, Integrator::Rk2Midpoint, horizon),
                &reference,
                dt,
            )
        })
        .collect()
}

/// Least-squares slope of `ln(error)` against `ln(dt)`: the observed order of
/// accuracy, measured on the **position** error.
///
/// A least-squares fit over the whole sweep rather than a single Richardson
/// ratio between two rows, because one pair reports the local slope — including
/// whatever asymptotic bias the coarsest step still carries — while the fit
/// uses every point the study paid for.
pub fn observed_order(results: &[ConvergenceResult]) -> f64 {
    observed_order_of(results, |r| r.err_pos)
}

/// [`observed_order`] for a chosen error component.
pub fn observed_order_of(
    results: &[ConvergenceResult],
    err: impl Fn(&ConvergenceResult) -> f64,
) -> f64 {
    let points: Vec<(f64, f64)> = results
        .iter()
        // A zero or non-finite error has no logarithm. It would mean the run
        // reproduced the reference exactly, which cannot happen for a method
        // of a different order, so dropping such a point is a guard and not a
        // filter on inconvenient data.
        .filter_map(|r| {
            let e = err(r);
            (e > 0.0 && e.is_finite() && r.dt > 0.0).then(|| (r.dt.ln(), e.ln()))
        })
        .collect();
    if points.len() < 2 {
        return f64::NAN;
    }
    let n = points.len() as f64;
    let mean_x = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_y = points.iter().map(|p| p.1).sum::<f64>() / n;
    let num: f64 = points.iter().map(|p| (p.0 - mean_x) * (p.1 - mean_y)).sum();
    let den: f64 = points.iter().map(|p| (p.0 - mean_x).powi(2)).sum();
    if den == 0.0 {
        return f64::NAN;
    }
    num / den
}
