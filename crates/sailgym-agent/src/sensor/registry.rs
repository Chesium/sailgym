//! The sensor registry (section 05 task 5.3).
//!
//! An **ordered `Vec`**, resolved by id, never a hash container (F9.3). Two
//! reasons, and the second is the one that bites: a hash container's iteration
//! order is not reproducible, and [`SensorRegistry::ids`] is what a UI lists
//! and what a study writes into its configuration. An order that moved between
//! runs would make two configurations that read identically produce different
//! observation layouts.

use crate::sensor::guidance::GuidanceSensor;
use crate::sensor::imu::Imu;
use crate::sensor::rig::{ActuatorState, RigState};
use crate::sensor::wind::ApparentWind;
use crate::sensor::Sensor;

/// One registered sensor: its id and how to build a fresh one.
struct Entry {
    id: &'static str,
    make: fn() -> Box<dyn Sensor>,
}

/// The catalogue of sensors an episode may configure.
pub struct SensorRegistry {
    entries: Vec<Entry>,
}

impl SensorRegistry {
    /// The tier-0 suite: what a real dinghy measures, plus the task.
    ///
    /// The order here is the catalogue's order, **not** an observation layout:
    /// a layout is whatever [`SensorRegistry::resolve`] was asked for, in the
    /// order it was asked for (F14.3).
    ///
    /// `true_wind`, `speed_log`, `wind_probe`, `lidar2d`, `polar_prior` and
    /// `layline_prior` are deliberately absent — see `crate::sensor::wind` for
    /// the `true_wind` reasoning and the PRD's debt table for the rest.
    pub fn tier0() -> Self {
        Self {
            entries: vec![
                Entry {
                    id: Imu::ID,
                    make: || Box::new(Imu),
                },
                Entry {
                    id: ApparentWind::ID,
                    make: || Box::new(ApparentWind::new()),
                },
                Entry {
                    id: RigState::ID,
                    make: || Box::new(RigState),
                },
                Entry {
                    id: ActuatorState::ID,
                    make: || Box::new(ActuatorState),
                },
                Entry {
                    id: GuidanceSensor::ID,
                    make: || Box::new(GuidanceSensor),
                },
            ],
        }
    }

    /// The registered ids, in catalogue order.
    pub fn ids(&self) -> Vec<&'static str> {
        self.entries.iter().map(|e| e.id).collect()
    }

    /// Build one sensor by id.
    pub fn make(&self, id: &str) -> Result<Box<dyn Sensor>, RegistryError> {
        self.entries
            .iter()
            .find(|e| e.id == id)
            .map(|e| (e.make)())
            .ok_or_else(|| RegistryError::Unknown(id.to_string()))
    }

    /// Build a configured suite, in the order given.
    ///
    /// A repeated id is **refused**: two copies of one sensor would emit two
    /// identical column names and share one noise substream (F14.8), so the
    /// layout would be ambiguous and the noise correlated. Neither is a thing
    /// anyone means.
    pub fn resolve(&self, ids: &[&str]) -> Result<Vec<Box<dyn Sensor>>, RegistryError> {
        let mut seen: Vec<&str> = Vec::new();
        let mut out: Vec<Box<dyn Sensor>> = Vec::new();
        for id in ids {
            if seen.contains(id) {
                return Err(RegistryError::Duplicate((*id).to_string()));
            }
            seen.push(id);
            out.push(self.make(id)?);
        }
        Ok(out)
    }
}

impl Default for SensorRegistry {
    fn default() -> Self {
        Self::tier0()
    }
}

/// What [`SensorRegistry::resolve`] refuses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryError {
    Unknown(String),
    Duplicate(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown(id) => write!(f, "no sensor is registered under the id `{id}`"),
            Self::Duplicate(id) => write!(
                f,
                "the sensor `{id}` is configured twice: the layout would carry two identical \
                 column names and one noise substream (F14.3, F14.8)"
            ),
        }
    }
}

impl std::error::Error for RegistryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tier0_suite_is_the_five_sensors_the_prd_names() {
        let r = SensorRegistry::tier0();
        assert_eq!(
            r.ids(),
            vec![
                "imu",
                "apparent_wind",
                "rig_state",
                "actuator_state",
                "guidance"
            ]
        );
        // RV30: `true_wind` is not in the suite, and its absence is asserted
        // rather than left to be noticed.
        assert!(!r.ids().contains(&"true_wind"));
        assert_eq!(
            r.make("true_wind").err(),
            Some(RegistryError::Unknown("true_wind".to_string()))
        );
    }

    /// The PRD's acceptance criterion, for **every** registered sensor.
    #[test]
    fn every_registered_sensor_declares_a_consistent_layout() {
        let r = SensorRegistry::tier0();
        let expect: Vec<(&str, usize)> = vec![
            ("imu", 5),
            ("apparent_wind", 2),
            ("rig_state", 4),
            ("actuator_state", 2),
            ("guidance", 5),
        ];
        for (id, width) in expect {
            let s = r.make(id).expect("registered");
            assert_eq!(s.id(), id);
            assert_eq!(s.width(), width, "{id}: the PRD's table says {width}");
            assert_eq!(s.width(), s.fields().len(), "{id}: width vs fields");
            assert_eq!(s.width(), s.field_names().len(), "{id}: width vs names");
            assert!(s.version() >= 1, "{id}: an unversioned sensor");
            // Column names are unique within a sensor, or the layout cannot
            // name a column.
            let mut names = s.field_names();
            names.sort();
            let before = names.len();
            names.dedup();
            assert_eq!(names.len(), before, "{id}: duplicate column name");
            // Every column declares a unit and a normalisation.
            for f in s.fields() {
                assert!(!f.unit.trim().is_empty(), "{id}.{}: no unit", f.name);
                assert!(
                    !f.normalisation.trim().is_empty(),
                    "{id}.{}: no normalisation",
                    f.name
                );
                assert_eq!(f.noise, 0.0, "{id}.{}: no noise model ships here", f.name);
            }
        }
    }

    /// RV31, at the registry level: no sensor emits an absolute coordinate or
    /// an absolute heading under any name.
    #[test]
    fn no_registered_sensor_emits_an_absolute_pose_column() {
        let r = SensorRegistry::tier0();
        for id in r.ids() {
            let s = r.make(id).expect("registered");
            for name in s.field_names() {
                for forbidden in [
                    "x",
                    "y",
                    "psi",
                    "heading",
                    "position",
                    "latitude",
                    "longitude",
                    "north",
                    "east",
                ] {
                    assert_ne!(name, forbidden, "{id} emits the absolute column `{name}`");
                }
            }
        }
    }

    /// Exactly one column in the whole suite is privileged, and the registry
    /// is where that is easiest to check.
    #[test]
    fn the_suite_has_one_privileged_column() {
        let r = SensorRegistry::tier0();
        let mut privileged: Vec<String> = Vec::new();
        for id in r.ids() {
            let s = r.make(id).expect("registered");
            for f in s.fields() {
                if f.privileged {
                    privileged.push(format!("{id}.{}", f.name));
                }
            }
        }
        assert_eq!(privileged, vec!["guidance.leg_bearing_vs_wind".to_string()]);
    }

    #[test]
    fn resolve_keeps_the_order_asked_for_and_refuses_a_repeat() {
        let r = SensorRegistry::tier0();
        let suite = r
            .resolve(&["guidance", "imu"])
            .expect("both are registered");
        assert_eq!(
            suite.iter().map(|s| s.id()).collect::<Vec<_>>(),
            vec!["guidance", "imu"],
            "resolve must not re-impose the catalogue order"
        );
        assert_eq!(
            r.resolve(&["imu", "imu"]).err(),
            Some(RegistryError::Duplicate("imu".to_string()))
        );
        assert_eq!(
            r.resolve(&["imu", "nope"]).err(),
            Some(RegistryError::Unknown("nope".to_string()))
        );
    }

    /// F9.3: the registry is a `Vec`, asserted against this file's own source
    /// rather than left to review.
    #[test]
    fn the_registry_is_not_a_hash_container() {
        let src = include_str!("registry.rs");
        let code: String = src
            .lines()
            .take_while(|l| l.trim() != "#[cfg(test)]")
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in ["HashMap", "HashSet", "BTreeMap"] {
            assert!(
                !code.contains(forbidden),
                "the registry uses a `{forbidden}`; F9.3 requires an ordered Vec"
            );
        }
    }
}
