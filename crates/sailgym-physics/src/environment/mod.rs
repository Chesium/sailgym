//! Environment models: the fluid the boat moves through.
//!
//! Only air in v1 — the brief defers water currents entirely (brief §18), so
//! the water is still and there is no `current.rs`.
//!
//! This module declares the [`WindField`] contract (`docs/v1/00-foundations.md`
//! F6.1) and the **one** conversion between the meteorological "from" bearing
//! used by scenario JSON and the UI, and the world-frame velocity vector used
//! by the physics. That conversion exists here and nowhere else; the
//! from/toward flip is the classic silent sign defect and `bearing_round_trip`
//! is the guard.

pub mod wind;

use crate::vec::Vec2;

/// A wind field, verbatim from F6.1.
///
/// `sample` returns the **velocity of the air in the world frame**, i.e. the
/// direction the wind is blowing *toward*.
///
/// `sample_grid` exists so the visualization can be filled with a single call
/// across the WASM boundary (brief §19). It must produce bit-identical values
/// to `sample` at the same points: the displayed wind and the simulated wind
/// are one field (brief §19, §47), and that is guaranteed structurally by
/// calling the same routine, not by matching two implementations.
pub trait WindField {
    /// Air velocity at `(x, y)` in the world frame, at time `t`.
    fn sample(&self, x: f64, y: f64, t: f64) -> Vec2;

    /// Fill `out` with `[wx, wy]` pairs, row-major, for the `nx × ny` grid
    /// whose node `(i, j)` is at `(x0 + i·dx, y0 + j·dy)`.
    ///
    /// `out.len()` must be `2 · nx · ny`.
    ///
    /// The nine-argument signature is verbatim from F6.1 and may not be
    /// repackaged into a struct: F6.1 is normative (F13.1).
    #[allow(clippy::too_many_arguments)]
    fn sample_grid(
        &self,
        x0: f64,
        y0: f64,
        dx: f64,
        dy: f64,
        nx: usize,
        ny: usize,
        t: f64,
        out: &mut [f32],
    );
}

/// Meteorological bearing (deg, from-direction, CW from north) → world
/// velocity vector. Verbatim from F6.1.
pub fn wind_from_bearing(speed: f64, bearing_deg: f64) -> Vec2 {
    let b = bearing_deg.to_radians();
    Vec2::new(-speed * b.sin(), -speed * b.cos())
}

/// The inverse of [`wind_from_bearing`]: `(speed, bearing_deg)`.
///
/// The bearing is normalised to `[0, 360)`. A zero vector reports a bearing of
/// zero, which is the only defensible answer and keeps the HUD readout finite
/// in a calm.
///
/// This is the only place a wind vector is turned back into a bearing.
/// TypeScript must call it through the WASM boundary rather than re-deriving
/// it (F8, section 03 task 3.5).
pub fn wind_to_bearing(w: Vec2) -> (f64, f64) {
    let speed = w.length();
    if speed == 0.0 {
        return (0.0, 0.0);
    }
    // `wind_from_bearing` gives w = -speed·(sin b, cos b), so -w points along
    // the from-direction and `atan2(x, y)` recovers a clockwise-from-north
    // bearing directly.
    let bearing = (-w.x).atan2(-w.y).to_degrees();
    (
        speed,
        if bearing < 0.0 {
            bearing + 360.0
        } else {
            bearing
        },
    )
}

/// `wind_to_bearing(wind_from_bearing(s, b))` round-trips, and a northerly
/// blows **toward the south**.
///
/// Lives at file scope rather than in a `mod tests` so the test path is
/// literally `environment::bearing_round_trip`, as task 3.1 requires.
#[cfg(test)]
#[test]
fn bearing_round_trip() {
    for b in [0.0_f64, 45.0, 90.0, 180.0, 270.0, 359.0] {
        for speed in [0.5_f64, 5.0, 17.3] {
            let (s, back) = wind_to_bearing(wind_from_bearing(speed, b));
            assert!((s - speed).abs() < 1e-9, "speed {s} != {speed} at {b}°");
            assert!((back - b).abs() < 1e-9, "bearing {back} != {b}");
        }
    }

    // The guard against the from/toward flip: a northerly (from the north)
    // blows toward the south, i.e. world -y.
    let w = wind_from_bearing(5.0, 0.0);
    assert!(w.x.abs() < 1e-12, "a northerly has no east-west component");
    assert!((w.y + 5.0).abs() < 1e-12, "a northerly must blow toward -y");

    // …and a westerly (from the west, bearing 270) blows toward the east.
    let w = wind_from_bearing(5.0, 270.0);
    assert!((w.x - 5.0).abs() < 1e-12, "a westerly must blow toward +x");
    assert!(w.y.abs() < 1e-12);

    assert_eq!(wind_to_bearing(Vec2::ZERO), (0.0, 0.0));
}
