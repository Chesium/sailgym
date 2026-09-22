//! What ends an episode, where the water stops, what a step is worth, and
//! which autoreset convention produced a log (v2 section 06 task 6.1).
//!
//! # `Outcome` is an enum, and that is the whole point
//!
//! Gymnasium distinguishes task **termination** from time-limit
//! **truncation**, and conflating them biases value bootstrapping — silently,
//! in the returns, which is where this section's whole risk lives (RV34). A
//! bootstrapped value target is `r + γ·V(s')` after a truncation and `r`
//! after a termination; a `bool done` cannot tell a learner which to use, so
//! no `bool done` exists here. [`Outcome::terminated`] and
//! [`Outcome::truncated`] are separate, and
//! [`Outcome::is_terminal`] is documented as their **union**, derived, never
//! a replacement for either. `recording::tests::no_done_flag_in_this_crate`
//! greps the crate's own sources for the shape this rule forbids.
//!
//! # The order the conditions are tested in is fixed and explicit
//!
//! F9.4 fixes force summation order for the same reason this file fixes
//! outcome order: two conditions that can hold on the same step must resolve
//! the same way on every run and on every stack.
//! [`crate::episode::Episode`] tests, in this order:
//!
//! 1. capsize (F6.10's accumulator, **read** from the simulation and never
//!    recomputed — RV36);
//! 2. out of bounds;
//! 3. a mark missed;
//! 4. the route finished;
//! 5. the step budget.
//!
//! # Nothing here is a physical coefficient
//!
//! [`Bounds`] is a sailing area, [`Reward`] is an experiment parameter and
//! the autoreset mode is a logging convention. v1 brief §43 governs
//! `parameters.rs`; v2 F14.9 says in as many words that it does not govern
//! anything above the crate boundary. No number in this file reaches a force,
//! and no default value here was chosen to make a scenario look better —
//! [`Bounds::Unbounded`] and [`ZeroReward`] are the defaults precisely so
//! that this crate invents no number at all.

use serde::{Deserialize, Serialize};

use sailgym_course::{Guidance, Progress, Vec2};
use sailgym_physics::state::{BoatState, Controls};

// ---------------------------------------------------------------------------
// Outcome
// ---------------------------------------------------------------------------

/// Why an episode was terminated — a task condition, never a time limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminationReason {
    /// F6.10's `capsized` flag went true. The flag is **read** from the
    /// simulation, which accumulates `|φ| > φ_capsize` over `t_capsize`
    /// continuously; recomputing it from the current state is RV36 and would
    /// make capsize fire early, or never.
    Capsized,
    /// The boat left the configured sailing area ([`Bounds`]).
    OutOfBounds,
    /// The boat crossed the current mark's plane, in the leg's direction,
    /// without satisfying the side-and-clearance clause of F15.3 — it cut
    /// the mark. See [`crate::episode::Episode`] for how that is detected
    /// without a second copy of the passage rule.
    MarkMissed,
}

impl TerminationReason {
    /// The name used in logs and in a research envelope.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Capsized => "capsized",
            Self::OutOfBounds => "out_of_bounds",
            Self::MarkMissed => "mark_missed",
        }
    }
}

/// Where an episode has got to.
///
/// An **enum, not a bool**: see the module documentation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Still sailing.
    Running,
    /// The route was completed. `time` is the simulated time of the step
    /// that completed it, in seconds since the episode's reset.
    Finished { time: f64 },
    /// A task condition ended the episode.
    Terminated(TerminationReason),
    /// The **step budget** ended the episode, and nothing else ever produces
    /// this variant. Gymnasium's `TimeLimit` wrapper is not used (F17.4): a
    /// wrapper-supplied truncation would make the single-env and the
    /// vectorised paths disagree about episode boundaries.
    Truncated,
}

impl Outcome {
    /// Whether the episode ended for a **task** reason: `Finished` or
    /// `Terminated`. This is the flag a learner must **not** bootstrap
    /// through.
    pub fn terminated(self) -> bool {
        matches!(self, Self::Finished { .. } | Self::Terminated(_))
    }

    /// Whether the episode ended because the step budget ran out. This is
    /// the flag a learner **must** bootstrap through.
    pub fn truncated(self) -> bool {
        matches!(self, Self::Truncated)
    }

    /// The union of [`Outcome::terminated`] and [`Outcome::truncated`],
    /// **derived** for the runner's own control flow.
    ///
    /// It is not a `done` flag and it is not returned by any batch API: a
    /// caller that receives only this has lost the distinction the enum
    /// exists to keep (RV34). `step_all` reports the two masks separately
    /// and never their union.
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
    }

    /// The name used in logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Finished { .. } => "finished",
            Self::Terminated(r) => r.as_str(),
            Self::Truncated => "truncated",
        }
    }

    /// The termination reason, if this outcome is a termination.
    pub fn reason(self) -> Option<TerminationReason> {
        match self {
            Self::Terminated(r) => Some(r),
            _ => None,
        }
    }

    /// The finish time, if the route was completed.
    pub fn finish_time(self) -> Option<f64> {
        match self {
            Self::Finished { time } => Some(time),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Bounds
// ---------------------------------------------------------------------------

/// The sailing area, in the world frame `W` (F2), metres.
///
/// A configuration, not a coefficient: the default is [`Bounds::Unbounded`]
/// so that this crate ships no distance of its own. Stored as `[f64; 2]`
/// rather than as [`Vec2`] because `Vec2` derives `Serialize` and not
/// `Deserialize`, and a research envelope has to read back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bounds {
    /// No boundary. The default: no number is invented.
    #[default]
    Unbounded,
    /// An axis-aligned box, `min` and `max` inclusive.
    Rect { min: [f64; 2], max: [f64; 2] },
}

/// Why a [`Bounds`] is unusable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundsError {
    /// A corner is not finite.
    NotFinite,
    /// `min` is not below `max` on one or both axes.
    Inverted,
}

impl std::fmt::Display for BoundsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFinite => write!(f, "a bounds corner is not finite"),
            Self::Inverted => write!(f, "bounds min is not below max on both axes"),
        }
    }
}

impl std::error::Error for BoundsError {}

impl Bounds {
    /// Whether `p` is inside. Inclusive at the edge, so a boat exactly on the
    /// line is still sailing: the half-open convention belongs to *crossing*
    /// tests (`passage.rs`), and this is a containment test.
    pub fn contains(&self, p: Vec2) -> bool {
        match self {
            Self::Unbounded => true,
            Self::Rect { min, max } => {
                p.x >= min[0] && p.x <= max[0] && p.y >= min[1] && p.y <= max[1]
            }
        }
    }

    /// Reject a box that cannot be sailed in.
    pub fn validate(&self) -> Result<(), BoundsError> {
        match self {
            Self::Unbounded => Ok(()),
            Self::Rect { min, max } => {
                if !min.iter().chain(max.iter()).all(|v| v.is_finite()) {
                    return Err(BoundsError::NotFinite);
                }
                if min[0] >= max[0] || min[1] >= max[1] {
                    return Err(BoundsError::Inverted);
                }
                Ok(())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The autoreset convention
// ---------------------------------------------------------------------------

/// Which autoreset convention an episode was stepped under.
///
/// The variant names, and their serialised text, are **Gymnasium's own**:
/// `gymnasium.vector.AutoresetMode` spells its values `"NextStep"`,
/// `"SameStep"` and `"Disabled"` (gymnasium 1.3.0,
/// `gymnasium/vector/vector_env.py:32-38`). Using the same vocabulary is the
/// point of recording it: a log says which convention produced it, in words
/// the other side of the binding already uses.
///
/// See [`crate::autoreset`] for what each one does, and
/// `crates/sailgym-env/tests/returns.rs` for the numeric proof that the two
/// live conventions produce the same discounted returns. The convention is
/// **checked**, never assumed: `docs/v2/progress/06-handoff.md` §3 records
/// which version established the semantics and how it was read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoresetMode {
    /// The terminating step reports the **final** observation with its
    /// flags set; the **next** call ignores its action, resets, and returns
    /// the reset observation with reward `0` and both flags clear.
    ///
    /// Gymnasium's default for `SyncVectorEnv` and `AsyncVectorEnv`
    /// (1.3.0, `gymnasium/vector/sync_vector_env.py:68`), and the default
    /// here for the same reason: an env that disagrees with the default of
    /// the thing that will wrap it is a bug waiting for section 07.
    #[default]
    NextStep,
    /// The terminating step resets immediately: its flags are set, its
    /// reward is the terminating step's, the returned observation is the
    /// **reset** one, and the final observation is reported separately
    /// (Gymnasium puts it in `infos["final_obs"]`; here it is
    /// [`crate::vec_env::VecEnv::final_obs`]).
    SameStep,
    /// No autoreset. A terminal episode stays terminal until the caller
    /// resets it, and a further step is refused rather than silently
    /// continuing.
    Disabled,
}

impl AutoresetMode {
    /// The name used in logs and in a research envelope — Gymnasium's own
    /// spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NextStep => "NextStep",
            Self::SameStep => "SameStep",
            Self::Disabled => "Disabled",
        }
    }

    /// The two conventions that autoreset. [`AutoresetMode::Disabled`] is
    /// not one of them, and the returns test compares exactly these.
    pub const AUTORESETTING: [Self; 2] = [Self::NextStep, Self::SameStep];
}

// ---------------------------------------------------------------------------
// Reward
// ---------------------------------------------------------------------------

/// Everything a reward may look at, for one completed physics step.
///
/// Deliberately a set of borrows of **published** quantities and not a
/// `&Simulation`: a reward that could reach `Simulation::forces` would depend
/// on how the caller chunked its `advance` calls, which is F14.7's trap
/// wearing a different hat.
pub struct RewardContext<'a> {
    /// The F3 state after this step.
    pub st: &'a BoatState,
    /// The F3 state before it.
    pub prev: &'a BoatState,
    /// The controls in force during it.
    pub controls: &'a Controls,
    /// What the course layer says the boat is being asked to sail, if there
    /// is a route.
    pub guidance: Option<&'a Guidance>,
    /// Where the boat has got to on the route, if there is one.
    pub progress: Option<&'a Progress>,
    /// The outcome **this step** produced.
    pub outcome: Outcome,
    /// s, the fixed physics timestep.
    pub dt: f64,
    /// The episode step index this observation describes.
    pub step: u64,
}

/// A reward function.
///
/// **No reward function ships.** v2 `docs/v2/prds/06-env.md` is explicit that
/// a reward is an experiment parameter and not an environment constant, so
/// the only implementation here is [`ZeroReward`] and everything else is
/// configured by the experiment. What this crate owns is the *shape*: called
/// once per completed physics step, summed over a decision period, pure in
/// its context, and identified in the research envelope by name and version
/// so two runs cannot be compared across a reward change without noticing.
///
/// `Send` because [`crate::vec_env::VecEnv`] steps episodes across rayon
/// threads (F16.5). It is **not** `Sync`: each episode owns its own reward,
/// so nothing is shared.
pub trait Reward: Send {
    /// The reward's stable id, as it appears in an experiment log.
    fn id(&self) -> &'static str;

    /// Bumped on any change to what this reward returns from the same
    /// context.
    fn version(&self) -> u32;

    /// Clear any accumulated state. Called once per episode, at reset.
    fn reset(&mut self) {}

    /// The reward for one completed physics step.
    ///
    /// Must be a pure function of the context: it may not read a wall clock
    /// and may not draw from a generator of its own (F9.1, F14.8).
    fn value(&mut self, ctx: &RewardContext<'_>) -> f64;

    /// Clone into a box, so an [`crate::episode::EpisodeConfig`] can be
    /// cloned into N independent episodes.
    fn boxed_clone(&self) -> Box<dyn Reward>;
}

impl Clone for Box<dyn Reward> {
    fn clone(&self) -> Self {
        self.boxed_clone()
    }
}

impl std::fmt::Debug for Box<dyn Reward> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Reward({} v{})", self.id(), self.version())
    }
}

/// The reward that ships: zero, everywhere, always.
///
/// Not a placeholder for a real one. It is the honest default for an
/// environment whose reward is an experiment parameter: a run that did not
/// configure a reward gets zeros in its buffer and a `zero` in its envelope,
/// rather than a number somebody has to go and find the definition of.
#[derive(Clone, Copy, Debug, Default)]
pub struct ZeroReward;

impl ZeroReward {
    pub const ID: &'static str = "zero";
    pub const VERSION: u32 = 1;
}

impl Reward for ZeroReward {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn version(&self) -> u32 {
        Self::VERSION
    }

    fn value(&mut self, _ctx: &RewardContext<'_>) -> f64 {
        0.0
    }

    fn boxed_clone(&self) -> Box<dyn Reward> {
        Box::new(*self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn termination_and_truncation_are_never_the_same_field() {
        // RV34, as an assertion rather than as a comment.
        let cases = [
            (Outcome::Running, false, false, false),
            (Outcome::Finished { time: 12.5 }, true, false, true),
            (
                Outcome::Terminated(TerminationReason::Capsized),
                true,
                false,
                true,
            ),
            (Outcome::Truncated, false, true, true),
        ];
        for (o, terminated, truncated, terminal) in cases {
            assert_eq!(o.terminated(), terminated, "{o:?}");
            assert_eq!(o.truncated(), truncated, "{o:?}");
            assert_eq!(o.is_terminal(), terminal, "{o:?}");
            // The two flags are never both set: a step is truncated or it is
            // terminated, and a learner that saw both would have to guess.
            assert!(!(o.terminated() && o.truncated()), "{o:?}");
        }
        assert_eq!(
            Outcome::Terminated(TerminationReason::MarkMissed).reason(),
            Some(TerminationReason::MarkMissed)
        );
        assert_eq!(Outcome::Truncated.reason(), None);
        assert_eq!(Outcome::Finished { time: 3.0 }.finish_time(), Some(3.0));
        assert_eq!(Outcome::Running.finish_time(), None);
    }

    #[test]
    fn an_outcome_round_trips_through_json() {
        for o in [
            Outcome::Running,
            Outcome::Finished { time: 41.125 },
            Outcome::Terminated(TerminationReason::Capsized),
            Outcome::Terminated(TerminationReason::OutOfBounds),
            Outcome::Terminated(TerminationReason::MarkMissed),
            Outcome::Truncated,
        ] {
            let text = serde_json::to_string(&o).expect("serialises");
            let back: Outcome = serde_json::from_str(&text).expect("reads back");
            assert_eq!(o, back, "{text}");
        }
        assert_eq!(
            serde_json::to_string(&Outcome::Terminated(TerminationReason::OutOfBounds))
                .expect("serialises"),
            r#"{"terminated":"out_of_bounds"}"#
        );
    }

    #[test]
    fn bounds_contain_their_edge_and_reject_an_inverted_box() {
        let b = Bounds::Rect {
            min: [-10.0, -20.0],
            max: [10.0, 20.0],
        };
        assert!(b.validate().is_ok());
        assert!(b.contains(Vec2::new(0.0, 0.0)));
        assert!(b.contains(Vec2::new(10.0, 20.0)), "the edge is inside");
        assert!(b.contains(Vec2::new(-10.0, -20.0)));
        assert!(!b.contains(Vec2::new(10.000_001, 0.0)));
        assert!(!b.contains(Vec2::new(0.0, -20.000_001)));
        assert!(Bounds::Unbounded.contains(Vec2::new(1.0e9, -1.0e9)));

        assert_eq!(
            Bounds::Rect {
                min: [10.0, 0.0],
                max: [-10.0, 1.0]
            }
            .validate(),
            Err(BoundsError::Inverted)
        );
        assert_eq!(
            Bounds::Rect {
                min: [f64::NAN, 0.0],
                max: [1.0, 1.0]
            }
            .validate(),
            Err(BoundsError::NotFinite)
        );
        // A NaN position is outside every box, because every comparison with
        // it is false. Asserted so the behaviour is chosen rather than
        // inherited.
        assert!(!b.contains(Vec2::new(f64::NAN, 0.0)));
    }

    /// The names are Gymnasium's, verbatim, and the default is its default.
    #[test]
    fn the_autoreset_modes_use_gymnasiums_own_vocabulary() {
        assert_eq!(AutoresetMode::default(), AutoresetMode::NextStep);
        for (mode, name) in [
            (AutoresetMode::NextStep, "NextStep"),
            (AutoresetMode::SameStep, "SameStep"),
            (AutoresetMode::Disabled, "Disabled"),
        ] {
            assert_eq!(mode.as_str(), name);
            assert_eq!(
                serde_json::to_string(&mode).expect("serialises"),
                format!("\"{name}\"")
            );
        }
        assert_eq!(
            AutoresetMode::AUTORESETTING,
            [AutoresetMode::NextStep, AutoresetMode::SameStep]
        );
    }

    #[test]
    fn the_shipped_reward_is_zero_and_clones_through_the_box() {
        let mut r: Box<dyn Reward> = Box::new(ZeroReward);
        let st = BoatState::ZERO;
        let c = Controls::default();
        let ctx = RewardContext {
            st: &st,
            prev: &st,
            controls: &c,
            guidance: None,
            progress: None,
            outcome: Outcome::Running,
            dt: 0.005,
            step: 0,
        };
        assert_eq!(r.value(&ctx), 0.0);
        let copy = r.clone();
        assert_eq!(copy.id(), "zero");
        assert_eq!(copy.version(), ZeroReward::VERSION);
        assert_eq!(format!("{copy:?}"), "Reward(zero v1)");
    }
}
