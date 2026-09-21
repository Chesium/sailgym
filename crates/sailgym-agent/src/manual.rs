//! `manual`: the external action source (v2 F14.2, section 05 task 5.7).
//!
//! # A human is an action source, not a privileged policy
//!
//! The source discussion had `manual` read the browser's held keys out of an
//! `EpisodeCtx`. That is corrected: an external source **receives** a
//! normalised action through [`Manual::set_action`] and has no access to held
//! keys, to episode state, or to anything else an ordinary agent cannot see.
//! [`Manual::decide`] ignores its observation entirely, which is the whole of
//! what it is: a latch.
//!
//! The point is not to make the human weaker. It is that the human's action and
//! a policy's action take the **same** path afterwards — the same bounds check,
//! the same adapter, the same cadence, the same log. Choosing which source
//! supplies the numbers is legitimate; giving them different semantics is the
//! defect RV27 names, and `tests/determinism.rs` compares the two trajectories
//! rather than trusting a source-pattern ban to prevent it.
//!
//! # Cadence
//!
//! [`Cadence::EVERY_STEP`] is the honest default for a browser, which pushes a
//! new action per frame and holds it between frames; a run that wants to
//! compare a human against a policy at 20 Hz sets the same period for both, and
//! records it (F14.6.3).

use sailgym_physics::rng::Pcg32;

use crate::spec::{Action, ActionSpace, ActionVec, Agent, AgentSpec, Cadence, SpecError};

/// The latch an external source writes into.
#[derive(Clone, Debug)]
pub struct Manual {
    action: ActionVec,
    cadence: Cadence,
    dim: usize,
}

impl Manual {
    pub const ID: &'static str = "manual";
    /// Bumped when what `manual` does with a given pushed action changes. It
    /// does nothing with it, so there is nothing to change.
    pub const VERSION: u32 = 1;

    /// A source for an adapter of `dim` scalars, latched at the zero action —
    /// hands off, which for the `rate` adapter is `Controls::default()`.
    pub fn new(cadence: Cadence, dim: usize) -> Self {
        Self {
            action: ActionVec::new(&vec![0.0; dim]).expect("zeros are in bounds"),
            cadence,
            dim,
        }
    }

    /// Push the action the external source wants applied from the next
    /// decision onward.
    ///
    /// Validated **here**, at the boundary, and validated again by
    /// [`crate::actuation::apply`]: an external source is not a trusted one.
    pub fn set_action(&mut self, values: &[f64]) -> Result<(), SpecError> {
        if values.len() != self.dim {
            return Err(SpecError::ActionWidthMismatch {
                got: values.len(),
                want: self.dim,
            });
        }
        self.action = ActionVec::new(values)?;
        Ok(())
    }

    /// The latched action.
    pub fn action(&self) -> &[f64] {
        self.action.as_slice()
    }
}

impl Agent for Manual {
    fn spec(&self) -> AgentSpec {
        AgentSpec::new(Self::ID, Self::VERSION, ActionSpace::Rates, self.cadence)
    }

    /// Nothing to seed, and nothing remembered across episodes: a reset
    /// returns the latch to hands-off rather than replaying whatever the
    /// previous run's last frame happened to hold.
    fn reset(&mut self, _fields: &[String], _rng: &mut Pcg32) {
        self.action = ActionVec::new(&vec![0.0; self.dim]).expect("zeros are in bounds");
    }

    /// The observation is ignored, deliberately. A human is looking at the
    /// screen, not at this vector.
    fn decide(&mut self, _obs: &[f64], _rng: &mut Pcg32) -> Action {
        Action::Rates(self.action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actuation::{apply, rate::Rate};
    use sailgym_physics::parameters::BoatParameters;
    use sailgym_physics::state::{BoatState, Controls};

    fn manual() -> Manual {
        Manual::new(Cadence::EVERY_STEP, Rate::DIM)
    }

    #[test]
    fn a_fresh_source_is_hands_off() {
        let m = manual();
        assert_eq!(m.action(), &[0.0, 0.0, 0.0]);
        assert!(m.spec().validate().is_ok());
        assert_eq!(m.spec().id, "manual");
        assert_eq!(m.spec().action_space, ActionSpace::Rates);
    }

    #[test]
    fn a_pushed_action_is_validated_at_the_boundary() {
        let mut m = manual();
        assert!(m.set_action(&[0.5, -0.5, -1.0]).is_ok());
        assert_eq!(m.action(), &[0.5, -0.5, -1.0]);

        // Out of range, and the latch keeps its previous value.
        assert!(matches!(
            m.set_action(&[1.5, 0.0, 0.0]),
            Err(SpecError::ActionOutOfBounds { index: 0, .. })
        ));
        assert_eq!(m.action(), &[0.5, -0.5, -1.0]);

        // Wrong width, likewise.
        assert!(m.set_action(&[0.0, 0.0]).is_err());
        assert_eq!(m.action(), &[0.5, -0.5, -1.0]);
    }

    #[test]
    fn the_observation_is_ignored() {
        let mut m = manual();
        m.set_action(&[0.25, -0.75, 1.0]).expect("in bounds");
        let mut rng = Pcg32::seed_from_u64(1);
        let a = m.decide(&[0.0; 18], &mut rng);
        let b = m.decide(&[7.0; 18], &mut rng);
        let c = m.decide(&[], &mut rng);
        assert_eq!(a, b);
        assert_eq!(a, c);
        assert_eq!(a.values(), &[0.25, -0.75, 1.0]);
        assert_eq!(a.space(), ActionSpace::Rates);
    }

    /// The latch goes through the ordinary funnel and produces ordinary
    /// `Controls` — no second path, no special case.
    #[test]
    fn a_manual_action_reaches_controls_through_the_funnel() {
        let p = BoatParameters::ilca7();
        let st = BoatState::ZERO;
        let mut m = manual();
        let mut rng = Pcg32::seed_from_u64(2);
        m.set_action(&[-0.4, 0.9, 1.0]).expect("in bounds");
        let action = m.decide(&[], &mut rng);
        let c = apply(&mut Rate, &action, &st, &p).expect("in bounds");
        assert_eq!(
            c,
            Controls {
                rudder_rate_cmd: -0.4,
                sheet_rate_cmd: 0.9,
                sheet_release: true,
            }
        );
    }

    #[test]
    fn a_reset_returns_the_latch_to_hands_off() {
        let mut m = manual();
        m.set_action(&[1.0, 1.0, 1.0]).expect("in bounds");
        let mut rng = Pcg32::seed_from_u64(3);
        m.reset(&[], &mut rng);
        assert_eq!(m.action(), &[0.0, 0.0, 0.0]);
    }
}
