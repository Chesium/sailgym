//! The actuation funnel (v2 F14.5, section 05 task 5.5).
//!
//! ```text
//! agent ─ normalised a ∈ [−1,1]^k ─► Actuation adapter ─► Controls ─► physics
//!                                     (the swept variable)
//! ```
//!
//! # Bounds are always `[−1, 1]^k`, and the contract is checked
//!
//! F14.5: every adapter presents `[−1, 1]^k` and denormalises internally.
//! Denormalisation lives in the adapter and nowhere else. This is not cosmetic:
//! an ablation that hands one arm radians and another normalised commands has
//! measured action **scaling**, not action **space**, and action scaling alone
//! will dominate the result.
//!
//! [`apply`] is the one way an action becomes `Controls`, and it checks the
//! contract at **both** ends — the action it is given, and the `Controls` the
//! adapter produced. Checking only the first would let an adapter emit a rate
//! command of 4.0 and have F3's normalised range quietly stop meaning anything;
//! checking only the second would let an out-of-range action through whenever
//! an adapter happened to clamp.
//!
//! # The boom angle is never commanded
//!
//! F6.8 and F6.9 stand. An adapter that wants "the sailor thinks in boom angle"
//! inverts `ℓ(β)` to a sheet **length** and commands that; it is named
//! `sheet_length_for_beta` so that nobody reading an experiment log believes
//! the boom was commanded. It is not built here, and neither are `angle` or
//! `angle_bangbang`: the funnel is this task's deliverable and the arms belong
//! to the action-space ablation.
//!
//! # Gains are not physical coefficients
//!
//! An adapter with an inner loop has a gain. F14.9: brief §43 governs
//! `parameters.rs` and does **not** govern adapter gains, which live in this
//! crate and may be tuned freely. The `rate` adapter has none — it is the
//! identity — so this section introduces no gain at all, which
//! `docs/v2/progress/05-handoff.md` records as the answer to acceptance
//! criterion 8.

pub mod rate;

use sailgym_physics::parameters::BoatParameters;
use sailgym_physics::recording::ActionIdentity;
use sailgym_physics::state::{BoatState, Controls};

use crate::spec::{Action, Cadence};

/// One action space, as machinery.
///
/// Object-safe, so a registry of adapters is a `Vec<Box<dyn Actuation>>` and
/// never a hash container (F9.3).
pub trait Actuation {
    /// The adapter's registered name, as it appears in an experiment log.
    fn id(&self) -> &'static str;

    /// Bumped when the denormalisation or the engagement semantics change.
    /// Two runs whose adapters differ only in version are not comparable.
    fn version(&self) -> u32;

    /// How many scalars the normalised action carries.
    fn dim(&self) -> usize;

    /// Denormalise. Called only through [`apply`], which checks the contract.
    ///
    /// `&mut self` because an adapter with an inner loop has state; the `rate`
    /// adapter has none, and says so.
    fn to_controls(&mut self, a: &[f64], st: &BoatState, p: &BoatParameters) -> Controls;

    /// Clear any adapter state. Called once per episode, at reset.
    fn reset(&mut self);
}

/// Turn a normalised action into `Controls`, checking F14.5's contract.
///
/// This is the **one** path from an agent's decision to the physics, and it is
/// the same path for a policy and for an external manual source: what differs
/// between them is where the numbers came from, which is legitimate, and not
/// what happens to them afterwards, which is not (RV27).
pub fn apply(
    adapter: &mut dyn Actuation,
    action: &Action,
    st: &BoatState,
    p: &BoatParameters,
) -> Result<Controls, ActionError> {
    apply_values(adapter, action.values(), st, p)
}

/// [`apply`] on a bare normalised slice.
///
/// The check on the incoming values is **not** redundant with
/// [`ActionVec::new`](crate::spec::ActionVec::new)'s. That one guards the type;
/// this one guards the funnel, and the funnel is what a later binding — a
/// Python `step(np.array([...]))`, a deserialised action from a log — arrives
/// through. A contract enforced in exactly one place is enforced wherever
/// nobody has added a second entry point yet.
pub fn apply_values(
    adapter: &mut dyn Actuation,
    values: &[f64],
    st: &BoatState,
    p: &BoatParameters,
) -> Result<Controls, ActionError> {
    if values.len() != adapter.dim() {
        return Err(ActionError::WrongDim {
            adapter: adapter.id(),
            got: values.len(),
            want: adapter.dim(),
        });
    }
    for (index, v) in values.iter().enumerate() {
        if !v.is_finite() || *v < -1.0 || *v > 1.0 {
            return Err(ActionError::OutOfBounds {
                adapter: adapter.id(),
                index,
                value: *v,
            });
        }
    }

    let c = adapter.to_controls(values, st, p);

    // The other end of the contract. F3 defines both rate commands as
    // normalised `[-1, 1]`; an adapter that leaves that range has denormalised
    // into the wrong units, and `dynamics::sheet_rate` would clamp it silently.
    for (name, v) in [
        ("rudder_rate_cmd", c.rudder_rate_cmd),
        ("sheet_rate_cmd", c.sheet_rate_cmd),
    ] {
        if !v.is_finite() || v < -1.0 || v > 1.0 {
            return Err(ActionError::AdapterOutOfBounds {
                adapter: adapter.id(),
                field: name,
                value: v,
            });
        }
    }
    Ok(c)
}

/// The adapter half of an experiment's identity (section 10, F16.4).
///
/// `ActionIdentity` carries the adapter's name, its version and the cadence —
/// "adapter/gains/engagement semantics/cadence" in F16.4's words. The `rate`
/// adapter has no gains and no engagement semantics, so its name and version
/// are the whole of them; an adapter that acquires either records it by
/// bumping [`Actuation::version`], which is what makes the omission safe.
pub fn action_identity(adapter: &dyn Actuation, cadence: Cadence) -> ActionIdentity {
    ActionIdentity {
        adapter: adapter.id().to_string(),
        version: adapter.version(),
        period_steps: cadence.period_steps,
    }
}

/// What [`apply`] refuses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ActionError {
    WrongDim {
        adapter: &'static str,
        got: usize,
        want: usize,
    },
    OutOfBounds {
        adapter: &'static str,
        index: usize,
        value: f64,
    },
    AdapterOutOfBounds {
        adapter: &'static str,
        field: &'static str,
        value: f64,
    },
}

impl std::fmt::Display for ActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongDim { adapter, got, want } => write!(
                f,
                "the `{adapter}` adapter takes {want} scalars and was given {got}"
            ),
            Self::OutOfBounds {
                adapter,
                index,
                value,
            } => write!(
                f,
                "action[{index}] = {value} is outside [-1, 1]: the `{adapter}` adapter presents \
                 normalised bounds (F14.5)"
            ),
            Self::AdapterOutOfBounds {
                adapter,
                field,
                value,
            } => write!(
                f,
                "the `{adapter}` adapter produced {field} = {value}, outside F3's normalised \
                 [-1, 1]: denormalisation has gone the wrong way"
            ),
        }
    }
}

impl std::error::Error for ActionError {}

#[cfg(test)]
mod tests {
    use super::rate::Rate;
    use super::*;
    use crate::spec::ActionVec;

    /// An adapter that leaves the normalised range. It exists only here, to
    /// prove the contract assertion is load-bearing.
    struct Runaway;

    impl Actuation for Runaway {
        fn id(&self) -> &'static str {
            "runaway"
        }
        fn version(&self) -> u32 {
            1
        }
        fn dim(&self) -> usize {
            1
        }
        fn to_controls(&mut self, a: &[f64], _: &BoatState, _: &BoatParameters) -> Controls {
            Controls {
                // "Denormalising" into a rate command of ±4 — the shape of
                // mistake F14.5 exists to catch.
                rudder_rate_cmd: a[0] * 4.0,
                ..Controls::default()
            }
        }
        fn reset(&mut self) {}
    }

    fn act(values: &[f64]) -> Action {
        Action::Rates(ActionVec::new(values).expect("in bounds"))
    }

    #[test]
    fn an_action_of_the_wrong_width_is_refused() {
        let p = BoatParameters::ilca7();
        let st = BoatState::ZERO;
        let mut r = Rate;
        assert_eq!(
            apply(&mut r, &act(&[0.0, 0.0]), &st, &p),
            Err(ActionError::WrongDim {
                adapter: "rate",
                got: 2,
                want: 3
            })
        );
        assert_eq!(act(&[0.0, 0.0, 0.0]).values().len(), r.dim());
    }

    /// The funnel refuses an out-of-range value, wherever it came from.
    #[test]
    fn an_out_of_range_action_is_refused_at_the_funnel() {
        let p = BoatParameters::ilca7();
        let st = BoatState::ZERO;
        let mut r = Rate;

        // The type refuses it at construction…
        assert!(ActionVec::new(&[2.0, 0.0, 0.0]).is_err());

        // …and the funnel refuses it again, for a caller that did not go
        // through the type: a Python binding, or an action read back from a
        // log.
        for (index, values) in [
            (0usize, [1.5, 0.0, 0.0]),
            (1, [0.0, -1.000_001, 0.0]),
            (2, [0.0, 0.0, f64::NAN]),
        ] {
            let why = apply_values(&mut r, &values, &st, &p).expect_err("out of bounds");
            assert!(
                matches!(why, ActionError::OutOfBounds { index: i, .. } if i == index),
                "{why}"
            );
            assert!(format!("{why}").contains("F14.5"), "{why}");
        }

        // The extremes themselves are legal.
        assert!(apply_values(&mut r, &[1.0, -1.0, 1.0], &st, &p).is_ok());
    }

    /// An adapter that produces a `Controls` outside F3's normalised range is
    /// rejected by the contract assertion.
    #[test]
    fn an_adapter_that_leaves_the_normalised_range_is_rejected() {
        let p = BoatParameters::ilca7();
        let st = BoatState::ZERO;
        let mut bad = Runaway;
        let why = apply(&mut bad, &act(&[1.0]), &st, &p).expect_err("4.0 is out of range");
        assert_eq!(
            why,
            ActionError::AdapterOutOfBounds {
                adapter: "runaway",
                field: "rudder_rate_cmd",
                value: 4.0
            }
        );
        // …and a small enough action still passes, so the check is on the
        // value and not on the adapter's name.
        assert!(apply(&mut bad, &act(&[0.25]), &st, &p).is_ok());
    }

    #[test]
    fn the_action_identity_names_the_adapter_and_the_cadence() {
        let id = action_identity(&Rate, Cadence::new(10));
        assert_eq!(id.adapter, "rate");
        assert_eq!(id.version, Rate::VERSION);
        assert_eq!(id.period_steps, 10);
        // F14.5: never "the boom angle".
        assert!(!id.adapter.contains("beta"));
        assert!(!id.adapter.contains("boom"));
    }
}
