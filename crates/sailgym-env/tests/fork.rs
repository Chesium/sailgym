//! `Episode::fork`: a planning branch is the live episode's future, and
//! nothing a branch does reaches the live episode.
//!
//! Every check here compares a fork against **deterministic reset/replay of
//! the live prefix** — a fresh episode from the same seed, driven by the same
//! logged actions — which is the correctness reference the ESE3990 plan
//! (§4.3, "Branching is a required early feasibility check") names. Equality
//! is bit-for-bit on the 13 state scalars, the observation vector, the
//! outcome, the executed physics steps and the capsize accumulator.
//!
//! The boundaries covered are the ones the plan lists: a pending capsize
//! crossing, a near-gate state ending in a finish, a failure, a nearly
//! exhausted step budget, a fork from a policy-driven episode, candidate
//! order, and a fork of an episode awaiting a `NextStep` reset.

use sailgym_agent::spec::Cadence;
use sailgym_agent::RuleSailor;
use sailgym_course::{Route, Vec2};
use sailgym_env::episode::{manual_source, EpisodeConfig, Source};
use sailgym_env::outcome::{AutoresetMode, Bounds};
use sailgym_env::{Episode, Outcome, TerminationReason};
use sailgym_physics::scenario::load_shipped;
use sailgym_physics::state::{STATE_FIELDS, STATE_LEN};

const CADENCE: u32 = 10;

/// Everything about one decision boundary that a branch must reproduce.
#[derive(Clone, Debug, PartialEq)]
struct Snap {
    state: [u64; STATE_LEN],
    obs: Vec<u64>,
    outcome: Outcome,
    steps: u64,
    capsized: bool,
    pending: Option<u64>,
}

fn snap(ep: &mut Episode) -> Snap {
    let state = ep.state().to_array().map(f64::to_bits);
    let capsize = *ep.simulation().capsize();
    Snap {
        state,
        obs: ep.observation().iter().map(|v| v.to_bits()).collect(),
        outcome: ep.outcome(),
        steps: ep.steps(),
        capsized: capsize.capsized,
        pending: capsize.pending_since().map(f64::to_bits),
    }
}

fn assert_snap_eq(a: &Snap, b: &Snap, what: &str) {
    for ((name, x), y) in STATE_FIELDS.iter().zip(&a.state).zip(&b.state) {
        assert_eq!(x, y, "{what}: state field `{name}` differs");
    }
    assert_eq!(a, b, "{what}");
}

/// Task R from the plan: the first leg of `courses/reach.json`.
fn reach_config(max_steps: u64) -> EpisodeConfig {
    let mut cfg = EpisodeConfig::new(load_shipped("free_sail").expect("shipped"));
    cfg.route = Some(
        Route::waypoints(Vec2::new(0.0, 0.0), &[Vec2::new(30.0, 0.0)], 5.0).expect("a valid gate"),
    );
    cfg.bounds = Bounds::Rect {
        min: [-80.0, -80.0],
        max: [120.0, 100.0],
    };
    cfg.max_steps = Some(max_steps);
    cfg.autoreset = AutoresetMode::Disabled;
    cfg
}

fn manual(cfg: &EpisodeConfig, seed: u64) -> Episode {
    Episode::new(cfg.clone(), manual_source(Cadence::new(CADENCE)), seed).expect("valid")
}

/// One decision period with `action`. Returns `None` once terminal.
fn drive(ep: &mut Episode, action: &[f64]) -> Option<Snap> {
    if ep.outcome().is_terminal() {
        return None;
    }
    ep.push_action(action).expect("in bounds");
    ep.step().expect("a decision boundary");
    Some(snap(ep))
}

/// Drive with `actions` until they run out or the episode ends.
fn drive_all(ep: &mut Episode, actions: &[Vec<f64>]) -> Vec<Snap> {
    actions.iter().map_while(|a| drive(ep, a)).collect()
}

/// A fresh episode from `seed`, replaying `prefix` then `future`.
fn replay(cfg: &EpisodeConfig, seed: u64, prefix: &[Vec<f64>], future: &[Vec<f64>]) -> Vec<Snap> {
    let mut ep = manual(cfg, seed);
    let p = drive_all(&mut ep, prefix);
    assert_eq!(p.len(), prefix.len(), "the replayed prefix ended early");
    drive_all(&mut ep, future)
}

/// The reference trajectory: the rule sailor sailing R, its actions logged.
fn rule_sailor_reference(cfg: &EpisodeConfig, seed: u64) -> (Vec<Vec<f64>>, Outcome) {
    let source = Source::Policy(Box::new(RuleSailor::new()));
    let mut ep = Episode::new(cfg.clone(), source, seed).expect("valid");
    while !ep.outcome().is_terminal() {
        ep.step().expect("a decision boundary");
    }
    let actions = ep.decisions().into_iter().map(|d| d.action).collect();
    (actions, ep.outcome())
}

fn the_live_episode_is_unchanged_by(live: &mut Episode, branch_work: impl FnOnce(&Episode)) {
    let before = snap(live);
    branch_work(live);
    assert_snap_eq(&snap(live), &before, "the live episode moved");
}

#[test]
fn a_fork_near_the_gate_finishes_exactly_where_replay_does() {
    let cfg = reach_config(9_000);
    let seed = 3;
    let (actions, outcome) = rule_sailor_reference(&cfg, seed);
    let Outcome::Finished { time } = outcome else {
        panic!("the rule sailor did not finish R: {outcome:?}");
    };
    assert!(time > 1.0);

    // A manual replay of the rule sailor reproduces it exactly.
    let full = replay(&cfg, seed, &[], &actions);
    assert_eq!(full.len(), actions.len());
    assert_eq!(full.last().unwrap().outcome, outcome);

    // Fork three decisions before the gate passage: inside the last metre or
    // two of the leg, where the cut test and the passage test both run.
    let j = actions.len() - 3;
    let mut live = manual(&cfg, seed);
    drive_all(&mut live, &actions[..j]);
    assert!(!live.outcome().is_terminal());
    let x = live.state().x;
    assert!(
        x < 30.0 && x > 25.0,
        "fork point x = {x} is not near the gate"
    );

    let mut branch = live.fork().expect("forkable");
    assert_snap_eq(&snap(&mut branch), &snap(&mut live), "fork at birth");

    let b = drive_all(&mut branch, &actions[j..]);
    let r = replay(&cfg, seed, &actions[..j], &actions[j..]);
    assert_eq!(b.len(), r.len());
    for (k, (x, y)) in b.iter().zip(&r).enumerate() {
        assert_snap_eq(x, y, &format!("near-gate branch, decision {k}"));
    }
    assert_eq!(b.last().unwrap().outcome, outcome);

    // The live episode never saw the branch, and continues to the same end.
    let l = drive_all(&mut live, &actions[j..]);
    assert_eq!(l, r);
}

#[test]
fn a_fork_of_a_policy_driven_episode_is_manually_driven_and_exact() {
    let cfg = reach_config(9_000);
    let seed = 8;
    let (actions, _) = rule_sailor_reference(&cfg, seed);
    let j = actions.len() / 2;

    let source = Source::Policy(Box::new(RuleSailor::new()));
    let mut live = Episode::new(cfg.clone(), source, seed).expect("valid");
    for _ in 0..j {
        live.step().expect("boundary");
    }
    let mut branch = live.fork().expect("forkable");
    assert!(
        live.push_action(&[0.0, 0.0, -1.0]).is_err(),
        "the live episode is still policy-driven"
    );
    let b = drive_all(&mut branch, &actions[j..]);
    let r = replay(&cfg, seed, &actions[..j], &actions[j..]);
    assert_eq!(b, r);
    // The policy-driven live episode continues on its own to the same end.
    while !live.outcome().is_terminal() {
        live.step().expect("boundary");
    }
    assert_eq!(snap(&mut live), *r.last().unwrap());
}

#[test]
fn a_fork_during_a_pending_capsize_keeps_the_pending_timer() {
    let mut cfg = EpisodeConfig::new(load_shipped("beam_reach_capsize").expect("shipped"));
    cfg.max_steps = Some(40_000);
    cfg.autoreset = AutoresetMode::Disabled;
    let seed = 2;
    let hold = vec![0.0, -1.0, -1.0];

    // Find the first decision boundary with a crossing pending and no capsize.
    let mut live = manual(&cfg, seed);
    let mut prefix = Vec::new();
    loop {
        let s = drive(&mut live, &hold).expect("capsized before a pending boundary");
        prefix.push(hold.clone());
        if s.pending.is_some() && !s.capsized {
            break;
        }
    }
    let pending = live.simulation().capsize().pending_since().unwrap();
    assert!(
        live.state().t > pending,
        "the crossing is genuinely in progress"
    );
    // The public `since` is empty here; only the accessor knows.
    assert_eq!(live.simulation().capsize().since, 0.0);

    let future = vec![hold.clone(); 4_000];
    let mut branch = live.fork().expect("forkable");
    let b = drive_all(&mut branch, &future);
    let r = replay(&cfg, seed, &prefix, &future);
    assert_eq!(b, r);
    assert_eq!(
        b.last().unwrap().outcome,
        Outcome::Terminated(TerminationReason::Capsized)
    );

    // A branch rebuilt from the 13 state scalars alone would restart the timer
    // and capsize later; the fork does not, because it carries the observer.
    // The capsize lands within one decision of the pending threshold.
    let p = live.params().stability.t_capsize;
    let capsize_t = f64::from_bits(b.last().unwrap().state[STATE_LEN - 1]);
    assert!(capsize_t - pending >= p);
    assert!(capsize_t - pending < p + live.params().sim.dt * f64::from(CADENCE));
}

#[test]
fn a_fork_near_the_budget_truncates_on_the_named_step() {
    // A budget that is **not** a multiple of the cadence: the last decision
    // period is shortened, and the branch must shorten it identically.
    let j = 40u64;
    let extra = 4u64;
    let cfg = reach_config(j * u64::from(CADENCE) + extra);
    let seed = 1;
    let a = vec![0.3, -0.5, -1.0];
    let prefix = vec![a.clone(); j as usize];
    let mut live = manual(&cfg, seed);
    drive_all(&mut live, &prefix);
    let mut branch = live.fork().expect("forkable");
    branch.push_action(&a).unwrap();
    let r = branch.step().unwrap();
    assert!(r.truncated && !r.terminated);
    assert_eq!(u64::from(r.steps), extra);
    assert_eq!(branch.steps(), j * u64::from(CADENCE) + extra);
    // Disabled autoreset: no reset happened, and stepping again is refused.
    assert!(!r.autoreset);
    assert!(branch.step().is_err());
    let rr = replay(&cfg, seed, &prefix, std::slice::from_ref(&a));
    assert_eq!(snap(&mut branch), rr[0]);
    assert_eq!(live.steps(), j * u64::from(CADENCE));
}

#[test]
fn candidate_order_cannot_change_any_candidate_or_the_live_episode() {
    let cfg = reach_config(9_000);
    let seed = 5;
    let mut live = manual(&cfg, seed);
    drive_all(&mut live, &vec![vec![0.1, -0.8, -1.0]; 60]);

    let candidates: Vec<Vec<Vec<f64>>> = [(0.0, -1.0), (1.0, 1.0), (-0.6, 0.2), (0.4, -0.3)]
        .iter()
        .map(|&(r, s)| vec![vec![r, s, -1.0]; 40])
        .collect();

    let run = |live: &Episode, c: &[Vec<f64>]| {
        let mut b = live.fork().unwrap();
        drive_all(&mut b, c)
    };
    let mut forward = Vec::new();
    the_live_episode_is_unchanged_by(&mut live, |l| {
        forward = candidates.iter().map(|c| run(l, c)).collect();
    });
    let mut backward = Vec::new();
    the_live_episode_is_unchanged_by(&mut live, |l| {
        backward = candidates.iter().rev().map(|c| run(l, c)).collect();
    });
    backward.reverse();
    assert_eq!(forward, backward);
    // Distinct candidates really did produce distinct futures.
    assert_ne!(forward[0].last(), forward[1].last());
}

#[test]
fn a_fork_of_an_episode_awaiting_a_next_step_reset_is_terminal() {
    let mut cfg = reach_config(25 * u64::from(CADENCE));
    cfg.autoreset = AutoresetMode::NextStep;
    let mut live = manual(&cfg, 0);
    let mut last = None;
    for _ in 0..25 {
        live.push_action(&[0.0, -1.0, -1.0]).unwrap();
        last = Some(live.step().unwrap());
    }
    assert!(last.unwrap().truncated);
    let mut branch = live.fork().unwrap();
    assert_eq!(branch.outcome(), Outcome::Truncated);
    branch.push_action(&[0.0, -1.0, -1.0]).unwrap();
    assert!(
        branch.step().is_err(),
        "a branch never simulates through a reset"
    );
    // The live episode's own NextStep reset is unaffected.
    live.push_action(&[0.0, -1.0, -1.0]).unwrap();
    assert!(live.step().unwrap().autoreset);
}
