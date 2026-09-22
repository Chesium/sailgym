//! The numeric test that is the point of the autoreset convention
//! (v2 section 06 task 6.3).
//!
//! > An off-by-one here shifts every bootstrapped value by one step and
//! > produces training curves that look merely *mediocre* rather than
//! > broken — which is the worst kind of bug, because it survives review,
//! > survives a demo, and is indistinguishable from "the task is hard" for
//! > as long as anyone is willing to keep tuning.
//!
//! So: one fixed action sequence, one fixed seed, an episode that ends
//! part-way, run under **both** conventions, discounted at **two** factors,
//! and the returns compared. The comparison is `to_bits()` — the tightest
//! bound there is — and it is tight enough to catch a one-step shift
//! because [`wrong_convention_moves_the_returns`] introduces exactly that
//! shift and measures how far it moves them.
//!
//! # Why the returns agree *exactly* rather than approximately
//!
//! The PRD asks for "a stated numeric bound". The bound this test states is
//! **zero ULP**, and that is not luck. The two conventions differ in one
//! thing only: whether the stream carries a neutral record between two
//! episodes. The episodes themselves are the same episodes — same root
//! seed, same chain of `STREAM_SCENARIO` draws, same script indexed by each
//! episode's **own** decision counter — so the per-episode reward sequences
//! are bit-identical, and `discounted_return` sums them forwards in index
//! order, which fixes the floating-point association. Anything less than
//! bit-identical would mean one of those four sentences is false, and a
//! tolerance would hide which.
//!
//! # The convention was read, not remembered
//!
//! `docs/v2/progress/06-handoff.md` §3 names the version, the file, the
//! lines and the URL, and records that **no Gymnasium is pinned in
//! `uv.lock`** — so section 07 must re-check them against whatever it pins.

use sailgym_agent::spec::Cadence;

use sailgym_env::autoreset::{discounted_return, discounted_returns, split_episodes, StepRecord};
use sailgym_env::episode::{manual_source, Episode, EpisodeConfig};
use sailgym_env::outcome::{AutoresetMode, Bounds, Outcome, Reward, RewardContext};

use sailgym_physics::scenario::load_shipped;

/// The two discount factors. A short horizon and a long one, because an
/// off-by-one is worth `(1 − γ)` of the shifted reward and a single γ could
/// be the one that hides it.
const GAMMAS: [f64; 2] = [0.9, 0.99];

/// How many complete episodes each run collects.
const EPISODES: usize = 4;

/// A cap, so a configuration that stops ending episodes fails loudly
/// instead of hanging.
const CALL_CAP: usize = 4000;

/// The fixed action sequence, indexed by each episode's **own** decision
/// counter.
///
/// Indexed per episode and not per call, deliberately: under
/// `AutoresetMode::NextStep` one call per boundary is a reset whose action
/// is ignored, so a globally indexed script would feed the two conventions
/// different actions and the test would fail for a reason that has nothing
/// to do with the convention.
const SCRIPT: [[f64; 3]; 7] = [
    [0.0, -1.0, -1.0],
    [0.4, -1.0, -1.0],
    [0.4, 0.0, -1.0],
    [-0.6, -1.0, -1.0],
    [-0.6, 0.5, -1.0],
    [0.2, -1.0, -1.0],
    [0.0, 1.0, -1.0],
];

/// A reward with the shape that makes an off-by-one visible: a dense term
/// that accumulates, a control-effort penalty, and a **terminal** term that
/// lands on exactly one step.
///
/// It is defined **here, in the experiment**, and not in the crate: a
/// reward is an experiment parameter, not an environment constant, and
/// `sailgym-env` ships only `ZeroReward`. Every number in it is an
/// experiment's choice and none of them reaches a force.
#[derive(Clone, Copy, Debug, Default)]
struct DemoReward;

impl Reward for DemoReward {
    fn id(&self) -> &'static str {
        "returns_demo"
    }

    fn version(&self) -> u32 {
        1
    }

    fn value(&mut self, ctx: &RewardContext<'_>) -> f64 {
        let mut r = ctx.st.u * ctx.dt - 0.05 * ctx.controls.rudder_rate_cmd.abs() * ctx.dt;
        match ctx.outcome {
            Outcome::Terminated(_) => r -= 5.0,
            Outcome::Finished { .. } => r += 10.0,
            _ => {}
        }
        r
    }

    fn boxed_clone(&self) -> Box<dyn Reward> {
        Box::new(*self)
    }
}

/// `gybe` is the only shipped scenario with a `Gust` field, so the chain's
/// seeds actually change its trajectories and the four episodes of a chain
/// are four different episodes rather than one repeated.
fn base(mode: AutoresetMode) -> EpisodeConfig {
    let mut cfg = EpisodeConfig::new(load_shipped("gybe").expect("a shipped scenario"));
    cfg.autoreset = mode;
    cfg.reward = Box::new(DemoReward);
    cfg
}

/// Episodes that **terminate**: a box small enough that the boat always
/// leaves it, and a budget large enough that it never runs out first.
fn terminating(mode: AutoresetMode) -> EpisodeConfig {
    let mut cfg = base(mode);
    cfg.max_steps = Some(6000);
    cfg.bounds = Bounds::Rect {
        min: [-8.0, -8.0],
        max: [8.0, 8.0],
    };
    cfg
}

/// Episodes that **truncate**: no box, and a budget that always runs out.
///
/// Both flags are covered because they are covered *separately*: an
/// off-by-one does not care which flag ended an episode, and a chain
/// engineered to mix the two would be a chain engineered around one
/// scenario's behaviour.
fn truncating(mode: AutoresetMode) -> EpisodeConfig {
    let mut cfg = base(mode);
    cfg.max_steps = Some(300);
    cfg
}

struct Run {
    stream: Vec<StepRecord>,
    outcomes: Vec<Outcome>,
    steps: Vec<u64>,
}

/// Drive one chain of episodes under `mode`, collecting the flat stream a
/// vector API would report.
fn run(config: EpisodeConfig, seed: u64) -> Run {
    let mut ep =
        Episode::new(config, manual_source(Cadence::new(10)), seed).expect("a valid episode");
    let mut stream = Vec::new();
    let mut outcomes = Vec::new();
    let mut steps = Vec::new();
    while outcomes.len() < EPISODES {
        assert!(
            stream.len() < CALL_CAP,
            "{EPISODES} episodes did not end in {CALL_CAP} calls"
        );
        let i = ep.decision_count();
        ep.push_action(&SCRIPT[i % SCRIPT.len()])
            .expect("the script is in bounds");
        let executed = ep.steps();
        let r = ep.step().expect("a valid step");
        stream.push(StepRecord {
            reward: r.reward,
            terminated: r.terminated,
            truncated: r.truncated,
        });
        if r.terminated || r.truncated {
            outcomes.push(r.outcome);
            steps.push(executed + u64::from(r.steps));
        }
    }
    Run {
        stream,
        outcomes,
        steps,
    }
}

/// Task 6.3's acceptance, and section acceptance 3's other half.
#[test]
fn the_two_autoreset_conventions_give_the_same_discounted_returns() {
    compare_conventions("terminating", &terminating, Outcome::terminated);
    compare_conventions("truncating", &truncating, Outcome::truncated);
}

/// One family of episodes, under both conventions.
fn compare_conventions(
    what: &str,
    config: &dyn Fn(AutoresetMode) -> EpisodeConfig,
    flag: fn(Outcome) -> bool,
) {
    let seed = 20_260_922;
    let next = run(config(AutoresetMode::NextStep), seed);
    let same = run(config(AutoresetMode::SameStep), seed);

    // The chain is the same chain: the same episodes, ending the same way,
    // after the same number of physics steps.
    assert_eq!(
        next.outcomes, same.outcomes,
        "{what}: the two chains differ"
    );
    assert_eq!(next.steps, same.steps);
    assert_eq!(next.outcomes.len(), EPISODES);

    // …and this family really is the family it claims to be.
    assert!(
        next.outcomes.iter().copied().all(flag),
        "{what}: the chain ended {:?}",
        next.outcomes
    );

    // The streams differ in exactly one way, and it is the one the
    // convention names: `NextStep` carries a neutral record **after** each
    // boundary. The driver stops on the last boundary, so the last reset
    // call is never made and the difference is `EPISODES − 1`.
    assert_eq!(
        next.stream.len(),
        same.stream.len() + EPISODES - 1,
        "{what}: NextStep must carry exactly one extra, neutral record after each \
         boundary it steps past"
    );

    // The split is where the two conventions are reconciled. That it
    // succeeds is itself the assertion that every `NextStep` reset call was
    // neutral — reward 0, both flags clear.
    let a = split_episodes(AutoresetMode::NextStep, &next.stream)
        .expect("every reset record is neutral");
    let b = split_episodes(AutoresetMode::SameStep, &same.stream).expect("no reset records");
    assert_eq!(a.len(), EPISODES);
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        assert_eq!(x.len(), y.len(), "{what}: episode {i}: different lengths");
        for (k, (p, q)) in x.iter().zip(y.iter()).enumerate() {
            assert_eq!(
                p.to_bits(),
                q.to_bits(),
                "{what}: episode {i}, step {k}: {p} vs {q}"
            );
        }
    }

    // The returns themselves, at two discount factors, bit for bit.
    let mut worst = 0.0f64;
    for gamma in GAMMAS {
        let ra = discounted_returns(AutoresetMode::NextStep, &next.stream, gamma).expect("clean");
        let rb = discounted_returns(AutoresetMode::SameStep, &same.stream, gamma).expect("clean");
        assert_eq!(ra.len(), EPISODES);
        for (i, (x, y)) in ra.iter().zip(rb.iter()).enumerate() {
            worst = worst.max((x - y).abs());
            assert_eq!(
                x.to_bits(),
                y.to_bits(),
                "{what}, γ = {gamma}, episode {i}: NextStep {x} vs SameStep {y}"
            );
        }
        // …and the returns are not all the same number, which would make
        // agreement meaningless.
        let spread = ra.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            - ra.iter().cloned().fold(f64::INFINITY, f64::min);
        assert!(
            spread.abs() > 1e-9,
            "{what}, γ = {gamma}: every return is the same number, {ra:?}"
        );
        eprintln!("returns [{what}] γ={gamma}: {ra:?}");
    }
    // The stated bound: zero. Printed, so the handoff quotes a measurement
    // and not a hope.
    eprintln!("returns [{what}]: max |Δ| between the two conventions = {worst:.3e} (0 ULP)");
    assert_eq!(worst, 0.0);
}

/// The bound is tight enough to catch a one-step shift, demonstrated here
/// permanently rather than only once in a handoff.
///
/// The shift is the one RV33 names: a `NextStep` stream read as though it
/// were a `SameStep` one, so each episode after the first acquires the
/// previous boundary's reset record at its front and every reward in it
/// slides one discount power later.
#[test]
fn wrong_convention_moves_the_returns() {
    let next = run(terminating(AutoresetMode::NextStep), 20_260_922);
    for gamma in GAMMAS {
        let right =
            discounted_returns(AutoresetMode::NextStep, &next.stream, gamma).expect("clean");
        let wrong =
            discounted_returns(AutoresetMode::SameStep, &next.stream, gamma).expect("clean");
        // Episode 0 is before any boundary and is unaffected; every later
        // one moves, and by more than any tolerance a reviewer would think
        // of writing.
        assert_eq!(right[0].to_bits(), wrong[0].to_bits());
        let mut worst = 0.0f64;
        for i in 1..right.len().min(wrong.len()) {
            worst = worst.max((right[i] - wrong[i]).abs());
        }
        eprintln!("returns γ={gamma}: a one-step shift moves a return by up to {worst:.6}");
        assert!(
            worst > 1e-3,
            "γ = {gamma}: the shift moved the returns by only {worst:.3e}; this test cannot \
             see the bug it exists for"
        );
    }
}

/// The arithmetic itself, against a hand-computed value, so a broken
/// `discounted_return` cannot make the two conventions agree on nonsense.
#[test]
fn the_discount_arithmetic_is_hand_checked() {
    let rewards = [1.0, -2.0, 0.5, 4.0];
    let gamma = 0.9;
    let want = 1.0 + 0.9 * -2.0 + 0.81 * 0.5 + 0.729 * 4.0;
    assert!((discounted_return(&rewards, gamma) - want).abs() < 1e-12);
    // Undiscounted is the plain sum, and a zero discount is the first
    // reward alone.
    assert!((discounted_return(&rewards, 1.0) - 3.5).abs() < 1e-12);
    assert_eq!(discounted_return(&rewards, 0.0), 1.0);
}
