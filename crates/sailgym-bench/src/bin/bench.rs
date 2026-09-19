//! Headless performance benchmark (section 10, task 10.4; brief §37).
//!
//! ```text
//! cargo run --release -p sailgym-bench --bin bench -- --scenario close_hauled --seconds 600
//! cargo run --release -p sailgym-bench --bin bench -- --all
//! ```
//!
//! **Always build `--release`.** A debug build measures the optimiser, not the
//! code; the binary says so on stderr if it finds itself unoptimised.
//!
//! brief §37 asks for "at least approximately 100× real time for a single
//! environment as an aspirational prototype benchmark", and adds that the exact
//! number is secondary to correctness and architecture. So this prints the
//! factor **and** a per-subsystem breakdown, because a bare multiple tells the
//! next person nothing about where to look.
//!
//! ## How the breakdown is obtained, and what it is not
//!
//! It is an attribution, not a sampling profiler. The whole run is timed; then
//! the three subsystems are timed separately over states drawn from that same
//! run, at the call counts the run actually made:
//!
//! * `Rk2Midpoint` evaluates the derivative **twice** per step (F4.3), and each
//!   evaluation samples the wind once and evaluates three foils once, so the
//!   per-step call counts are `2 × wind`, `2 × (sail + board + rudder)` and
//!   `2 × evaluate`.
//! * "integration" is the remainder: `total − 2 × evaluate`, i.e. the RK2 stage
//!   arithmetic, the actuator clamps, the angle wrap and the capsize observer.
//!
//! The parts are measured in isolation, so they see warmer caches than they do
//! inside the loop and the shares should be read as ± a few per cent rather
//! than as an exact decomposition. What they are good for is the thing brief
//! §37 asks of them: telling the next optimisation where the time is.
//!
//! The wall clock is read here and only here; the physics crate never reads one
//! (F9.1, enforced by `tests/determinism.rs::no_wall_clock`).

use std::hint::black_box;
use std::time::Instant;

use sailgym_physics::environment::WindField;
use sailgym_physics::forces::evaluate;
use sailgym_physics::hydro::{centerboard::centerboard_load, rudder::rudder_load};
use sailgym_physics::scenario::{load_shipped, shipped_names};
use sailgym_physics::simulation::Simulation;
use sailgym_physics::state::BoatState;

/// States sampled out of the real trajectory and cycled through by the
/// microbenchmarks, so the parts are timed on the states the whole was.
const SAMPLES: usize = 4096;

struct Args {
    scenario: String,
    seconds: f64,
    all: bool,
}

fn parse_args() -> Args {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| -> Option<String> {
        argv.iter()
            .position(|a| a == flag)
            .and_then(|i| argv.get(i + 1).cloned())
    };
    Args {
        scenario: value("--scenario").unwrap_or_else(|| "close_hauled".to_string()),
        seconds: value("--seconds")
            .and_then(|s| s.parse().ok())
            .unwrap_or(600.0),
        all: argv.iter().any(|a| a == "--all"),
    }
}

fn main() {
    let args = parse_args();
    println!("sailgym bench {}", env!("CARGO_PKG_VERSION"));
    if cfg!(debug_assertions) {
        eprintln!("WARNING: debug build — every figure below is meaningless. Use --release.");
    }

    let scenarios: Vec<String> = if args.all {
        shipped_names().iter().map(|s| (*s).to_string()).collect()
    } else {
        vec![args.scenario.clone()]
    };

    for scenario in scenarios {
        bench(&scenario, args.seconds);
    }
}

/// One scenario, run twice.
///
/// Task 10.4 requires the benchmark to be deterministic: "two runs report
/// identical step counts". Step counts alone would be a weak claim — they are
/// `seconds / dt` by construction — so the two runs are also compared **on the
/// final state, bit for bit**, which is the property brief §34 actually cares
/// about and which a non-deterministic core would break.
fn bench(scenario: &str, seconds: f64) {
    let sc = load_shipped(scenario).unwrap_or_else(|e| panic!("{scenario}: {e}"));
    let params = sc
        .to_parameters()
        .unwrap_or_else(|e| panic!("{scenario}: {e}"));
    let dt = params.sim.dt;
    let steps = (seconds / dt).round() as u32;

    let build = || {
        let mut sim = Simulation::new(params, sc.seed);
        sim.load_scenario(&sc).expect("the scenario loads");
        sim
    };

    // Warm up: page in the code and let the boat leave its initial transient,
    // so the timed run measures the steady state and not the first tenth of a
    // second.
    let mut warm = build();
    warm.advance(2_000);
    black_box(warm.state());

    let timed = |sim: &mut Simulation| -> f64 {
        let start = Instant::now();
        let taken = sim.advance(steps);
        let elapsed = start.elapsed().as_secs_f64();
        assert_eq!(taken, steps, "advance() must take the steps it is given");
        elapsed
    };

    let mut a = build();
    let seconds_a = timed(&mut a);
    let mut b = build();
    let seconds_b = timed(&mut b);

    assert_eq!(a.steps(), b.steps(), "{scenario}: step counts differ");
    for (i, (x, y)) in a
        .state()
        .to_array()
        .iter()
        .zip(b.state().to_array().iter())
        .enumerate()
    {
        assert_eq!(
            x.to_bits(),
            y.to_bits(),
            "{scenario}: two runs diverged in field {i} ({x} vs {y})"
        );
    }

    let best = seconds_a.min(seconds_b);
    let per_second = steps as f64 / best;
    let real_time = steps as f64 * dt / best;

    println!();
    println!("--- {scenario} — {seconds} s simulated, dt = {dt} s, {steps} steps ---");
    println!("  runs           : {seconds_a:.4} s and {seconds_b:.4} s wall; identical step counts ({}) and bit-identical final states", a.steps());
    println!(
        "  throughput     : {:.3} M steps/s   ({:.0}x real time)",
        per_second / 1e6,
        real_time
    );
    println!("  per step       : {:.1} ns", best / steps as f64 * 1e9);

    breakdown(scenario, &sc, steps, best);
}

/// Per-subsystem attribution: wind sampling, foil evaluation, whole-model
/// force assembly, and the integration remainder.
fn breakdown(
    scenario: &str,
    sc: &sailgym_physics::scenario::Scenario,
    steps: u32,
    total_seconds: f64,
) {
    let params = sc.to_parameters().expect("valid scenario parameters");
    let wind = sailgym_physics::environment::wind::ProceduralWind::new(sc.wind, sc.seed);

    // Collect states from the real trajectory. Timing the parts on a single
    // frozen state would measure one branch of a stalled foil rather than the
    // mix the run actually saw.
    let mut sim = Simulation::new(params, sc.seed);
    sim.load_scenario(sc).expect("the scenario loads");
    let stride = (steps as usize / SAMPLES).max(1);
    let mut states: Vec<BoatState> = Vec::with_capacity(SAMPLES);
    for i in 0..steps {
        sim.advance(1);
        if (i as usize).is_multiple_of(stride) && states.len() < SAMPLES {
            states.push(*sim.state());
        }
    }
    assert!(!states.is_empty());

    let controls = sc.to_controls();
    // RK2 midpoint evaluates the derivative twice per step (F4.3).
    let evals = 2 * steps as usize;

    let time = |label: &str, mut body: Box<dyn FnMut(&BoatState)>| -> (String, f64) {
        // Warm up over one pass, then measure over `evals` calls, cycling the
        // sampled states.
        for st in &states {
            body(st);
        }
        let start = Instant::now();
        for i in 0..evals {
            body(&states[i % states.len()]);
        }
        (label.to_string(), start.elapsed().as_secs_f64())
    };

    let w = &wind;
    let (_, wind_s) = time(
        "wind",
        Box::new(move |st| {
            black_box(w.sample(black_box(st.x), black_box(st.y), black_box(st.t)));
        }),
    );

    let p = params;
    let (_, foil_s) = time(
        "foil",
        Box::new(move |st| {
            let wind_world = sailgym_physics::vec::Vec2::new(4.0, 1.0);
            black_box(sailgym_physics::aero::sail::sail_load(st, wind_world, &p));
            black_box(centerboard_load(st, &p));
            black_box(rudder_load(st, &p));
        }),
    );

    let c = controls;
    let (_, eval_s) = time(
        "evaluate",
        Box::new(move |st| {
            black_box(evaluate(st, &c, &p, w, st.t));
        }),
    );

    let integration_s = (total_seconds - eval_s).max(0.0);
    let pct = |s: f64| 100.0 * s / total_seconds;

    println!("  breakdown, attributed over {evals} derivative evaluations:");
    println!(
        "    wind sampling        {:>8.3} s  {:>5.1} %   ({:.1} ns/call)",
        wind_s,
        pct(wind_s),
        wind_s / evals as f64 * 1e9
    );
    println!(
        "    foil evaluation      {:>8.3} s  {:>5.1} %   ({:.1} ns/call, sail + board + rudder)",
        foil_s,
        pct(foil_s),
        foil_s / evals as f64 * 1e9
    );
    println!(
        "    whole force model    {:>8.3} s  {:>5.1} %   ({:.1} ns/call, `forces::evaluate`, includes both rows above)",
        eval_s,
        pct(eval_s),
        eval_s / evals as f64 * 1e9
    );
    println!(
        "    integration, rest    {:>8.3} s  {:>5.1} %   (stages, clamps, wrap, capsize observer)",
        integration_s,
        pct(integration_s)
    );
    let other = (eval_s - wind_s - foil_s).max(0.0);
    println!(
        "    of the force model: hull, mainsheet, hydrostatics and assembly {:>6.3} s  {:>5.1} %",
        other,
        pct(other)
    );
    let _ = scenario;
}
