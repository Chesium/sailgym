//! The section's contract test (section 05 task 5.7).
//!
//! Three properties, each of which is a bug no ordinary test would catch:
//!
//! 1. **F9.7 holds with an agent attached.** `advance(n)` equals `n` calls to
//!    `advance(1)`, bit for bit, with `manual` and with a stub policy at
//!    cadence 10. `Simulation::advance` refreshes the force breakdown once per
//!    call *as an optimisation whose justification a per-step agent hook could
//!    invalidate*, which is exactly why this earns its keep (RV25).
//! 2. **Cadence keys off the episode step counter** (F14.6), asserted over six
//!    chunkings of the same episode rather than over one (RV26).
//! 3. **An external manual action and a policy action take the same path**,
//!    proven by equal resulting trajectories rather than by a ban on a source
//!    pattern (RV27, section acceptance 2).
//!
//! Plus the wall-clock grep, **extended** to the v2 crates rather than a second
//! one written (F14.8).
//!
//! # The driver lives here, and that is deliberate
//!
//! [`Episode`] is the smallest thing that can attach an agent to a
//! `Simulation`: observe, decide, denormalise, hold. The real episode runner —
//! with `Outcome`, terminations, a decision log and `VecEnv` — is section 06's,
//! and this crate deliberately ships none of it. What this file proves is that
//! the *contract* those things will be built on holds.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sailgym_agent::actuation::{apply, rate::Rate, Actuation};
use sailgym_agent::observation::{observe, sensor_streams, ObsLayout};
use sailgym_agent::sensor::{Sensor, SensorRegistry};
use sailgym_agent::spec::{agent_rng, Action, ActionSpace, ActionVec, Agent, AgentSpec, Cadence};
use sailgym_agent::worldview::WorldView;
use sailgym_agent::Manual;

use sailgym_physics::parameters::BoatParameters;
use sailgym_physics::rng::Pcg32;
use sailgym_physics::scenario::load_shipped;
use sailgym_physics::simulation::Simulation;
use sailgym_physics::state::{BoatState, Controls, STATE_FIELDS, STATE_LEN};

/// The tier-0 suite, in catalogue order. `guidance` is included with no route,
/// so it contributes five zero columns: the layout is what is under test, not
/// the course.
const SUITE: [&str; 5] = [
    "imu",
    "apparent_wind",
    "rig_state",
    "actuator_state",
    "guidance",
];

// ---------------------------------------------------------------------------
// A stub policy
// ---------------------------------------------------------------------------

/// A deterministic policy that actually reads its observation.
///
/// It steers against the sway acceleration and trims on the apparent wind
/// speed, which makes it sensitive to precisely the quantity trap 1 is about: a
/// stale force cache would change `imu.accel_sway`, which would change the
/// action, which would change the trajectory. A policy that ignored its
/// observation would make the F9.7 test vacuous.
///
/// It also draws from its `Pcg32` on every decision, so the number of draws is
/// part of what the chunking test compares (F14.8).
struct Stub {
    cadence: Cadence,
    /// Which column is `imu.accel_sway`, resolved from the layout at reset —
    /// never a hard-coded index, because the layout is runtime data (F14.3).
    accel_sway: usize,
    aws: usize,
}

impl Stub {
    fn new(cadence: Cadence) -> Self {
        Self {
            cadence,
            accel_sway: 0,
            aws: 0,
        }
    }
}

impl Agent for Stub {
    fn spec(&self) -> AgentSpec {
        AgentSpec::new("stub", 1, ActionSpace::Rates, self.cadence)
    }

    fn reset(&mut self, fields: &[String], _rng: &mut Pcg32) {
        let find = |name: &str| {
            fields
                .iter()
                .position(|f| f == name)
                .unwrap_or_else(|| panic!("the layout has no column `{name}`: {fields:?}"))
        };
        self.accel_sway = find("imu.accel_sway");
        self.aws = find("apparent_wind.aws");
    }

    fn decide(&mut self, obs: &[f64], rng: &mut Pcg32) -> Action {
        let jitter = 0.01 * (rng.next_f64() - 0.5);
        let rudder = (-0.5 * obs[self.accel_sway] + jitter).clamp(-1.0, 1.0);
        let sheet = (0.2 * obs[self.aws] - 0.6).clamp(-1.0, 1.0);
        Action::Rates(ActionVec::new(&[rudder, sheet, -1.0]).expect("clamped"))
    }
}

// ---------------------------------------------------------------------------
// The driver
// ---------------------------------------------------------------------------

/// The smallest thing that attaches an agent to a `Simulation`.
struct Episode {
    sim: Simulation,
    sensors: Vec<Box<dyn Sensor>>,
    streams: Vec<Pcg32>,
    adapter: Box<dyn Actuation>,
    cadence: Cadence,
    rng: Pcg32,
    /// The **episode** step counter. Not a per-`advance` counter, and not a
    /// clock (F14.6).
    step: u64,
    obs: Vec<f64>,
    /// `(episode step, action)` for every decision taken, so the chunking test
    /// can compare *when* decisions happened and not only where the boat ended
    /// up.
    log: Vec<(u64, Vec<f64>)>,
}

impl Episode {
    fn new(scenario: &str, seed: u64, cadence: Cadence, agent: &mut dyn Agent) -> Self {
        let sc = load_shipped(scenario).expect("a shipped scenario");
        let params = sc.to_parameters().expect("a valid catalogue");
        let mut sim = Simulation::new(params, sc.seed);
        sim.load_scenario(&sc).expect("the scenario loads");

        let sensors = SensorRegistry::tier0()
            .resolve(&SUITE)
            .expect("the tier-0 suite");
        let layout = ObsLayout::of(&sensors);
        let root = Pcg32::seed_from_u64(seed);
        let rng = agent_rng(&root);
        let streams = sensor_streams(&rng, &sensors);

        let mut rng_for_reset = rng.clone();
        agent.reset(&layout.names(), &mut rng_for_reset);

        Self {
            sim,
            sensors,
            streams,
            adapter: Box::new(Rate),
            cadence,
            rng,
            step: 0,
            obs: Vec::new(),
            log: Vec::new(),
        }
    }

    /// One decision, at the current episode step.
    fn decide(&mut self, agent: &mut dyn Agent) {
        let st = *self.sim.state();
        let controls = *self.sim.controls();
        let params = *self.sim.params();
        let view = WorldView {
            st: &st,
            controls: &controls,
            p: &params,
            wind: self.sim.wind(),
            guidance: None,
            others: &[],
            t: st.t,
        };
        observe(&mut self.sensors, &mut self.streams, &view, &mut self.obs);
        let action = agent.decide(&self.obs, &mut self.rng);
        let c = apply(self.adapter.as_mut(), &action, &st, &params).expect("a valid action");
        self.log.push((self.step, action.values().to_vec()));
        self.sim.set_controls(c);
    }

    /// Advance `n` physics steps, deciding whenever the **episode** step
    /// counter says to.
    ///
    /// The loop splits `n` at decision boundaries, so where the decisions land
    /// is a property of the episode and not of the caller's chunk size. That is
    /// the whole of F14.6.1, and `cadence_lands_on_episode_steps` is what says
    /// it is true.
    fn advance(&mut self, n: u32, agent: &mut dyn Agent) {
        let mut left = u64::from(n);
        while left > 0 {
            if self.cadence.decides_at(self.step) {
                self.decide(agent);
            }
            let chunk = left.min(self.cadence.steps_to_next_decision(self.step));
            self.sim
                .advance(u32::try_from(chunk).expect("a chunk fits in u32"));
            self.step += chunk;
            left -= chunk;
        }
    }

    fn state(&self) -> [f64; STATE_LEN] {
        self.sim.state().to_array()
    }
}

/// Run one episode of `total` steps, delivering them in chunks of `chunk`.
fn run(
    scenario: &str,
    seed: u64,
    cadence: Cadence,
    total: u32,
    chunk: u32,
    agent: &mut dyn Agent,
) -> ([f64; STATE_LEN], Vec<(u64, Vec<f64>)>) {
    let mut ep = Episode::new(scenario, seed, cadence, agent);
    let mut done = 0u32;
    while done < total {
        let n = chunk.min(total - done);
        ep.advance(n, agent);
        done += n;
    }
    (ep.state(), ep.log.clone())
}

fn assert_same_state(a: &[f64; STATE_LEN], b: &[f64; STATE_LEN], what: &str) {
    for i in 0..STATE_LEN {
        assert_eq!(
            a[i].to_bits(),
            b[i].to_bits(),
            "{what}: field {} — {} vs {}",
            STATE_FIELDS[i],
            a[i],
            b[i]
        );
    }
}

// ---------------------------------------------------------------------------
// 1. F9.7 with an agent attached
// ---------------------------------------------------------------------------

/// `advance(n) == n × advance(1)`, bit for bit, with an agent attached.
///
/// Demonstrated able to fail: `docs/v2/progress/05-handoff.md` §5 records the
/// run in which `Episode::advance` keyed its cadence off a per-call counter
/// instead of the episode step, and this test went red.
#[test]
fn advance_n_equals_n_advance_1_with_an_agent_attached() {
    // 15 s at the F7 default `dt`. Long enough that both boats start from
    // rest, sheet in and sail — a match between two boats that never moved
    // would prove nothing, which the displacement assertions below enforce.
    let total = 3000u32;

    // The stub policy, at 20 Hz over 200 Hz physics.
    let (batched, log_b) = run(
        "close_hauled",
        7,
        Cadence::new(10),
        total,
        total,
        &mut Stub::new(Cadence::new(10)),
    );
    let (single, log_s) = run(
        "close_hauled",
        7,
        Cadence::new(10),
        total,
        1,
        &mut Stub::new(Cadence::new(10)),
    );
    assert_same_state(&batched, &single, "stub at cadence 10");
    assert_eq!(log_b, log_s, "the decisions themselves differ");
    assert_eq!(log_b.len(), (total / 10) as usize);

    // …and the boat actually went somewhere, or the match proves nothing.
    assert!(
        batched[0].abs() + batched[1].abs() > 1.0,
        "the boat never moved"
    );

    // `manual`, which decides every step: the degenerate cadence, and the one
    // a browser uses. The latch is pushed **after** the episode is built,
    // because `Manual::reset` deliberately returns it to hands-off.
    let manual_run = |chunk: u32| -> [f64; STATE_LEN] {
        let mut m = Manual::new(Cadence::EVERY_STEP, Rate::DIM);
        let mut ep = Episode::new("free_sail", 11, Cadence::EVERY_STEP, &mut m);
        m.set_action(&[0.3, -1.0, -1.0]).expect("in bounds");
        let mut done = 0u32;
        while done < total {
            let n = chunk.min(total - done);
            ep.advance(n, &mut m);
            done += n;
        }
        ep.state()
    };
    let single_stepped = manual_run(1);
    assert_same_state(&manual_run(total), &single_stepped, "manual");
    assert_same_state(&manual_run(7), &single_stepped, "manual, chunked by 7");
    assert!(
        single_stepped[0].abs() + single_stepped[1].abs() > 1.0,
        "the manual boat never moved"
    );
}

// ---------------------------------------------------------------------------
// 2. Cadence
// ---------------------------------------------------------------------------

/// Decisions land on `step % period == 0` regardless of how the caller chunks
/// `advance`, over six chunkings (RV26).
#[test]
fn cadence_lands_on_episode_steps_whatever_the_chunking() {
    let total = 1000u32;
    let cadence = Cadence::new(10);

    let (reference, log) = run("tack", 3, cadence, total, 1, &mut Stub::new(cadence));
    // Every decision is on a multiple of the period, and there are no others.
    let steps: Vec<u64> = log.iter().map(|(s, _)| *s).collect();
    assert_eq!(
        steps,
        (0..u64::from(total)).step_by(10).collect::<Vec<_>>(),
        "decisions did not land on the episode's own step multiples"
    );

    for chunk in [1u32, 3, 7, 10, 13, 200] {
        let (state, chunked) = run("tack", 3, cadence, total, chunk, &mut Stub::new(cadence));
        assert_eq!(
            chunked, log,
            "chunk {chunk}: the decisions moved with the caller's chunk size"
        );
        assert_same_state(&reference, &state, &format!("chunk {chunk}"));
    }

    // A different period gives a different — and equally chunk-independent —
    // decision sequence, so the test above is not passing because nothing
    // depends on the period.
    let (_, at_seven) = run(
        "tack",
        3,
        Cadence::new(7),
        total,
        13,
        &mut Stub::new(Cadence::new(7)),
    );
    assert_eq!(
        at_seven.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
        (0..u64::from(total)).step_by(7).collect::<Vec<_>>()
    );
    assert_ne!(at_seven.len(), log.len());
}

// ---------------------------------------------------------------------------
// 3. The same path for both sources
// ---------------------------------------------------------------------------

/// Section acceptance 2: an external manual action and a policy action produce
/// the **same** trajectory when the numbers are the same (RV27).
///
/// This is a behavioural test on purpose. A ban on a source pattern — "no
/// branch on whether an agent is attached" — would be satisfied by two code
/// paths that happened to look alike; equal trajectories are not.
#[test]
fn manual_and_policy_actions_take_the_same_path() {
    /// A policy that replays a fixed sequence of actions by decision index.
    struct Scripted {
        cadence: Cadence,
        script: Vec<[f64; Rate::DIM]>,
        at: usize,
    }

    impl Agent for Scripted {
        fn spec(&self) -> AgentSpec {
            AgentSpec::new("scripted", 1, ActionSpace::Rates, self.cadence)
        }
        fn reset(&mut self, _fields: &[String], _rng: &mut Pcg32) {
            self.at = 0;
        }
        fn decide(&mut self, _obs: &[f64], _rng: &mut Pcg32) -> Action {
            let a = self.script[self.at % self.script.len()];
            self.at += 1;
            Action::Rates(ActionVec::new(&a).expect("in bounds"))
        }
    }

    let cadence = Cadence::new(10);
    let script: Vec<[f64; Rate::DIM]> = vec![
        [0.0, -1.0, -1.0],
        [0.4, -0.2, -1.0],
        [-0.7, 0.0, -1.0],
        [0.0, 1.0, 1.0],
        [0.2, 0.3, -1.0],
    ];
    let total = 800u32;

    // Arm A: a policy, deciding for itself.
    let mut policy = Scripted {
        cadence,
        script: script.clone(),
        at: 0,
    };
    let mut ep_a = Episode::new("gybe", 5, cadence, &mut policy);
    ep_a.advance(total, &mut policy);

    // Arm B: an external source, pushing the same numbers from outside. The
    // push happens between decision periods, which is exactly what a browser
    // does between frames.
    let mut manual = Manual::new(cadence, Rate::DIM);
    let mut ep_b = Episode::new("gybe", 5, cadence, &mut manual);
    let mut done = 0u32;
    let mut at = 0usize;
    while done < total {
        manual
            .set_action(&script[at % script.len()])
            .expect("in bounds");
        at += 1;
        let n = cadence.period_steps.min(total - done);
        ep_b.advance(n, &mut manual);
        done += n;
    }

    assert_same_state(&ep_a.state(), &ep_b.state(), "manual vs policy");
    assert_eq!(
        ep_a.log, ep_b.log,
        "the two sources produced different actions"
    );
    assert_eq!(ep_a.log.len(), (total / cadence.period_steps) as usize);
    assert!(
        ep_a.state()[0].abs() + ep_a.state()[1].abs() > 1.0,
        "the boat never moved, so the comparison proves nothing"
    );

    // …and a *different* pushed action does change the trajectory, so the
    // agreement above is not the agreement of two things that ignore their
    // input.
    let mut other = Manual::new(cadence, Rate::DIM);
    let mut ep_c = Episode::new("gybe", 5, cadence, &mut other);
    let mut done = 0u32;
    while done < total {
        other.set_action(&[-0.9, 1.0, -1.0]).expect("in bounds");
        let n = cadence.period_steps.min(total - done);
        ep_c.advance(n, &mut other);
        done += n;
    }
    assert_ne!(ep_c.state()[2], ep_a.state()[2], "the arms are identical");
}

// ---------------------------------------------------------------------------
// 4. The wall-clock grep, extended
// ---------------------------------------------------------------------------

/// The v2 crates this grep covers.
///
/// `crates/sailgym-physics` is **not** here: it has run this grep since M1, in
/// `crates/sailgym-physics/tests/determinism.rs`, and this section changes
/// exactly one file in that crate (section acceptance 6). F14.8 asks for the
/// existing grep to be extended to the new crates rather than rewritten; this
/// is that extension, with the same needles and the same exclusion rule.
/// `sailgym-course` is included because the section-04 handoff §11.5 asks for
/// it and nothing enforced it before.
fn v2_crate_sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
            .map(|e| e.expect("directory entry").path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut out = Vec::new();
    for krate in ["sailgym-agent", "sailgym-course"] {
        walk(&root.join(krate).join("src"), &mut out);
    }
    assert!(out.len() > 8, "the source scan found almost nothing");
    out
}

/// Lines **not** inside a `#[cfg(test)]` item.
///
/// The same exclusion `tests/no_shortcuts.rs` applies, for the same reason: a
/// test that names a forbidden pattern in order to refuse it is evidence, not a
/// violation. `rustfmt` closes a module at column 0, which is what this keys
/// on, and gate step 1 is what keeps that true.
fn code_lines(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut skipping = false;
    for (i, line) in source.lines().enumerate() {
        if skipping {
            if line == "}" {
                skipping = false;
            }
            continue;
        }
        if line.trim() == "#[cfg(test)]" {
            skipping = true;
            continue;
        }
        out.push((i + 1, line.to_string()));
    }
    out
}

fn offenders(needles: &[&str]) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    for path in v2_crate_sources() {
        let src = std::fs::read_to_string(&path).expect("source must be readable");
        for (n, line) in code_lines(&src) {
            if let Some(needle) = needles.iter().find(|needle| line.contains(**needle)) {
                found.insert(
                    format!("{}:{}", path.display(), n),
                    format!("{needle} in `{}`", line.trim()),
                );
                break;
            }
        }
    }
    found
}

#[test]
fn no_wall_clock_in_the_v2_crates() {
    // The needles are the physics crate's, verbatim (F9.1, F14.8): `decide`
    // may not read a wall clock, and an agent inside the step loop is physics
    // for this purpose.
    let found = offenders(&["Instant", "SystemTime", "now(", "rand::thread_rng"]);
    assert!(
        found.is_empty(),
        "wall-clock or unseeded randomness in a v2 crate: {found:#?}"
    );
}

#[test]
fn no_hash_iteration_in_the_v2_crates() {
    // F9.3, and the reason the sensor registry is a `Vec`.
    let found = offenders(&["HashMap", "HashSet"]);
    assert!(found.is_empty(), "hash container in a v2 crate: {found:#?}");
}

/// The grep has to be able to fail, or it is decoration.
#[test]
fn the_grep_would_notice() {
    let src = "fn f() {\n    let t = std::time::Instant::now();\n}\n";
    let lines = code_lines(src);
    assert!(lines.iter().any(|(_, l)| l.contains("Instant")));

    // …and it ignores a `#[cfg(test)]` item, which is what lets a test name a
    // forbidden pattern in order to refuse it.
    let with_test = "fn f() {}\n#[cfg(test)]\nmod tests {\n    use std::time::Instant;\n}\n";
    assert!(!code_lines(with_test)
        .iter()
        .any(|(_, l)| l.contains("Instant")));
}

// ---------------------------------------------------------------------------
// 5. The dependency direction
// ---------------------------------------------------------------------------

/// Section acceptance 5, asserted in the gate rather than left to review.
#[test]
fn physics_depends_on_no_v2_crate() {
    let out = std::process::Command::new(env!("CARGO"))
        .args(["tree", "-p", "sailgym-physics", "--edges", "all"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output();
    let Ok(out) = out else {
        eprintln!("skip: cargo tree is not runnable here");
        return;
    };
    if !out.status.success() {
        eprintln!(
            "skip: cargo tree failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        return;
    }
    let tree = String::from_utf8_lossy(&out.stdout);
    for name in [
        "sailgym-agent",
        "sailgym-course",
        "sailgym-task",
        "wasm-bindgen",
    ] {
        assert!(
            !tree.contains(name),
            "cargo tree -p sailgym-physics mentions {name} (F14.1):\n{tree}"
        );
    }
}

/// Section acceptance 7: **no literal from the F7 catalogue appears in
/// `crates/sailgym-agent/src`.**
///
/// `tests/provenance.rs::no_stray_constants` enforces the same F7 rule over
/// `crates/sailgym-physics/src` and scans nothing else, so without this the
/// agent crate would be the one place a coefficient could be copied to and
/// nothing would notice (RV28). The scan is the physics one's: shipped lines
/// only — no `#[cfg(test)]` item, no comment — because a fixture that says
/// `u = 4.0` is evidence and a doc comment quoting a value is documentation.
///
/// Only `src/` is scanned. This file is test code by construction, and the
/// scripted action components in it (`0.2`, `0.3`, `-0.9`) are normalised
/// actions that happen to share a decimal with an F7 row.
#[test]
fn no_f7_literal_appears_in_the_agent_crate() {
    /// `0.0`, `1.0`, `2.0`, `3.0` and `0.5` are structural, not coefficients —
    /// the same universal set `tests/provenance.rs` exempts.
    fn universal(v: f64) -> bool {
        [0.0, 1.0, 2.0, 3.0, 0.5].contains(&v)
    }

    fn float_literals(line: &str) -> Vec<f64> {
        let mut out = Vec::new();
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if !bytes[i].is_ascii_digit() {
                i += 1;
                continue;
            }
            // Not a literal if an identifier character precedes it.
            if i > 0
                && (bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'_'
                    || bytes[i - 1] == b'.')
            {
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.')
                {
                    i += 1;
                }
                continue;
            }
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                i += 1;
            }
            if i < bytes.len()
                && bytes[i] == b'.'
                && i + 1 < bytes.len()
                && bytes[i + 1].is_ascii_digit()
            {
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                    i += 1;
                }
                let text: String = line[start..i].chars().filter(|c| *c != '_').collect();
                if let Ok(v) = text.parse::<f64>() {
                    out.push(v);
                }
            }
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
        }
        out
    }

    // The F7 catalogue, read from `parameters.rs` rather than copied here: a
    // copy would go stale the first time a value moved, and then this audit
    // would be checking a number nothing uses.
    let params = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sailgym-physics/src/parameters.rs");
    let catalogue = std::fs::read_to_string(&params).expect("parameters.rs must be readable");
    let f7: Vec<f64> = catalogue
        .lines()
        .flat_map(float_literals)
        .filter(|v| !universal(*v))
        .collect();
    assert!(f7.len() > 40, "the F7 scan found only {} values", f7.len());

    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders: Vec<String> = Vec::new();
    let mut files = 0usize;
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
            .map(|e| e.expect("entry").path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    walk(&src, &mut paths);
    for path in paths {
        let text = std::fs::read_to_string(&path).expect("source");
        for (n, line) in code_lines(&text) {
            let code = line.split("//").next().unwrap_or("");
            for v in float_literals(code) {
                if !universal(v) && f7.contains(&v) {
                    offenders.push(format!("{}:{n}: {v} in `{}`", path.display(), code.trim()));
                }
            }
        }
        files += 1;
    }
    assert!(files >= 10, "the agent source scan found almost nothing");
    assert!(
        offenders.is_empty(),
        "an F7 coefficient has been copied into the agent crate (RV28, brief §43):\n{}",
        offenders.join("\n")
    );
    eprintln!(
        "{files} agent source files scanned against {} F7 values",
        f7.len()
    );
}

// ---------------------------------------------------------------------------
// 6. The observation never reads the cache
// ---------------------------------------------------------------------------

/// RV25, from the other end: the observation at a given episode step is the
/// same however the caller chunked `advance`.
///
/// `advance_n_equals_n_advance_1_with_an_agent_attached` compares the states
/// the decisions produced; this compares the observations the decisions were
/// made from, which is where a stale cache would show first.
#[test]
fn the_observation_does_not_depend_on_advance_chunking() {
    let p = BoatParameters::ilca7();
    let sensors_of = || {
        SensorRegistry::tier0()
            .resolve(&SUITE)
            .expect("the tier-0 suite")
    };
    let c = Controls {
        rudder_rate_cmd: 0.6,
        sheet_rate_cmd: -1.0,
        sheet_release: false,
    };
    let start = BoatState {
        u: 1.5,
        psi: 0.2,
        ..Simulation::initial_state(&p)
    };

    let at_step = |chunk: u32, total: u32| -> Vec<f64> {
        let mut sim = Simulation::new(p, 4);
        sim.reset(start, 4);
        sim.set_controls(c);
        let mut done = 0u32;
        while done < total {
            let n = chunk.min(total - done);
            sim.advance(n);
            done += n;
        }
        let mut sensors = sensors_of();
        let root = Pcg32::seed_from_u64(4);
        let mut streams = sensor_streams(&agent_rng(&root), &sensors);
        let st = *sim.state();
        let controls = *sim.controls();
        let params = *sim.params();
        let view = WorldView {
            st: &st,
            controls: &controls,
            p: &params,
            wind: sim.wind(),
            guidance: None,
            others: &[],
            t: st.t,
        };
        let mut out = Vec::new();
        observe(&mut sensors, &mut streams, &view, &mut out);
        out
    };

    let reference = at_step(1, 500);
    assert_eq!(reference.len(), 5 + 2 + 4 + 2 + 5);
    for chunk in [3u32, 7, 10, 13, 200, 500] {
        let got = at_step(chunk, 500);
        for (k, (a, b)) in reference.iter().zip(got.iter()).enumerate() {
            assert_eq!(
                a.to_bits(),
                b.to_bits(),
                "chunk {chunk}, column {k}: {a} vs {b}"
            );
        }
    }
    // The accelerations are not all zero, or the assertion is about nothing.
    assert!(
        reference[3].abs() + reference[4].abs() > 1e-6,
        "the imu read no acceleration at all"
    );
}
