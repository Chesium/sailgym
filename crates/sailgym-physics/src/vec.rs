//! Minimal 2-D and 3-D vector types (`docs/00-foundations.md` task 1.1).
//!
//! All components are `f64`; the physics core uses no `f32` intermediates
//! (F1, F9.5). Degenerate `normalize` returns exactly `ZERO` rather than
//! `NaN`, which is what keeps the zero-flow foil behaviour of F5.3 total.

use crate::foil::EPS_FLOW;
use serde::Serialize;
use std::ops::{Add, Div, Mul, Neg, Sub};

// `Serialize` only, and only so the section 08 diagnostics record can publish
// a vector without a `serialize_with` shim per field (section 02 handoff
// §2.10 named this as the clean fix). The emitted shape is `{x, y, z}`, which
// is exactly what `parameters::vec3_serde` already produces, so no JSON
// document changes. Nothing here deserialises: the physics core still takes
// its vectors from `parameters.rs` and `state.rs`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn dot(self, o: Self) -> f64 {
        self.x * o.x + self.y * o.y
    }

    /// 2-D cross product `self.x * o.y - self.y * o.x`, i.e. the `z`
    /// component of the 3-D cross product. Positive means `o` is
    /// counter-clockwise from `self`.
    pub fn cross(self, o: Self) -> f64 {
        self.x * o.y - self.y * o.x
    }

    pub fn length(self) -> f64 {
        self.length_squared().sqrt()
    }

    pub fn length_squared(self) -> f64 {
        self.dot(self)
    }

    /// Unit vector, or exactly [`Vec2::ZERO`] when the length is below
    /// [`EPS_FLOW`].
    pub fn normalize(self) -> Self {
        let l = self.length();
        if l < EPS_FLOW {
            Self::ZERO
        } else {
            self / l
        }
    }
}

impl Vec3 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn dot(self, o: Self) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    /// Right-handed cross product: `x_hat.cross(y_hat) == z_hat`.
    pub fn cross(self, o: Self) -> Self {
        Self {
            x: self.y * o.z - self.z * o.y,
            y: self.z * o.x - self.x * o.z,
            z: self.x * o.y - self.y * o.x,
        }
    }

    pub fn length(self) -> f64 {
        self.length_squared().sqrt()
    }

    pub fn length_squared(self) -> f64 {
        self.dot(self)
    }

    /// Unit vector, or exactly [`Vec3::ZERO`] when the length is below
    /// [`EPS_FLOW`].
    pub fn normalize(self) -> Self {
        let l = self.length();
        if l < EPS_FLOW {
            Self::ZERO
        } else {
            self / l
        }
    }

    /// Horizontal part, dropping the spanwise `z` component.
    pub fn xy(self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }
}

impl Add for Vec2 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y)
    }
}

impl Neg for Vec2 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

impl Mul<f64> for Vec2 {
    type Output = Self;
    fn mul(self, s: f64) -> Self {
        Self::new(self.x * s, self.y * s)
    }
}

impl Div<f64> for Vec2 {
    type Output = Self;
    fn div(self, s: f64) -> Self {
        Self::new(self.x / s, self.y / s)
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

impl Mul<f64> for Vec3 {
    type Output = Self;
    fn mul(self, s: f64) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }
}

impl Div<f64> for Vec3 {
    type Output = Self;
    fn div(self, s: f64) -> Self {
        Self::new(self.x / s, self.y / s, self.z / s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_sign_is_right_handed() {
        assert_eq!(Vec2::new(1.0, 0.0).cross(Vec2::new(0.0, 1.0)), 1.0);
        assert_eq!(Vec2::new(0.0, 1.0).cross(Vec2::new(1.0, 0.0)), -1.0);
        assert_eq!(Vec2::new(2.0, 3.0).cross(Vec2::new(2.0, 3.0)), 0.0);
    }

    #[test]
    fn dot_and_length() {
        assert_eq!(Vec2::new(3.0, 4.0).length(), 5.0);
        assert_eq!(Vec2::new(3.0, 4.0).length_squared(), 25.0);
        assert_eq!(Vec2::new(1.0, 2.0).dot(Vec2::new(3.0, 4.0)), 11.0);
        assert_eq!(Vec3::new(1.0, 2.0, 2.0).length(), 3.0);
        assert_eq!(Vec3::new(1.0, 2.0, 3.0).dot(Vec3::new(4.0, 5.0, 6.0)), 32.0);
    }

    #[test]
    fn normalize_zero_vector_is_exactly_zero() {
        assert_eq!(Vec2::ZERO.normalize(), Vec2::ZERO);
        assert_eq!(Vec3::ZERO.normalize(), Vec3::ZERO);
        // Below EPS_FLOW but non-zero: still exactly ZERO, never NaN.
        let tiny2 = Vec2::new(EPS_FLOW * 0.5, 0.0);
        let tiny3 = Vec3::new(0.0, EPS_FLOW * 0.5, 0.0);
        assert_eq!(tiny2.normalize(), Vec2::ZERO);
        assert_eq!(tiny3.normalize(), Vec3::ZERO);
    }

    #[test]
    fn normalize_unit_length() {
        let n = Vec2::new(3.0, 4.0).normalize();
        assert_eq!(n, Vec2::new(0.6, 0.8));
        assert!((Vec3::new(1.0, 2.0, 3.0).normalize().length() - 1.0).abs() < 1e-15);
    }

    #[test]
    fn vec3_cross_is_right_handed() {
        let x = Vec3::new(1.0, 0.0, 0.0);
        let y = Vec3::new(0.0, 1.0, 0.0);
        let z = Vec3::new(0.0, 0.0, 1.0);
        assert_eq!(x.cross(y), z);
        assert_eq!(y.cross(z), x);
        assert_eq!(z.cross(x), y);
        assert_eq!(y.cross(x), -z);
    }

    #[test]
    fn vec3_cross_is_orthogonal_to_both_operands() {
        let a = Vec3::new(1.0, -2.0, 0.5);
        let b = Vec3::new(-0.25, 3.0, 2.0);
        let c = a.cross(b);
        assert!(c.dot(a).abs() < 1e-15);
        assert!(c.dot(b).abs() < 1e-15);
    }

    #[test]
    fn vec2_operator_round_trips() {
        let a = Vec2::new(1.5, -2.5);
        let b = Vec2::new(-0.5, 4.0);
        assert_eq!((a + b) - b, a);
        assert_eq!(-(-a), a);
        assert_eq!(a * 4.0 / 4.0, a);
        assert_eq!(a + (-a), Vec2::ZERO);
        assert_eq!(a * 2.0, a + a);
    }

    #[test]
    fn vec3_operator_round_trips() {
        let a = Vec3::new(1.5, -2.5, 3.0);
        let b = Vec3::new(-0.5, 4.0, -1.0);
        assert_eq!((a + b) - b, a);
        assert_eq!(-(-a), a);
        assert_eq!(a * 4.0 / 4.0, a);
        assert_eq!(a + (-a), Vec3::ZERO);
        assert_eq!(a * 2.0, a + a);
        assert_eq!(a.xy(), Vec2::new(1.5, -2.5));
    }
}
