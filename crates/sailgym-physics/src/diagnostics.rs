//! Current-state diagnostics. Section 08 extends this record for debugging.
//! Evaluated from the same state, controls, parameters and field as the EOM;
//! these describe the published snapshot, not the RK2 midpoint of the last step.

use crate::forces::evaluate;
use crate::simulation::Simulation;
use crate::vec::Vec3;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

#[derive(Clone, Debug, Serialize)]
pub struct Diagnostics {
    pub t: f64,
    pub steps: u64,
    #[serde(serialize_with = "serialize_vec3")]
    pub apparent_wind_body: Vec3,
    pub apparent_wind_speed: f64,
    /// FROM angle off the bow in radians, positive to starboard.
    pub apparent_wind_angle: f64,
    pub speed_over_ground: f64,
    pub alpha_sail: f64,
    pub cl_sail: f64,
    pub cd_sail: f64,
    /// N, mainsheet tension, `≥ 0` always (F6.8, brief §11).
    pub sheet_tension: f64,
    /// m, `ℓ(β)` — the geometric rope path length. The renderer draws the rope
    /// from this and `l_sheet`, so no rope geometry is re-derived in
    /// TypeScript (F8).
    pub rope_length: f64,
    /// m, `e = ℓ − L`. Negative when the rope is slack; the renderer's sag is
    /// `max(0, −e)`.
    pub sheet_extension: f64,
}

fn serialize_vec3<S: Serializer>(v: &Vec3, s: S) -> Result<S::Ok, S::Error> {
    let mut record = s.serialize_struct("Vec3", 3)?;
    record.serialize_field("x", &v.x)?;
    record.serialize_field("y", &v.y)?;
    record.serialize_field("z", &v.z)?;
    record.end()
}

pub fn diagnostics(sim: &Simulation) -> Diagnostics {
    let st = sim.state();
    let f = evaluate(st, sim.controls(), sim.params(), sim.wind(), st.t);
    let aw = f.aw_boat;
    Diagnostics {
        t: st.t,
        steps: sim.steps(),
        apparent_wind_body: aw,
        apparent_wind_speed: aw.length(),
        apparent_wind_angle: if aw.x == 0.0 && aw.y == 0.0 {
            0.0
        } else {
            aw.y.atan2(-aw.x)
        },
        speed_over_ground: st.u.hypot(st.v),
        alpha_sail: f.alpha_sail,
        cl_sail: f.cl_sail,
        cd_sail: f.cd_sail,
        sheet_tension: f.sheet_tension,
        rope_length: f.rope_length,
        sheet_extension: f.sheet_extension,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::wind::{WindConfig, WindMode};
    use crate::parameters::BoatParameters;

    #[test]
    fn wind_from_angle_and_serialization() {
        let mut sim = Simulation::new(BoatParameters::ilca7(), 0);
        for (bearing, angle) in [
            (90.0, 0.0),
            (180.0, std::f64::consts::FRAC_PI_2),
            (0.0, -std::f64::consts::FRAC_PI_2),
        ] {
            sim.set_wind(WindConfig {
                mode: WindMode::Uniform,
                speed: 5.0,
                bearing_deg: bearing,
                ..Default::default()
            });
            let d = diagnostics(&sim);
            assert!((d.apparent_wind_speed - 5.0).abs() < 1e-12);
            assert!((d.apparent_wind_angle - angle).abs() < 1e-12);
            let json = serde_json::to_value(&d).unwrap();
            assert_eq!(
                json["apparent_wind_body"]["x"].as_f64().unwrap(),
                d.apparent_wind_body.x
            );
        }
    }
}
