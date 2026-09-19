//! Native headless benchmark and golden-trajectory generator (F8.1).
//!
//! The physics core builds and runs on the host with no WASM toolchain, which
//! is what makes this binary — and later the RL work of brief §45 — possible.
//!
//! Usage:
//!
//! ```text
//! cargo run -p sailgym-bench --release -- wind      # wind-field throughput
//! cargo run -p sailgym-bench --release -- step      # simulation step rate
//! cargo run -p sailgym-bench --release              # everything
//! ```
//!
//! **Always build `--release`.** A debug build measures the optimiser, not the
//! code. The wall clock is read here and only here; the physics crate itself
//! never does (F9.1, enforced by `tests/determinism.rs`).

use std::hint::black_box;
use std::time::Instant;

use sailgym_physics::environment::wind::{ProceduralWind, WindConfig, WindMode};
use sailgym_physics::environment::WindField;
use sailgym_physics::parameters::BoatParameters;
use sailgym_physics::simulation::Simulation;

fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "all".to_string());
    println!("sailgym-bench {}", env!("CARGO_PKG_VERSION"));
    if cfg!(debug_assertions) {
        println!("WARNING: debug build — figures below are meaningless. Use --release.");
    }
    match which.as_str() {
        "wind" => wind(),
        "step" => step(),
        _ => {
            wind();
            step();
        }
    }
}

/// Section 03 task 3.2: `sample` throughput, single-threaded.
///
/// The points sweep a wide domain so the measurement is not a single cached
/// argument, and `black_box` keeps the whole loop from being optimised away.
fn wind() {
    for mode in [WindMode::Uniform, WindMode::Spatial, WindMode::Gust] {
        let cfg = WindConfig {
            mode,
            ..WindConfig::default()
        };
        let field = ProceduralWind::new(cfg, 0x5A11);

        let n: usize = 4_000_000;
        // Irrational-ish strides: successive samples never repeat a point and
        // never fall on a lattice the compiler could exploit.
        let (dx, dy, dt) = (0.618_033_988_75, 0.414_213_562_37, 0.001_732_050_81);

        // Warm up, then measure.
        let run = |count: usize| -> f64 {
            let start = Instant::now();
            let mut acc = 0.0;
            for i in 0..count {
                let f = i as f64;
                let w = field.sample(black_box(f * dx), black_box(f * dy), black_box(f * dt));
                acc += w.x + w.y;
            }
            black_box(acc);
            start.elapsed().as_secs_f64()
        };
        run(n / 10);
        let seconds = run(n);

        let per_second = n as f64 / seconds;
        println!(
            "wind sample  {mode:?}: K={:<3} {:>7.2} M calls/s  ({:.1} ns/call)",
            field.mode_count(),
            per_second / 1e6,
            seconds / n as f64 * 1e9,
        );
    }

    // The batched path the visualization actually uses (brief §19): one call
    // per frame for the whole grid.
    let field = ProceduralWind::new(WindConfig::default(), 0x5A11);
    for n in [64usize, 128] {
        let mut out = vec![0.0f32; 2 * n * n];
        let reps = 200;
        field.sample_grid(-500.0, -500.0, 8.0, 8.0, n, n, 0.0, &mut out);
        let start = Instant::now();
        for i in 0..reps {
            field.sample_grid(-500.0, -500.0, 8.0, 8.0, n, n, i as f64 * 0.016, &mut out);
        }
        let seconds = start.elapsed().as_secs_f64();
        println!(
            "wind grid    {n}x{n}: {:>7.3} ms/call  ({:.2} M nodes/s)",
            seconds / reps as f64 * 1e3,
            (reps * n * n) as f64 / seconds / 1e6,
        );
    }
}

/// Simulation throughput: how far past real time the core runs headless
/// (brief §2, §37).
fn step() {
    let params = BoatParameters::ilca7();
    let dt = params.sim.dt;
    let mut sim = Simulation::new(params, 1);
    let steps: u32 = 2_000_000;

    sim.advance(10_000);
    let start = Instant::now();
    sim.advance(steps);
    let seconds = start.elapsed().as_secs_f64();
    black_box(sim.state());

    println!(
        "sim step            : {:>7.2} M steps/s  ({:.0}x real time at dt={dt} s)",
        steps as f64 / seconds / 1e6,
        steps as f64 * dt / seconds,
    );
}
