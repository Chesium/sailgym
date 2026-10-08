//! How well does the rule sailor sail? (v2 section 12, task 12.6.)
//!
//! ```text
//! cargo run --release -p sailgym-bench --bin course_bench -- --write docs/v2/baseline-validation.md
//! ```
//!
//! The rule sailor is a **baseline**: a number a player's time is compared
//! against. A baseline nobody has measured is a number nobody can interpret, so
//! this binary runs it over two suites and reports what it did — finish rate,
//! mean and p90 time, capsizes, misses, how much helm and sheet it used, and how
//! sensitive it is to the one placeholder in the course layer.
//!
//! # The two suites, and what each is for
//!
//! | suite | varies | the question |
//! |---|---|---|
//! | **uniform × 8 headings** | the boat's initial heading, every 45° | does it get going and finish from *any* start, or only from the one the scenario happens to give it? |
//! | **spatial × 16 seeds** | the wind field, through the seed | does it finish when the wind is not the same everywhere? |
//!
//! The spatial suite is **headless only**: the browser's shipped courses all
//! run in `free_sail`'s uniform northerly (task 12.1), and that is a tracked
//! debt rather than an omission — see the section PRD's debt table.
//!
//! # What this is not
//!
//! Not a physical validation and not a claim about a real boat. It measures
//! this controller, on this model, on one machine. Nothing here tunes a
//! coefficient: `parameters.rs` is byte identical through the whole section
//! (RV69), and the controller's own gains are F14.9 tunables recorded in
//! `crates/sailgym-agent/src/pilot/rule_sailor.rs`.
//!
//! # The time-limit rule, stated once
//!
//! A course challenge's `time_limit_s` is **three times the baseline's time
//! under the browser's own conditions**, rounded up to the next five seconds.
//! Three, because the suites below show the baseline's own p90 inside twice its
//! browser time and because a challenge is a lesson rather than a race; the
//! rounding, because a limit printed on a page should be a number and not a
//! measurement artefact. The measured times and the limits they produce are in
//! the document this writes.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use sailgym_agent::pilot::RuleSailor;
use sailgym_agent::spec::Agent;
use sailgym_course::guidance::CourseParams;
use sailgym_course::CourseId;

use sailgym_env::episode::{EpisodeConfig, Source};
use sailgym_env::outcome::TerminationReason;
use sailgym_env::{Episode, Outcome};

use sailgym_physics::environment::wind::WindMode;
use sailgym_physics::identity::ModelIdentity;
use sailgym_physics::recording::ToolchainInfo;
use sailgym_physics::scenario::load_shipped;

/// The step budget per run: 600 s at the F7 `dt`, five times the slowest
/// shipped course's browser-condition time.
const BUDGET: u64 = 120_000;

/// Initial headings for the uniform suite, degrees (compass, as the scenario
/// documents spell them: clockwise from north, 90 is due east).
const HEADINGS_DEG: [f64; 8] = [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0];

/// Seeds for the spatial suite.
const SEEDS: [u64; 16] = [
    1, 2, 3, 5, 8, 13, 21, 34, 55, 89, 144, 233, 377, 610, 987, 1597,
];

/// Guidance lookaheads to sweep, metres. The middle one is
/// `CourseParams::DEFAULT_LOOKAHEAD`, the placeholder the section inherits.
const LOOKAHEADS_M: [f64; 3] = [10.0, 20.0, 40.0];

/// The multiple of the browser-condition time a challenge's limit is set to.
const LIMIT_MULTIPLE: f64 = 3.0;

/// The rounding, seconds, applied upward to the product.
const LIMIT_ROUNDING_S: f64 = 5.0;

/// One run's result.
#[derive(Clone, Copy, Debug)]
struct Run {
    outcome: Kind,
    /// s, simulated time at the end.
    t: f64,
    /// rad, total rudder travel: `Σ |Δδr|` over every step.
    rudder_travel: f64,
    /// m, total sheet travel: `Σ |ΔL|`.
    sheet_travel: f64,
    /// rad, the largest `|φ|` seen.
    peak_heel: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Finished,
    Capsized,
    Missed,
    OutOfBounds,
    Unfinished,
}

impl Kind {
    fn of(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Finished { .. } => Self::Finished,
            Outcome::Terminated(TerminationReason::Capsized) => Self::Capsized,
            Outcome::Terminated(TerminationReason::MarkMissed) => Self::Missed,
            Outcome::Terminated(TerminationReason::OutOfBounds) => Self::OutOfBounds,
            Outcome::Truncated | Outcome::Running => Self::Unfinished,
        }
    }
}

/// One suite's summary.
#[derive(Clone, Debug, Default)]
struct Summary {
    runs: usize,
    finished: usize,
    capsized: usize,
    missed: usize,
    out_of_bounds: usize,
    unfinished: usize,
    /// s, the finishers' times, sorted.
    times: Vec<f64>,
    rudder: Vec<f64>,
    sheet: Vec<f64>,
    peak_heel: f64,
}

impl Summary {
    fn add(&mut self, r: Run) {
        self.runs += 1;
        match r.outcome {
            Kind::Finished => {
                self.finished += 1;
                self.times.push(r.t);
            }
            Kind::Capsized => self.capsized += 1,
            Kind::Missed => self.missed += 1,
            Kind::OutOfBounds => self.out_of_bounds += 1,
            Kind::Unfinished => self.unfinished += 1,
        }
        self.rudder.push(r.rudder_travel);
        self.sheet.push(r.sheet_travel);
        self.peak_heel = self.peak_heel.max(r.peak_heel);
    }

    fn finish_rate(&self) -> f64 {
        if self.runs == 0 {
            0.0
        } else {
            self.finished as f64 / self.runs as f64
        }
    }

    fn sorted_times(&self) -> Vec<f64> {
        let mut v = self.times.clone();
        v.sort_by(f64::total_cmp);
        v
    }

    fn mean_time(&self) -> f64 {
        mean(&self.times)
    }

    /// The p90 of the finishers' times: the **nearest-rank** value, so a suite
    /// of eight runs reports one of its own measurements and not an
    /// interpolation between two.
    fn p90_time(&self) -> f64 {
        let v = self.sorted_times();
        if v.is_empty() {
            return f64::NAN;
        }
        let rank = ((0.9 * v.len() as f64).ceil() as usize).clamp(1, v.len());
        v[rank - 1]
    }
}

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() {
        f64::NAN
    } else {
        v.iter().sum::<f64>() / v.len() as f64
    }
}

/// Run the rule sailor once.
fn run_once(
    course: CourseId,
    heading_deg: Option<f64>,
    wind_mode: WindMode,
    seed: u64,
    lookahead: f64,
) -> Run {
    let doc = course.load().expect("a shipped course");
    let route = doc.route().expect("a valid route");
    let mut sc = load_shipped(&doc.scenario).expect("a shipped scenario");
    if let Some(h) = heading_deg {
        sc.initial_state.heading_deg = h;
    }
    sc.wind.mode = wind_mode;
    let mut cfg = EpisodeConfig::new(sc);
    cfg.route = Some(route);
    cfg.max_steps = Some(BUDGET);
    cfg.course = CourseParams { lookahead };
    cfg.log_decisions = false;
    let mut ep = Episode::new(cfg, Source::Policy(Box::new(RuleSailor::new())), seed)
        .expect("a valid episode");

    let mut rudder_travel = 0.0;
    let mut sheet_travel = 0.0;
    let mut peak_heel = 0.0f64;
    let mut prev = *ep.state();
    while !ep.outcome().is_terminal() {
        let taken = ep.advance(1).expect("a valid action");
        if taken == 0 {
            break;
        }
        let st = *ep.state();
        rudder_travel += (st.delta_r - prev.delta_r).abs();
        sheet_travel += (st.l_sheet - prev.l_sheet).abs();
        peak_heel = peak_heel.max(st.phi.abs());
        prev = st;
    }
    Run {
        outcome: Kind::of(ep.outcome()),
        t: ep.state().t,
        rudder_travel,
        sheet_travel,
        peak_heel,
    }
}

/// The browser's own conditions: the scenario exactly as it ships, at the
/// default lookahead.
fn browser_run(course: CourseId) -> Run {
    run_once(
        course,
        None,
        WindMode::Uniform,
        12,
        CourseParams::DEFAULT_LOOKAHEAD,
    )
}

/// The limit rule, applied.
fn limit_for(baseline_s: f64) -> f64 {
    (baseline_s * LIMIT_MULTIPLE / LIMIT_ROUNDING_S).ceil() * LIMIT_ROUNDING_S
}

fn value(flag: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
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

fn pct(x: f64) -> String {
    format!("{:.0} %", x * 100.0)
}

fn secs(x: f64) -> String {
    if x.is_finite() {
        format!("{x:.2} s")
    } else {
        "—".to_string()
    }
}

struct Measured {
    browser: BTreeMap<&'static str, Run>,
    uniform: BTreeMap<&'static str, Summary>,
    spatial: BTreeMap<&'static str, Summary>,
    lookahead: BTreeMap<(&'static str, usize), Summary>,
}

fn measure() -> Measured {
    let mut browser = BTreeMap::new();
    let mut uniform = BTreeMap::new();
    let mut spatial = BTreeMap::new();
    let mut lookahead = BTreeMap::new();

    for course in CourseId::ALL {
        let name = course.as_str();
        let b = browser_run(course);
        println!(
            "{name}: browser conditions -> {:?} at {:.2} s",
            b.outcome, b.t
        );
        browser.insert(name, b);

        let mut u = Summary::default();
        for h in HEADINGS_DEG {
            u.add(run_once(
                course,
                Some(h),
                WindMode::Uniform,
                12,
                CourseParams::DEFAULT_LOOKAHEAD,
            ));
        }
        println!(
            "{name}: uniform × 8 headings -> {} finished, mean {:.1} s",
            u.finished,
            u.mean_time()
        );
        uniform.insert(name, u);

        let mut s = Summary::default();
        for seed in SEEDS {
            s.add(run_once(
                course,
                None,
                WindMode::Spatial,
                seed,
                CourseParams::DEFAULT_LOOKAHEAD,
            ));
        }
        println!(
            "{name}: spatial × 16 seeds -> {} finished, mean {:.1} s",
            s.finished,
            s.mean_time()
        );
        spatial.insert(name, s);

        for (i, la) in LOOKAHEADS_M.iter().enumerate() {
            let mut l = Summary::default();
            for h in HEADINGS_DEG {
                l.add(run_once(course, Some(h), WindMode::Uniform, 12, *la));
            }
            println!(
                "{name}: lookahead {la} m -> {} finished, mean {:.1} s",
                l.finished,
                l.mean_time()
            );
            lookahead.insert((name, i), l);
        }
    }
    Measured {
        browser,
        uniform,
        spatial,
        lookahead,
    }
}

fn suite_table(d: &mut String, title: &str, suite: &BTreeMap<&'static str, Summary>) {
    let _ = writeln!(d, "{title}\n");
    let _ = writeln!(
        d,
        "| course | runs | finish rate | mean | p90 | capsizes | misses | out of bounds | \
         unfinished | mean rudder travel | mean sheet travel | peak heel |"
    );
    let _ = writeln!(d, "|---|---|---|---|---|---|---|---|---|---|---|---|");
    for course in CourseId::ALL {
        let name = course.as_str();
        let Some(s) = suite.get(name) else { continue };
        let _ = writeln!(
            d,
            "| `{name}` | {} | {} | {} | {} | {} | {} | {} | {} | {:.1} rad | {:.1} m | {:.1}° |",
            s.runs,
            pct(s.finish_rate()),
            secs(s.mean_time()),
            secs(s.p90_time()),
            s.capsized,
            s.missed,
            s.out_of_bounds,
            s.unfinished,
            mean(&s.rudder),
            mean(&s.sheet),
            s.peak_heel.to_degrees()
        );
    }
    let _ = writeln!(d);
}

fn document(m: &Measured) -> String {
    let tool = ToolchainInfo::current();
    let model = ModelIdentity::current();
    let mut d = String::new();
    let _ = writeln!(d, "# The rule sailor, measured\n");
    let _ = writeln!(
        d,
        "Generated by `cargo run --release -p sailgym-bench --bin course_bench -- --write \
         docs/v2/baseline-validation.md`. **Do not edit by hand.**\n"
    );
    let _ = writeln!(
        d,
        "This is a measurement of **one controller on one model on one machine**. It is not a \
         physical validation, it says nothing about a real boat, and it certifies nothing: \
         `conformance.md` is implementation agreement and this is a controller's behaviour. \
         Nothing in this section tuned a physical coefficient — `parameters.rs` is byte \
         identical through the whole of v2 section 12 (its acceptance criterion 3, RV69) — and \
         the rule sailor's own gains are F14.9 tunables, recorded with their provenance in \
         `crates/sailgym-agent/src/pilot/rule_sailor.rs`.\n"
    );

    let _ = writeln!(d, "## 0. The host, and what was run\n");
    let _ = writeln!(d, "| | |");
    let _ = writeln!(d, "|---|---|");
    let _ = writeln!(
        d,
        "| Physics model | version {}, source `{}` ({}) |",
        model.model_version,
        model.source.tree,
        format!("{:?}", model.source.state).to_lowercase()
    );
    let _ = writeln!(d, "| Host triple | `{}` |", tool.target);
    let _ = writeln!(d, "| rustc / profile | {} / {} |", tool.rustc, tool.profile);
    let _ = writeln!(
        d,
        "| Logical CPUs | {} |",
        std::thread::available_parallelism()
            .map(|n| n.get().to_string())
            .unwrap_or_else(|_| unknown())
    );
    let _ = writeln!(d, "| Memory | {} |", meminfo().unwrap_or_else(unknown));
    let _ = writeln!(d, "| OS | {} |", os_name().unwrap_or_else(unknown));
    let _ = writeln!(
        d,
        "| Controller | `rule_sailor` v{}, cadence {} steps |",
        sailgym_agent::pilot::rule_sailor::RULE_SAILOR_VERSION,
        RuleSailor::new().spec().cadence.period_steps
    );
    let _ = writeln!(d, "| Step budget per run | {BUDGET} steps |");
    let _ = writeln!(
        d,
        "| Courses | {} |",
        CourseId::ALL
            .iter()
            .map(|c| format!("`{}`", c.as_str()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(d);
    let _ = writeln!(
        d,
        "A **dirty** or **unknown** source above means the physics this was measured against \
         is not a committed baseline, and the numbers are therefore evidence about a working \
         tree rather than about a release (F18.1d).\n"
    );

    // --- §1, the trim table ------------------------------------------------
    let _ = writeln!(d, "## 1. The trim table, and where it came from\n");
    let _ = writeln!(
        d,
        "The rule sailor's trim layer is a table from `|awa|` to a sheet length. It is \
         **measured**, by `cargo run --release -p sailgym-bench --bin trim_sweep`, on \
         `free_sail`'s own field — a uniform 5 m/s northerly, the conditions all three \
         shipped courses are sailed in.\n"
    );
    let _ = writeln!(
        d,
        "For each target true wind angle, on **both** tacks, the boat is held on that angle \
         with the sheet pinned at each of 41 lengths between the catalogue's two stops; the \
         settled surge speed, heel and `|awa|` are averaged over the last quarter of a 40 s \
         hold. The heading and the sheet are held by the bench's own proportional loops and \
         **not** by the rule sailor, because tuning a controller from a measurement the \
         controller produced makes the table a fixed point of its own gains.\n"
    );
    let _ = writeln!(
        d,
        "Each entry is the **midpoint of the admissible plateau**: the lengths within 2 % of \
         the best settled speed whose heel stays under 30°. The bare argmax was not used, and \
         the reason is recorded rather than assumed — the speed-versus-length curve is flat \
         near its optimum, so re-running the sweep at 25 and at 41 samples moved the argmax by \
         up to 0.9 m while moving the speed it bought by under 3 %, and a table built from it \
         was not monotonic in `|awa|`. The two tacks agreed to within **5.1e-5 m/s** of \
         settled speed at every point, which is a measured number and not a symmetry anyone \
         asserted.\n"
    );
    let _ = writeln!(d, "| `\\|awa\\|` | sheet length | settled speed | heel |");
    let _ = writeln!(d, "|---|---|---|---|");
    for row in [
        ("21.9°", "1.213 m", "1.412 m/s", "23.7°"),
        ("26.5°", "1.300 m", "1.771 m/s", "28.9°"),
        ("32.3°", "1.473 m", "2.043 m/s", "28.8°"),
        ("38.1°", "1.646 m", "2.306 m/s", "28.7°"),
        ("49.0°", "2.078 m", "2.553 m/s", "22.3°"),
        ("60.3°", "2.511 m", "2.711 m/s", "16.4°"),
        ("74.6°", "3.116 m", "2.580 m/s", "8.2°"),
        ("90.6°", "3.722 m", "2.393 m/s", "2.0°"),
        ("109.1°", "2.900 m", "2.327 m/s", "10.8°"),
        ("144.3°", "3.505 m", "2.236 m/s", "4.8°"),
        ("177.7°", "4.068 m", "2.190 m/s", "0.3°"),
    ] {
        let _ = writeln!(d, "| {} | {} | {} | {} |", row.0, row.1, row.2, row.3);
    }
    let _ = writeln!(d);
    let _ = writeln!(
        d,
        "**It decreases once**, at 90.6° → 109.1° (3.722 m → 2.900 m), and it is left that \
         way. At an apparent 91° the quick trim is a boom at 78° with attached flow; at 109° \
         it is a boom at 57° working as a stalled plate. Flattening the step would be choosing \
         a number from how a run looks, which is v1 brief §43's discipline whether or not the \
         number is a coefficient.\n"
    );
    let _ = writeln!(
        d,
        "v1's one close-hauled measurement — 2.0 m giving about 1.78 m/s against 0.40 m/s \
         fully hauled — was taken on `close_hauled`'s 3.5 m/s and is the data point this \
         replaces, not an entry in it.\n"
    );

    // --- §2, browser conditions and the limits -----------------------------
    let _ = writeln!(d, "## 2. Browser conditions, and the time limits\n");
    let _ = writeln!(
        d,
        "The shipped scenario exactly as it ships, at the default {} m lookahead. This is the \
         run a player is compared against, and the row a challenge's `time_limit_s` is derived \
         from: **{LIMIT_MULTIPLE:.0} × the baseline's time, rounded up to the next \
         {LIMIT_ROUNDING_S:.0} s**.\n",
        CourseParams::DEFAULT_LOOKAHEAD
    );
    let _ = writeln!(
        d,
        "| course | outcome | baseline time | peak heel | rudder travel | sheet travel | \
         `time_limit_s` |"
    );
    let _ = writeln!(d, "|---|---|---|---|---|---|---|");
    for course in CourseId::ALL {
        let name = course.as_str();
        let Some(b) = m.browser.get(name) else {
            continue;
        };
        let _ = writeln!(
            d,
            "| `{name}` | {:?} | {} | {:.1}° | {:.1} rad | {:.1} m | {} |",
            b.outcome,
            secs(b.t),
            b.peak_heel.to_degrees(),
            b.rudder_travel,
            b.sheet_travel,
            secs(limit_for(b.t))
        );
    }
    let _ = writeln!(d);
    let _ = writeln!(
        d,
        "`sailgym_task::course::shipped_time_limit_s` carries those limits, and \
         `crates/sailgym-task/tests/course.rs` asserts the shipped configuration validates \
         against them.\n"
    );

    // --- §3, the suites ----------------------------------------------------
    let _ = writeln!(d, "## 3. The two suites\n");
    suite_table(
        &mut d,
        "### 3.1 Uniform wind × 8 initial headings\n\nThe field the browser uses, with the \
         boat pointed every 45° at `t = 0`. A controller that only works from the heading its \
         scenario happens to give it is not a baseline.",
        &m.uniform,
    );
    suite_table(
        &mut d,
        "### 3.2 `free_sail`'s field in `Spatial` mode × 16 seeds\n\nA divergence-free \
         perturbation frozen in time (F6.1), so the wind is not the same everywhere on the \
         course. **Headless only**: the browser's courses run in the uniform field, which the \
         section PRD records as a tracked debt.",
        &m.spatial,
    );

    // --- §4, lookahead -----------------------------------------------------
    let _ = writeln!(d, "## 4. Sensitivity to the guidance lookahead\n");
    let _ = writeln!(
        d,
        "`CourseParams::lookahead` is still the {} m placeholder section 04 shipped, and it is \
         **not** recorded in `ResearchIdentity` — so changing it silently changes what a \
         `guidance` observation means. That is the section's first tracked debt. This sweep \
         measures how much it matters, over the uniform × 8-heading suite; it does not repay \
         the debt.\n",
        CourseParams::DEFAULT_LOOKAHEAD
    );
    let _ = writeln!(
        d,
        "| course | lookahead | finish rate | mean | p90 | capsizes | misses | mean rudder \
         travel |"
    );
    let _ = writeln!(d, "|---|---|---|---|---|---|---|---|");
    for course in CourseId::ALL {
        let name = course.as_str();
        for (i, la) in LOOKAHEADS_M.iter().enumerate() {
            let Some(s) = m.lookahead.get(&(name, i)) else {
                continue;
            };
            let _ = writeln!(
                d,
                "| `{name}` | {la} m | {} | {} | {} | {} | {} | {:.1} rad |",
                pct(s.finish_rate()),
                secs(s.mean_time()),
                secs(s.p90_time()),
                s.capsized,
                s.missed,
                mean(&s.rudder)
            );
        }
    }
    let _ = writeln!(d);

    // --- §5, what fired ----------------------------------------------------
    let _ = writeln!(d, "## 5. What this says about RV68\n");
    let _ = writeln!(
        d,
        "RV68 is the risk that this boat cannot be tacked or run reliably at all: \
         `practice-validation.md` §3.1 swept 1 344 helm-only scripts and **none** of them \
         completed a tack. The section's acceptance names a threshold — any course below an \
         **80 %** finish rate on the uniform-wind suite is RV68 firing and must be named in \
         the handoff.\n"
    );
    let _ = writeln!(d, "| course | uniform finish rate | below 80 %? |");
    let _ = writeln!(d, "|---|---|---|");
    for course in CourseId::ALL {
        let name = course.as_str();
        let Some(s) = m.uniform.get(name) else {
            continue;
        };
        let _ = writeln!(
            d,
            "| `{name}` | {} | {} |",
            pct(s.finish_rate()),
            if s.finish_rate() < 0.8 {
                "**yes**"
            } else {
                "no"
            }
        );
    }
    let _ = writeln!(d);
    let _ = writeln!(
        d,
        "The three things that make a tack work in this model, all measured and all built into \
         the controller: **full helm and full haul together** (`practice-validation.md` §3.1 — \
         helm alone never completes one); a tack **only above about 1.6 m/s**, which is why \
         `min_tack_interval` gates the first manoeuvre as well as every later one; and a turn \
         through the eye only when its arc is short, because bearing away round through the \
         run always works and a 156° tack at 0.37 m/s does not.\n"
    );

    let _ = writeln!(
        d,
        "### 5.1 Why the uniform suite's non-finishes are what they are\n"
    );
    let _ = writeln!(
        d,
        "The `misses` column of §3.1 is the whole story, and it is **not** a controller that \
         cannot sail. Traced heading by heading on `triangle`, the five non-finishes are:\n"
    );
    let _ = writeln!(
        d,
        "* **Four cuts of waypoint 1** (headings 45°, 135°, 225°, 315°). The boat beats the \
           40 m leg and crosses the gate's line at 5.7–8.6 m from its centre against posts \
           5 m either side — outside by 0.7 to 3.6 m. It sailed the leg; it arrived off \
           centre.\n\
         * **One drift** (heading 0°). The scenario puts the boat **exactly head to wind, at \
           rest**, which is an exactly mirror-symmetric state; the controller is exactly \
           mirror-symmetric (its decision-level mirror test is bit-for-bit, RV72), so it has \
           no side to choose and does not invent one. It lies there and drifts downwind. That \
           is the other face of the mirror property rather than a defect in the layers, and \
           breaking it would mean breaking the symmetry.\n"
    );
    let _ = writeln!(
        d,
        "Two things follow, and both matter for reading the table above.\n"
    );
    let _ = writeln!(
        d,
        "**This suite measures a stricter thing than the challenge does.** It runs the \
         **research** episode runner, where F19.2 makes a cut `Terminated(MarkMissed)` and the \
         episode is over. The practice challenge is D3: a cut emits `waypoint_missed` and the \
         attempt **continues**, so a boat that arrived 1 m wide comes back behind the line and \
         goes through. The finish rate here is therefore a lower bound on what a player would \
         see the baseline do, and the two numbers are measuring different questions on \
         purpose — the same disagreement D3 licenses, visible in a table.\n"
    );
    let _ = writeln!(
        d,
        "**The corridor is inert on the shipped courses, and that is a measurement.** Sweeping \
         `corridor_half_width` over 6, 8, 10, 12, 14 and 18 m changed **not one** outcome or \
         time in this suite: the tack that happens is always the one the target crossing the \
         wind's eye asks for, which fires as the boat crosses the leg line and therefore before \
         any corridor does. What actually bounds the zigzag is `min_tack_interval` — 15 s at \
         about 1.3 m/s of lateral speed is roughly 20 m either side of the line, which is wider \
         than a 10 m gate. The corridor rule is kept because the PRD's layer table specifies \
         it and because a course with legs long enough to need it is a course away; it is \
         recorded here as *not* the thing doing the work.\n"
    );
    d
}

fn main() {
    let m = measure();
    if let Some(path) = value("--write") {
        let doc = document(&m);
        std::fs::write(&path, &doc).unwrap_or_else(|e| panic!("{path}: {e}"));
        eprintln!("course_bench: wrote {path}");
    } else {
        eprintln!(
            "course_bench: pass `--write docs/v2/baseline-validation.md` to regenerate the \
             document."
        );
    }
    // The numbers the handoff quotes, on stdout either way.
    for course in CourseId::ALL {
        let name = course.as_str();
        if let (Some(b), Some(u), Some(s)) = (
            m.browser.get(name),
            m.uniform.get(name),
            m.spatial.get(name),
        ) {
            println!(
                "{name}: browser {:.2} s -> limit {:.0} s | uniform {} finished of {} | \
                 spatial {} of {}",
                b.t,
                limit_for(b.t),
                u.finished,
                u.runs,
                s.finished,
                s.runs
            );
        }
    }
}
