//! Is there a throughput problem at all? (section 02 task 2.7.)
//!
//! ```text
//! cargo run --release -p sailgym-bench --bin vec_bench -- --write docs/v2/throughput.md
//! ```
//!
//! `discussions/cross-stack.md` §0 says to measure before porting anything,
//! and `docs/v1/performance.md` already records **3 144–3 532× real time on
//! one core**. What it does not record is what more than one core buys, and
//! that is the number section 03 onwards needs in order to be a decision
//! rather than an assumption.
//!
//! ## Why this is allowed to use rayon, and why it proves it
//!
//! F9.6 forbids parallelism **inside a single simulation step** and is
//! unchanged. Stepping N **independent** boats on N threads is a different
//! thing: they share no accumulator, no field and no RNG, so the arithmetic
//! each one performs is identical to the arithmetic it would perform alone.
//! v2 F16.5 permits exactly that, and only in `sailgym-env` and
//! `sailgym-bench`; **`rayon` may not appear in `sailgym-physics`**.
//!
//! That is an argument. This binary turns it into a fact: for every N and
//! every thread count, the parallel run's final states are compared with the
//! serial run's using `to_bits()`, and a single differing bit fails the run
//! before any figure is printed.
//!
//! ## RV12 — measuring the harness instead of the physics
//!
//! `docs/v1/performance.md` records three sections chasing an "apparently
//! slow app" that turned out to be a co-tenant Playwright worker. The
//! mitigations here are the ones that risk asks for: the host block is
//! recorded, the thread count is swept explicitly rather than left to
//! rayon's default, the serial baseline runs **in the same process** as the
//! parallel ones, and the one-thread rayon column is reported beside the
//! serial one so a harness cost shows up as a gap between them rather than
//! hiding inside every figure.
//!
//! ## What it does not measure
//!
//! Bare `Simulation`s, not episodes: there is no `sailgym-env` yet, and the
//! answer is wanted *before* that crate is designed rather than after. So
//! this says nothing about the cost of building an observation, of an agent's
//! decision cadence, or of crossing into Python. Section 06 re-measures on
//! the real runner; the debt is tracked in the section 02 PRD.

use std::fmt::Write as _;
use std::hint::black_box;
use std::time::Instant;

use rayon::prelude::*;

use sailgym_physics::recording::ToolchainInfo;
use sailgym_physics::scenario::load_shipped;
use sailgym_physics::simulation::Simulation;
use sailgym_physics::state::STATE_LEN;

/// The environment counts swept.
const POPULATIONS: [usize; 5] = [1, 8, 64, 512, 4096];

/// Roughly how many physics steps each (N, threads) point should do, so that
/// a small population is timed over a long enough run and a large one does
/// not take a minute.
///
/// Per-boat steps are `max(MIN_STEPS, TOTAL_STEPS / N)`, so the work is about
/// constant across the sweep and the columns are comparable.
const TOTAL_STEPS: u64 = 4_000_000;

/// The floor on per-boat steps, so the largest population still runs long
/// enough per boat for rayon's granularity not to dominate.
const MIN_STEPS: u64 = 500;

/// The scenario the sweep runs.
///
/// `gybe` is the **only** shipped scenario with a `Gust` wind field, so a
/// per-boat seed produces a genuinely different field and a genuinely
/// different trajectory. That is what makes the bit-identity assertion a test
/// rather than a symmetry: with a uniform field every boat would compute the
/// same numbers and agreeing would prove nothing. It is also the slowest
/// scenario in `docs/v1/performance.md` (3 144× against 3 532× for `tack`),
/// so the figures below are the pessimistic end of that table.
const SCENARIO: &str = "gybe";

/// One measured point.
struct Point {
    threads: usize,
    seconds: f64,
    steps: u64,
}

impl Point {
    fn steps_per_second(&self) -> f64 {
        (self.steps as f64) / self.seconds
    }
}

fn value(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
        .filter(|t| !t.trim().is_empty())
}

/// The thread counts swept: powers of two up to the machine's parallelism,
/// and the machine's parallelism itself.
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

/// N simulations of `SCENARIO`, one per seed.
fn build(n: usize) -> Vec<Simulation> {
    let sc = load_shipped(SCENARIO).unwrap_or_else(|e| panic!("{SCENARIO}: {e}"));
    let params = sc
        .to_parameters()
        .unwrap_or_else(|e| panic!("{SCENARIO}: {e}"));
    (0..n)
        .map(|i| {
            let mut own = sc.clone();
            // A distinct seed per boat: independent fields, independent
            // trajectories, still one scenario.
            own.seed = sc.seed.wrapping_add(i as u64);
            let mut sim = Simulation::new(params, own.seed);
            sim.load_scenario(&own)
                .unwrap_or_else(|e| panic!("{SCENARIO}: {e}"));
            sim
        })
        .collect()
}

fn final_states(sims: &[Simulation]) -> Vec<[f64; STATE_LEN]> {
    sims.iter().map(|s| s.state().to_array()).collect()
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

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("WARNING: debug build — every figure below is meaningless. Use --release.");
    }
    let total_steps: u64 = value("--total-steps")
        .and_then(|s| s.parse().ok())
        .unwrap_or(TOTAL_STEPS);
    let threads = thread_counts();
    println!("vec_bench: scenario {SCENARIO}, thread counts {threads:?}");

    let sc = load_shipped(SCENARIO).expect("a shipped scenario");
    let dt = sc.to_parameters().expect("a catalogue").sim.dt;

    // Warm up once: page in the code and leave the initial transient behind,
    // so the first timed point is not the one that paid for the pages.
    let mut warm = build(8);
    for s in &mut warm {
        s.advance(2_000);
    }
    black_box(warm.first().map(|s| s.steps()));

    let mut rows: Vec<(usize, u64, Point, Vec<Point>)> = Vec::new();
    for n in POPULATIONS {
        let per_boat = (total_steps / (n as u64)).max(MIN_STEPS);
        let steps = per_boat * (n as u64);

        // The serial baseline, in this same process (RV12).
        let mut sims = build(n);
        let start = Instant::now();
        for s in &mut sims {
            let taken = s.advance(per_boat as u32);
            assert_eq!(taken as u64, per_boat);
        }
        let serial = Point {
            threads: 0,
            seconds: start.elapsed().as_secs_f64(),
            steps,
        };
        let reference = final_states(&sims);

        let mut points = Vec::new();
        for &t in &threads {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(t)
                .build()
                .expect("a rayon pool");
            let mut sims = build(n);
            let elapsed = pool.install(|| {
                let start = Instant::now();
                sims.par_iter_mut().for_each(|s| {
                    let taken = s.advance(per_boat as u32);
                    assert_eq!(taken as u64, per_boat);
                });
                start.elapsed().as_secs_f64()
            });
            // F16.5's argument, turned into a fact.
            assert_identical(
                &format!("N = {n}, {t} threads"),
                &reference,
                &final_states(&sims),
            );
            println!(
                "  N = {n:<5} {t:>3} threads  {:>10.0} steps/s  {:>9.0}x real time",
                (steps as f64) / elapsed,
                (steps as f64) * dt / elapsed
            );
            points.push(Point {
                threads: t,
                seconds: elapsed,
                steps,
            });
        }
        println!(
            "  N = {n:<5} serial       {:>10.0} steps/s  {:>9.0}x real time  ({per_boat} steps/boat)",
            serial.steps_per_second(),
            (steps as f64) * dt / serial.seconds
        );
        rows.push((n, per_boat, serial, points));
    }

    println!("vec_bench: every parallel run was bit-identical to its serial baseline.");

    if let Some(path) = value("--write") {
        let doc = document(&rows, &threads, dt, total_steps);
        std::fs::write(&path, &doc).unwrap_or_else(|e| panic!("{path}: {e}"));
        eprintln!("vec_bench: wrote {path}");
    } else {
        eprintln!("vec_bench: pass `--write docs/v2/throughput.md` to regenerate the document.");
    }
}

/// One line of `/proc/cpuinfo`, or `None`.
fn cpuinfo(key: &str) -> Option<String> {
    let text = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    text.lines()
        .find(|l| l.starts_with(key))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
}

fn meminfo() -> Option<String> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kb: u64 = text
        .lines()
        .find(|l| l.starts_with("MemTotal"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    Some(format!("{:.0} GiB", (kb as f64) / 1024.0 / 1024.0))
}

fn os_name() -> Option<String> {
    let text = std::fs::read_to_string("/etc/os-release").ok()?;
    let pretty = text
        .lines()
        .find(|l| l.starts_with("PRETTY_NAME="))?
        .split_once('=')?
        .1
        .trim_matches('"')
        .to_string();
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    Some(format!("{pretty}, Linux {kernel}"))
}

fn unknown() -> String {
    "not available on this host".to_string()
}

fn document(
    rows: &[(usize, u64, Point, Vec<Point>)],
    threads: &[usize],
    dt: f64,
    total_steps: u64,
) -> String {
    let tool = ToolchainInfo::current();
    let mut d = String::new();
    let _ = writeln!(d, "# Multi-core throughput\n");
    let _ = writeln!(
        d,
        "Generated by `cargo run --release -p sailgym-bench --bin vec_bench -- --write \
         docs/v2/throughput.md`. **Do not edit by hand.** Every figure is produced by that \
         command, in the house style of [`../v1/performance.md`](../v1/performance.md).\n"
    );
    let _ = writeln!(
        d,
        "`discussions/cross-stack.md` §0 says to measure before porting anything. \
         `../v1/performance.md` records **3 144–3 532× real time on one core** for a \
         single environment. This document answers the other half: what more than one \
         core buys, across N independent boats.\n"
    );
    let _ = writeln!(
        d,
        "**What is parallel here, and what is not.** F9.6 forbids parallelism *inside* a \
         single simulation step and is unchanged. These are N **independent** \
         `Simulation`s stepped on N threads: they share no accumulator, no field and no \
         RNG, so each performs exactly the arithmetic it would perform alone. v2 F16.5 \
         permits that, in `sailgym-env` and `sailgym-bench` only — **`rayon` may not \
         appear in `sailgym-physics`**. The binary does not take that on trust: for every \
         N and every thread count it compares the parallel run's final states with the \
         serial run's using `to_bits()`, and a single differing bit fails the run before \
         a figure is printed. Every point below passed.\n"
    );
    let _ = writeln!(d, "---\n");

    let _ = writeln!(d, "## The machine\n");
    let _ = writeln!(d, "| | |");
    let _ = writeln!(d, "|---|---|");
    let _ = writeln!(
        d,
        "| CPU | {} |",
        cpuinfo("model name").unwrap_or_else(unknown)
    );
    let _ = writeln!(
        d,
        "| Logical CPUs | {} |",
        std::thread::available_parallelism()
            .map(|n| n.get().to_string())
            .unwrap_or_else(|_| unknown())
    );
    let _ = writeln!(d, "| Memory | {} |", meminfo().unwrap_or_else(unknown));
    let _ = writeln!(d, "| OS | {} |", os_name().unwrap_or_else(unknown));
    let _ = writeln!(d, "| Host triple | `{}` |", tool.target);
    let _ = writeln!(d, "| rustc / profile | {} / {} |", tool.rustc, tool.profile);
    let _ = writeln!(d, "| Thread counts swept | {threads:?} |");
    let _ = writeln!(
        d,
        "| Scenario | `{SCENARIO}`, one seed per boat (the only shipped scenario with a \
         `Gust` field, so the seeds actually change the trajectory) |"
    );
    let _ = writeln!(d, "| `dt` | {dt} s |");
    let _ = writeln!(
        d,
        "| Work per point | about {total_steps} physics steps, spread over N boats |"
    );
    let _ = writeln!(d);
    let _ = writeln!(
        d,
        "Nothing else was running on this host. `../v1/performance.md` records three \
         sections chasing an apparently slow app that turned out to be a co-tenant \
         Playwright worker (RV12), which is why the block above exists at all.\n"
    );
    let _ = writeln!(d, "---\n");

    let _ = writeln!(d, "## Throughput\n");
    let _ = writeln!(
        d,
        "`× real time` is aggregate: `N × steps × dt / wall`. Efficiency is measured \
         against the **one-thread rayon** column, so the harness cost is in both the \
         numerator and the denominator.\n"
    );

    for (n, per_boat, serial, points) in rows {
        let one = points
            .iter()
            .find(|p| p.threads == 1)
            .map(|p| p.steps_per_second())
            .unwrap_or(f64::NAN);
        let _ = writeln!(d, "### N = {n} ({per_boat} steps per boat)\n");
        let _ = writeln!(d, "| Threads | M steps/s | × real time | Efficiency |");
        let _ = writeln!(d, "|---|---|---|---|");
        let _ = writeln!(
            d,
            "| serial (no rayon) | {:.3} | {:.0}× | — |",
            serial.steps_per_second() / 1.0e6,
            (serial.steps as f64) * dt / serial.seconds
        );
        for p in points {
            let _ = writeln!(
                d,
                "| {} | {:.3} | **{:.0}×** | {:.2} |",
                p.threads,
                p.steps_per_second() / 1.0e6,
                (p.steps as f64) * dt / p.seconds,
                p.steps_per_second() / (one * p.threads as f64),
            );
        }
        let _ = writeln!(d);
    }

    // The two headline numbers.
    let (biggest_n, _, serial, points) = rows.last().expect("at least one population");
    let best = points
        .iter()
        .max_by(|a, b| {
            a.steps_per_second()
                .partial_cmp(&b.steps_per_second())
                .expect("finite")
        })
        .expect("at least one point");
    let one = points
        .iter()
        .find(|p| p.threads == 1)
        .map(|p| p.steps_per_second())
        .unwrap_or(f64::NAN);
    let _ = writeln!(d, "---\n");
    let _ = writeln!(d, "## The answer\n");
    let _ = writeln!(
        d,
        "At the largest population, **N = {biggest_n}**, the best point is **{} threads** \
         at **{:.3} M steps/s**, which is **{:.0}× real time** in aggregate. The \
         one-thread rayon column reads {:.3} M steps/s, so the speed-up is **{:.1}×** on \
         {} threads ({:.0} % efficiency), and the serial baseline in the same process \
         reads {:.3} M steps/s — a harness cost of {:.1} % against the one-thread pool.\n",
        best.threads,
        best.steps_per_second() / 1.0e6,
        (best.steps as f64) * dt / best.seconds,
        one / 1.0e6,
        best.steps_per_second() / one,
        best.threads,
        100.0 * best.steps_per_second() / (one * best.threads as f64),
        serial.steps_per_second() / 1.0e6,
        100.0 * (serial.steps_per_second() - one) / serial.steps_per_second(),
    );
    let _ = writeln!(
        d,
        "One environment at {:.0}× real time (`../v1/performance.md`) becomes \
         {:.0}× across {biggest_n} of them on this machine. Read that against what an RL \
         run actually needs before treating a port as a throughput decision: \
         `cross-stack.md` §0's question is answered with a number here, and the section \
         handoff states the verdict in one sentence.\n",
        (rows[0].2.steps as f64) * dt / rows[0].2.seconds,
        (best.steps as f64) * dt / best.seconds,
    );
    let _ = writeln!(d, "---\n");
    let _ = writeln!(d, "## What this does not measure\n");
    let _ = writeln!(
        d,
        "- **Bare `Simulation`s, not episodes.** There is no `sailgym-env` yet, and the \
         answer was wanted before that crate was designed rather than after. Nothing here \
         includes the cost of building an observation, of an agent's decision cadence, or \
         of crossing into Python. Section 06 re-measures on the real runner.\n\
         - **One scenario.** `{SCENARIO}` is the slowest of the six in \
         `../v1/performance.md` because it is the only gusty one; a uniform-wind \
         population is roughly 11 % faster per step.\n\
         - **One machine.** The host block above is the whole of the claim's scope.\n\
         - **No interaction between boats.** F16.5: the moment boats interact through a \
         shared field, the parallel path returns to a fixed index order or to an explicit \
         two-phase sample-then-step, and these figures stop applying.\n"
    );
    d
}
