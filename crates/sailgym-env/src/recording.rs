//! The research envelope: a versioned wrapper **around** a recorded episode
//! (v2 section 06 task 6.1).
//!
//! # It wraps; it does not replace and it does not migrate
//!
//! Section 10 owns `sailgym_physics::recording`: the schema, the codecs, the
//! [`ExperimentIdentity`] and the legacy-viewing policy. This section owns
//! none of that and changes none of it — `git diff --name-only
//! crates/sailgym-physics/` is empty for the whole of section 06. What it
//! adds is the research half of an episode's identity, which section 10 had
//! no way to write because the agent, the action adapter, the observation
//! layout, the route and the autoreset convention did not exist yet.
//!
//! So [`EPISODE_SCHEMA_VERSION`](sailgym_physics::recording::EPISODE_SCHEMA_VERSION)
//! is **not** touched and **not** bumped. The envelope carries its own
//! [`RESEARCH_ENVELOPE_VERSION`], because the document layout and the
//! research contract change for different reasons and at different times —
//! exactly the argument section 10 used to separate `IDENTITY_VERSION` from
//! the schema version. A schema-1 recording wrapped in an envelope stays a
//! schema-1 recording: [`ResearchEnvelope::to_json`] and
//! [`ResearchEnvelope::from_json`] put the nested document through
//! [`Episode::to_json`] and [`Episode::from_json`], the **existing** codec,
//! so its version check and its `validate` run unchanged and a legacy file
//! round-trips as itself.
//!
//! # A legacy recording cannot acquire agent metadata it never had
//!
//! [`ResearchEnvelope::viewing`] is the constructor for "here is an episode
//! somebody recorded; I know nothing else about it". Every research field
//! comes back [`Recorded::Unknown`], and `Unknown` is not equal to anything,
//! including another `Unknown` — so
//! [`ResearchEnvelope::comparable_with`] returns
//! [`Comparability::Indeterminate`] and
//! [`ResearchEnvelope::resimulate_actions_against`] **refuses**. Viewing is
//! fine; comparing is not. That is section 10's rule, reused rather than
//! restated: `Recorded`, `Comparability` and `ExperimentIdentity::compare`
//! are section 10's types and this file calls them.
//!
//! # `Decision` lives here, and `DecisionLog` does not
//!
//! A [`Decision`] is a *record* — `(step, action)` — and records are this
//! task's. The live recorder that produces them, replays them and asserts
//! which indices may appear is [`crate::decision_log::DecisionLog`], task
//! 6.4's. They are separate for the reason task 6.4 gives for keeping the
//! decision array and the sampled frames apart: different rates, different
//! consumers, different lifetimes (RV39).

use serde::{Deserialize, Serialize};

use sailgym_agent::observation::ObsLayout;
use sailgym_agent::spec::{AgentSpec, Cadence};
use sailgym_course::Route;
use sailgym_physics::recording::{
    ActionIdentity, Comparability, Episode, EpisodeError, ObservationIdentity, Recorded,
};

use crate::outcome::{AutoresetMode, Bounds, Outcome, Reward};

/// The version of the envelope **document**: its field set and its
/// serialised shape.
pub const RESEARCH_ENVELOPE_VERSION: u32 = 1;

/// The version of the research **comparison contract**: which fields have to
/// agree before two episodes may be compared quantity for quantity.
///
/// Separate from [`RESEARCH_ENVELOPE_VERSION`] for the reason section 10
/// separated `IDENTITY_VERSION` from `EPISODE_SCHEMA_VERSION`: adding a
/// displayed field is not the same event as changing what "the same
/// experiment" means.
pub const RESEARCH_IDENTITY_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Decision
// ---------------------------------------------------------------------------

/// One logged decision: the **episode** step it was taken at, and the
/// normalised action taken.
///
/// The action is the `[−1, 1]^k` vector the agent emitted (F14.5), not the
/// `Controls` it denormalised into: replaying the log through
/// [`crate::actuation`](sailgym_agent::actuation) is what proves the two
/// paths are the same path, and a log of `Controls` would have gone round
/// the funnel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    /// The episode step index. A decision at step `k` is taken **before**
    /// step `k` executes, so `0 ≤ k < executed_steps` (task 6.4).
    pub step: u64,
    /// The normalised action, `adapter.dim()` scalars.
    pub action: Vec<f64>,
}

// ---------------------------------------------------------------------------
// The research identity
// ---------------------------------------------------------------------------

/// The reward an episode was scored under, as a comparable record.
///
/// Name and version and nothing else, for the same reason
/// [`ActionIdentity`] carries the adapter's name and version and not its
/// gains: a reward that acquires a parameter records it by bumping
/// [`Reward::version`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewardIdentity {
    pub id: String,
    pub version: u32,
}

impl RewardIdentity {
    /// The identity of a configured reward.
    pub fn of(reward: &dyn Reward) -> Self {
        Self {
            id: reward.id().to_string(),
            version: reward.version(),
        }
    }
}

/// The research half of an episode's identity: everything section 10's
/// [`ExperimentIdentity`](sailgym_physics::recording::ExperimentIdentity)
/// could not know because this crate did not exist.
///
/// Every field is [`Recorded`], section 10's three-valued type, so
/// "this episode had no route" ([`Recorded::NotApplicable`]) stays
/// distinguishable from "this document does not say"
/// ([`Recorded::Unknown`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResearchIdentity {
    /// [`RESEARCH_IDENTITY_VERSION`], or `0` in a document written before
    /// the record existed.
    pub identity_version: u32,
    /// Which agent decided, at what version, in what action space, at what
    /// cadence (F14.2, F14.6).
    pub agent: Recorded<AgentSpec>,
    /// The adapter the normalised action was denormalised through (F14.5).
    pub action: Recorded<ActionIdentity>,
    /// Section 10's per-column record of the observation layout.
    pub observation: Recorded<ObservationIdentity>,
    /// The **full** layout record F16.4 requires to travel beside any
    /// digest — it is where the column bounds live, which section 10's
    /// `ObservationField` has no room for (F14.10 §8).
    pub obs_layout: Recorded<ObsLayout>,
    /// Which autoreset convention produced this log.
    pub autoreset: Recorded<AutoresetMode>,
    /// The course sailed, or `NotApplicable` for a free sail.
    pub route: Recorded<Route>,
    /// The sailing area.
    pub bounds: Recorded<Bounds>,
    /// The step budget, or `NotApplicable` when there is none.
    pub max_steps: Recorded<u64>,
    /// The reward the episode was scored under.
    pub reward: Recorded<RewardIdentity>,
}

impl Default for ResearchIdentity {
    /// Everything unknown. The identity of an episode nothing is known
    /// about, which is what a legacy recording is.
    fn default() -> Self {
        Self {
            identity_version: 0,
            agent: Recorded::Unknown,
            action: Recorded::Unknown,
            observation: Recorded::Unknown,
            obs_layout: Recorded::Unknown,
            autoreset: Recorded::Unknown,
            route: Recorded::Unknown,
            bounds: Recorded::Unknown,
            max_steps: Recorded::Unknown,
            reward: Recorded::Unknown,
        }
    }
}

/// How two [`Recorded`] fields compare.
///
/// The three-way rule is section 10's, stated in `Recorded`'s own doc
/// comment: two known values are `Same` or `Different`; two
/// `NotApplicable`s are `Same`; a `NotApplicable` against a value is
/// `Different`; anything involving an `Unknown` concludes nothing.
/// `Recorded::compare` implements exactly this and is **private to
/// `sailgym-physics`**, so this function is that rule applied from outside
/// the crate rather than a second rule. `tests::the_field_rule_is_section_tens`
/// pins every one of the nine cases.
fn field_match<T: PartialEq>(a: &Recorded<T>, b: &Recorded<T>) -> FieldMatch {
    match (a.value(), b.value()) {
        (Some(x), Some(y)) if x == y => FieldMatch::Same,
        (Some(_), Some(_)) => FieldMatch::Different,
        (Some(_), None) | (None, Some(_)) => {
            if a.is_not_applicable() || b.is_not_applicable() {
                FieldMatch::Different
            } else {
                FieldMatch::Indeterminate
            }
        }
        (None, None) => {
            if a.is_not_applicable() && b.is_not_applicable() {
                FieldMatch::Same
            } else {
                FieldMatch::Indeterminate
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FieldMatch {
    Same,
    Different,
    Indeterminate,
}

/// Assemble a [`Comparability`] from a fixed, ordered list of field verdicts.
///
/// The order is the order the reasons come back in, so two runs of a
/// comparison on the same pair produce identical text (F9.3, F9.4) — the
/// same property `ExperimentIdentity::compare` has.
fn verdict(fields: Vec<(&str, FieldMatch)>) -> Comparability {
    let mut different = Vec::new();
    let mut indeterminate = Vec::new();
    for (name, m) in fields {
        match m {
            FieldMatch::Same => {}
            FieldMatch::Different => different.push(name.to_string()),
            FieldMatch::Indeterminate => indeterminate.push(name.to_string()),
        }
    }
    if !different.is_empty() {
        Comparability::Different(different)
    } else if !indeterminate.is_empty() {
        Comparability::Indeterminate(indeterminate)
    } else {
        Comparability::SameConditions
    }
}

impl ResearchIdentity {
    /// Whether two episodes agree on the research contract.
    ///
    /// The **identity version itself is compared**: a record written under a
    /// contract this build does not implement cannot be declared equivalent
    /// to one written under this contract, however well its fields happen to
    /// line up.
    pub fn compare(&self, other: &Self) -> Comparability {
        let version = match (self.identity_version, other.identity_version) {
            // A document that predates the record says nothing about the
            // contract it was written under, so nothing may be concluded —
            // the same reading section 10 gives `identity_version == 0`.
            (0, _) | (_, 0) => FieldMatch::Indeterminate,
            (a, b) if a == b => FieldMatch::Same,
            _ => FieldMatch::Different,
        };
        verdict(vec![
            ("identity_version", version),
            ("agent", field_match(&self.agent, &other.agent)),
            ("action", field_match(&self.action, &other.action)),
            (
                "observation",
                field_match(&self.observation, &other.observation),
            ),
            (
                "obs_layout",
                field_match(&self.obs_layout, &other.obs_layout),
            ),
            ("autoreset", field_match(&self.autoreset, &other.autoreset)),
            ("route", field_match(&self.route, &other.route)),
            ("bounds", field_match(&self.bounds, &other.bounds)),
            ("max_steps", field_match(&self.max_steps, &other.max_steps)),
            ("reward", field_match(&self.reward, &other.reward)),
        ])
    }

    /// The cadence the agent decided at, if it is known.
    pub fn period_steps(&self) -> Option<u32> {
        self.agent.value().map(|a| a.cadence.period_steps)
    }

    /// The canonical text form, for a manifest or a log.
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// The envelope
// ---------------------------------------------------------------------------

/// A recorded episode plus everything a research consumer needs to know
/// about how it was produced.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResearchEnvelope {
    /// [`RESEARCH_ENVELOPE_VERSION`] when written by this build.
    pub envelope_version: u32,
    /// The research half of the identity.
    pub research: ResearchIdentity,
    /// The outcome the episode ended with.
    pub outcome: Recorded<Outcome>,
    /// The **complete** decision log (task 6.4), not a sample of it.
    pub decisions: Recorded<Vec<Decision>>,
    /// Physics steps executed before the episode ended.
    pub executed_steps: Recorded<u64>,
    /// The `u64` the episode was reset from (F17.3: one seed, one episode).
    pub seed: Recorded<u64>,
    /// The sampled recording, in **its own** schema. Written and read
    /// through section 10's codec, never re-encoded into another version.
    pub recording: Episode,
}

/// Why an envelope was refused.
#[derive(Clone, Debug, PartialEq)]
pub enum EnvelopeError {
    /// The nested recording, or the document around it, is malformed. The
    /// message is section 10's own where the nested codec produced it.
    Recording(EpisodeError),
    /// The document is not an envelope.
    Malformed(String),
    /// The document declares an envelope version this build does not
    /// implement.
    UnsupportedEnvelopeVersion { found: u32 },
    /// Two episodes may not be compared quantity for quantity.
    Incomparable(Comparability),
    /// The other episode carries no decision log, so there is nothing to
    /// resimulate.
    NoDecisionLog,
}

impl std::fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Recording(e) => write!(f, "the nested recording is not usable: {e}"),
            Self::Malformed(e) => write!(f, "research envelope is malformed: {e}"),
            Self::UnsupportedEnvelopeVersion { found } => write!(
                f,
                "research envelope version {found} is not supported; this build writes and \
                 reads {RESEARCH_ENVELOPE_VERSION}"
            ),
            Self::Incomparable(c) => write!(
                f,
                "these two episodes may not be compared action for action: {}",
                c.describe()
            ),
            Self::NoDecisionLog => write!(
                f,
                "the other episode carries no decision log, so its actions cannot be replayed"
            ),
        }
    }
}

impl std::error::Error for EnvelopeError {}

impl From<EpisodeError> for EnvelopeError {
    fn from(e: EpisodeError) -> Self {
        Self::Recording(e)
    }
}

impl ResearchEnvelope {
    /// An envelope around an episode nothing else is known about.
    ///
    /// Every research field is [`Recorded::Unknown`]. This is the only way a
    /// legacy recording enters the research world, and it enters it
    /// **unable to be compared** — which is the point.
    pub fn viewing(recording: Episode) -> Self {
        Self {
            envelope_version: RESEARCH_ENVELOPE_VERSION,
            research: ResearchIdentity::default(),
            outcome: Recorded::Unknown,
            decisions: Recorded::Unknown,
            executed_steps: Recorded::Unknown,
            seed: Recorded::Unknown,
            recording,
        }
    }

    /// The canonical comparison: section 10's on the nested header, this
    /// section's on the research contract, merged in that order.
    ///
    /// Reason names are prefixed `recording.` and `research.` so a caller can
    /// tell which half refused, and the two lists keep their own internal
    /// order.
    pub fn comparable_with(&self, other: &Self) -> Comparability {
        let base = self
            .recording
            .header
            .identity()
            .compare(&other.recording.header.identity());
        let mine = self.research.compare(&other.research);

        let mut different = Vec::new();
        let mut indeterminate = Vec::new();
        for (prefix, c) in [("recording", &base), ("research", &mine)] {
            match c {
                Comparability::SameConditions => {}
                Comparability::Different(r) => {
                    different.extend(r.iter().map(|n| format!("{prefix}.{n}")));
                }
                Comparability::Indeterminate(r) => {
                    indeterminate.extend(r.iter().map(|n| format!("{prefix}.{n}")));
                }
            }
        }
        if !different.is_empty() {
            Comparability::Different(different)
        } else if !indeterminate.is_empty() {
            Comparability::Indeterminate(indeterminate)
        } else {
            Comparability::SameConditions
        }
    }

    /// The other episode's decisions, for replaying **against this one** —
    /// or a refusal.
    ///
    /// Resimulating one episode's actions in another's conditions is only
    /// meaningful when the conditions are the same, so anything short of
    /// [`Comparability::SameConditions`] is an error and not a warning. A
    /// legacy recording is refused here because its identity is unknown, not
    /// because anybody remembered to check.
    pub fn resimulate_actions_against<'a>(
        &self,
        other: &'a Self,
    ) -> Result<&'a [Decision], EnvelopeError> {
        let c = self.comparable_with(other);
        if !c.is_same_conditions() {
            return Err(EnvelopeError::Incomparable(c));
        }
        match other.decisions.value() {
            Some(d) => Ok(d),
            None => Err(EnvelopeError::NoDecisionLog),
        }
    }

    /// JSON, with the nested recording written **through section 10's
    /// codec**.
    ///
    /// `Episode::to_json` runs its version check and its completeness check
    /// first, so an envelope can never carry a document the physics crate
    /// would refuse to write on its own.
    pub fn to_json(&self) -> Result<String, EnvelopeError> {
        let nested = self.recording.to_json()?;
        let nested: serde_json::Value =
            serde_json::from_str(&nested).map_err(|e| EnvelopeError::Malformed(e.to_string()))?;
        let mut doc =
            serde_json::to_value(self).map_err(|e| EnvelopeError::Malformed(e.to_string()))?;
        doc["recording"] = nested;
        serde_json::to_string(&doc).map_err(|e| EnvelopeError::Malformed(e.to_string()))
    }

    /// The inverse of [`ResearchEnvelope::to_json`].
    ///
    /// The nested recording is decoded by [`Episode::from_json`] — the
    /// existing codec, with its version check and its `validate` — so a
    /// schema-1 document is accepted as a schema-1 document and a schema this
    /// build does not implement is refused with section 10's own message.
    pub fn from_json(text: &str) -> Result<Self, EnvelopeError> {
        let doc: serde_json::Value =
            serde_json::from_str(text).map_err(|e| EnvelopeError::Malformed(e.to_string()))?;
        let found = doc
            .get("envelope_version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                EnvelopeError::Malformed("no envelope_version in the document".to_string())
            })?;
        let capped = found.min(u64::from(u32::MAX)) as u32;
        if u64::from(capped) != found || capped != RESEARCH_ENVELOPE_VERSION {
            return Err(EnvelopeError::UnsupportedEnvelopeVersion { found: capped });
        }
        let nested = doc
            .get("recording")
            .ok_or_else(|| EnvelopeError::Malformed("no recording in the document".to_string()))?;
        let nested_text =
            serde_json::to_string(nested).map_err(|e| EnvelopeError::Malformed(e.to_string()))?;
        // Section 10's codec, not a second reader.
        let recording = Episode::from_json(&nested_text)?;

        let mut shell = doc.clone();
        shell["recording"] = serde_json::Value::Null;
        #[derive(Deserialize)]
        struct Shell {
            envelope_version: u32,
            research: ResearchIdentity,
            outcome: Recorded<Outcome>,
            decisions: Recorded<Vec<Decision>>,
            executed_steps: Recorded<u64>,
            seed: Recorded<u64>,
        }
        let shell: Shell =
            serde_json::from_value(shell).map_err(|e| EnvelopeError::Malformed(e.to_string()))?;
        Ok(Self {
            envelope_version: shell.envelope_version,
            research: shell.research,
            outcome: shell.outcome,
            decisions: shell.decisions,
            executed_steps: shell.executed_steps,
            seed: shell.seed,
            recording,
        })
    }

    /// The cadence the decisions were taken at, if it is known.
    pub fn cadence(&self) -> Option<Cadence> {
        self.research.agent.value().map(|a| a.cadence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sailgym_physics::parameters::BoatParameters;
    use sailgym_physics::recording::{EpisodeHeader, Recorder, EPISODE_SCHEMA_VERSION};
    use sailgym_physics::scenario::load_shipped;
    use sailgym_physics::simulation::Simulation;

    /// A real, short, schema-2 recording made the way the browser makes one.
    fn recorded(scenario: &str) -> Episode {
        let sc = load_shipped(scenario).expect("a shipped scenario");
        let params: BoatParameters = sc.to_parameters().expect("a catalogue");
        let mut sim = Simulation::new(params, sc.seed);
        sim.load_scenario(&sc).expect("the scenario loads");
        let header = EpisodeHeader::manual(
            sc.clone(),
            *sim.params(),
            10.0,
            "1970-01-01T00:00:00Z".to_string(),
            *sim.state(),
            *sim.controls(),
        );
        let mut rec = Recorder::start(10.0, header);
        for _ in 0..200 {
            sim.advance(1);
            if rec.due(sim.state().t) {
                let d = sailgym_physics::diagnostics::diagnostics(&sim);
                rec.observe(&sim, &d);
            }
        }
        rec.finish()
    }

    /// A schema-1 document, written by hand the way a file from before
    /// section 10 would look: the seven schema-1 header fields and no more.
    fn legacy() -> Episode {
        let full = recorded("free_sail");
        let mut doc = serde_json::to_value(&full).expect("serialises");
        let header = doc["header"].as_object_mut().expect("an object");
        header.insert("schema_version".into(), serde_json::json!(1));
        for added in [
            "identity_version",
            "model",
            "initial_state",
            "initial_controls",
            "practice",
            "action",
            "observation",
        ] {
            header.remove(added);
        }
        // Schema 1 has no per-frame diagnostics block.
        for frame in doc["frames"].as_array_mut().expect("frames") {
            frame.as_object_mut().expect("a frame").remove("diag");
        }
        Episode::from_json(&serde_json::to_string(&doc).expect("serialises"))
            .expect("a schema-1 document is still readable")
    }

    fn identity(seed_note: &str) -> ResearchIdentity {
        ResearchIdentity {
            identity_version: RESEARCH_IDENTITY_VERSION,
            agent: Recorded::Value(AgentSpec::new(
                seed_note,
                1,
                sailgym_agent::spec::ActionSpace::Rates,
                Cadence::new(10),
            )),
            action: Recorded::Value(ActionIdentity {
                adapter: "rate".to_string(),
                version: 1,
                period_steps: 10,
            }),
            observation: Recorded::NotApplicable,
            obs_layout: Recorded::NotApplicable,
            autoreset: Recorded::Value(AutoresetMode::NextStep),
            route: Recorded::NotApplicable,
            bounds: Recorded::Value(Bounds::Unbounded),
            max_steps: Recorded::NotApplicable,
            reward: Recorded::Value(RewardIdentity {
                id: "zero".to_string(),
                version: 1,
            }),
        }
    }

    fn envelope(scenario: &str, agent_id: &str) -> ResearchEnvelope {
        ResearchEnvelope {
            envelope_version: RESEARCH_ENVELOPE_VERSION,
            research: identity(agent_id),
            outcome: Recorded::Value(Outcome::Truncated),
            decisions: Recorded::Value(vec![
                Decision {
                    step: 0,
                    action: vec![0.0, -1.0, -1.0],
                },
                Decision {
                    step: 10,
                    action: vec![0.25, -1.0, -1.0],
                },
            ]),
            executed_steps: Recorded::Value(200),
            seed: Recorded::Value(7),
            recording: recorded(scenario),
        }
    }

    /// Task 6.1's acceptance, first half: the nested recording and the
    /// research metadata both survive a round trip, and the nested document
    /// keeps its own schema.
    #[test]
    fn a_nested_recording_and_its_research_metadata_round_trip() {
        let env = envelope("free_sail", "stub");
        let text = env.to_json().expect("writes");
        let back = ResearchEnvelope::from_json(&text).expect("reads");
        assert_eq!(env, back);
        assert_eq!(back.recording.schema_version(), EPISODE_SCHEMA_VERSION);
        assert!(!back.recording.frames.is_empty());
        assert_eq!(back.cadence(), Some(Cadence::new(10)));
        assert_eq!(
            back.decisions.value().expect("a log").len(),
            2,
            "the decision array travels in the envelope"
        );

        // The nested document is written by section 10's codec, so the text
        // inside the envelope is the text `Episode::to_json` produces.
        let doc: serde_json::Value = serde_json::from_str(&text).expect("json");
        let nested = serde_json::to_string(&doc["recording"]).expect("json");
        assert_eq!(
            Episode::from_json(&nested).expect("the nested codec accepts it"),
            env.recording
        );
    }

    /// Task 6.1's acceptance, second half: a legacy recording stays
    /// inspectable and **cannot acquire fabricated agent metadata**.
    #[test]
    fn a_legacy_recording_is_viewable_and_never_gains_agent_metadata() {
        let old = legacy();
        assert_eq!(old.schema_version(), 1, "the fixture must be schema 1");

        let env = ResearchEnvelope::viewing(old.clone());
        // Nothing was invented.
        for (name, known) in [
            ("agent", env.research.agent.is_known()),
            ("action", env.research.action.is_known()),
            ("observation", env.research.observation.is_known()),
            ("obs_layout", env.research.obs_layout.is_known()),
            ("autoreset", env.research.autoreset.is_known()),
            ("route", env.research.route.is_known()),
            ("bounds", env.research.bounds.is_known()),
            ("max_steps", env.research.max_steps.is_known()),
            ("reward", env.research.reward.is_known()),
        ] {
            assert!(!known, "viewing invented a value for `{name}`");
        }
        assert!(!env.decisions.is_known());
        assert_eq!(env.research.identity_version, 0);

        // It round-trips, and it is **still schema 1** afterwards: the
        // envelope does not migrate the document it wraps (RV60).
        let back = ResearchEnvelope::from_json(&env.to_json().expect("writes")).expect("reads");
        assert_eq!(back.recording.schema_version(), 1);
        assert_eq!(back, env);
        assert_eq!(back.recording, old);

        // And it is viewable: the frames are all there.
        assert_eq!(back.recording.frames.len(), old.frames.len());
        assert!(back.recording.frames.iter().all(|f| f.diag.is_none()));
    }

    /// Task 6.1's acceptance, third half: incompatible identities refuse
    /// action-resimulation comparisons.
    #[test]
    fn incompatible_identities_refuse_an_action_resimulation() {
        let a = envelope("free_sail", "stub");
        let b = envelope("free_sail", "stub");
        assert_eq!(a.comparable_with(&b), Comparability::SameConditions);
        assert_eq!(
            a.resimulate_actions_against(&b).expect("comparable").len(),
            2
        );

        // A different agent is a different experiment.
        let other_agent = envelope("free_sail", "rule_sailor");
        let why = a
            .resimulate_actions_against(&other_agent)
            .expect_err("a different agent is not the same conditions");
        assert!(format!("{why}").contains("research.agent"), "{why}");

        // A different scenario is refused by **section 10's** half.
        let other_scenario = envelope("tack", "stub");
        let why = a
            .resimulate_actions_against(&other_scenario)
            .expect_err("a different scenario is not the same conditions");
        let text = format!("{why}");
        assert!(text.contains("recording.scenario"), "{text}");

        // A legacy recording is refused because nothing is known about it —
        // Indeterminate, not Different.
        let old = ResearchEnvelope::viewing(legacy());
        let why = old
            .resimulate_actions_against(&a)
            .expect_err("an unknown identity may not be compared");
        assert!(
            matches!(&why, EnvelopeError::Incomparable(c) if !c.is_same_conditions()),
            "{why}"
        );
        assert!(format!("{why}").contains("not comparable"), "{why}");

        // Comparable conditions but no log is a *different* refusal, so a
        // caller can tell "not the same experiment" from "nothing recorded".
        let mut no_log = envelope("free_sail", "stub");
        no_log.decisions = Recorded::NotApplicable;
        assert_eq!(
            a.resimulate_actions_against(&no_log),
            Err(EnvelopeError::NoDecisionLog)
        );
    }

    /// The nine cases of section 10's three-valued rule, applied from
    /// outside the crate that owns it.
    #[test]
    fn the_field_rule_is_section_tens() {
        use Recorded::{NotApplicable, Unknown, Value};
        let cases: [(Recorded<u32>, Recorded<u32>, FieldMatch); 9] = [
            (Value(1), Value(1), FieldMatch::Same),
            (Value(1), Value(2), FieldMatch::Different),
            (Value(1), Unknown, FieldMatch::Indeterminate),
            (Value(1), NotApplicable, FieldMatch::Different),
            (Unknown, Value(1), FieldMatch::Indeterminate),
            (Unknown, Unknown, FieldMatch::Indeterminate),
            (Unknown, NotApplicable, FieldMatch::Indeterminate),
            (NotApplicable, Value(1), FieldMatch::Different),
            (NotApplicable, NotApplicable, FieldMatch::Same),
        ];
        for (a, b, want) in cases {
            assert_eq!(field_match(&a, &b), want, "{a:?} vs {b:?}");
        }
    }

    /// An envelope from a version this build does not implement is refused,
    /// with a message that names what it does implement.
    #[test]
    fn an_unknown_envelope_version_is_refused() {
        let env = envelope("free_sail", "stub");
        let mut doc: serde_json::Value =
            serde_json::from_str(&env.to_json().expect("writes")).expect("json");
        doc["envelope_version"] = serde_json::json!(2);
        let why = ResearchEnvelope::from_json(&doc.to_string())
            .expect_err("envelope version 2 does not exist");
        assert_eq!(why, EnvelopeError::UnsupportedEnvelopeVersion { found: 2 });
        assert!(format!("{why}").contains('1'), "{why}");

        // A nested recording from an unsupported schema is refused by
        // section 10's codec, with section 10's message.
        let mut doc: serde_json::Value =
            serde_json::from_str(&env.to_json().expect("writes")).expect("json");
        doc["recording"]["header"]["schema_version"] = serde_json::json!(99);
        let why =
            ResearchEnvelope::from_json(&doc.to_string()).expect_err("schema 99 does not exist");
        assert!(
            matches!(
                why,
                EnvelopeError::Recording(EpisodeError::UnsupportedSchemaVersion { found: 99 })
            ),
            "{why}"
        );
    }

    /// RV34, as a grep over this crate's own sources: the word this section
    /// is not allowed to spell.
    ///
    /// A `bool done` anywhere in a signature conflates termination with
    /// truncation, which biases every bootstrapped value silently. The
    /// scanner is shown able to find one.
    #[test]
    fn no_done_flag_in_this_crate() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut offenders = Vec::new();
        let mut scanned = 0usize;
        for file in rust_sources(&root.join("src"))
            .into_iter()
            .chain(rust_sources(&root.join("tests")))
        {
            let text = std::fs::read_to_string(&file).expect("a readable source");
            scanned += 1;
            // Outside `#[cfg(test)]`, and outside comments — the same
            // exclusion the physics and agent crates' greps use, which is
            // what lets this very test spell the forbidden shapes out.
            for (n, line) in code_lines(&text) {
                let code = line.split("//").next().unwrap_or("");
                if looks_like_a_done_flag(code) {
                    offenders.push(format!(
                        "{}:{n}: {}",
                        file.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
                        code.trim()
                    ));
                }
            }
        }
        assert!(scanned >= 3, "the scanner saw only {scanned} files");
        assert!(
            offenders.is_empty(),
            "a `done` flag has appeared; RV34 says termination and truncation stay \
             separate fields:\n{}",
            offenders.join("\n")
        );
        // …and the scanner would notice one.
        assert!(looks_like_a_done_flag("    pub done: bool,"));
        assert!(looks_like_a_done_flag(
            "fn step(&mut self, dones: &mut [u8]) {}"
        ));
        assert!(!looks_like_a_done_flag("    the episode is done"));
        assert!(!looks_like_a_done_flag("let undone = 1;"));
        // …and it ignores a `#[cfg(test)]` item, which is why the needles
        // above may be written at all.
        let fixture = "#[cfg(test)]\nmod t {\n    pub done: bool,\n}\nfn f() {}\n";
        assert!(code_lines(fixture).iter().all(|(_, l)| !l.contains("done")));
    }

    fn looks_like_a_done_flag(line: &str) -> bool {
        ["done: bool", "done : bool", "dones: "]
            .iter()
            .any(|needle| line.contains(needle))
    }

    /// Lines outside a `#[cfg(test)]` item, one-based.
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

    fn rust_sources(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return out;
        };
        let mut entries: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
        // A fixed order: F9.3 forbids hash iteration and a directory listing
        // is no more ordered than a hash map.
        entries.sort();
        for path in entries {
            if path.is_dir() {
                out.extend(rust_sources(&path));
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                out.push(path);
            }
        }
        out
    }
}
