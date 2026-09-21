//! `apparent_wind` — the masthead vane and anemometer (section 05 task 5.3).
//!
//! Two columns: AWA and AWS. **This is what a boat actually measures.**
//!
//! # `true_wind` is deliberately not in the tier-0 suite
//!
//! A real boat derives true wind from apparent wind and boat speed. Handing it
//! to a policy that has no speed log hands that policy a free velocity estimate
//! through the back door, and an IMU-only arm of an ablation then quietly stops
//! being IMU-only (RV30). When a `true_wind` sensor is added it is either
//! marked privileged or derived **inside the sensor from sensed quantities
//! only**; it is not added here, and this paragraph is the reason.
//!
//! # Where the vane sits, and why it is not a new number
//!
//! The PRD says "at the masthead". The F7 catalogue has no mast height — the
//! rig's declared points are `mast_pos_b` and the centre of effort `z_ce` above
//! the CG — and inventing one would put a length in this crate that looks like
//! a boat dimension without being one. So the vane sits at
//! `mast_pos_b + (0, 0, z_ce)`: on the mast, at the highest **declared** point
//! of the rig, derived entirely from F7 values, with no literal of its own.
//!
//! It is a sensor **mounting position**, not a physical coefficient (F14.9), so
//! [`ApparentWind::at`] moves it freely for a study — and moving it bumps
//! nothing, because the position is part of the sensor's configuration and a
//! configured sensor records its configuration. A study that moves it should
//! bump [`ApparentWind::VERSION`], which is why the constructor says so.
//!
//! # The reading convention
//!
//! `AWA` is the **FROM** angle off the bow, positive to starboard — the same
//! convention `Diagnostics::apparent_wind_angle` publishes, stated the same way
//! and computed the same way, so the HUD and the observation never disagree
//! about which way the wind is coming from. The apparent wind vector itself
//! comes from `aero::apparent::apparent_wind_at` (F6.2); nothing here
//! re-derives it, and the `ω × r` term that makes the sail unload during a fast
//! tack is therefore in the reading too.

use sailgym_physics::aero::apparent::apparent_wind_at;
use sailgym_physics::rng::Pcg32;
use sailgym_physics::vec::Vec3;

use super::{FieldSpec, Sensor};
use crate::worldview::WorldView;

/// The masthead vane and anemometer.
#[derive(Clone, Copy, Debug, Default)]
pub struct ApparentWind {
    /// Where the unit is mounted, in `B`, relative to the CG. `None` is the
    /// default mount described in the module documentation.
    mount_b: Option<Vec3>,
}

impl ApparentWind {
    pub const ID: &'static str = "apparent_wind";
    /// Bumped on any change to what this sensor emits — the mount included,
    /// if a study ever makes the mount a fixed part of the sensor rather than
    /// configuration recorded beside it.
    pub const VERSION: u32 = 1;
    pub const WIDTH: usize = 2;

    /// The default mount: up the mast, at the declared CE height.
    pub fn new() -> Self {
        Self { mount_b: None }
    }

    /// Mount the unit somewhere else, in `B`, relative to the CG.
    pub fn at(mount_b: Vec3) -> Self {
        Self {
            mount_b: Some(mount_b),
        }
    }

    /// The mount point for a given catalogue.
    pub fn mount(&self, p: &sailgym_physics::parameters::BoatParameters) -> Vec3 {
        self.mount_b
            .unwrap_or_else(|| p.sail.mast_pos_b + Vec3::new(0.0, 0.0, p.sail.z_ce))
    }
}

impl Sensor for ApparentWind {
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
            FieldSpec::sensed(
                "awa",
                "rad",
                Some(-std::f64::consts::PI),
                Some(std::f64::consts::PI),
            ),
            FieldSpec::sensed("aws", "m/s", Some(0.0), None),
        ]
    }

    fn sense(&mut self, view: &WorldView, _rng: &mut Pcg32, out: &mut [f64]) {
        assert_eq!(
            out.len(),
            Self::WIDTH,
            "apparent_wind writes exactly {} scalars",
            Self::WIDTH
        );
        let aw = apparent_wind_at(view.st, view.true_wind_world(), self.mount(view.p));
        // Identical to `Diagnostics::apparent_wind_angle`, including the
        // degenerate case: with no flow at all there is no direction, and 0 is
        // the answer that keeps the column continuous rather than jumping to
        // whatever `atan2(0, 0)` happens to be.
        out[0] = if aw.x == 0.0 && aw.y == 0.0 {
            0.0
        } else {
            aw.y.atan2(-aw.x)
        };
        out[1] = aw.length();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sailgym_physics::parameters::BoatParameters;
    use sailgym_physics::state::{BoatState, Controls};
    use sailgym_physics::testkit::{mirror_state, still_air, uniform_wind};

    fn read(
        st: &BoatState,
        p: &BoatParameters,
        wind: &dyn sailgym_physics::environment::WindField,
    ) -> [f64; ApparentWind::WIDTH] {
        let c = Controls::default();
        let view = WorldView {
            st,
            controls: &c,
            p,
            wind,
            guidance: None,
            others: &[],
            t: st.t,
        };
        let mut rng = Pcg32::seed_from_u64(2);
        let mut out = [0.0; ApparentWind::WIDTH];
        let mut s = ApparentWind::new();
        s.sense(&view, &mut rng, &mut out);
        out
    }

    #[test]
    fn width_matches_the_declared_layout() {
        let s = ApparentWind::new();
        assert_eq!(s.width(), s.fields().len());
        assert_eq!(s.width(), s.field_names().len());
        assert!(!s.privileged(), "apparent wind is what the boat measures");
    }

    #[test]
    fn the_mount_is_derived_from_f7_and_carries_no_literal_of_its_own() {
        let p = BoatParameters::ilca7();
        let m = ApparentWind::new().mount(&p);
        assert_eq!(m.x, p.sail.mast_pos_b.x);
        assert_eq!(m.y, p.sail.mast_pos_b.y);
        assert_eq!(m.z, p.sail.mast_pos_b.z + p.sail.z_ce);
        // And it follows a live parameter edit rather than being frozen.
        let mut edited = p;
        edited.sail.z_ce = p.sail.z_ce + 1.0;
        assert_eq!(ApparentWind::new().mount(&edited).z, m.z + 1.0);
        // A study may move it.
        let elsewhere = Vec3::new(0.0, 0.0, 1.0);
        assert_eq!(ApparentWind::at(elsewhere).mount(&p), elsewhere);
    }

    /// Sailing forward in still air: the apparent wind is dead ahead at the
    /// boat's own speed — which is the sanity check that says AWA's sign
    /// convention is the FROM angle and not the TO angle.
    #[test]
    fn still_air_gives_a_dead_ahead_apparent_wind() {
        let p = BoatParameters::ilca7();
        let st = BoatState {
            u: 3.0,
            ..BoatState::ZERO
        };
        let air = still_air();
        let [awa, aws] = read(&st, &p, &air);
        assert!(awa.abs() < 1e-12, "AWA {awa} is not dead ahead");
        assert!((aws - 3.0).abs() < 1e-12, "AWS {aws} is not the boat speed");
    }

    /// A wind blowing the boat's air toward port comes **from starboard**, and
    /// AWA is positive there (F2: `+y` is port).
    #[test]
    fn awa_is_positive_when_the_wind_is_on_the_starboard_side() {
        let p = BoatParameters::ilca7();
        let st = BoatState::ZERO;
        // Heading east (psi = 0), wind from the south: air moves toward +y,
        // which is to port.
        let from_starboard = uniform_wind(5.0, 180.0);
        let [awa, aws] = read(&st, &p, &from_starboard);
        assert!(
            awa > 0.0,
            "AWA {awa} should be positive for wind from starboard"
        );
        assert!(
            (awa - std::f64::consts::FRAC_PI_2).abs() < 1e-9,
            "AWA {awa}"
        );
        assert!((aws - 5.0).abs() < 1e-9);

        let from_port = uniform_wind(5.0, 0.0);
        let [awa, _] = read(&st, &p, &from_port);
        assert!(
            (awa + std::f64::consts::FRAC_PI_2).abs() < 1e-9,
            "AWA {awa}"
        );
    }

    /// The mirrored boat in the mirrored wind reads the negated AWA and the
    /// same AWS (R3).
    #[test]
    fn the_vane_mirrors() {
        let p = BoatParameters::ilca7();
        let st = BoatState {
            u: 2.5,
            v: -0.2,
            r: 0.3,
            p: 0.1,
            phi: 0.4,
            psi: 0.8,
            beta: -0.5,
            ..BoatState::ZERO
        };
        let a = read(&st, &p, &uniform_wind(6.0, 40.0));
        // W = (-s sin b, -s cos b) reflects about the world x axis to the FROM
        // bearing 180 - b.
        let b = read(&mirror_state(&st), &p, &uniform_wind(6.0, 180.0 - 40.0));
        assert!((b[0] + a[0]).abs() < 1e-12, "AWA {} vs {}", b[0], a[0]);
        assert!((b[1] - a[1]).abs() < 1e-12, "AWS {} vs {}", b[1], a[1]);
    }

    /// The rotation term of F6.2 is in the reading: a boat spinning on the
    /// spot in still air measures a wind at the masthead, because the masthead
    /// is moving.
    #[test]
    fn the_vane_feels_the_boats_rotation() {
        let p = BoatParameters::ilca7();
        let air = still_air();
        let spinning = BoatState {
            r: 0.5,
            ..BoatState::ZERO
        };
        let [_, aws] = read(&spinning, &p, &air);
        assert!(
            aws > 0.1,
            "a rotating boat must measure apparent wind aloft (F6.2 step 4): {aws}"
        );
        let still = read(&BoatState::ZERO, &p, &air);
        assert_eq!(still[1], 0.0);
        assert_eq!(still[0], 0.0);
    }
}
