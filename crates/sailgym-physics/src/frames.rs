//! World / horizontal-body / boat-fixed frame conversions (F2).
//!
//! **This is the only file in the repository containing a rotation matrix.**
//! Sign-convention drift across independently written physics modules is R3,
//! the highest-probability defect class in this project; keeping every
//! rotation here is the structural mitigation.
//!
//! Conventions, restated only as a reading aid — F2 is normative:
//!
//! - `W` world: `x` east, `y` north, `z` up.
//! - `H` horizontal body: yaw `ψ` about `+z` from `W`; `+x` forward,
//!   `+y` **to port**.
//! - `B` boat-fixed: roll `φ` about `+x_H` from `H`; `φ > 0` is starboard down.

use std::f64::consts::PI;

use crate::vec::{Vec2, Vec3};

const TWO_PI: f64 = 2.0 * PI;

/// A 2×2 rotation matrix, stored row-major.
///
/// Deliberately local to this module: no other file may construct one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat2 {
    pub m00: f64,
    pub m01: f64,
    pub m10: f64,
    pub m11: f64,
}

impl Mat2 {
    /// `self * v`.
    pub fn mul_vec2(self, v: Vec2) -> Vec2 {
        Vec2::new(
            self.m00 * v.x + self.m01 * v.y,
            self.m10 * v.x + self.m11 * v.y,
        )
    }
}

/// Yaw rotation about `+z`, body `H` → world `W` (F2):
///
/// ```text
/// R_z(ψ) = [ cos ψ  −sin ψ ]
///          [ sin ψ   cos ψ ]
/// ```
pub fn rot_z(psi: f64) -> Mat2 {
    let (s, c) = psi.sin_cos();
    Mat2 {
        m00: c,
        m01: -s,
        m10: s,
        m11: c,
    }
}

/// Horizontal body frame `H` → world frame `W`.
pub fn body_to_world(v: Vec2, psi: f64) -> Vec2 {
    rot_z(psi).mul_vec2(v)
}

/// World frame `W` → horizontal body frame `H`, i.e. `R_z(−ψ)`.
pub fn world_to_body(v: Vec2, psi: f64) -> Vec2 {
    rot_z(-psi).mul_vec2(v)
}

/// Roll rotation about `+x`, boat-fixed `B` → horizontal `H` (F2):
///
/// ```text
/// R_x(φ) = [ 1    0        0     ]
///          [ 0  cos φ   −sin φ   ]
///          [ 0  sin φ    cos φ   ]
/// ```
pub fn rot_x(phi: f64, v: Vec3) -> Vec3 {
    let (s, c) = phi.sin_cos();
    Vec3::new(v.x, c * v.y - s * v.z, s * v.y + c * v.z)
}

/// Horizontal `H` → boat-fixed `B`, i.e. `R_x(−φ)`.
///
/// Applied to a horizontal vector `(0, a, 0)` this gives
/// `(0, a cos φ, −a sin φ)` — the classical heel correction, derived rather
/// than fudged (F6.4). Do not add a second `cos φ` anywhere.
pub fn rot_x_inv(phi: f64, v: Vec3) -> Vec3 {
    rot_x(-phi, v)
}

/// Boom unit vector in `B` (F2.1): `b̂(β) = (−cos β, −sin β, 0)`.
///
/// The boom points **aft**, so a positive (right-handed, counter-clockwise
/// from above) `β` swings the boom tip to **starboard** — `b̂.y < 0`, and
/// `+y` is port. This is deliberate: every rotation about `+z` in this
/// codebase is right-handed, so `I_b β̈ = Σ M_z` carries no sign flip.
pub fn boom_dir(beta: f64) -> Vec3 {
    let (s, c) = beta.sin_cos();
    Vec3::new(-c, -s, 0.0)
}

/// `d b̂/dβ = (sin β, −cos β, 0)` (F2.1).
pub fn boom_dir_dbeta(beta: f64) -> Vec3 {
    let (s, c) = beta.sin_cos();
    Vec3::new(s, -c, 0.0)
}

/// Rudder chord direction in `B`, leading edge at the stock (F2.2):
/// `ĉ_r(δr) = (−cos δr, −sin δr, 0)`.
///
/// `δr > 0` deflects the trailing edge to starboard, which generates rudder
/// side force to port and, the rudder being aft of the CG, `M_z < 0` — so the
/// **bow turns to starboard**.
pub fn rudder_chord(delta_r: f64) -> Vec3 {
    let (s, c) = delta_r.sin_cos();
    Vec3::new(-c, -s, 0.0)
}

/// Wrap an angle to the half-open interval (−π, π].
///
/// The closed end is at `+π`: `wrap_pi(π) == wrap_pi(−π) == π`.
///
/// An angle already in range is returned **bit-identically**. Without that
/// fast path, `(a + π) − π` perturbs the low bits of every heading on every
/// step, so a boat sitting at rest on a heading of 0.6 rad would creep — which
/// `integrator::zero_force_zero_motion` would catch, and which would otherwise
/// be a slow, silent determinism leak.
pub fn wrap_pi(a: f64) -> f64 {
    if a > -PI && a <= PI {
        return a;
    }
    let mut x = (a + PI) % TWO_PI;
    if x <= 0.0 {
        x += TWO_PI;
    }
    x - PI
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic test-local generator; see the note in `state.rs`.
    struct Lcg(u64);

    impl Lcg {
        fn unit(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
        }

        fn range(&mut self, lo: f64, hi: f64) -> f64 {
            lo + self.unit() * (hi - lo)
        }
    }

    #[test]
    fn rot_round_trip() {
        let mut rng = Lcg(0xF00D_0001_0002_0003);
        for _ in 0..100 {
            let v = Vec2::new(rng.range(-50.0, 50.0), rng.range(-50.0, 50.0));
            let psi = rng.range(-20.0, 20.0);
            let back = world_to_body(body_to_world(v, psi), psi);
            assert!((back.x - v.x).abs() < 1e-12, "x: {back:?} vs {v:?}");
            assert!((back.y - v.y).abs() < 1e-12, "y: {back:?} vs {v:?}");
        }
    }

    #[test]
    fn rot_z_maps_bow_to_world_heading() {
        // ψ = 0: bow points world east. ψ = π/2: bow points world north.
        let bow = Vec2::new(1.0, 0.0);
        let east = body_to_world(bow, 0.0);
        assert!((east.x - 1.0).abs() < 1e-15 && east.y.abs() < 1e-15);
        let north = body_to_world(bow, PI / 2.0);
        assert!(north.x.abs() < 1e-15 && (north.y - 1.0).abs() < 1e-15);
    }

    #[test]
    fn boom_dir_sign() {
        // Section acceptance criterion 7: positive β puts the boom to
        // starboard (F2.1). +y is port, so the boom tip has y < 0.
        assert!(boom_dir(0.3).y < 0.0);
        assert!(boom_dir(-0.3).y > 0.0);
        // β = 0 is dead aft.
        assert_eq!(boom_dir(0.0), Vec3::new(-1.0, 0.0, 0.0));
    }

    #[test]
    fn boom_dir_derivative() {
        let h = 1e-6;
        for &beta in &[-2.5, -0.7, 0.0, 0.3, 1.2, 3.0] {
            let fd = (boom_dir(beta + h) - boom_dir(beta - h)) / (2.0 * h);
            let an = boom_dir_dbeta(beta);
            assert!(
                (fd.x - an.x).abs() < 1e-7,
                "x at β={beta}: {fd:?} vs {an:?}"
            );
            assert!(
                (fd.y - an.y).abs() < 1e-7,
                "y at β={beta}: {fd:?} vs {an:?}"
            );
            assert!(
                (fd.z - an.z).abs() < 1e-7,
                "z at β={beta}: {fd:?} vs {an:?}"
            );
        }
    }

    #[test]
    fn rot_x_horizontal_vector() {
        // Section acceptance criterion 7: the F6.4 heel correction is
        // geometric. R_x(−φ)·(0, a, 0) = (0, a cos φ, −a sin φ).
        let a = 3.5;
        for &phi in &[-1.3, -0.4, 0.0, 0.2, 0.9, 1.5] {
            let out = rot_x_inv(phi, Vec3::new(0.0, a, 0.0));
            assert!((out.y - a * phi.cos()).abs() < 1e-12, "y at φ={phi}");
            assert!((out.z + a * phi.sin()).abs() < 1e-12, "z at φ={phi}");
            assert_eq!(out.x, 0.0);
        }
    }

    #[test]
    fn rot_x_round_trip() {
        let mut rng = Lcg(0x1234_5678_9ABC_DEF0);
        for _ in 0..100 {
            let v = Vec3::new(
                rng.range(-5.0, 5.0),
                rng.range(-5.0, 5.0),
                rng.range(-5.0, 5.0),
            );
            let phi = rng.range(-8.0, 8.0);
            let back = rot_x_inv(phi, rot_x(phi, v));
            assert!((back.x - v.x).abs() < 1e-12);
            assert!((back.y - v.y).abs() < 1e-12);
            assert!((back.z - v.z).abs() < 1e-12);
        }
    }

    #[test]
    fn rudder_chord_points_aft_and_deflects_trailing_edge_to_starboard() {
        assert_eq!(rudder_chord(0.0), Vec3::new(-1.0, 0.0, 0.0));
        // +y is port, so a positive δr puts the trailing edge to starboard.
        assert!(rudder_chord(0.4).y < 0.0);
    }

    #[test]
    fn wrap_pi_boundaries() {
        assert_eq!(wrap_pi(PI), PI);
        assert_eq!(wrap_pi(-PI), PI);
        assert_eq!(wrap_pi(3.0 * PI), PI);
        assert_eq!(wrap_pi(0.0), 0.0);
        assert!((wrap_pi(2.0 * PI) - 0.0).abs() < 1e-12);
        assert!((wrap_pi(1.5 * PI) + 0.5 * PI).abs() < 1e-12);
    }

    #[test]
    fn wrap_pi_is_idempotent_and_in_range() {
        let mut rng = Lcg(0x0BAD_C0DE_0BAD_C0DE);
        for _ in 0..200 {
            let a = rng.range(-100.0, 100.0);
            let w = wrap_pi(a);
            assert!(w > -PI && w <= PI, "{a} wrapped to {w}");
            // Idempotent to round-off: `(w + π) − π` is not bit-exact for a
            // general `w`, so this is a tolerance, not an equality.
            assert!((wrap_pi(w) - w).abs() < 1e-12, "{a} -> {w}");
        }
    }
}
