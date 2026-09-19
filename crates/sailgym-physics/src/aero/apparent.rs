//! Apparent wind at a point on the boat, following F6.2 in order.

use crate::frames::{rot_x, rot_x_inv, world_to_body};
use crate::state::BoatState;
use crate::vec::{Vec2, Vec3};

/// True wind expressed in the horizontal body frame H, for diagnostics.
pub fn true_wind_body(st: &BoatState, wind_world: Vec2) -> Vec2 {
    world_to_body(wind_world, st.psi)
}

/// Apparent wind at a point in the boat-fixed frame B.
///
/// `wind_world` is sampled at the boat position by the caller. `r_b` is the
/// point's position relative to the CG in B. Retain the spanwise component;
/// dropping it is the sail model's responsibility (F6.3).
pub fn apparent_wind_at(st: &BoatState, wind_world: Vec2, r_b: Vec3) -> Vec3 {
    let wind_h = true_wind_body(st, wind_world);
    let velocity_cg_h = Vec3::new(st.u, st.v, 0.0);
    let omega_h = Vec3::new(st.p, 0.0, st.r);
    let velocity_pt_h = velocity_cg_h + omega_h.cross(rot_x(st.phi, r_b));
    let apparent_h = Vec3::new(wind_h.x, wind_h.y, 0.0) - velocity_pt_h;
    rot_x_inv(st.phi, apparent_h)
}

/// Apparent wind at the CG in B, for the HUD and diagnostics.
pub fn apparent_wind_cg(st: &BoatState, wind_world: Vec2) -> Vec3 {
    apparent_wind_at(st, wind_world, Vec3::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Pcg32;
    use crate::testkit::mirror_state;

    fn assert_near(actual: Vec3, expected: Vec3, tolerance: f64) {
        assert!((actual.x - expected.x).abs() < tolerance);
        assert!((actual.y - expected.y).abs() < tolerance);
        assert!((actual.z - expected.z).abs() < tolerance);
    }

    #[test]
    fn stationary_equals_true_wind() {
        let wind = Vec2::new(-4.0, 3.0);
        for psi in [-2.4, -0.6, 0.0, 0.7, 2.9] {
            let st = BoatState {
                psi,
                ..BoatState::ZERO
            };
            let expected = world_to_body(wind, psi);
            assert_eq!(true_wind_body(&st, wind), expected);
            assert_near(
                apparent_wind_cg(&st, wind),
                Vec3::new(expected.x, expected.y, 0.0),
                1e-14,
            );
        }
    }

    #[test]
    fn head_to_wind_adds() {
        let st = BoatState {
            u: 3.0,
            ..BoatState::ZERO
        };
        assert_near(
            apparent_wind_cg(&st, Vec2::new(-5.0, 0.0)),
            Vec3::new(-8.0, 0.0, 0.0),
            1e-12,
        );
    }

    #[test]
    fn running_subtracts() {
        let st = BoatState {
            u: 3.0,
            ..BoatState::ZERO
        };
        assert_near(
            apparent_wind_cg(&st, Vec2::new(5.0, 0.0)),
            Vec3::new(2.0, 0.0, 0.0),
            1e-12,
        );
    }

    #[test]
    fn rotation_term_present() {
        let st = BoatState {
            r: 1.0,
            ..BoatState::ZERO
        };
        assert_eq!(
            apparent_wind_at(&st, Vec2::ZERO, Vec3::new(0.0, 0.0, 2.4)),
            Vec3::ZERO
        );
        assert_near(
            apparent_wind_at(&st, Vec2::ZERO, Vec3::new(1.2, 0.0, 2.4)),
            Vec3::new(0.0, -1.2, 0.0),
            1e-12,
        );
    }

    #[test]
    fn roll_rate_term_present() {
        let st = BoatState {
            p: 1.0,
            ..BoatState::ZERO
        };
        assert_near(
            apparent_wind_at(&st, Vec2::ZERO, Vec3::new(0.0, 0.0, 2.4)),
            Vec3::new(0.0, 2.4, 0.0),
            1e-12,
        );
    }

    #[test]
    fn heel_reduces_lateral_component() {
        let st = BoatState {
            phi: 0.6,
            ..BoatState::ZERO
        };
        let wind = Vec2::new(0.0, 5.0);
        let upright = apparent_wind_cg(&BoatState::ZERO, wind);
        let heeled = apparent_wind_cg(&st, wind);
        assert!((heeled.y - upright.y * 0.6_f64.cos()).abs() < 1e-12);
        assert!((heeled.z + upright.y * 0.6_f64.sin()).abs() < 1e-12);
        assert_ne!(heeled.z, 0.0);
    }

    #[test]
    fn mirror_symmetry() {
        let mut rng = Pcg32::seed_from_u64(501);
        for _ in 0..100 {
            let st = BoatState {
                psi: rng.range(-3.0, 3.0),
                phi: rng.range(-1.5, 1.5),
                u: rng.range(-3.0, 3.0),
                v: rng.range(-3.0, 3.0),
                r: rng.range(-1.0, 1.0),
                p: rng.range(-1.0, 1.0),
                ..BoatState::ZERO
            };
            let wind = Vec2::new(rng.range(-8.0, 8.0), rng.range(-8.0, 8.0));
            let point = Vec3::new(
                rng.range(-2.0, 2.0),
                rng.range(-2.0, 2.0),
                rng.range(0.0, 3.0),
            );
            let original = apparent_wind_at(&st, wind, point);
            let mirrored = apparent_wind_at(
                &mirror_state(&st),
                Vec2::new(wind.x, -wind.y),
                Vec3::new(point.x, -point.y, point.z),
            );
            assert_near(
                mirrored,
                Vec3::new(original.x, -original.y, original.z),
                1e-14,
            );
        }
    }
}
