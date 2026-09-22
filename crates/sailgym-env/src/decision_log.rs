//! The complete decision record, and its replay (v2 section 06 task 6.4).
//!
//! # Why the sampled frames are not enough
//!
//! Section 10's [`Recorder`](sailgym_physics::recording::Recorder) samples
//! at `log_hz` — 5 to 50 Hz against 200 Hz physics — and would miss the
//! actions between two samples. For a replay that is fine: the frames are
//! for looking at. For reinforcement learning it is not, because **the
//! action sequence is the artifact**: it is what a policy produced, what an
//! off-policy method replays and what an ablation compares.
//!
//! Decisions happen at a fixed cadence with zero-order hold between them
//! (F14.6), so `(step_index, action)` pairs are a *complete* record and a
//! small one: at 20 Hz over a 60-second episode, 1200 rows of three
//! scalars.
//!
//! # They are two arrays, and they stay two arrays
//!
//! The decision log and the sampled frames have different rates, different
//! consumers and different lifetimes. Merging them "to simplify" would
//! either resample the actions to the frame rate — losing the ones in
//! between, which is the whole problem — or raise the frame rate to the
//! decision rate, which multiplies a 44-scalar frame by ten. RV39 fires the
//! moment `frames` gains an action column.
//!
//! # The index contract
//!
//! A decision at step `k` is taken **before** step `k` executes, so the
//! logged indices are exactly
//!
//! ```text
//! { k : 0 ≤ k < executed_steps  and  k % period_steps == 0 }
//! ```
//!
//! — in ascending order, with no gaps and no repeats, and **empty for zero
//! steps**. [`DecisionLog::validate`] is that sentence, and it is checked
//! rather than described, because "the log looks about right" is how an
//! off-by-one in the *other* direction survives.

use serde::{Deserialize, Serialize};

use sailgym_agent::spec::Cadence;

use crate::episode::{manual_source, EnvError, Episode, EpisodeConfig};
use crate::recording::Decision;

/// Every decision one episode took, with the cadence that produced them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecisionLog {
    /// F14.6's cadence. Frozen at reset, so one number describes the whole
    /// log.
    pub period_steps: u32,
    /// The adapter's width. Every action carries exactly this many
    /// scalars.
    pub action_dim: usize,
    /// Physics steps the episode executed. The log's indices are checked
    /// against it.
    pub executed_steps: u64,
    /// The decisions, in ascending step order.
    pub decisions: Vec<Decision>,
}

/// Why a log is not the log of an episode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionLogError {
    /// The cadence is zero, so nothing ever decides.
    ZeroCadence,
    /// A decision sits at a step that is not a multiple of the period.
    NotOnTheCadence { at: usize, step: u64 },
    /// A decision sits at or past the last executed step.
    PastTheEnd { at: usize, step: u64 },
    /// The decisions are not in ascending step order.
    OutOfOrder { at: usize },
    /// A decision the contract requires is absent.
    Missing { step: u64 },
    /// An action of the wrong width.
    WrongWidth { at: usize, got: usize },
    /// An action outside `[−1, 1]`, or not finite (F14.5).
    OutOfBounds { at: usize, index: usize },
}

impl std::fmt::Display for DecisionLogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroCadence => write!(f, "a cadence period of 0 decides never (F14.6)"),
            Self::NotOnTheCadence { at, step } => write!(
                f,
                "decision {at} is at step {step}, which is not on the cadence"
            ),
            Self::PastTheEnd { at, step } => write!(
                f,
                "decision {at} is at step {step}, at or past the last executed step: a \
                 decision at step k is taken before step k executes"
            ),
            Self::OutOfOrder { at } => write!(
                f,
                "decision {at} is not after decision {}",
                at.saturating_sub(1)
            ),
            Self::Missing { step } => {
                write!(f, "no decision at step {step}, which the cadence requires")
            }
            Self::WrongWidth { at, got } => {
                write!(f, "decision {at} carries {got} scalars")
            }
            Self::OutOfBounds { at, index } => write!(
                f,
                "decision {at}, scalar {index} is outside [-1, 1]: every action is normalised \
                 (F14.5)"
            ),
        }
    }
}

impl std::error::Error for DecisionLogError {}

impl DecisionLog {
    /// Snapshot the log of a running or finished episode.
    pub fn of(episode: &Episode) -> Self {
        Self {
            period_steps: episode.agent_spec().cadence.period_steps,
            action_dim: episode.action_dim(),
            executed_steps: episode.steps(),
            decisions: episode.decisions(),
        }
    }

    /// How many decisions the contract requires for `executed_steps` at
    /// this cadence: `ceil(executed_steps / period_steps)`, and zero for
    /// zero steps.
    pub fn expected_count(&self) -> u64 {
        if self.period_steps == 0 {
            return 0;
        }
        self.executed_steps.div_ceil(u64::from(self.period_steps))
    }

    /// The index contract, checked.
    pub fn validate(&self) -> Result<(), DecisionLogError> {
        if self.period_steps == 0 {
            return Err(DecisionLogError::ZeroCadence);
        }
        let period = u64::from(self.period_steps);
        let mut previous: Option<u64> = None;
        for (at, d) in self.decisions.iter().enumerate() {
            if !d.step.is_multiple_of(period) {
                return Err(DecisionLogError::NotOnTheCadence { at, step: d.step });
            }
            if d.step >= self.executed_steps {
                return Err(DecisionLogError::PastTheEnd { at, step: d.step });
            }
            if previous.is_some_and(|p| d.step <= p) {
                return Err(DecisionLogError::OutOfOrder { at });
            }
            previous = Some(d.step);
            if d.action.len() != self.action_dim {
                return Err(DecisionLogError::WrongWidth {
                    at,
                    got: d.action.len(),
                });
            }
            for (index, v) in d.action.iter().enumerate() {
                if !v.is_finite() || *v < -1.0 || *v > 1.0 {
                    return Err(DecisionLogError::OutOfBounds { at, index });
                }
            }
        }
        // No gaps: the count settles it, given that every index is a
        // distinct in-range multiple.
        let want = self.expected_count();
        if (self.decisions.len() as u64) < want {
            let present: Vec<u64> = self.decisions.iter().map(|d| d.step).collect();
            let missing = (0..self.executed_steps)
                .step_by(self.period_steps as usize)
                .find(|k| !present.contains(k))
                .unwrap_or(self.executed_steps);
            return Err(DecisionLogError::Missing { step: missing });
        }
        Ok(())
    }

    /// Replay the log into a fresh episode of `config`, from `seed`.
    ///
    /// The actions are pushed through `manual`, which is an [`Agent`] like
    /// any other, so they take the **same** validated actuation path a
    /// policy's take (F14.5, RV27). That is why the replay can reproduce
    /// the original bit for bit: what differs between the two runs is where
    /// the numbers came from, and nothing else.
    ///
    /// [`Agent`]: sailgym_agent::spec::Agent
    pub fn replay(&self, config: EpisodeConfig, seed: u64) -> Result<Episode, EnvError> {
        self.validate()
            .map_err(|e| EnvError::Action(e.to_string()))?;
        let period = u64::from(self.period_steps);
        let mut ep = Episode::new(config, manual_source(Cadence::new(self.period_steps)), seed)?;
        for d in &self.decisions {
            debug_assert_eq!(ep.steps(), d.step, "the replay walked off the cadence");
            ep.push_action(&d.action)?;
            let left = self.executed_steps.saturating_sub(ep.steps());
            let n = u32::try_from(left.min(period)).unwrap_or(u32::MAX);
            ep.advance(n)?;
        }
        Ok(ep)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::{AutoresetMode, Bounds, Outcome};
    use sailgym_agent::spec::{Action, ActionSpace, ActionVec, Agent, AgentSpec};
    use sailgym_physics::rng::Pcg32;
    use sailgym_physics::scenario::load_shipped;
    use sailgym_physics::state::{STATE_FIELDS, STATE_LEN};

    /// A policy that reads its observation and jitters, so the log is a
    /// sequence nothing else could have produced.
    struct Wanderer {
        period: u32,
        aws: usize,
    }

    impl Agent for Wanderer {
        fn spec(&self) -> AgentSpec {
            AgentSpec::new("wanderer", 1, ActionSpace::Rates, Cadence::new(self.period))
        }
        fn reset(&mut self, fields: &[String], _rng: &mut Pcg32) {
            self.aws = fields
                .iter()
                .position(|f| f == "apparent_wind.aws")
                .expect("the tier-0 suite has an apparent-wind sensor");
        }
        fn decide(&mut self, obs: &[f64], rng: &mut Pcg32) -> Action {
            let turn = (rng.next_f64() - 0.5).clamp(-1.0, 1.0);
            let sheet = (0.2 * obs[self.aws] - 0.7).clamp(-1.0, 1.0);
            Action::Rates(ActionVec::new(&[turn, sheet, -1.0]).expect("clamped"))
        }
    }

    fn config() -> EpisodeConfig {
        let mut cfg = EpisodeConfig::new(load_shipped("gybe").expect("a shipped scenario"));
        cfg.autoreset = AutoresetMode::Disabled;
        cfg.max_steps = Some(1234);
        cfg.bounds = Bounds::Unbounded;
        cfg
    }

    fn flown(period: u32, seed: u64) -> Episode {
        let source = crate::episode::Source::Policy(Box::new(Wanderer { period, aws: 0 }));
        let mut ep = Episode::new(config(), source, seed).expect("a valid episode");
        while !ep.outcome().is_terminal() {
            ep.advance(100).expect("a valid action");
        }
        ep
    }

    /// Task 6.4's acceptance, first clause: replaying the logged decisions
    /// reproduces the episode bit for bit.
    #[test]
    fn replaying_the_log_reproduces_the_episode_bit_for_bit() {
        let original = flown(10, 4242);
        assert_eq!(original.outcome(), Outcome::Truncated);
        assert_eq!(original.steps(), 1234);

        let log = DecisionLog::of(&original);
        let replayed = log.replay(config(), 4242).expect("the log replays");
        assert_eq!(replayed.steps(), original.steps());
        assert_eq!(replayed.outcome(), original.outcome());

        let a = original.state().to_array();
        let b = replayed.state().to_array();
        for i in 0..STATE_LEN {
            assert_eq!(
                a[i].to_bits(),
                b[i].to_bits(),
                "field {}: {} vs {}",
                STATE_FIELDS[i],
                a[i],
                b[i]
            );
        }
        assert_eq!(
            DecisionLog::of(&replayed),
            log,
            "the replay's own log differs"
        );

        // …and the agreement is not the agreement of two things that
        // ignore their input: one changed action changes the trajectory.
        let mut tampered = log.clone();
        tampered.decisions[3].action[0] = -tampered.decisions[3].action[0] - 0.5;
        tampered.decisions[3].action[0] = tampered.decisions[3].action[0].clamp(-1.0, 1.0);
        let other = tampered.replay(config(), 4242).expect("still a valid log");
        assert_ne!(
            other.state().to_array()[0].to_bits(),
            a[0].to_bits(),
            "changing a logged action changed nothing"
        );
    }

    /// Task 6.4's acceptance, second clause: the indices are exactly those
    /// `k` with `0 ≤ k < executed_steps` and `k % period_steps == 0`, and
    /// there are **zero decisions for zero steps**.
    #[test]
    fn the_decision_indices_are_exactly_the_cadence_multiples_below_the_end() {
        for period in [1u32, 7, 10, 100] {
            let ep = flown(period, 11);
            let log = DecisionLog::of(&ep);
            log.validate().expect("a real episode's log is valid");
            let want: Vec<u64> = (0..log.executed_steps).step_by(period as usize).collect();
            assert_eq!(
                log.decisions.iter().map(|d| d.step).collect::<Vec<_>>(),
                want,
                "period {period}"
            );
            assert_eq!(log.decisions.len() as u64, log.expected_count());
        }

        // Zero steps, zero decisions — the boundary the contract names.
        let source = crate::episode::Source::Policy(Box::new(Wanderer { period: 10, aws: 0 }));
        let fresh = Episode::new(config(), source, 11).expect("valid");
        let log = DecisionLog::of(&fresh);
        assert_eq!(log.executed_steps, 0);
        assert!(
            log.decisions.is_empty(),
            "a decision was taken before a step"
        );
        log.validate().expect("an empty log is valid");
        assert_eq!(log.expected_count(), 0);
    }

    /// Every clause of the contract refuses its own violation, so a
    /// hand-built or corrupted log cannot be replayed as if it were real.
    #[test]
    fn the_contract_refuses_each_way_of_breaking_it() {
        let base = DecisionLog {
            period_steps: 10,
            action_dim: 3,
            executed_steps: 30,
            decisions: vec![
                Decision {
                    step: 0,
                    action: vec![0.0, 0.0, -1.0],
                },
                Decision {
                    step: 10,
                    action: vec![0.1, 0.0, -1.0],
                },
                Decision {
                    step: 20,
                    action: vec![0.2, 0.0, -1.0],
                },
            ],
        };
        base.validate().expect("the base log is valid");

        let mut off = base.clone();
        off.decisions[1].step = 11;
        assert_eq!(
            off.validate(),
            Err(DecisionLogError::NotOnTheCadence { at: 1, step: 11 })
        );

        let mut past = base.clone();
        past.decisions[2].step = 30;
        assert_eq!(
            past.validate(),
            Err(DecisionLogError::PastTheEnd { at: 2, step: 30 })
        );

        let mut backwards = base.clone();
        backwards.decisions.swap(0, 1);
        assert_eq!(
            backwards.validate(),
            Err(DecisionLogError::OutOfOrder { at: 1 })
        );

        let mut gap = base.clone();
        gap.decisions.remove(1);
        assert_eq!(gap.validate(), Err(DecisionLogError::Missing { step: 10 }));

        let mut wide = base.clone();
        wide.decisions[0].action.push(0.0);
        assert_eq!(
            wide.validate(),
            Err(DecisionLogError::WrongWidth { at: 0, got: 4 })
        );

        let mut loud = base.clone();
        loud.decisions[2].action[1] = 1.5;
        assert_eq!(
            loud.validate(),
            Err(DecisionLogError::OutOfBounds { at: 2, index: 1 })
        );

        let mut never = base.clone();
        never.period_steps = 0;
        assert_eq!(never.validate(), Err(DecisionLogError::ZeroCadence));

        // A log that fails the contract is refused **before** anything is
        // simulated.
        assert!(gap.replay(config(), 1).is_err());
    }

    /// Task 6.4's acceptance, third clause.
    #[test]
    fn the_log_survives_a_json_round_trip() {
        let log = DecisionLog::of(&flown(10, 7));
        let text = serde_json::to_string(&log).expect("serialises");
        let back: DecisionLog = serde_json::from_str(&text).expect("reads back");
        assert_eq!(log, back);
        // Every scalar, bit for bit: `serde_json`'s `float_roundtrip`
        // feature is on in the workspace, and the replay depends on it.
        for (a, b) in log.decisions.iter().zip(back.decisions.iter()) {
            assert_eq!(a.step, b.step);
            for (x, y) in a.action.iter().zip(b.action.iter()) {
                assert_eq!(x.to_bits(), y.to_bits());
            }
        }
        assert!(back.replay(config(), 7).is_ok());
    }

    /// RV39, as a grep: the sampled frames must not grow an action column.
    ///
    /// The two arrays have different rates, different consumers and
    /// different lifetimes, and the day `EpisodeFrame` gains an `action`
    /// field is the day the decision log stops being the complete record.
    #[test]
    fn the_sampled_frames_carry_no_action_column() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../sailgym-physics/src/recording.rs"),
        )
        .expect("recording.rs must be readable");
        let frame = src
            .split_once("pub struct EpisodeFrame {")
            .expect("the frame record")
            .1
            .split_once("\n}")
            .expect("a closed struct")
            .0;
        for forbidden in ["action", "decision", "Decision"] {
            assert!(
                !frame.contains(forbidden),
                "`EpisodeFrame` mentions `{forbidden}`; the decision log and the sampled \
                 frames are two arrays and stay two arrays (RV39):\n{frame}"
            );
        }
    }
}
