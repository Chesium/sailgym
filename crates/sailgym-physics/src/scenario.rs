//! Scenario description and loading (brief §32, task 9.1).
//!
//! A scenario is **an initial condition and an environment, and nothing
//! more** (brief §32, last line). There is no scripted future control
//! sequence, no forced outcome and no timed event anywhere in this file or in
//! the six shipped documents; `shipped::no_scripted_outcomes` is the standing
//! proof.
//!
//! ## Degrees live here and only here
//!
//! F1 puts radians everywhere in Rust and degrees only at UI boundaries and in
//! scenario JSON. Scenario JSON *is* such a boundary, so [`InitialState`]
//! carries `heading_deg`, `heel_deg` and `boom_deg_to_port` — and they are
//! converted in exactly one place, [`Scenario::to_boat_state`], with
//! [`InitialState::from_boat_state`] as its documented inverse. The round trip
//! is asserted by `tests::angle_round_trip`; nothing else in the crate may
//! restate the conversions.
//!
//! The two that are easy to get wrong, stated once:
//!
//! * `heading_deg` is a **compass** heading, degrees clockwise from north.
//!   `ψ` is counter-clockwise from world `+x` (east), so `ψ = 90° − heading`.
//! * `boom_deg_to_port` is the human-facing boom angle F2.1 defines as
//!   `−β·180/π`, so `β = −boom_deg_to_port·π/180`. The sign flip is applied
//!   once, here.
//!
//! ## Overrides are sparse and ordered
//!
//! `parameter_overrides` is a [`BTreeMap`], never a hash container (F9.3): the
//! overrides are applied in key order, so two maps built by different
//! insertion orders produce bit-identical [`BoatParameters`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::environment::wind::WindConfig;
use crate::frames::wrap_pi;
use crate::parameters::{BoatParameters, ParamError};
use crate::stability::hydrostatics::GzCurve;
use crate::state::{BoatState, Controls};

/// The only scenario schema this build understands.
pub const SCENARIO_SCHEMA_VERSION: u32 = 1;

/// The scenario the application loads when nothing else is asked for
/// (brief §32, `free_sail` — "neutral general sandbox").
pub const DEFAULT_SCENARIO: &str = "free_sail";

/// The six shipped configurations of brief §32, embedded from `scenarios/`.
///
/// Embedded rather than read from disk so the browser and the native headless
/// simulator get the same six documents without either of them needing a file
/// system, and so there is exactly one copy in the repository.
pub const SHIPPED: [(&str, &str); 6] = [
    (
        "beam_reach_capsize",
        include_str!("../../../scenarios/beam_reach_capsize.json"),
    ),
    (
        "close_hauled",
        include_str!("../../../scenarios/close_hauled.json"),
    ),
    (
        "free_sail",
        include_str!("../../../scenarios/free_sail.json"),
    ),
    ("gybe", include_str!("../../../scenarios/gybe.json")),
    (
        "sheet_release_recovery",
        include_str!("../../../scenarios/sheet_release_recovery.json"),
    ),
    ("tack", include_str!("../../../scenarios/tack.json")),
];

/// Why a scenario was rejected.
///
/// Each cause is its own variant: `validate_rejects_bad` asserts that a
/// negative sheet length, a missing seed and an unsupported schema version are
/// distinguishable, because "the scenario is bad" is not an error message
/// anybody can act on.
#[derive(Clone, Debug, PartialEq)]
pub enum ScenarioError {
    /// The document is not valid JSON, or not shaped like a scenario.
    Parse(String),
    /// A required key is absent from the document.
    MissingField(&'static str),
    /// The document declares a schema this build does not implement.
    UnsupportedSchemaVersion { found: u32, supported: u32 },
    /// A field is present and well-typed but not a usable value.
    OutOfRange { field: &'static str, reason: String },
    /// `parameter_overrides` names a path `BoatParameters` does not have.
    UnknownOverride(String),
    /// The resolved catalogue is not one the equations of motion can use.
    Parameter(ParamError),
    /// The wind configuration is not one the field can be built from.
    Wind(String),
}

impl std::fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "scenario is not a valid document: {e}"),
            Self::MissingField(k) => write!(f, "scenario is missing the required field `{k}`"),
            Self::UnsupportedSchemaVersion { found, supported } => write!(
                f,
                "scenario schema_version {found} is not supported; this build reads {supported}"
            ),
            Self::OutOfRange { field, reason } => write!(f, "scenario field `{field}`: {reason}"),
            Self::UnknownOverride(p) => {
                write!(f, "scenario overrides unknown parameter path `{p}`")
            }
            Self::Parameter(e) => write!(f, "scenario parameters: {e}"),
            Self::Wind(e) => write!(f, "scenario wind: {e}"),
        }
    }
}

impl std::error::Error for ScenarioError {}

/// Which way the camera should look when a scenario loads. A **suggestion**:
/// it is view state, and the player may override it at any time (brief §27).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CameraMode {
    /// Chase the boat, rotating with its heading.
    Follow,
    /// World-fixed, north up.
    #[default]
    NorthUp,
}

/// The camera the scenario suggests (brief §32, "camera suggestion").
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraSuggestion {
    pub mode: CameraMode,
    /// Dimensionless zoom multiplier; `1.0` is the renderer's base scale.
    pub zoom: f64,
}

impl Default for CameraSuggestion {
    fn default() -> Self {
        Self {
            mode: CameraMode::NorthUp,
            zoom: 1.0,
        }
    }
}

/// Optional initial control values.
///
/// **Not a script of future inputs** (brief §32). These are the commands in
/// force at `t = 0`; the player or the agent owns every command after that.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ControlsSpec {
    /// normalised `[−1, 1]`; `+1` = steer bow to starboard (F3).
    pub rudder_rate: f64,
    /// normalised `[−1, 1]`; `+1` = ease, `−1` = haul (F3).
    pub sheet_rate: f64,
    /// Ease at the release rate, overriding `sheet_rate` (F3).
    pub release: bool,
}

impl ControlsSpec {
    pub fn to_controls(self) -> Controls {
        Controls {
            rudder_rate_cmd: self.rudder_rate,
            sheet_rate_cmd: self.sheet_rate,
            sheet_release: self.release,
        }
    }

    pub fn from_controls(c: &Controls) -> Self {
        Self {
            rudder_rate: c.rudder_rate_cmd,
            sheet_rate: c.sheet_rate_cmd,
            release: c.sheet_release,
        }
    }
}

/// The human-facing initial condition (task 9.1).
///
/// Angles are degrees, because this is a UI/JSON boundary (F1). They become
/// radians in [`Scenario::to_boat_state`] and nowhere else.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct InitialState {
    /// m, world east.
    pub x: f64,
    /// m, world north.
    pub y: f64,
    /// deg, **compass**: clockwise from north. 90 is due east.
    pub heading_deg: f64,
    /// deg, roll; positive is starboard down (F2).
    pub heel_deg: f64,
    /// m/s, initial forward speed (surge).
    pub speed: f64,
    /// deg, boom angle to port — the human-facing form of `β` (F2.1).
    pub boom_deg_to_port: f64,
    /// m, available mainsheet length at the boom attachment.
    pub sheet_length: f64,
}

impl Default for InitialState {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            heading_deg: 90.0,
            heel_deg: 0.0,
            speed: 0.0,
            boom_deg_to_port: 0.0,
            sheet_length: 0.0,
        }
    }
}

impl InitialState {
    /// The inverse of [`Scenario::to_boat_state`]'s conversions.
    ///
    /// Used when a run was started from something other than a shipped
    /// document (an ad-hoc reset from the browser) and a recording still has
    /// to say honestly where the episode began. `tests::angle_round_trip` is
    /// what keeps the two in step.
    ///
    /// The velocity components the schema cannot express — sway, yaw rate,
    /// roll rate, boom rate, rudder angle — are dropped, which is exactly why
    /// [`EpisodeHeader`](crate::recording::EpisodeHeader) also carries the
    /// resolved state rather than only the scenario.
    pub fn from_boat_state(st: &BoatState) -> Self {
        Self {
            x: st.x,
            y: st.y,
            heading_deg: (90.0 - st.psi.to_degrees()).rem_euclid(360.0),
            heel_deg: st.phi.to_degrees(),
            speed: st.u,
            boom_deg_to_port: -st.beta.to_degrees(),
            sheet_length: st.l_sheet,
        }
    }
}

/// A serialisable scenario: initial condition plus environment (brief §32).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    /// Always [`SCENARIO_SCHEMA_VERSION`] in this build.
    pub schema_version: u32,
    /// The scenario id. Equal to the file stem for the shipped six.
    pub name: String,
    pub description: String,
    pub seed: u64,
    /// Sparse dotted-path overrides onto [`BoatParameters::ilca7`]. Not a full
    /// copy, and applied in key order so the result is deterministic (F9.3).
    pub parameter_overrides: BTreeMap<String, f64>,
    pub initial_state: InitialState,
    pub wind: WindConfig,
    pub camera: CameraSuggestion,
    /// Optional initial control values. **Not** a script of future inputs
    /// (brief §32).
    pub initial_controls: Option<ControlsSpec>,
}

/// Keys a scenario document must carry. A missing one is its own error rather
/// than a shape complaint from `serde`, so the message names the field.
const REQUIRED: [&str; 5] = ["schema_version", "name", "seed", "initial_state", "wind"];

impl Scenario {
    /// Parse and validate a scenario document.
    pub fn load(json: &str) -> Result<Self, ScenarioError> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|e| ScenarioError::Parse(e.to_string()))?;
        let object = value.as_object().ok_or_else(|| {
            ScenarioError::Parse("a scenario document must be a JSON object".to_string())
        })?;

        for key in REQUIRED {
            if !object.contains_key(key) {
                // `REQUIRED` is a `const`, so the `&'static str` is genuine.
                let known = REQUIRED
                    .iter()
                    .find(|k| **k == key)
                    .expect("key comes from REQUIRED");
                return Err(ScenarioError::MissingField(known));
            }
        }

        // Checked before deserialising the rest: a document from a future
        // schema should report its version, not a field-shape complaint about
        // whichever field happened to move.
        let found = object["schema_version"].as_u64().ok_or_else(|| {
            ScenarioError::Parse("schema_version must be a non-negative integer".to_string())
        })?;
        if found != u64::from(SCENARIO_SCHEMA_VERSION) {
            return Err(ScenarioError::UnsupportedSchemaVersion {
                found: found.min(u64::from(u32::MAX)) as u32,
                supported: SCENARIO_SCHEMA_VERSION,
            });
        }

        let scenario: Scenario =
            serde_json::from_value(value).map_err(|e| ScenarioError::Parse(e.to_string()))?;
        scenario.validate()?;
        Ok(scenario)
    }

    /// The initial [`BoatState`], with every human-facing angle converted
    /// **here and nowhere else**.
    pub fn to_boat_state(&self) -> BoatState {
        let s = &self.initial_state;
        BoatState {
            x: s.x,
            y: s.y,
            // Compass (CW from north) → ψ (CCW from world +x): 90° is east,
            // which is ψ = 0.
            psi: wrap_pi((90.0 - s.heading_deg).to_radians()),
            // φ is deliberately not wrapped (F3).
            phi: s.heel_deg.to_radians(),
            u: s.speed,
            v: 0.0,
            r: 0.0,
            p: 0.0,
            // F2.1: the UI shows `boom_deg_to_port = −β·180/π`, so the sign
            // flips exactly once, here.
            beta: wrap_pi(-s.boom_deg_to_port.to_radians()),
            beta_dot: 0.0,
            delta_r: 0.0,
            l_sheet: s.sheet_length,
            t: 0.0,
        }
    }

    /// The fully resolved parameter catalogue: [`BoatParameters::ilca7`] with
    /// the sparse overrides applied in key order.
    ///
    /// The validation mirrors `Simulation::set_parameter` (section 08 handoff
    /// §2.1): the catalogue must validate *and* the F6.7 `GZ` curve must fit,
    /// because a `stability` group `fit` rejects yields a boat with no
    /// righting arm at all, silently.
    pub fn to_parameters(&self) -> Result<BoatParameters, ScenarioError> {
        let mut p = BoatParameters::ilca7();
        for (path, value) in &self.parameter_overrides {
            p.set_path(path, *value).map_err(|e| match e {
                ParamError::UnknownPath(path) => ScenarioError::UnknownOverride(path),
                other => ScenarioError::Parameter(other),
            })?;
        }
        p.validate().map_err(ScenarioError::Parameter)?;
        GzCurve::fit(
            p.stability.gm,
            p.stability.phi_peak,
            p.stability.gz_max,
            p.stability.phi_vanish,
        )
        .map_err(ScenarioError::Parameter)?;
        Ok(p)
    }

    /// The controls in force at `t = 0` (`Controls::default()` when the
    /// scenario names none).
    pub fn to_controls(&self) -> Controls {
        self.initial_controls.unwrap_or_default().to_controls()
    }

    /// Everything that can be checked without running the simulation.
    pub fn validate(&self) -> Result<(), ScenarioError> {
        if self.schema_version != SCENARIO_SCHEMA_VERSION {
            return Err(ScenarioError::UnsupportedSchemaVersion {
                found: self.schema_version,
                supported: SCENARIO_SCHEMA_VERSION,
            });
        }
        if self.name.trim().is_empty() {
            return Err(ScenarioError::OutOfRange {
                field: "name",
                reason: "must not be empty".to_string(),
            });
        }

        let s = &self.initial_state;
        let finite = |v: f64, field: &'static str| -> Result<(), ScenarioError> {
            if v.is_finite() {
                Ok(())
            } else {
                Err(ScenarioError::OutOfRange {
                    field,
                    reason: format!("must be finite, got {v}"),
                })
            }
        };
        finite(s.x, "initial_state.x")?;
        finite(s.y, "initial_state.y")?;
        finite(s.heading_deg, "initial_state.heading_deg")?;
        finite(s.heel_deg, "initial_state.heel_deg")?;
        finite(s.speed, "initial_state.speed")?;
        finite(s.boom_deg_to_port, "initial_state.boom_deg_to_port")?;
        finite(s.sheet_length, "initial_state.sheet_length")?;
        if s.sheet_length < 0.0 {
            return Err(ScenarioError::OutOfRange {
                field: "initial_state.sheet_length",
                reason: format!(
                    "a rope cannot be shorter than nothing, got {}",
                    s.sheet_length
                ),
            });
        }

        if !(self.camera.zoom.is_finite() && self.camera.zoom > 0.0) {
            return Err(ScenarioError::OutOfRange {
                field: "camera.zoom",
                reason: format!("must be finite and positive, got {}", self.camera.zoom),
            });
        }
        if let Some(c) = self.initial_controls {
            finite(c.rudder_rate, "initial_controls.rudder_rate")?;
            finite(c.sheet_rate, "initial_controls.sheet_rate")?;
        }

        self.wind
            .validate()
            .map_err(|e| ScenarioError::Wind(e.to_string()))?;
        // Rejects an unknown override path, and an override that makes the
        // catalogue unusable, at load time rather than at the first step.
        self.to_parameters()?;
        Ok(())
    }
}

/// The names of the six shipped scenarios, in the order they are listed.
pub fn shipped_names() -> [&'static str; 6] {
    [
        SHIPPED[0].0,
        SHIPPED[1].0,
        SHIPPED[2].0,
        SHIPPED[3].0,
        SHIPPED[4].0,
        SHIPPED[5].0,
    ]
}

/// The raw JSON text of a shipped scenario.
pub fn shipped_source(name: &str) -> Option<&'static str> {
    SHIPPED
        .iter()
        .find(|(id, _)| *id == name)
        .map(|(_, text)| *text)
}

/// Load one shipped scenario by id.
pub fn load_shipped(name: &str) -> Result<Scenario, ScenarioError> {
    let source = shipped_source(name)
        .ok_or_else(|| ScenarioError::Parse(format!("no shipped scenario named `{name}`")))?;
    Scenario::load(source)
}

/// All six shipped scenarios, in listing order.
pub fn load_all_shipped() -> Result<Vec<Scenario>, ScenarioError> {
    SHIPPED
        .iter()
        .map(|(_, source)| Scenario::load(source))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    fn free_sail() -> Scenario {
        load_shipped("free_sail").expect("the default scenario must load")
    }

    #[test]
    fn round_trip() {
        // serialise → deserialise → serialise is byte-identical, for all six.
        for (name, source) in SHIPPED {
            let first = Scenario::load(source).unwrap_or_else(|e| panic!("{name}: {e}"));
            let once = serde_json::to_string(&first).expect("a scenario must serialise");
            let back: Scenario = serde_json::from_str(&once).expect("…and deserialise");
            let twice = serde_json::to_string(&back).expect("…and serialise again");
            assert_eq!(once, twice, "{name} did not survive the round trip");
            assert_eq!(first, back, "{name} changed value across the round trip");
        }
    }

    #[test]
    fn boom_conversion() {
        // F2.1's sign flip, applied once and correctly: 30° to port is a
        // *negative* β, because +β swings the boom tip to starboard.
        let mut sc = free_sail();
        sc.initial_state.boom_deg_to_port = 30.0;
        let beta = sc.to_boat_state().beta;
        // 30° is π/6, so the PRD's ≈ −0.524 rad is exactly −π/6.
        assert!(
            (beta + std::f64::consts::FRAC_PI_6).abs() < 1e-12,
            "beta = {beta}, expected ≈ -0.524"
        );
        assert!((beta + 0.524).abs() < 1e-3, "beta = {beta}");
        assert!(beta < 0.0, "a boom to port must give a negative beta");

        // …and the mirror image, so the sign is a convention and not a
        // constant.
        sc.initial_state.boom_deg_to_port = -30.0;
        assert!((sc.to_boat_state().beta - std::f64::consts::FRAC_PI_6).abs() < 1e-12);
    }

    #[test]
    fn heading_conversion() {
        // Hand-computed against F2: ψ is CCW from world +x (east), the
        // compass is CW from north (world +y).
        let cases = [
            // heading_deg, expected psi
            (90.0_f64, 0.0_f64),                        // east  → +x
            (0.0, FRAC_PI_2),                           // north → +y
            (180.0, -FRAC_PI_2),                        // south → −y
            (270.0, std::f64::consts::PI),              // west  → −x, wrapped to +π
            (45.0, std::f64::consts::FRAC_PI_4),        // north-east
            (315.0, 3.0 * std::f64::consts::FRAC_PI_4), // north-west
        ];
        let mut sc = free_sail();
        for (heading, expected) in cases {
            sc.initial_state.heading_deg = heading;
            let psi = sc.to_boat_state().psi;
            assert!(
                (psi - expected).abs() < 1e-12,
                "heading {heading}° gave psi {psi}, expected {expected}"
            );
        }
    }

    #[test]
    fn angle_round_trip() {
        // `from_boat_state` is documented as the inverse of `to_boat_state`;
        // this is what keeps that true.
        let mut sc = free_sail();
        for (heading, heel, boom, speed, sheet) in [
            (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 1.0_f64),
            (45.0, 12.5, -35.0, 1.8, 2.0),
            (170.0, -7.25, 75.0, 2.5, 4.0),
            (315.0, 30.0, 100.0, -0.5, 0.9),
        ] {
            sc.initial_state = InitialState {
                heading_deg: heading,
                heel_deg: heel,
                boom_deg_to_port: boom,
                speed,
                sheet_length: sheet,
                x: 12.5,
                y: -3.25,
            };
            let back = InitialState::from_boat_state(&sc.to_boat_state());
            assert!((back.heading_deg - heading).abs() < 1e-9, "{back:?}");
            assert!((back.heel_deg - heel).abs() < 1e-9);
            assert!((back.boom_deg_to_port - boom).abs() < 1e-9);
            assert_eq!(back.speed, speed);
            assert_eq!(back.sheet_length, sheet);
            assert_eq!(back.x, 12.5);
            assert_eq!(back.y, -3.25);
        }
    }

    #[test]
    fn override_determinism() {
        // The same overrides, inserted in two different orders, must give
        // bit-identical parameters. `BTreeMap` is what guarantees it (F9.3).
        let pairs = [
            ("sail.area", 6.5_f64),
            ("resistance.x_uu", 9.5),
            ("board.area", 0.22),
            ("inertia.i_zz", 160.0),
            ("sheet.c_sheet", 310.0),
        ];
        let mut forward = BTreeMap::new();
        for (k, v) in pairs {
            forward.insert(k.to_string(), v);
        }
        let mut backward = BTreeMap::new();
        for (k, v) in pairs.iter().rev() {
            backward.insert((*k).to_string(), *v);
        }
        assert_ne!(
            pairs.first().map(|p| p.0),
            pairs.last().map(|p| p.0),
            "the two insertion orders must differ"
        );

        let build = |map: BTreeMap<String, f64>| {
            let mut sc = free_sail();
            sc.parameter_overrides = map;
            sc.to_parameters().expect("the overrides are all valid")
        };
        let a = build(forward);
        let b = build(backward);
        assert_eq!(a, b);
        // Bit-identical, not merely `PartialEq`-equal: the regression files
        // depend on the last bit.
        for path in pairs.map(|p| p.0) {
            assert_eq!(
                a.get_path(path).unwrap().to_bits(),
                b.get_path(path).unwrap().to_bits(),
                "{path} differs in its bits"
            );
        }
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }

    #[test]
    fn unknown_override_rejected() {
        let mut sc = free_sail();
        sc.parameter_overrides
            .insert("sail.no_such_field".to_string(), 1.0);
        assert_eq!(
            sc.to_parameters().unwrap_err(),
            ScenarioError::UnknownOverride("sail.no_such_field".to_string())
        );
        // …and it is rejected at load, not silently carried into the run.
        assert!(matches!(
            sc.validate(),
            Err(ScenarioError::UnknownOverride(_))
        ));

        let document = serde_json::to_string(&sc).unwrap();
        assert!(matches!(
            Scenario::load(&document),
            Err(ScenarioError::UnknownOverride(_))
        ));
    }

    #[test]
    fn validate_rejects_bad() {
        // Three distinct causes, three distinct variants.
        let base: serde_json::Value =
            serde_json::from_str(shipped_source("free_sail").unwrap()).unwrap();

        let mut negative = base.clone();
        negative["initial_state"]["sheet_length"] = serde_json::json!(-1.0);
        assert_eq!(
            Scenario::load(&negative.to_string()).unwrap_err(),
            ScenarioError::OutOfRange {
                field: "initial_state.sheet_length",
                reason: "a rope cannot be shorter than nothing, got -1".to_string(),
            }
        );

        let mut seedless = base.clone();
        seedless.as_object_mut().unwrap().remove("seed");
        assert_eq!(
            Scenario::load(&seedless.to_string()).unwrap_err(),
            ScenarioError::MissingField("seed")
        );

        let mut future = base.clone();
        future["schema_version"] = serde_json::json!(2);
        assert_eq!(
            Scenario::load(&future.to_string()).unwrap_err(),
            ScenarioError::UnsupportedSchemaVersion {
                found: 2,
                supported: SCENARIO_SCHEMA_VERSION,
            }
        );

        // The three are all different, which is the point of the criterion.
        let errors = [
            Scenario::load(&negative.to_string()).unwrap_err(),
            Scenario::load(&seedless.to_string()).unwrap_err(),
            Scenario::load(&future.to_string()).unwrap_err(),
        ];
        for i in 0..errors.len() {
            for j in (i + 1)..errors.len() {
                assert_ne!(errors[i], errors[j]);
            }
        }

        // Not JSON at all, and JSON that is not an object.
        assert!(matches!(
            Scenario::load("not json"),
            Err(ScenarioError::Parse(_))
        ));
        assert!(matches!(Scenario::load("[]"), Err(ScenarioError::Parse(_))));
    }
}

/// The six shipped configurations of brief §32 (task 9.2).
#[cfg(test)]
mod shipped {
    use super::*;
    use crate::simulation::Simulation;

    /// Every key in a JSON document, recursively.
    fn keys(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    out.push(k.clone());
                    keys(v, out);
                }
            }
            serde_json::Value::Array(items) => {
                for v in items {
                    keys(v, out);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn exactly_six_shipped() {
        // brief §32's list, and nothing else: "exactly the six".
        let mut names = shipped_names().to_vec();
        names.sort_unstable();
        assert_eq!(
            names,
            [
                "beam_reach_capsize",
                "close_hauled",
                "free_sail",
                "gybe",
                "sheet_release_recovery",
                "tack",
            ]
        );
        // The id in the table is the `name` inside the document.
        for (id, source) in SHIPPED {
            assert_eq!(Scenario::load(source).unwrap().name, id);
        }
    }

    #[test]
    fn all_load_validate_and_run_finite() {
        for (id, source) in SHIPPED {
            let sc = Scenario::load(source).unwrap_or_else(|e| panic!("{id}: {e}"));
            sc.validate().unwrap_or_else(|e| panic!("{id}: {e}"));

            let params = sc.to_parameters().unwrap();
            let mut sim = Simulation::new(params, sc.seed);
            sim.load_scenario(&sc)
                .unwrap_or_else(|e| panic!("{id}: {e}"));

            // 60 s under neutral controls — nothing steers, nothing sheets.
            let steps = (60.0 / params.sim.dt).round() as u32;
            for i in 0..steps {
                sim.advance(1);
                assert!(
                    sim.state().is_finite(),
                    "{id}: step {i} produced {:?}",
                    sim.state()
                );
            }
            assert!(
                (sim.state().t - 60.0).abs() < 1e-9,
                "{id}: t = {}",
                sim.state().t
            );
        }
    }

    #[test]
    fn recovery_matches_capsize_setup() {
        // brief §46 demonstrates capsize and recovery from the *same* setup,
        // differing only in what the human does. If the two documents differ
        // in any physical field the demonstration proves nothing.
        let capsize = load_shipped("beam_reach_capsize").unwrap();
        let recovery = load_shipped("sheet_release_recovery").unwrap();

        assert_eq!(capsize.seed, recovery.seed);
        assert_eq!(capsize.parameter_overrides, recovery.parameter_overrides);
        assert_eq!(capsize.initial_state, recovery.initial_state);
        assert_eq!(capsize.wind, recovery.wind);
        assert_eq!(capsize.initial_controls, recovery.initial_controls);
        assert_eq!(capsize.schema_version, recovery.schema_version);

        // Field by field, so a future field added to `InitialState` cannot be
        // left out of the comparison by accident.
        let a = serde_json::to_value(capsize.initial_state).unwrap();
        let b = serde_json::to_value(recovery.initial_state).unwrap();
        let object = a.as_object().unwrap();
        assert!(object.len() >= 7);
        for (field, value) in object {
            assert_eq!(Some(value), b.get(field), "initial_state.{field} differs");
        }

        // The resolved catalogues are identical too, bit for bit.
        assert_eq!(
            serde_json::to_string(&capsize.to_parameters().unwrap()).unwrap(),
            serde_json::to_string(&recovery.to_parameters().unwrap()).unwrap()
        );

        // …and the only things that *do* differ are the two labels.
        assert_ne!(capsize.name, recovery.name);
        assert_ne!(capsize.description, recovery.description);

        // The camera is view state, not physics, but the two demonstrations
        // should still look the same.
        assert_eq!(capsize.camera, recovery.camera);
    }

    #[test]
    fn free_sail_is_default() {
        assert_eq!(DEFAULT_SCENARIO, "free_sail");
        assert!(shipped_source(DEFAULT_SCENARIO).is_some());

        // …and the browser agrees. `web/src/sim/scenarioTypes.ts` mirrors the
        // constant; the physics crate must still test on its own, so a
        // missing web tree skips rather than failing.
        let ts = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../web/src/sim/scenarioTypes.ts");
        let Ok(src) = std::fs::read_to_string(&ts) else {
            eprintln!("skip: {} not present", ts.display());
            return;
        };
        let declared = src
            .split_once("DEFAULT_SCENARIO = '")
            .expect("scenarioTypes.ts must declare DEFAULT_SCENARIO")
            .1
            .split_once('\'')
            .expect("DEFAULT_SCENARIO must be a closed string literal")
            .0;
        assert_eq!(declared, DEFAULT_SCENARIO);
    }

    #[test]
    fn no_scripted_outcomes() {
        // brief §32: "These scenarios should not script outcomes."
        const FORBIDDEN: [&str; 5] = ["script", "sequence", "events", "timeline", "forced"];
        // The whole key vocabulary a scenario document may use. Asserting the
        // allowlist is strictly stronger than the substring scan below: a
        // scripted outcome cannot hide behind a key that merely fails to
        // contain one of the five words.
        const ALLOWED: [&str; 22] = [
            "schema_version",
            "name",
            "description",
            "seed",
            "parameter_overrides",
            "initial_state",
            "x",
            "y",
            "heading_deg",
            "heel_deg",
            "speed",
            "boom_deg_to_port",
            "sheet_length",
            "wind",
            "mode",
            "bearing_deg",
            "variation",
            "length_scale",
            "time_scale",
            "modes",
            "spectral_slope",
            "camera",
        ];

        for (id, source) in SHIPPED {
            let value: serde_json::Value = serde_json::from_str(source).unwrap();
            let mut found = Vec::new();
            keys(&value, &mut found);
            assert!(!found.is_empty(), "{id}: the key walk found nothing");
            for key in &found {
                assert!(
                    ALLOWED.contains(&key.as_str()) || key == "zoom" || key == "initial_controls",
                    "{id} carries an unexpected key `{key}`"
                );
                // `description` is the one schema key that contains one of
                // the five words as a substring (de-`script`-ion), and it is
                // prose about the initial condition, not a script.
                if key == "description" {
                    continue;
                }
                let lower = key.to_ascii_lowercase();
                for needle in FORBIDDEN {
                    assert!(
                        !lower.contains(needle),
                        "{id} carries a `{key}` key — a scenario is an initial \
                         condition and an environment, nothing more (brief §32)"
                    );
                }
            }
            // `initial_controls` is the one control-shaped key the schema
            // has, and brief §32 allows it explicitly as an *initial* value.
            assert!(found.iter().any(|k| k == "initial_controls"));
        }
    }
}
