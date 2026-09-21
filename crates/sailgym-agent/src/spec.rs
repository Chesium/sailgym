//! The contracts (v2 F14.2, F14.5, F14.6, F14.8; section 05 task 5.1).
//!
//! What an agent declares ([`AgentSpec`]), what it emits ([`Action`]), when it
//! is asked ([`Cadence`]), and the trait itself ([`Agent`]).
//!
//! # `Action` carries a normalised vector, not `Controls`
//!
//! The source discussion sketched `Action::Rates(Controls)`. F14.5 supersedes
//! it: **every** actuation adapter presents `[−1, 1]^k` and denormalises
//! internally, so an agent that emitted `Controls` would have gone round the
//! funnel rather than through it, and the `rate` arm of an ablation would be
//! the only arm whose numbers were not comparable with the others. `Action`
//! therefore carries [`ActionVec`], the normalised action, and
//! [`crate::actuation`] is the one place it becomes `Controls`.
//!
//! # Why `decide` takes `&[f64]`
//!
//! F14.4 says the agent sees only the concatenated observation vector. Spelling
//! that as the parameter type makes it true **by construction**: there is no
//! path from an `Agent` implementation to a `WindField`, a `Route` or another
//! boat's state, because no such value is ever in scope inside `decide`.
//! `tests::the_agent_trait_cannot_reach_the_world` asserts the signature, and
//! `docs/v2/progress/05-handoff.md` records the compiler's own refusal of an
//! agent that tries.

use serde::{Deserialize, Serialize};

use sailgym_physics::rng::{Pcg32, STREAM_AGENT};

/// The widest normalised action this crate's adapters present.
///
/// Three today (`rate`: rudder, sheet, release) with one spare, so [`Action`]
/// is `Copy` and `decide` allocates nothing. Raising it is a deliberate,
/// versioned change: an adapter wider than this is a new adapter, and a new
/// adapter bumps [`crate::actuation::Actuation::version`] anyway.
pub const ACTION_MAX_DIM: usize = 4;

/// A normalised action: `k` scalars in `[−1, 1]`, with `k` at most
/// [`ACTION_MAX_DIM`] (F14.5).
///
/// The bounds are checked **here**, at construction, rather than being assumed
/// by each adapter: an out-of-range action is a contract violation, and a
/// contract that is only checked in the adapter that happens to look is not a
/// contract.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActionVec {
    len: usize,
    values: [f64; ACTION_MAX_DIM],
}

impl ActionVec {
    /// Build from a slice, rejecting anything outside `[−1, 1]`, anything
    /// non-finite, and anything wider than [`ACTION_MAX_DIM`].
    pub fn new(values: &[f64]) -> Result<Self, SpecError> {
        if values.len() > ACTION_MAX_DIM {
            return Err(SpecError::ActionTooWide {
                got: values.len(),
                max: ACTION_MAX_DIM,
            });
        }
        let mut out = [0.0; ACTION_MAX_DIM];
        for (i, v) in values.iter().enumerate() {
            if !v.is_finite() || *v < -1.0 || *v > 1.0 {
                return Err(SpecError::ActionOutOfBounds {
                    index: i,
                    value: *v,
                });
            }
            out[i] = *v;
        }
        Ok(Self {
            len: values.len(),
            values: out,
        })
    }

    /// The action, `len()` scalars long.
    pub fn as_slice(&self) -> &[f64] {
        &self.values[..self.len]
    }

    /// How many scalars the action carries. Must equal the adapter's `dim()`.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the action is empty. (`clippy::len_without_is_empty`.)
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// What an agent emits.
///
/// One variant, because one [`ActionSpace`] is implemented. `Setpoint` is not
/// scaffolded here: an `Action::Setpoint` with no `Helm` beneath it would be a
/// value nothing could consume, and task 5.6 is deferred.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// F14.2 `Rates`: the normalised action the registered adapter
    /// denormalises. **Not `Controls`** — see the module documentation.
    Rates(ActionVec),
}

impl Action {
    /// The normalised action, whatever the space.
    pub fn values(&self) -> &[f64] {
        match self {
            Self::Rates(a) => a.as_slice(),
        }
    }

    /// The space this action belongs to.
    pub fn space(&self) -> ActionSpace {
        match self {
            Self::Rates(_) => ActionSpace::Rates,
        }
    }
}

/// Where in the control chain `route → guidance → tactic → setpoint → helm →
/// Controls` an agent is substituted (F14.2).
///
/// **Exactly two variants, and one of them is refused.** F14.2 fixes the
/// enumeration at two, and the enumeration is *logged data*: an episode header
/// written today and one written after task 5.6 lands must use the same
/// vocabulary, or two runs of the same experiment become incomparable for a
/// reason that has nothing to do with the experiment. So the name is reserved
/// and [`AgentSpec::validate`] refuses it, loudly, naming the deferral —
/// rather than the variant being added later and quietly renumbering the
/// serialised form.
///
/// Nothing beneath `Setpoint` exists: there is no `Setpoint` struct, no `Helm`,
/// no adapter and no `Action` variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionSpace {
    /// The agent replaces tactic and helm. Shared machinery below it: none.
    Rates,
    /// The agent replaces the tactic only, driving the one shared `Helm`.
    /// **Deferred** (task 5.6): engaged/released semantics have no contract
    /// yet, because a zero rudder-rate command currently means
    /// *released, self-centring* and a position servo needs *engaged, hold*.
    Setpoint,
}

impl ActionSpace {
    /// Whether this section implements the space.
    pub fn is_implemented(self) -> bool {
        matches!(self, Self::Rates)
    }

    /// The name used in logs and in [`sailgym_physics::recording::ActionIdentity`].
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rates => "rates",
            Self::Setpoint => "setpoint",
        }
    }
}

/// How often an agent decides (F14.6).
///
/// `period_steps = 10` is 20 Hz over 200 Hz physics at the F7 default
/// `dt = 0.005`. The value is frozen at reset and recorded in the episode
/// header; it is **not** a physical coefficient (F14.9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cadence {
    pub period_steps: u32,
}

impl Cadence {
    /// Every step — the degenerate cadence, and the one a manual source uses
    /// when the browser pushes a new action each frame.
    pub const EVERY_STEP: Self = Self { period_steps: 1 };

    pub const fn new(period_steps: u32) -> Self {
        Self { period_steps }
    }

    pub fn validate(&self) -> Result<(), SpecError> {
        if self.period_steps == 0 {
            return Err(SpecError::ZeroCadence);
        }
        Ok(())
    }

    /// Whether a decision happens on this **episode** step (F14.6.1).
    ///
    /// The argument is the episode step counter and nothing else: not a
    /// counter that resets per `advance` call, not elapsed time. That is trap
    /// 2 of the section, and `tests/determinism.rs` asserts it over six
    /// different chunkings of the same episode.
    pub fn decides_at(&self, episode_step: u64) -> bool {
        self.period_steps != 0 && episode_step.is_multiple_of(u64::from(self.period_steps))
    }

    /// Steps from `episode_step` to the **next** decision after it.
    ///
    /// A driver uses this to split an `advance(n)` at decision boundaries, so
    /// that where the decisions land is a property of the episode and not of
    /// the caller's chunk size. Never zero, so a driver cannot spin.
    pub fn steps_to_next_decision(&self, episode_step: u64) -> u64 {
        if self.period_steps == 0 {
            return u64::MAX;
        }
        let period = u64::from(self.period_steps);
        period - (episode_step % period)
    }
}

/// Fixed, declarative metadata, serialised into the episode header and never
/// inferred (F14.2, F14.6.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSpec {
    /// The agent's stable id: `manual`, `rule_sailor`, `policy_onnx`, …
    pub id: String,
    /// Bumped on **any** change to what the agent decides from the same
    /// observation. The digest of a comparison depends on it.
    pub version: u32,
    pub action_space: ActionSpace,
    pub cadence: Cadence,
}

impl AgentSpec {
    pub fn new(id: &str, version: u32, action_space: ActionSpace, cadence: Cadence) -> Self {
        Self {
            id: id.to_string(),
            version,
            action_space,
            cadence,
        }
    }

    /// Reject a spec no runner in this section could honour.
    pub fn validate(&self) -> Result<(), SpecError> {
        if self.id.trim().is_empty() {
            return Err(SpecError::EmptyId);
        }
        self.cadence.validate()?;
        if !self.action_space.is_implemented() {
            return Err(SpecError::ActionSpaceDeferred(self.action_space));
        }
        Ok(())
    }
}

/// UI-rate introspection, written for observers and **read by no controller**.
///
/// The same discipline `CapsizeState` has under F6.10: it is reported, never
/// acted on. An ordered `Vec`, never a hash container, so two runs produce the
/// same text (F9.3).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct AgentDebug {
    pub notes: Vec<(String, f64)>,
}

/// One controller, of any kind: a human's action source, a rule sailor, a
/// polar racer or a policy.
///
/// Object-safe, so `Box<dyn Agent>` works and a registry is a `Vec` and not a
/// hash container (F9.3).
///
/// # Three constraints, because each is a determinism bug waiting to happen
///
/// 1. **`decide` may not read a wall clock.** Physics never does (F9.1) and an
///    agent inside the step loop is physics for this purpose.
///    `tests/determinism.rs::no_wall_clock_in_the_v2_crates` is the same grep
///    the physics crate has always run, pointed at the new crates.
/// 2. **Agent randomness goes through `Pcg32::stream(STREAM_AGENT)`**
///    ([`agent_rng`]), with per-sensor substreams below it
///    ([`crate::sensor::sensor_stream`]). Two RNGs feeding one episode is how a
///    deterministic environment stops being reproducible, and a private
///    generator inside an agent is a second RNG however it is seeded.
/// 3. **[`Agent::debug`] is not an input.** It is written for observers and
///    read by no controller — F6.10's discipline, applied to agents.
pub trait Agent {
    /// The declared metadata. Constant for the episode.
    fn spec(&self) -> AgentSpec;

    /// Called once per episode, before the first decision, and the **only**
    /// place an agent may seed itself.
    ///
    /// `fields` is the observation layout the episode was configured with, in
    /// column order — an agent that cares which column is which reads it here
    /// rather than assuming an order that a sensor change would silently
    /// invalidate (F14.3: there is no `OBS_LEN`).
    fn reset(&mut self, fields: &[String], rng: &mut Pcg32);

    /// Decide, from the concatenated observation vector and nothing else.
    ///
    /// `obs.len()` is the sum of the configured sensors' widths.
    fn decide(&mut self, obs: &[f64], rng: &mut Pcg32) -> Action;

    /// Introspection for observers. Never read by a runner.
    fn debug(&self) -> AgentDebug {
        AgentDebug::default()
    }
}

/// The agent's own RNG stream (F14.8).
///
/// Derived from the episode's root generator with `Pcg32::stream`, which takes
/// `&self` and does not advance the parent — so attaching an agent cannot shift
/// the wind field by a single bit, which is the whole point of the named-stream
/// table in `rng.rs`.
pub fn agent_rng(root: &Pcg32) -> Pcg32 {
    root.stream(STREAM_AGENT)
}

/// What [`AgentSpec::validate`] and [`ActionVec::new`] refuse.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpecError {
    EmptyId,
    ZeroCadence,
    ActionSpaceDeferred(ActionSpace),
    ActionTooWide { got: usize, max: usize },
    ActionWidthMismatch { got: usize, want: usize },
    ActionOutOfBounds { index: usize, value: f64 },
}

impl std::fmt::Display for SpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyId => write!(f, "an AgentSpec must carry a non-empty id"),
            Self::ZeroCadence => write!(
                f,
                "Cadence::period_steps must be at least 1: a period of 0 decides never (F14.6)"
            ),
            Self::ActionSpaceDeferred(s) => write!(
                f,
                "ActionSpace::{} is a reserved name with no implementation: task 5.6 (Helm) is \
                 deferred until engaged/released semantics have a contract and a consumer \
                 (v2 F14.2)",
                s.as_str()
            ),
            Self::ActionTooWide { got, max } => write!(
                f,
                "an action of {got} scalars is wider than ACTION_MAX_DIM = {max}"
            ),
            Self::ActionWidthMismatch { got, want } => write!(
                f,
                "an action of {got} scalars was pushed where {want} are expected"
            ),
            Self::ActionOutOfBounds { index, value } => write!(
                f,
                "action[{index}] = {value} is outside [-1, 1]: every adapter presents \
                 normalised bounds (F14.5)"
            ),
        }
    }
}

impl std::error::Error for SpecError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_action_is_normalised_or_it_is_refused() {
        assert!(ActionVec::new(&[-1.0, 0.0, 1.0]).is_ok());
        assert_eq!(
            ActionVec::new(&[0.0, 1.000_000_1]),
            Err(SpecError::ActionOutOfBounds {
                index: 1,
                value: 1.000_000_1
            })
        );
        assert!(matches!(
            ActionVec::new(&[f64::NAN]),
            Err(SpecError::ActionOutOfBounds { .. })
        ));
        assert_eq!(
            ActionVec::new(&[0.0; ACTION_MAX_DIM + 1]),
            Err(SpecError::ActionTooWide {
                got: ACTION_MAX_DIM + 1,
                max: ACTION_MAX_DIM
            })
        );
        let a = ActionVec::new(&[0.25, -0.5]).expect("in bounds");
        assert_eq!(a.len(), 2);
        assert!(!a.is_empty());
        assert_eq!(a.as_slice(), &[0.25, -0.5]);
        assert!(ActionVec::new(&[]).expect("empty is legal").is_empty());
    }

    #[test]
    fn the_setpoint_space_is_a_reserved_name_and_is_refused() {
        // F14.2 fixes the enumeration at two variants because it is logged
        // data; task 5.6 defers the implementation. Both halves are asserted,
        // so neither can be quietly dropped.
        assert!(ActionSpace::Rates.is_implemented());
        assert!(!ActionSpace::Setpoint.is_implemented());
        assert_eq!(ActionSpace::Setpoint.as_str(), "setpoint");

        let spec = AgentSpec::new("stub", 1, ActionSpace::Setpoint, Cadence::new(10));
        let why = spec
            .validate()
            .expect_err("Setpoint has no Helm beneath it");
        assert_eq!(why, SpecError::ActionSpaceDeferred(ActionSpace::Setpoint));
        assert!(format!("{why}").contains("5.6"), "{why}");

        // The reserved name serialises stably, so a header written now and one
        // written after 5.6 lands use the same vocabulary.
        assert_eq!(
            serde_json::to_string(&ActionSpace::Setpoint).expect("serialises"),
            "\"setpoint\""
        );
    }

    #[test]
    fn a_spec_is_validated() {
        assert!(
            AgentSpec::new("manual", 1, ActionSpace::Rates, Cadence::new(10))
                .validate()
                .is_ok()
        );
        assert_eq!(
            AgentSpec::new("  ", 1, ActionSpace::Rates, Cadence::new(1)).validate(),
            Err(SpecError::EmptyId)
        );
        assert_eq!(
            AgentSpec::new("manual", 1, ActionSpace::Rates, Cadence::new(0)).validate(),
            Err(SpecError::ZeroCadence)
        );
    }

    #[test]
    fn cadence_keys_off_the_episode_step() {
        let c = Cadence::new(10);
        for step in 0..100u64 {
            assert_eq!(c.decides_at(step), step % 10 == 0, "step {step}");
        }
        // Never zero, so a driver that splits on it cannot spin.
        for step in 0..100u64 {
            let d = c.steps_to_next_decision(step);
            assert!((1..=10).contains(&d), "step {step}: {d}");
            assert!(
                c.decides_at(step + d),
                "step {step} + {d} is not a decision"
            );
            for k in 1..d {
                assert!(!c.decides_at(step + k), "step {step} + {k} decides early");
            }
        }
        assert!(Cadence::EVERY_STEP.decides_at(7));
    }

    /// The observation vector is the whole of an agent's world (F14.4).
    ///
    /// A source assertion rather than a `trybuild` case, so the check costs no
    /// dependency: `decide` takes `&[f64]`, so no `WorldView`, `WindField`,
    /// `Route` or `BoatState` is ever in scope inside an implementation. The
    /// compiler's own refusal of an agent that tries is recorded in
    /// `docs/v2/progress/05-handoff.md` §5.
    #[test]
    fn the_agent_trait_cannot_reach_the_world() {
        let src = include_str!("spec.rs");
        let body = src
            .split_once("pub trait Agent {")
            .expect("the Agent trait")
            .1
            .split_once("\n}\n")
            .expect("a closed trait")
            .0;
        let decide = body
            .lines()
            .find(|l| l.trim_start().starts_with("fn decide"))
            .expect("Agent::decide");
        assert!(
            decide.contains("obs: &[f64]"),
            "Agent::decide must take the concatenated vector (F14.4): {decide}"
        );
        for forbidden in ["WorldView", "WindField", "Route", "BoatState", "Guidance"] {
            assert!(
                !body.contains(forbidden),
                "the Agent trait mentions `{forbidden}`; F14.4 makes the observation vector \
                 the whole of an agent's world"
            );
        }
    }

    /// F14.1: the arrow runs agent → physics and never the reverse.
    ///
    /// The same assertion `sailgym-task` and `sailgym-course` each make, and
    /// section acceptance criterion 5. It runs in gate step 3.
    #[test]
    fn physics_does_not_depend_on_the_agent_crate() {
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
        for crate_name in [
            "sailgym-agent",
            "sailgym-course",
            "sailgym-task",
            "wasm-bindgen",
        ] {
            assert!(
                !tree.contains(crate_name),
                "cargo tree -p sailgym-physics mentions {crate_name} (F14.1):\n{tree}"
            );
        }
    }
}
