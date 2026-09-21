//! The `Sensor` trait and the tier-0 suite (v2 F14.3, F14.4, F14.8;
//! section 05 tasks 5.2 and 5.3).
//!
//! # A sensor list, not a mask over a fixed field set
//!
//! F14.3: the observation layout is **runtime data**. There is no `OBS_LEN` and
//! no `const OBS_FIELDS`, because a ray-casting sensor's width is configurable
//! and is therefore not a subset of anything fixed. The observation vector is
//! the ordered concatenation of the configured sensors' outputs, and its layout
//! is the concatenation of their [`FieldSpec`]s, recorded in the episode
//! header. `ObsMask` survives only as one flag per column: *is this one
//! privileged?*
//!
//! # `version()` is bumped on any change to what a sensor emits
//!
//! Including a field **reorder**, which changes no width and no name set and is
//! exactly the change a reviewer waves through. The observation digest depends
//! on the version, so a bump invalidates comparisons loudly rather than letting
//! two incomparable runs compare cleanly (RV29).
//!
//! # Metadata is per column, not per sensor
//!
//! F14.3 says the surviving `ObsMask` is "one flag per sensor". Per **column**
//! is what section 10's [`ObservationField`](sailgym_physics::recording::ObservationField)
//! already records, and it is strictly finer: `guidance` emits four sensed
//! columns and one derived from the true wind, and marking the whole sensor
//! privileged would either hide four honest columns or leak one dishonest one.
//! [`Sensor::privileged`] reports the sensor-level flag as *any column
//! privileged*, so nothing is lost.

pub mod guidance;
pub mod imu;
pub mod registry;
pub mod rig;
pub mod wind;

use serde::{Deserialize, Serialize};

use sailgym_physics::recording::ObservationField;
use sailgym_physics::rng::Pcg32;

use crate::worldview::WorldView;

pub use registry::{RegistryError, SensorRegistry};

/// One column of an observation, and everything an experiment identity needs
/// to say what it means (F16.4: "ordered fields, units, bounds, normalization,
/// sensor config/noise/privilege and versions").
///
/// `lo`/`hi` are `Option` rather than `±f64::INFINITY` on purpose: an
/// unbounded column is a *fact about the quantity*, and `serde_json` writes a
/// non-finite float as `null`, which would make the canonical record ambiguous
/// between "unbounded" and "not serialisable".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldSpec {
    /// The column's name, unique within its sensor.
    pub name: String,
    /// The F1 unit, spelled as F1 spells it: `rad`, `rad/s`, `m`, `m/s`,
    /// `m/s^2`, or `1` for a dimensionless column.
    pub unit: String,
    /// Declared lower bound, or `None` when the quantity has none.
    pub lo: Option<f64>,
    /// Declared upper bound, or `None` when the quantity has none.
    pub hi: Option<f64>,
    /// How the column is scaled before a policy sees it; `none` if it is not.
    pub normalisation: String,
    /// The noise standard deviation applied to this column, in its own unit.
    /// Zero everywhere in this section: no noise model is built (F14.8's
    /// substream scheme exists; the models are the sensor-quality ablation's).
    pub noise: f64,
    /// F14.3's surviving `ObsMask`: is this column privileged?
    pub privileged: bool,
}

impl FieldSpec {
    /// An ordinary sensed column: no normalisation, no noise, not privileged.
    pub fn sensed(name: &str, unit: &str, lo: Option<f64>, hi: Option<f64>) -> Self {
        Self {
            name: name.to_string(),
            unit: unit.to_string(),
            lo,
            hi,
            normalisation: "none".to_string(),
            noise: 0.0,
            privileged: false,
        }
    }

    /// The same, but derived from something the boat cannot measure (F14.4).
    pub fn mark_privileged(mut self) -> Self {
        self.privileged = true;
        self
    }

    /// The same, but scaled. `how` is the expression, written out, so a reader
    /// of an experiment log can undo it.
    pub fn normalised(mut self, how: &str) -> Self {
        self.normalisation = how.to_string();
        self
    }

    /// Section 10's record for this column, with the sensor's id and version
    /// folded into the name so the episode header is unambiguous about which
    /// sensor produced it.
    pub fn to_recorded(&self, sensor: &str, version: u32) -> ObservationField {
        ObservationField {
            name: format!("{sensor}.v{version}.{}", self.name),
            unit: self.unit.clone(),
            normalisation: self.normalisation.clone(),
            noise: self.noise,
            privileged: self.privileged,
        }
    }
}

/// One source of observation columns.
///
/// Object-safe: the registry is a `Vec<Box<dyn Sensor>>` and never a hash
/// container (F9.3).
///
/// # The contract
///
/// * [`Sensor::width`] is fixed for the episode and known at reset.
/// * `fields().len() == width()` and `field_names().len() == width()`. Both are
///   asserted for every registered sensor, which is why `width` is declared
///   rather than derived: a derived width would make the assertion vacuous.
/// * [`Sensor::sense`] writes **exactly** `width()` scalars into `out`, reads
///   no wall clock, and takes all its randomness from the `Pcg32` it is handed
///   — which is a per-sensor substream of `STREAM_AGENT` (F14.8).
/// * `sense` is pure in `(self, view, rng)`. It may not read
///   `Simulation::forces`; a sensor that needs accelerations calls
///   `forces::evaluate` itself, at the decision instant (F14.7, trap 1).
pub trait Sensor {
    /// The stable id. Also keys the sensor's noise substream (F14.8), so
    /// renaming a sensor changes its noise — which is correct: a renamed
    /// sensor is a different sensor.
    fn id(&self) -> &'static str;

    /// Bumped on **any** change to what this sensor emits, a field reorder
    /// included (RV29).
    fn version(&self) -> u32;

    /// How many scalars [`Sensor::sense`] writes. Fixed for the episode.
    fn width(&self) -> usize;

    /// The per-column metadata, in emission order.
    fn fields(&self) -> Vec<FieldSpec>;

    /// The column names, in emission order.
    fn field_names(&self) -> Vec<String> {
        self.fields().into_iter().map(|f| f.name).collect()
    }

    /// Whether **any** column of this sensor is privileged — F14.3's
    /// sensor-level flag, derived rather than declared twice.
    fn privileged(&self) -> bool {
        self.fields().iter().any(|f| f.privileged)
    }

    /// Write this sensor's `width()` scalars into `out`.
    fn sense(&mut self, view: &WorldView, rng: &mut Pcg32, out: &mut [f64]);
}

/// A sensor's own RNG substream, keyed by its id (F14.8).
///
/// Keying by **id** rather than by position in the registry is the whole
/// requirement: with a positional key, adding a sensor to one ablation arm
/// would shift every later sensor's noise in that arm, and the arm would differ
/// from its sibling for a reason that is not the variable under study.
///
/// [`fnv1a64`] is a stream-label derivation, not a digest and not
/// cryptography: nothing here is compared, committed or relied on for
/// integrity, so F16.4's "use an established SHA-256, not handwritten
/// cryptography" is not in play. It is pinned by a known-answer test all the
/// same, because changing it would silently change every noisy sensor's draws.
pub fn sensor_stream(agent_rng: &Pcg32, id: &str) -> Pcg32 {
    agent_rng.stream(fnv1a64(id))
}

/// FNV-1a, 64-bit, from the reference specification.
pub fn fnv1a64(s: &str) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = OFFSET_BASIS;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(PRIME);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the label derivation against the published FNV-1a 64 test vectors.
    ///
    /// If this fails the change is the defect: every noisy sensor's draws move
    /// with it.
    #[test]
    fn fnv1a64_known_vectors() {
        assert_eq!(fnv1a64(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64("foobar"), 0x8594_4171_f739_67e8);
    }

    /// The five tier-0 ids get five distinct substreams, and deriving one does
    /// not disturb another.
    #[test]
    fn sensor_substreams_are_distinct_and_order_independent() {
        let root = Pcg32::seed_from_u64(11);
        let agent = crate::spec::agent_rng(&root);
        let ids = [
            "imu",
            "apparent_wind",
            "rig_state",
            "actuator_state",
            "guidance",
        ];

        let draw = |id: &str| -> Vec<u32> {
            let mut s = sensor_stream(&agent, id);
            (0..8).map(|_| s.next_u32()).collect()
        };

        let mut seen: Vec<(u64, &str)> = ids.iter().map(|id| (fnv1a64(id), *id)).collect();
        seen.sort();
        for pair in seen.windows(2) {
            assert_ne!(pair[0].0, pair[1].0, "label collision: {pair:?}");
        }

        let first: Vec<Vec<u32>> = ids.iter().map(|id| draw(id)).collect();
        // Deriving in the reverse order gives the same draws: `Pcg32::stream`
        // takes `&self` and never advances the parent.
        let again: Vec<Vec<u32>> = ids.iter().rev().map(|id| draw(id)).collect();
        for (k, id) in ids.iter().enumerate() {
            assert_eq!(first[k], again[ids.len() - 1 - k], "{id} moved");
        }
        for pair in first.windows(2) {
            assert_ne!(pair[0], pair[1]);
        }
    }

    #[test]
    fn a_field_spec_carries_the_whole_identity_record() {
        let f = FieldSpec::sensed(
            "awa",
            "rad",
            Some(-std::f64::consts::PI),
            Some(std::f64::consts::PI),
        );
        assert_eq!(f.normalisation, "none");
        assert_eq!(f.noise, 0.0);
        assert!(!f.privileged);
        let r = f.to_recorded("apparent_wind", 3);
        assert_eq!(r.name, "apparent_wind.v3.awa");
        assert_eq!(r.unit, "rad");
        assert!(!r.privileged);

        let g = FieldSpec::sensed("leg_bearing_vs_wind", "rad", None, None).mark_privileged();
        assert!(g.privileged);
        assert!(g.to_recorded("guidance", 1).privileged);

        let n = FieldSpec::sensed("sheet_slack", "1", Some(-1.0), Some(1.0))
            .normalised("(l_sheet - rope_path) / (l_sheet_max - l_sheet_min)");
        assert!(n.normalisation.contains("l_sheet_max"));
    }
}
