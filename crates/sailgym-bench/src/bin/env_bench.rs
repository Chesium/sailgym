//! What does an **episode** cost? (v2 section 06 task 6.6.)
//!
//! ```text
//! cargo run --release -p sailgym-bench --bin env_bench -- --write docs/v2/throughput.md
//! ```
//!
//! Section 02's `vec_bench` measured bare `Simulation`s and said so in its
//! own "what this does not measure": it could not see the cost of building
//! an observation, of an agent's decision cadence or of the course layer,
//! because none of those existed. This re-measures the same sweep on
//! `sailgym-env::VecEnv`, so `discussions/cross-stack.md` §0's question is
//! answered against the thing that will actually run.
//!
//! # It measures both arms in one process
//!
//! The ratio that matters is `VecEnv` against a bare `Simulation`, and a
//! ratio taken across two runs on two days is not a ratio. So this binary
//! runs **both**: the bare-`Simulation` loop `vec_bench` times, and the
//! episode loop, at the same N, on the same thread pool, in the same
//! process. The tables `vec_bench` wrote stay above; the ratio below is
//! measured here.
//!
//! # And it proves the parallel path before it reports a figure
//!
//! For every N and every thread count the parallel run's final states are
//! compared with the serial run's using `to_bits()`, exactly as `vec_bench`
//! does and exactly as `sailgym-env`'s own test does — a single differing
//! bit fails the run before a number is printed (F16.5, RV35).
//!
//! # This document is generated, and one command drops this section
//!
//! `vec_bench --write docs/v2/throughput.md` rewrites the **whole** file
//! from its own template and would drop everything below the marker. That
//! is the same hazard section 03 recorded for `conformance.md`, handled the
//! same way: the section is delimited, this binary replaces it in place
//! rather than appending a second copy, and
//! `sailgym_env::vec_env::tests::the_throughput_document_still_carries_the_env_section`
//! fails in gate step 3 when it has gone missing.

use std::fmt::Write as _;
use std::hint::black_box;
use std::time::Instant;

use sailgym_env::episode::{manual_source, Episode, EpisodeConfig};
use sailgym_env::outcome::AutoresetMode;
use sailgym_env::vec_env::VecEnv;
// Re-exported by `sailgym-env` so a consumer of `VecEnv::new` needs no
// second dependency for the one type its signature mentions.
use sailgym_env::Cadence;

use sailgym_physics::scenario::load_shipped;
use sailgym_physics::simulation::Simulation;
use sailgym_physics::state::STATE_LEN;

/// The delimiters. `vec_bench` knows nothing about them, which is exactly
/// why a regeneration by it drops what is between them.
const BEGIN: &str = "<!-- BEGIN sailgym-env throughput (section 06) -->";
const END: &str = "<!-- END sailgym-env throughput (section 06) -->";

/// The same populations `vec_bench` swept, so the two halves of the
/// document are comparable row for row.
const POPULATIONS: [usize; 5] = [1, 8, 64, 512, 4096];

/// Roughly how many physics steps each (N, threads) point should do.
const TOTAL_STEPS: u64 = 2_000_000;

/// The floor on per-boat steps, so the largest population still runs long
/// enough per boat for rayon's granularity not to dominate.
const MIN_STEPS: u64 = 500;

/// The decision period: 20 Hz over 200 Hz physics, F14.6's own example.
const PERIOD_STEPS: u32 = 10;

/// A second, sparser cadence, run serially at every N.
///
/// Two points at two cadences are what turn "the observation is the smaller
/// half" from an assertion into arithmetic: the per-step cost is common to
/// both and the per-decision cost is ten times smaller in the sparse one, so
/// the two measured rates separate them.
const SPARSE_PERIOD_STEPS: u32 = 100;

/// `gybe` — the only shipped scenario with a `Gust` field, so a per-slot
/// seed produces a genuinely different field and a genuinely different
/// trajectory. Agreement between the serial and parallel runs is then a
/// test rather than a symmetry. It is also `vec_bench`'s scenario, which is
/// what makes the ratio a ratio.
const SCENARIO: &str = "gybe";

struct Point {
    threads: usize,
    seconds: f64,
    steps: u64,
    decisions: u64,
}

impl Point {
    fn steps_per_second(&self) -> f64 {
        (self.steps as f64) / self.seconds
    }
    fn decisions_per_second(&self) -> f64 {
        (self.decisions as f64) / self.seconds
    }
}

fn value(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
        .filter(|t| !t.trim().is_empty())
}

fn thread_counts() -> Vec<usize> {
    let all = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let mut out = Vec::new();
    let mut t = 1usize;
    while t < all {
        out.push(t);
        t *= 2;
    }
    out.push(all);
    out.dedup();
    out
}

fn seeds(n: usize) -> Vec<u64> {
    let base = load_shipped(SCENARIO).expect("a shipped scenario").seed;
    (0..n as u64).map(|i| base.wrapping_add(i)).collect()
}

/// The configuration the sweep measures.
///
/// No recording and no decision log: the log allocates per decision and
/// the recorder builds a `Diagnostics`, and neither is part of the
/// question "what does a step cost". Both are named in the document so the
/// figure is not read as including them.
fn config() -> EpisodeConfig {
    let mut cfg = EpisodeConfig::new(load_shipped(SCENARIO).expect("a shipped scenario"));
    cfg.autoreset = AutoresetMode::Disabled;
    cfg.log_hz = None;
    cfg.log_decisions = false;
    cfg
}

/// N bare `Simulation`s, one per seed — `vec_bench`'s subject, rebuilt here
/// so the ratio is measured in one process.
fn build_bare(n: usize) -> Vec<Simulation> {
    let sc = load_shipped(SCENARIO).expect("a shipped scenario");
    let params = sc.to_parameters().expect("a catalogue");
    seeds(n)
        .into_iter()
        .map(|seed| {
            let mut own = sc.clone();
            own.seed = seed;
            let mut sim = Simulation::new(params, seed);
            sim.load_scenario(&own).expect("the scenario loads");
            sim
        })
        .collect()
}

fn env_states(env: &VecEnv) -> Vec<[f64; STATE_LEN]> {
    (0..env.len())
        .map(|i| env.episode(i).expect("a slot").state().to_array())
        .collect()
}

fn assert_identical(what: &str, a: &[[f64; STATE_LEN]], b: &[[f64; STATE_LEN]]) {
    assert_eq!(a.len(), b.len(), "{what}: population changed");
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        for k in 0..STATE_LEN {
            assert_eq!(
                x[k].to_bits(),
                y[k].to_bits(),
                "{what}: boat {i}, field {}: {} vs {}",
                sailgym_physics::state::STATE_FIELDS[k],
                x[k],
                y[k]
            );
        }
    }
}

/// One action per slot, held for the run. A stub, not a controller: no
/// controller exists yet, and the document says so.
fn actions(n: usize, dim: usize) -> Vec<f64> {
    let mut out = vec![0.0; n * dim];
    for (i, a) in out.chunks_mut(dim).enumerate() {
        a[0] = ((i % 7) as f64 / 6.0) - 0.5;
        a[1] = -1.0;
        a[2] = -1.0;
    }
    out
}

/// Step a `VecEnv` for `calls` decision periods and return the wall time.
fn time_env(env: &mut VecEnv, calls: usize, parallel: bool) -> f64 {
    env.set_parallel(parallel);
    let n = env.len();
    let a = actions(n, env.action_dim());
    let mut obs = vec![0.0f32; n * env.obs_len()];
    let mut rewards = vec![0.0f64; n];
    let mut terminated = vec![0u8; n];
    let mut truncated = vec![0u8; n];
    let start = Instant::now();
    for _ in 0..calls {
        env.step_all(&a, &mut obs, &mut rewards, &mut terminated, &mut truncated)
            .expect("a valid batch step");
    }
    let elapsed = start.elapsed().as_secs_f64();
    black_box(obs.first().copied());
    elapsed
}

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("WARNING: debug build — every figure below is meaningless. Use --release.");
    }
    let total_steps: u64 = value("--total-steps")
        .and_then(|s| s.parse().ok())
        .unwrap_or(TOTAL_STEPS);
    let threads = thread_counts();
    println!("env_bench: scenario {SCENARIO}, cadence {PERIOD_STEPS}, threads {threads:?}");

    let sc = load_shipped(SCENARIO).expect("a shipped scenario");
    let dt = sc.to_parameters().expect("a catalogue").sim.dt;
    let cadence = Cadence::new(PERIOD_STEPS);

    // Warm up: page in the code and leave the first-touch cost behind.
    {
        let mut warm = VecEnv::new(config(), cadence, &seeds(8)).expect("a valid batch");
        time_env(&mut warm, 20, false);
        black_box(warm.len());
    }

    let obs_len = {
        let probe = Episode::new(config(), manual_source(cadence), 1).expect("a valid episode");
        probe.layout().len()
    };

    let mut rows: Vec<Row> = Vec::new();
    for n in POPULATIONS {
        let per_boat = (total_steps / (n as u64)).max(MIN_STEPS);
        let calls = (per_boat / u64::from(PERIOD_STEPS)).max(1);
        let steps_per_boat = calls * u64::from(PERIOD_STEPS);
        let steps = steps_per_boat * (n as u64);
        let decisions = calls * (n as u64);

        // --- the bare-`Simulation` baseline, in this same process -------
        let mut sims = build_bare(n);
        let start = Instant::now();
        for s in &mut sims {
            s.advance(u32::try_from(steps_per_boat).expect("fits"));
        }
        let bare = Point {
            threads: 0,
            seconds: start.elapsed().as_secs_f64(),
            steps,
            decisions: 0,
        };

        // --- the episode runner ----------------------------------------
        let mut serial_env = VecEnv::new(config(), cadence, &seeds(n)).expect("a valid batch");
        let seconds = time_env(&mut serial_env, calls as usize, false);
        let reference = env_states(&serial_env);
        let serial = Point {
            threads: 0,
            seconds,
            steps,
            decisions,
        };

        // The same work at a tenth of the decision rate, serially.
        let sparse_period = u64::from(SPARSE_PERIOD_STEPS);
        let sparse_calls = (steps_per_boat / sparse_period).max(1);
        let mut sparse_env = VecEnv::new(config(), Cadence::new(SPARSE_PERIOD_STEPS), &seeds(n))
            .expect("a valid batch");
        let sparse_seconds = time_env(&mut sparse_env, sparse_calls as usize, false);
        let sparse = Point {
            threads: 0,
            seconds: sparse_seconds,
            steps: sparse_calls * sparse_period * (n as u64),
            decisions: sparse_calls * (n as u64),
        };

        let mut points = Vec::new();
        for &t in &threads {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(t)
                .build()
                .expect("a rayon pool");
            let mut env = VecEnv::new(config(), cadence, &seeds(n)).expect("a valid batch");
            let seconds = pool.install(|| time_env(&mut env, calls as usize, true));
            // F16.5's argument, turned into a fact, on the real runner.
            assert_identical(
                &format!("N = {n}, {t} threads"),
                &reference,
                &env_states(&env),
            );
            println!(
                "  N = {n:<5} {t:>3} threads  {:>10.0} steps/s  {:>9.0} decisions/s",
                (steps as f64) / seconds,
                (decisions as f64) / seconds
            );
            points.push(Point {
                threads: t,
                seconds,
                steps,
                decisions,
            });
        }
        println!(
            "  N = {n:<5} serial       {:>10.0} steps/s  bare Simulation {:>10.0} steps/s",
            serial.steps_per_second(),
            bare.steps_per_second()
        );
        rows.push(Row {
            n,
            steps_per_boat,
            calls,
            bare,
            serial,
            sparse,
            points,
        });
    }

    println!("env_bench: every parallel run was bit-identical to its serial baseline.");

    if let Some(path) = value("--write") {
        let section = document(&rows, &threads, dt, obs_len);
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let merged = splice(&existing, &section);
        std::fs::write(&path, &merged).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert!(
            std::fs::read_to_string(&path)
                .unwrap_or_default()
                .contains(BEGIN),
            "the env section did not land in {path}"
        );
        eprintln!("env_bench: wrote the env section of {path}");
    } else {
        eprintln!("env_bench: pass `--write docs/v2/throughput.md` to regenerate the section.");
    }
}

struct Row {
    n: usize,
    steps_per_boat: u64,
    calls: u64,
    bare: Point,
    serial: Point,
    sparse: Point,
    points: Vec<Point>,
}

/// Replace the delimited section, or append it after a rule.
fn splice(document: &str, section: &str) -> String {
    match (document.find(BEGIN), document.find(END)) {
        (Some(a), Some(b)) if b > a => {
            let mut out = String::with_capacity(document.len() + section.len());
            out.push_str(&document[..a]);
            out.push_str(section);
            out.push_str(&document[b + END.len()..]);
            out
        }
        _ => format!("{}\n\n---\n\n{section}\n", document.trim_end()),
    }
}

fn document(rows: &[Row], threads: &[usize], dt: f64, obs_len: usize) -> String {
    let mut d = String::new();
    let _ = writeln!(d, "{BEGIN}\n");
    let _ = writeln!(d, "## The episode runner (section 06)\n");
    let _ = writeln!(
        d,
        "Generated by `cargo run --release -p sailgym-bench --bin env_bench -- --write \
         docs/v2/throughput.md`. **Do not edit by hand.** This section is delimited: \
         `env_bench` replaces it in place, and a regeneration of this file by \
         `vec_bench --write` **drops it**, because `vec_bench` rewrites the whole document \
         from its own template. `sailgym-env`'s `vec_env` tests fail in gate step 3 when it \
         has gone missing — the same arrangement section 03 uses for `conformance.md`.\n"
    );
    let _ = writeln!(
        d,
        "The tables above measure bare `Simulation`s and say in as many words that they \
         cannot see the cost of an observation, a decision cadence or the course layer. \
         This section measures `sailgym_env::VecEnv`: sensors, observation, agent, \
         actuation funnel, `Outcome` evaluated **after every physics step**, and the \
         mark-passage probe when a route is configured. The bare baseline is **re-measured \
         in this same process**, at the same N on the same pool, so the ratio below is a \
         ratio and not a comparison across two runs.\n"
    );
    let _ = writeln!(
        d,
        "| | |\n|---|---|\n| Cadence | {} steps ({} Hz at dt = {dt} s) |\n\
         | Observation | {obs_len} columns, the tier-0 suite |\n\
         | Route | none — `Outcome` still evaluated every step |\n\
         | Recording / decision log | off; both are measured nowhere in this table |\n\
         | Second cadence | {SPARSE_PERIOD_STEPS} steps, serial, so the per-step and \
         per-decision costs can be separated |\n\
         | Action source | `manual`, one held action per slot — **not** a controller |\n",
        PERIOD_STEPS,
        (1.0 / (dt * f64::from(PERIOD_STEPS))).round(),
    );
    let _ = writeln!(
        d,
        "The host block is the one above; nothing about the machine changed between the \
         two sweeps. Thread counts swept: {threads:?}.\n"
    );

    for row in rows {
        let one = row
            .points
            .iter()
            .find(|p| p.threads == 1)
            .map(Point::steps_per_second)
            .unwrap_or(f64::NAN);
        let _ = writeln!(
            d,
            "### N = {} ({} steps per boat, {} decisions per boat)\n",
            row.n, row.steps_per_boat, row.calls
        );
        let _ = writeln!(
            d,
            "| Threads | M steps/s | k decisions/s | × real time | Efficiency |"
        );
        let _ = writeln!(d, "|---|---|---|---|---|");
        let _ = writeln!(
            d,
            "| bare `Simulation`, serial | {:.3} | — | {:.0}× | — |",
            row.bare.steps_per_second() / 1.0e6,
            (row.bare.steps as f64) * dt / row.bare.seconds
        );
        let _ = writeln!(
            d,
            "| `VecEnv`, serial (no rayon) | {:.3} | {:.1} | {:.0}× | — |",
            row.serial.steps_per_second() / 1.0e6,
            row.serial.decisions_per_second() / 1.0e3,
            (row.serial.steps as f64) * dt / row.serial.seconds
        );
        let _ = writeln!(
            d,
            "| `VecEnv`, serial, cadence {SPARSE_PERIOD_STEPS} | {:.3} | {:.1} | {:.0}× | — |",
            row.sparse.steps_per_second() / 1.0e6,
            row.sparse.decisions_per_second() / 1.0e3,
            (row.sparse.steps as f64) * dt / row.sparse.seconds
        );
        for p in &row.points {
            let _ = writeln!(
                d,
                "| `VecEnv`, {} | {:.3} | {:.1} | **{:.0}×** | {:.2} |",
                p.threads,
                p.steps_per_second() / 1.0e6,
                p.decisions_per_second() / 1.0e3,
                (p.steps as f64) * dt / p.seconds,
                p.steps_per_second() / (one * p.threads as f64),
            );
        }
        let _ = writeln!(d);
    }

    let last = rows.last().expect("at least one population");
    let best = last
        .points
        .iter()
        .max_by(|a, b| {
            a.steps_per_second()
                .partial_cmp(&b.steps_per_second())
                .expect("finite")
        })
        .expect("at least one point");
    let ratio = best.steps_per_second() / last.bare.steps_per_second();
    let serial_ratio = last.serial.steps_per_second() / last.bare.steps_per_second();

    let _ = writeln!(d, "### The answer, on the real runner\n");
    let _ = writeln!(
        d,
        "At the largest population, **N = {}**, the best point is **{} threads** at \
         **{:.3} M steps/s** and **{:.1} k agent-decisions/s**, which is **{:.0}× real \
         time** in aggregate.\n",
        last.n,
        best.threads,
        best.steps_per_second() / 1.0e6,
        best.decisions_per_second() / 1.0e3,
        (best.steps as f64) * dt / best.seconds,
    );
    // Two cadences, one arithmetic. `t_step` is the per-physics-step cost
    // the two share; `t_obs` is the cost of one decision. Solving
    //   1/r10  = t_step + t_obs/10
    //   1/r100 = t_step + t_obs/100
    // separates them, which is why the sparse arm is measured at all.
    let inv10 = 1.0 / last.serial.steps_per_second();
    let inv100 = 1.0 / last.sparse.steps_per_second();
    let p = f64::from(PERIOD_STEPS);
    let q = f64::from(SPARSE_PERIOD_STEPS);
    let t_obs = (inv10 - inv100) / (1.0 / p - 1.0 / q);
    let t_step = inv10 - t_obs / p;
    let t_bare = 1.0 / last.bare.steps_per_second();

    let _ = writeln!(
        d,
        "The bare-`Simulation` baseline measured in the same process at the same N reads \
         **{:.3} M steps/s** on one thread, so `VecEnv` runs at **{:.2}×** the bare rate \
         per step ({:.3} against {:.3} M steps/s serial) — a step costs **{:.2}×** what a \
         bare one costs. Across all threads the runner reaches **{:.2}×** the \
         single-threaded bare figure.\n",
        last.bare.steps_per_second() / 1.0e6,
        serial_ratio,
        last.serial.steps_per_second() / 1.0e6,
        last.bare.steps_per_second() / 1.0e6,
        1.0 / serial_ratio,
        ratio,
    );
    let _ = writeln!(
        d,
        "**Where that cost is**, from the two cadences rather than from an argument. \
         Solving the two serial rates for a per-step cost and a per-decision cost gives \
         **{:.0} ns per physics step** and **{:.0} ns per decision**, against \
         **{:.0} ns** for a bare step. At cadence {PERIOD_STEPS} the decision contributes \
         {:.0} ns per step, which is **{:.0} %** of the difference: the rest is the \
         per-step loop itself. `Outcome` is evaluated after **every** physics step, so \
         the episode calls `Simulation::advance(1)` rather than `advance(period)` — and \
         `advance` refreshes its cached force breakdown once per call, so a per-step loop \
         pays one extra force evaluation per step on top of RK2's two. That is what buys \
         a capsize resolved on the step it happens and a mark passage tested at the \
         physics rate rather than at the sample rate.\n",
        t_step * 1.0e9,
        t_obs * 1.0e9,
        t_bare * 1.0e9,
        t_obs / p * 1.0e9,
        100.0 * (t_obs / p) / (inv10 - t_bare),
    );
    let _ = writeln!(
        d,
        "### What this section does not measure\n\n\
         - **No controller.** The action source is `manual` with one held action per slot. \
         A rule sailor or a policy adds its own cost and none exists yet.\n\
         - **No recording and no decision log.** Both were switched off; the decision log \
         allocates per decision and the recorder builds a `Diagnostics`.\n\
         - **No route.** `Outcome` is evaluated every step either way, but the \
         mark-passage probe and the guidance sensor's course arithmetic are not in these \
         figures.\n\
         - **No Python.** Section 07 measures the binding, separately (F17.5).\n\
         - **One machine, one scenario.** As above.\n"
    );
    let _ = writeln!(d, "{END}");
    d
}
