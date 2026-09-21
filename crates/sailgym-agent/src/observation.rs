//! The observation: the ordered concatenation, its layout, and `obs_digest`
//! (v2 F14.3, F14.7, F16.4; section 05 task 5.4).
//!
//! # The layout is runtime data
//!
//! There is no `OBS_LEN` and no `const OBS_FIELDS`. [`ObsLayout::names`]
//! returns a `Vec<String>` built from the configured sensors, because a
//! ray-casting sensor's width is configurable and is therefore not a subset of
//! any fixed field set (F14.3). Adding a sensor appends columns and perturbs
//! none of the ones before it, which is asserted rather than assumed.
//!
//! # `observe` is pure in its arguments
//!
//! F14.7 writes the signature as `observe(st, p, wind, guidance, sensors)`.
//! Here the first four travel together in a [`WorldView`], which is the same
//! set of arguments with the containment boundary of F14.4 given a name. What
//! matters is what is **not** in the signature: no `Simulation`, no
//! `ForceBreakdown`, no `Diagnostics`. The cached breakdown is refreshed once
//! per `advance(n)` call, so an observation built on it would depend on how the
//! caller chunked its calls and F9.7 would break where nobody tests (trap 1).
//!
//! # Randomness
//!
//! Each sensor draws from **its own** substream, keyed by its id (F14.8), and
//! the streams are built once per episode and carried across decisions by the
//! caller. Rebuilding them inside `observe` would hand every decision the same
//! draws, which is a noise model that does not move — the most plausible way to
//! ship a broken one and not notice.
//!
//! # `obs_digest` is the canonical record, not a hash
//!
//! F16.4 says observation identity is "ordered fields, units, bounds,
//! normalization, sensor config/noise/privilege and versions", that
//! canonical-record equality is sufficient for comparison, and that a compact
//! key is optional but must use an established SHA-256 rather than handwritten
//! cryptography. Section 10 made the same choice for
//! [`ExperimentIdentity`](sailgym_physics::recording::ExperimentIdentity) and
//! ships no digest at all. So [`obs_digest`] returns the **canonical text** of
//! the layout: stable across runs, different whenever any sensor's version,
//! width or order differs, and carrying no second hash implementation into a
//! crate that ships. The one SHA-256 in this repository stays where section 02
//! put it, behind the `testkit` feature, out of the browser build.

use serde::{Deserialize, Serialize};

use sailgym_physics::recording::ObservationIdentity;
use sailgym_physics::rng::Pcg32;

use crate::sensor::{sensor_stream, FieldSpec, Sensor};
use crate::worldview::WorldView;

/// The version of the layout **record** — its field set and its serialised
/// shape, not its contents.
///
/// Bumped by hand when a reader written against the previous shape would
/// misread a layout. A sensor changing what it emits bumps the *sensor's*
/// version, which is a different fact.
pub const LAYOUT_VERSION: u32 = 1;

/// One column of the observation vector, with everything needed to say what it
/// means (F16.4).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObsColumn {
    /// The id of the sensor that produced it.
    pub sensor: String,
    /// That sensor's version. A bump here changes the digest, loudly (RV29).
    pub sensor_version: u32,
    /// Which of that sensor's `width()` scalars this is.
    pub index_in_sensor: usize,
    /// Name, unit, bounds, normalisation, noise and privilege.
    pub field: FieldSpec,
}

impl ObsColumn {
    /// The column's fully qualified name: `<sensor>.<field>`.
    pub fn name(&self) -> String {
        format!("{}.{}", self.sensor, self.field.name)
    }
}

/// The observation layout an episode was configured with (F14.3).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObsLayout {
    pub layout_version: u32,
    pub columns: Vec<ObsColumn>,
}

impl ObsLayout {
    /// Build the layout of a configured suite, in the order given.
    pub fn of(sensors: &[Box<dyn Sensor>]) -> Self {
        let mut columns = Vec::new();
        for s in sensors {
            let fields = s.fields();
            assert_eq!(
                fields.len(),
                s.width(),
                "sensor `{}` declares width {} and {} fields",
                s.id(),
                s.width(),
                fields.len()
            );
            for (index_in_sensor, field) in fields.into_iter().enumerate() {
                columns.push(ObsColumn {
                    sensor: s.id().to_string(),
                    sensor_version: s.version(),
                    index_in_sensor,
                    field,
                });
            }
        }
        Self {
            layout_version: LAYOUT_VERSION,
            columns,
        }
    }

    /// How many scalars an observation carries.
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// The column names, in order. **A `Vec<String>`, not a constant** — the
    /// layout is data (F14.3).
    pub fn names(&self) -> Vec<String> {
        self.columns.iter().map(ObsColumn::name).collect()
    }

    /// The indices of the privileged columns, in order.
    ///
    /// F14.3's surviving `ObsMask`. An arm that must not see privileged
    /// information filters on this rather than on a remembered rule.
    pub fn privileged_columns(&self) -> Vec<usize> {
        self.columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.field.privileged)
            .map(|(i, _)| i)
            .collect()
    }

    /// Section 10's record for the episode header.
    ///
    /// Section 10's [`ObservationField`](sailgym_physics::recording::ObservationField)
    /// carries no bounds, and this crate may not add one to it (this section
    /// changes exactly one file in `sailgym-physics`, and it is `rng.rs`). So
    /// the header carries name, unit, normalisation, noise and privilege, and
    /// the **bounds live in the full record** that F16.4 requires to travel
    /// beside any digest: [`ObsLayout`] itself, serialised by [`obs_digest`].
    pub fn to_identity(&self) -> ObservationIdentity {
        ObservationIdentity {
            layout_version: self.layout_version,
            fields: self
                .columns
                .iter()
                .map(|c| c.field.to_recorded(&c.sensor, c.sensor_version))
                .collect(),
        }
    }

    /// The canonical text form: `serde_json` over a record whose fields are
    /// declared in one order, so equal layouts produce equal text on every
    /// platform. The same device
    /// [`ExperimentIdentity::canonical_json`](sailgym_physics::recording::ExperimentIdentity::canonical_json)
    /// uses.
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// The observation identity, as a comparable value (F16.4).
///
/// See the module documentation: this is the canonical record, not a hash.
/// Comparing two of these for equality is the comparison authority; a compact
/// key would be a convenience and would need a SHA-256 this crate does not
/// carry.
pub fn obs_digest(layout: &ObsLayout) -> String {
    layout.canonical_json()
}

/// The per-sensor RNG substreams for an episode (F14.8).
///
/// Built once, at reset, and carried across decisions by the caller. Keyed by
/// sensor id, so adding a sensor to one ablation arm cannot perturb another
/// sensor's noise in the same arm.
pub fn sensor_streams(agent_rng: &Pcg32, sensors: &[Box<dyn Sensor>]) -> Vec<Pcg32> {
    sensors
        .iter()
        .map(|s| sensor_stream(agent_rng, s.id()))
        .collect()
}

/// Fill `out` with the ordered concatenation of the sensors' outputs.
///
/// `out` is resized to the total width, so a caller may reuse one buffer across
/// an episode and allocate nothing after the first decision.
///
/// # Panics
///
/// If `streams.len() != sensors.len()`. The streams are built from the sensors
/// by [`sensor_streams`]; a mismatch means one list was rebuilt and the other
/// was not, which would silently give a sensor another sensor's noise.
pub fn observe(
    sensors: &mut [Box<dyn Sensor>],
    streams: &mut [Pcg32],
    view: &WorldView,
    out: &mut Vec<f64>,
) {
    assert_eq!(
        sensors.len(),
        streams.len(),
        "one RNG substream per sensor (F14.8)"
    );
    let total: usize = sensors.iter().map(|s| s.width()).sum();
    out.clear();
    out.resize(total, 0.0);
    let mut at = 0usize;
    for (s, rng) in sensors.iter_mut().zip(streams.iter_mut()) {
        let w = s.width();
        s.sense(view, rng, &mut out[at..at + w]);
        at += w;
    }
    debug_assert_eq!(at, total);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensor::SensorRegistry;
    use crate::spec::agent_rng;
    use sailgym_physics::parameters::BoatParameters;
    use sailgym_physics::recording::IDENTITY_VERSION;
    use sailgym_physics::state::{BoatState, Controls};
    use sailgym_physics::testkit::uniform_wind;

    fn suite(ids: &[&str]) -> Vec<Box<dyn Sensor>> {
        SensorRegistry::tier0().resolve(ids).expect("registered")
    }

    const TIER0: [&str; 5] = [
        "imu",
        "apparent_wind",
        "rig_state",
        "actuator_state",
        "guidance",
    ];

    #[test]
    fn the_layout_length_is_the_sum_of_the_widths() {
        let sensors = suite(&TIER0);
        let layout = ObsLayout::of(&sensors);
        let expect: usize = sensors.iter().map(|s| s.width()).sum();
        assert_eq!(layout.len(), expect);
        assert_eq!(layout.len(), 5 + 2 + 4 + 2 + 5);
        assert!(!layout.is_empty());
        assert_eq!(layout.names().len(), layout.len());
        // The layout is a Vec<String>, not a constant (F14.3).
        let names: Vec<String> = layout.names();
        assert_eq!(names[0], "imu.roll_rate");
        assert_eq!(names[5], "apparent_wind.awa");
        assert_eq!(
            names.last().expect("a last column"),
            "guidance.rounding_side"
        );
    }

    #[test]
    fn the_observation_is_the_ordered_concatenation() {
        let p = BoatParameters::ilca7();
        let c = Controls {
            rudder_rate_cmd: 0.4,
            ..Controls::default()
        };
        let st = BoatState {
            u: 2.0,
            beta: 0.3,
            delta_r: -0.1,
            l_sheet: 2.0,
            ..BoatState::ZERO
        };
        let air = uniform_wind(5.0, 45.0);
        let view = WorldView {
            st: &st,
            controls: &c,
            p: &p,
            wind: &air,
            guidance: None,
            others: &[],
            t: 0.0,
        };

        let mut sensors = suite(&TIER0);
        let layout = ObsLayout::of(&sensors);
        let root = Pcg32::seed_from_u64(9);
        let mut streams = sensor_streams(&agent_rng(&root), &sensors);
        let mut obs = Vec::new();
        observe(&mut sensors, &mut streams, &view, &mut obs);
        assert_eq!(obs.len(), layout.len());

        // Each block is what that sensor alone would have produced.
        let mut at = 0usize;
        for id in TIER0 {
            let mut one = suite(&[id]);
            let mut s1 = sensor_streams(&agent_rng(&root), &one);
            let mut block = Vec::new();
            observe(&mut one, &mut s1, &view, &mut block);
            assert_eq!(
                obs[at..at + block.len()].to_vec(),
                block,
                "the {id} block does not match"
            );
            at += block.len();
        }
        assert_eq!(at, obs.len());
    }

    /// The acceptance criterion, in four parts.
    #[test]
    fn the_digest_is_stable_and_moves_with_version_width_and_order() {
        let a = ObsLayout::of(&suite(&TIER0));
        let b = ObsLayout::of(&suite(&TIER0));
        assert_eq!(obs_digest(&a), obs_digest(&b), "the digest must be stable");
        assert_eq!(
            obs_digest(&a),
            obs_digest(&ObsLayout::of(&suite(&TIER0))),
            "and stable across a third build"
        );

        // Order: the same sensors in a different order give a different digest.
        let reordered = ObsLayout::of(&suite(&[
            "apparent_wind",
            "imu",
            "rig_state",
            "actuator_state",
            "guidance",
        ]));
        assert_ne!(obs_digest(&a), obs_digest(&reordered));

        // Width: dropping a sensor changes it.
        let narrower = ObsLayout::of(&suite(&["imu", "apparent_wind"]));
        assert_ne!(obs_digest(&a), obs_digest(&narrower));

        // Version: bumping one sensor's version changes it, and **only** the
        // version differs between the two records.
        let mut bumped = a.clone();
        bumped.columns[0].sensor_version += 1;
        assert_ne!(obs_digest(&a), obs_digest(&bumped), "RV29");

        // A field reorder inside one sensor changes it, which is the case a
        // reviewer waves through (RV29).
        let mut swapped = a.clone();
        swapped.columns.swap(0, 1);
        assert_ne!(obs_digest(&a), obs_digest(&swapped));

        // The record round-trips, so a stored digest can be read back and
        // compared field by field rather than only as text.
        let back: ObsLayout = serde_json::from_str(&obs_digest(&a)).expect("canonical JSON");
        assert_eq!(back, a);
    }

    /// Adding a sensor appends columns and perturbs none of the ones before it
    /// — the property that makes an observation ablation affordable.
    #[test]
    fn adding_a_sensor_does_not_perturb_the_preceding_columns() {
        let before = ObsLayout::of(&suite(&["imu", "apparent_wind"]));
        let after = ObsLayout::of(&suite(&["imu", "apparent_wind", "guidance"]));
        assert_eq!(after.len(), before.len() + 5);
        for k in 0..before.len() {
            assert_eq!(before.columns[k], after.columns[k], "column {k} moved");
        }
        // The prefix of the canonical text is shared, which is the same
        // statement about the record rather than about the columns.
        let (bt, at) = (before.canonical_json(), after.canonical_json());
        let shared = bt
            .chars()
            .zip(at.chars())
            .take_while(|(x, y)| x == y)
            .count();
        assert!(shared > bt.len() / 2, "the records diverge early: {shared}");
    }

    #[test]
    fn the_identity_record_carries_every_column_and_its_privilege() {
        let layout = ObsLayout::of(&suite(&TIER0));
        let id = layout.to_identity();
        assert_eq!(id.layout_version, LAYOUT_VERSION);
        assert_eq!(id.fields.len(), layout.len());
        // Pinned: if section 10's identity record changes version, what
        // `ObservationIdentity` carries has probably changed with it, and this
        // mapping wants re-reading rather than silently still compiling.
        assert_eq!(IDENTITY_VERSION, 1);
        let privileged: Vec<&str> = id
            .fields
            .iter()
            .filter(|f| f.privileged)
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(privileged, vec!["guidance.v1.leg_bearing_vs_wind"]);
        assert_eq!(layout.privileged_columns(), vec![layout.len() - 2]);
        // The sensor's version travels in the recorded name, so two runs with
        // different sensor versions cannot compare cleanly (RV29).
        assert!(id.fields[0].name.starts_with("imu.v1."));
    }

    #[test]
    fn one_substream_per_sensor_or_it_panics() {
        let sensors = suite(&TIER0);
        let root = Pcg32::seed_from_u64(3);
        let streams = sensor_streams(&agent_rng(&root), &sensors);
        assert_eq!(streams.len(), sensors.len());

        let p = BoatParameters::ilca7();
        let c = Controls::default();
        let air = uniform_wind(4.0, 0.0);
        let st = BoatState::ZERO;
        let view = WorldView {
            st: &st,
            controls: &c,
            p: &p,
            wind: &air,
            guidance: None,
            others: &[],
            t: 0.0,
        };
        let mut sensors = sensors;
        let mut short = vec![Pcg32::seed_from_u64(1)];
        let mut out = Vec::new();
        let boom = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            observe(&mut sensors, &mut short, &view, &mut out);
        }));
        assert!(boom.is_err(), "a mismatched stream list must not pass");
    }
}
