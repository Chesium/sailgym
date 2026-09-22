//! The autoreset convention, and the discounted returns that prove it
//! (v2 section 06 task 6.3).
//!
//! # Why this file exists at all
//!
//! Vector APIs differ over whether the reset observation appears on the step
//! that reports `done` or on the next one, and over how the final
//! observation is returned. An off-by-one here shifts every bootstrapped
//! value by one step and produces training curves that look merely
//! *mediocre* rather than broken — which is the worst kind of bug, because
//! it survives review, survives a demo, and is indistinguishable from "the
//! task is hard" for as long as anyone is willing to keep tuning (RV33).
//!
//! # The convention was checked, not assumed
//!
//! The PRD says to check the convention of the exact version pinned in
//! `uv.lock` and not to take it from a document. **No Gymnasium is pinned in
//! `uv.lock`** — section 03's Python workspace has `jax`, `numpy`, `pytest`
//! and `ruff`, and the binding that would need Gymnasium is section 07's. So
//! the semantics below were read out of the **source** of the current
//! release rather than out of anybody's memory:
//! `gymnasium/vector/vector_env.py:32-38` for the enum and its three values,
//! `gymnasium/vector/sync_vector_env.py:68` for the default and `:252-295`
//! for what each mode does. `docs/v2/progress/06-handoff.md` §3 records the
//! version, the files, the lines and the URL, and records that section 07
//! must re-check them against whatever it pins.
//!
//! Both live conventions are implemented, and which one an episode used is
//! recorded in its envelope as
//! [`AutoresetMode`](crate::outcome::AutoresetMode) — Gymnasium's own
//! spelling — so section 07 selects the one its pinned version declares in
//! `metadata["autoreset_mode"]` rather than inheriting a guess made here.
//!
//! # What each convention does to a stream
//!
//! A vector API hands a learner a flat stream of
//! `(reward, terminated, truncated)` per env per call. The conventions
//! differ in **exactly one** thing: whether that stream contains a record
//! that belongs to no episode.
//!
//! ```text
//!  episode A ends on call 4, episode B starts
//!
//!  NextStep   call: 0    1    2    3    4*   5R   6    7
//!             r:    a0   a1   a2   a3   a4   0    b0   b1
//!             term:  .    .    .    .    T    .    .    .
//!
//!  SameStep   call: 0    1    2    3    4*   5    6    7
//!             r:    a0   a1   a2   a3   a4   b0   b1   b2
//!             term:  .    .    .    .    T    .    .    .
//! ```
//!
//! `5R` is the reset call: its action is ignored, its reward is `0` and both
//! its flags are clear. [`split_episodes`] is the whole of the difference,
//! and it is eleven lines.
//!
//! # And the returns are compared numerically
//!
//! `crates/sailgym-env/tests/returns.rs` runs the **same** fixed action
//! sequence, from the same seed, under both conventions, splits the two
//! streams with the functions below and asserts the discounted returns
//! agree — at two discount factors, and bit for bit. The bound was shown
//! tight enough to catch a one-step shift by introducing the shift and
//! watching the test go red; the discrepancy is recorded in the handoff.

use crate::outcome::AutoresetMode;

/// One entry of the flat stream a vector API reports, as a learner sees it.
///
/// There is **no `done` field** (RV34).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StepRecord {
    pub reward: f64,
    /// The episode ended for a task reason.
    pub terminated: bool,
    /// The episode ended because the step budget ran out.
    pub truncated: bool,
}

impl StepRecord {
    /// Whether this record ends an episode. Derived, never transmitted as a
    /// single flag.
    pub fn is_boundary(&self) -> bool {
        self.terminated || self.truncated
    }
}

/// Why a stream could not be split.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AutoresetError {
    /// Under [`AutoresetMode::NextStep`] the record after a boundary is the
    /// reset call: its reward must be `0` and both its flags clear. A
    /// producer that puts a reward on it has attributed one episode's
    /// reward to the gap between two, which is precisely the off-by-one
    /// this module exists to prevent.
    ResetRecordNotNeutral { index: usize, record: StepRecord },
}

impl std::fmt::Display for AutoresetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ResetRecordNotNeutral { index, record } => write!(
                f,
                "record {index} is a NextStep reset call and must be neutral, but it carries \
                 reward {} terminated {} truncated {}",
                record.reward, record.terminated, record.truncated
            ),
        }
    }
}

impl std::error::Error for AutoresetError {}

/// Split a flat stream into the reward sequence of each **complete**
/// episode, under `mode`.
///
/// A trailing partial episode is dropped: a return needs an end.
///
/// [`AutoresetMode::Disabled`] splits the same way as
/// [`AutoresetMode::SameStep`] — the caller resets explicitly, so the stream
/// carries no reset record either way. What differs between them is who
/// calls `reset`, which a stream cannot show.
pub fn split_episodes(
    mode: AutoresetMode,
    stream: &[StepRecord],
) -> Result<Vec<Vec<f64>>, AutoresetError> {
    let mut episodes = Vec::new();
    let mut current: Vec<f64> = Vec::new();
    let mut skip_next = false;
    for (index, record) in stream.iter().enumerate() {
        if skip_next {
            skip_next = false;
            if record.reward != 0.0 || record.is_boundary() {
                return Err(AutoresetError::ResetRecordNotNeutral {
                    index,
                    record: *record,
                });
            }
            continue;
        }
        current.push(record.reward);
        if record.is_boundary() {
            episodes.push(std::mem::take(&mut current));
            skip_next = mode == AutoresetMode::NextStep;
        }
    }
    Ok(episodes)
}

/// `Σ γ^t r_t` over one episode's own reward sequence.
///
/// Summed **forwards from `t = 0`**, in index order, so the floating-point
/// association is fixed and two streams carrying the same rewards produce
/// the same bits (F9.4's discipline, applied outside the physics).
pub fn discounted_return(rewards: &[f64], gamma: f64) -> f64 {
    let mut total = 0.0;
    let mut discount = 1.0;
    for r in rewards {
        total += discount * r;
        discount *= gamma;
    }
    total
}

/// The discounted return of every complete episode in the stream.
pub fn discounted_returns(
    mode: AutoresetMode,
    stream: &[StepRecord],
    gamma: f64,
) -> Result<Vec<f64>, AutoresetError> {
    Ok(split_episodes(mode, stream)?
        .iter()
        .map(|rewards| discounted_return(rewards, gamma))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(reward: f64) -> StepRecord {
        StepRecord {
            reward,
            ..StepRecord::default()
        }
    }

    fn term(reward: f64) -> StepRecord {
        StepRecord {
            reward,
            terminated: true,
            truncated: false,
        }
    }

    fn trunc(reward: f64) -> StepRecord {
        StepRecord {
            reward,
            terminated: false,
            truncated: true,
        }
    }

    /// The two conventions, on the two streams a correct producer emits for
    /// the *same* two episodes.
    #[test]
    fn the_two_conventions_split_the_same_episodes_out_of_their_own_streams() {
        let next_step = [
            r(1.0),
            r(2.0),
            term(3.0),
            r(0.0), // the reset call
            r(4.0),
            trunc(5.0),
            r(0.0), // the reset call
            r(6.0), // a partial episode, dropped
        ];
        let same_step = [r(1.0), r(2.0), term(3.0), r(4.0), trunc(5.0), r(6.0)];

        let a = split_episodes(AutoresetMode::NextStep, &next_step).expect("a clean stream");
        let b = split_episodes(AutoresetMode::SameStep, &same_step).expect("a clean stream");
        assert_eq!(a, vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0]]);
        assert_eq!(a, b);

        for gamma in [0.9, 0.99] {
            let ra = discounted_returns(AutoresetMode::NextStep, &next_step, gamma)
                .expect("a clean stream");
            let rb = discounted_returns(AutoresetMode::SameStep, &same_step, gamma)
                .expect("a clean stream");
            assert_eq!(ra, rb, "γ = {gamma}");
            // Hand-checked: 1 + 2γ + 3γ², then 4 + 5γ.
            assert!((ra[0] - (1.0 + 2.0 * gamma + 3.0 * gamma * gamma)).abs() < 1e-15);
            assert!((ra[1] - (4.0 + 5.0 * gamma)).abs() < 1e-15);
        }
    }

    /// Reading a `NextStep` stream as a `SameStep` one — the exact
    /// off-by-one RV33 names — changes every return after the first
    /// boundary. The split is what catches it.
    #[test]
    fn reading_a_stream_under_the_wrong_convention_shifts_every_later_episode() {
        let next_step = [r(1.0), term(2.0), r(0.0), r(3.0), term(4.0), r(0.0)];
        let right = split_episodes(AutoresetMode::NextStep, &next_step).expect("clean");
        let wrong = split_episodes(AutoresetMode::SameStep, &next_step).expect("clean");
        assert_eq!(right, vec![vec![1.0, 2.0], vec![3.0, 4.0]]);
        // The reset record is swallowed into the next episode, at its
        // front, which shifts every reward in it by one discount power.
        assert_eq!(wrong, vec![vec![1.0, 2.0], vec![0.0, 3.0, 4.0]]);
        let a = discounted_return(&right[1], 0.9);
        let b = discounted_return(&wrong[1], 0.9);
        assert!((a - b).abs() > 0.3, "the shift was invisible: {a} vs {b}");
    }

    /// A reset record that is not neutral is refused, not absorbed.
    #[test]
    fn a_non_neutral_reset_record_is_refused() {
        let bad = [r(1.0), term(2.0), r(0.5)];
        assert_eq!(
            split_episodes(AutoresetMode::NextStep, &bad),
            Err(AutoresetError::ResetRecordNotNeutral {
                index: 2,
                record: r(0.5)
            })
        );
        let flagged = [r(1.0), term(2.0), term(0.0)];
        assert!(matches!(
            split_episodes(AutoresetMode::NextStep, &flagged),
            Err(AutoresetError::ResetRecordNotNeutral { index: 2, .. })
        ));
        // Under SameStep the same stream is two episodes, not an error: the
        // rule is the convention's, not the stream's.
        assert_eq!(
            split_episodes(AutoresetMode::SameStep, &bad).expect("clean"),
            vec![vec![1.0, 2.0]]
        );
    }

    #[test]
    fn disabled_splits_like_same_step() {
        let stream = [r(1.0), term(2.0), r(3.0), trunc(4.0)];
        assert_eq!(
            split_episodes(AutoresetMode::Disabled, &stream).expect("clean"),
            split_episodes(AutoresetMode::SameStep, &stream).expect("clean")
        );
    }

    #[test]
    fn an_empty_or_unterminated_stream_yields_no_returns() {
        assert!(split_episodes(AutoresetMode::NextStep, &[])
            .expect("clean")
            .is_empty());
        assert!(
            split_episodes(AutoresetMode::SameStep, &[r(1.0), r(2.0)])
                .expect("clean")
                .is_empty(),
            "a return needs an end"
        );
        assert_eq!(discounted_return(&[], 0.99), 0.0);
        assert_eq!(discounted_return(&[2.0], 0.0), 2.0);
    }
}
