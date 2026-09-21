//! `imu` — what an inertial unit on the hull measures (section 05 task 5.3).
//!
//! Five columns: `p`, `r`, `φ`, and the body accelerations `ax`, `ay`.
//!
//! **No velocity, no position, and no absolute heading.** That is the sensor,
//! not a restriction bolted onto it: an IMU integrates rates, and a policy
//! given only this one has to estimate its own speed, under a real
//! partial-observability problem. A magnetometer column would be a different
//! sensor with a different id and a different version. `psi` never appears here
//! (RV31).
//!
//! # Trap 1, and why this sensor costs a force evaluation
//!
//! Body accelerations are **derived**, never integrated (brief §5).
//! `Diagnostics::acceleration_body` carries them — and it is computed from
//! `Simulation::forces`, the breakdown refreshed **once per `advance(n)`
//! call**. Reading it from inside the step loop would give an acceleration up
//! to `n` steps stale, and the observation would then depend on how the caller
//! chunked its calls: F9.7 would break in the one place nobody tests.
//!
//! So this sensor calls [`forces::evaluate`] itself, at the decision instant,
//! and hands the result to the **same** `dynamics::derivative` the equations of
//! motion use (F14.7). No acceleration is recomputed here; `Frozen` is the same
//! device `diagnostics.rs` uses for the same reason.
//!
//! At 20 Hz decisions over 200 Hz physics that is one extra evaluation per ten
//! steps, about 5 % on top of RK2's two per step.

use sailgym_physics::dynamics::{derivative, ForceModel, Generalized};
use sailgym_physics::forces::{evaluate, ForceBreakdown};
use sailgym_physics::parameters::BoatParameters;
use sailgym_physics::rng::Pcg32;
use sailgym_physics::state::{BoatState, Controls};

use super::{FieldSpec, Sensor};
use crate::worldview::WorldView;

/// A [`ForceModel`] that reports one already-computed breakdown.
///
/// The accelerations in the observation are therefore the accelerations of the
/// published state, through the one implementation of F4.1 and F4.2, rather
/// than a second arithmetic path that could drift.
struct Frozen(ForceBreakdown);

impl ForceModel for Frozen {
    fn generalized(&self, _: &BoatState, _: &Controls, _: &BoatParameters, _: f64) -> Generalized {
        self.0.total
    }
    fn boom_moment(&self, _: &BoatState, _: &Controls, _: &BoatParameters, _: f64) -> f64 {
        self.0.m_beta
    }
}

/// Rates, heel and body accelerations.
#[derive(Clone, Copy, Debug, Default)]
pub struct Imu;

impl Imu {
    pub const ID: &'static str = "imu";
    /// Bumped on any change to what this sensor emits, a reorder included.
    pub const VERSION: u32 = 1;
    pub const WIDTH: usize = 5;
}

impl Sensor for Imu {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn version(&self) -> u32 {
        Self::VERSION
    }

    fn width(&self) -> usize {
        Self::WIDTH
    }

    fn fields(&self) -> Vec<FieldSpec> {
        vec![
            // `p` and `r` are F3 quantities in `H`, signed per F2: `p > 0` is
            // rolling toward starboard, `r > 0` is turning to port.
            FieldSpec::sensed("roll_rate", "rad/s", None, None),
            FieldSpec::sensed("yaw_rate", "rad/s", None, None),
            // `phi` is deliberately not wrapped (F3), so it has no bound: the
            // boat passes dynamically through |φ| > 90°.
            FieldSpec::sensed("heel", "rad", None, None),
            FieldSpec::sensed("accel_surge", "m/s^2", None, None),
            FieldSpec::sensed("accel_sway", "m/s^2", None, None),
        ]
    }

    fn sense(&mut self, view: &WorldView, _rng: &mut Pcg32, out: &mut [f64]) {
        assert_eq!(
            out.len(),
            Self::WIDTH,
            "imu writes exactly {} scalars",
            Self::WIDTH
        );
        // A **fresh** evaluation, never `Simulation::forces` (F14.7, trap 1).
        let f = evaluate(view.st, view.controls, view.p, view.wind, view.t);
        let dot = derivative(view.st, view.controls, view.p, &Frozen(f), view.t);
        out[0] = view.st.p;
        out[1] = view.st.r;
        out[2] = view.st.phi;
        out[3] = dot.u;
        out[4] = dot.v;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sailgym_physics::simulation::Simulation;
    use sailgym_physics::testkit::{mirror_controls, mirror_state, uniform_wind};

    fn read(
        st: &BoatState,
        c: &Controls,
        p: &BoatParameters,
        wind: &dyn sailgym_physics::environment::WindField,
    ) -> [f64; Imu::WIDTH] {
        let view = WorldView {
            st,
            controls: c,
            p,
            wind,
            guidance: None,
            others: &[],
            t: st.t,
        };
        let mut rng = Pcg32::seed_from_u64(1);
        let mut out = [0.0; Imu::WIDTH];
        let mut imu = Imu;
        imu.sense(&view, &mut rng, &mut out);
        out
    }

    #[test]
    fn width_matches_the_declared_layout() {
        assert_eq!(Imu.width(), Imu.fields().len());
        assert_eq!(Imu.width(), Imu.field_names().len());
        assert!(!Imu.privileged());
    }

    /// RV31: the emitted layout contains no absolute coordinate and no
    /// absolute heading, by name **and** by value.
    #[test]
    fn the_imu_emits_no_position_and_no_absolute_heading() {
        for name in Imu.field_names() {
            for forbidden in ["psi", "heading", "position", "north", "east", "x", "y"] {
                assert_ne!(name, forbidden, "imu emits `{name}`");
            }
        }

        // The value test is the one that matters: translating and rotating the
        // boat leaves every column bit-identical, so no column can be carrying
        // absolute pose however it is named.
        let p = BoatParameters::ilca7();
        let wind = uniform_wind(6.0, 45.0);
        let c = Controls {
            rudder_rate_cmd: 0.3,
            sheet_rate_cmd: -0.5,
            sheet_release: false,
        };
        let base = BoatState {
            u: 2.4,
            v: -0.3,
            r: 0.15,
            p: -0.08,
            phi: 0.35,
            beta: -0.4,
            beta_dot: 0.05,
            delta_r: 0.12,
            l_sheet: 2.0,
            ..BoatState::ZERO
        };
        let here = read(&base, &c, &p, &wind);

        // Translation: the field is uniform, so the same boat somewhere else
        // reads the same.
        let moved = BoatState {
            x: 500.0,
            y: -820.0,
            ..base
        };
        assert_eq!(
            read(&moved, &c, &p, &wind),
            here,
            "the imu moved with the boat"
        );

        // Rotation: a uniform wind rotated with the boat gives the same
        // reading, which is what "no absolute heading" means numerically.
        let turned = BoatState { psi: 1.1, ..base };
        // Rotating the boat by +psi rotates the field's FROM bearing by -psi
        // (F6.1: bearing is CW from north, psi is CCW from world +x).
        let turned_wind = uniform_wind(6.0, 45.0 - 1.1_f64.to_degrees());
        let after = read(&turned, &c, &p, &turned_wind);
        for k in 0..Imu::WIDTH {
            assert!(
                (after[k] - here[k]).abs() < 1e-9,
                "column {k}: {} vs {} — the imu reads the boat's heading",
                after[k],
                here[k]
            );
        }
    }

    /// Port/starboard symmetry (F5.2, R3): mirroring the boat negates the
    /// signed columns and leaves the rest alone.
    #[test]
    fn the_imu_mirrors() {
        let p = BoatParameters::ilca7();
        let c = Controls {
            rudder_rate_cmd: 0.4,
            sheet_rate_cmd: 0.0,
            sheet_release: false,
        };
        let st = BoatState {
            u: 3.0,
            v: 0.2,
            r: -0.1,
            p: 0.2,
            phi: -0.25,
            beta: 0.6,
            beta_dot: -0.1,
            delta_r: -0.2,
            l_sheet: 2.2,
            psi: 0.7,
            ..BoatState::ZERO
        };
        let wind = uniform_wind(5.0, 30.0);
        // The mirror is about the world x axis: W = (-s sin b, -s cos b)
        // reflects to (-s sin b, +s cos b), which is the FROM bearing 180 - b.
        let mirrored_wind = uniform_wind(5.0, 180.0 - 30.0);
        let a = read(&st, &c, &p, &wind);
        let b = read(&mirror_state(&st), &mirror_controls(&c), &p, &mirrored_wind);
        // roll rate, yaw rate, heel and sway acceleration flip; surge does not.
        for (k, flips) in [true, true, true, false, true].into_iter().enumerate() {
            let expect = if flips { -a[k] } else { a[k] };
            assert!(
                (b[k] - expect).abs() < 1e-9,
                "column {k}: {} vs {expect}",
                b[k]
            );
        }
    }

    /// The lines of a source file that actually ship: no `#[cfg(test)]` item,
    /// and no comment.
    ///
    /// Both exclusions are `tests/no_shortcuts.rs`'s and `tests/provenance.rs`'s,
    /// for their reasons: a test that names a forbidden pattern in order to
    /// refuse it is evidence, and a doc comment that explains why a sensor does
    /// **not** read `Simulation::forces` is exactly the documentation this
    /// section is asking for more of.
    fn shipped_code(src: &str) -> String {
        src.lines()
            .take_while(|l| l.trim() != "#[cfg(test)]")
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Trap 1, as a source grep in the shape of `tests/no_shortcuts.rs`.
    ///
    /// The numeric test below is the stronger statement; this one is the
    /// cheaper one, and it is the one that names the defect. No sensor in this
    /// crate may reach `Simulation`, its cached `forces()` breakdown or
    /// `Diagnostics` — the observation is computed, never read from the cache
    /// (F14.7, RV25) — and `imu` must call `evaluate` itself.
    #[test]
    fn no_sensor_reads_the_cached_force_breakdown() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/sensor");
        let mut scanned = 0usize;
        let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(&root)
            .expect("the sensor module")
            .map(|e| e.expect("entry").path())
            .collect();
        entries.sort();
        for path in entries {
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let src = std::fs::read_to_string(&path).expect("source");
            let code = shipped_code(&src);
            for needle in [
                "Simulation",
                "Diagnostics",
                ".forces()",
                "acceleration_body",
            ] {
                assert!(
                    !code.contains(needle),
                    "{}: a sensor reaches `{needle}`; the observation is computed, never \
                     read from the once-per-advance cache (F14.7, trap 1)",
                    path.display()
                );
            }
            scanned += 1;
        }
        assert!(scanned >= 5, "the sensor scan found almost nothing");

        // …and the positive half: `imu` does call `evaluate` itself.
        let code = shipped_code(include_str!("imu.rs"));
        assert!(
            code.contains("let f = evaluate(view.st, view.controls, view.p, view.wind, view.t);"),
            "imu must call `forces::evaluate` at the decision instant"
        );
    }

    /// Trap 1, asserted numerically rather than only by grep: the reading at a
    /// given step is the same however the caller chunked `advance`.
    #[test]
    fn the_imu_does_not_depend_on_advance_chunking() {
        let p = BoatParameters::ilca7();
        let wind = uniform_wind(7.0, 60.0);
        let c = Controls {
            rudder_rate_cmd: 0.5,
            sheet_rate_cmd: -1.0,
            sheet_release: false,
        };
        let start = BoatState {
            u: 1.0,
            psi: 0.3,
            ..Simulation::initial_state(&p)
        };

        let at_step_200 = |chunk: u32| -> [f64; Imu::WIDTH] {
            let mut sim = Simulation::new(p, 5);
            sim.reset(start, 5);
            sim.set_controls(c);
            let mut done = 0u32;
            while done < 200 {
                let n = chunk.min(200 - done);
                sim.advance(n);
                done += n;
            }
            read(sim.state(), sim.controls(), sim.params(), &wind)
        };

        let single = at_step_200(1);
        for chunk in [3u32, 7, 10, 13, 200] {
            assert_eq!(
                at_step_200(chunk),
                single,
                "chunk {chunk} gave a different imu reading"
            );
        }
    }
}
