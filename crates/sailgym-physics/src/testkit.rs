//! Test-only helpers (section 04, task 4.1).
//!
//! Compiled only under `cfg(test)` or the `testkit` feature, so **nothing
//! here reaches the wasm build**. `crates/sailgym-wasm` does not enable the
//! feature and neither does a plain `cargo build`.
//!
//! ## Why this module exists
//!
//! There is no sail until section 05, so after section 04 the boat has no way
//! to accelerate itself. Rather than keep a fake thrust term alive "just for
//! tests" (R4 is closed in this very section), [`WithExternalLoad`] is the
//! sanctioned way to drive a test: it wraps a real [`ForceModel`] and adds one
//! named external [`Load`], leaving the physics untouched.
//!
//! [`mirror_state`] is the backbone of the port/starboard symmetry invariant
//! (brief §35). Its definition follows directly from F2 and is given in
//! task 4.1 of `docs/v1/04-hydro.md`.
//!
//! ## `npy` (v2 section 02, task 2.4)
//!
//! [`npy`] is the bounded `.npy` codec the conformance bundle is written in.
//! It lives here rather than in the generator because **two** consumers read
//! the same bytes — `crates/sailgym-bench`'s `gen_conformance` writes them
//! and `crates/sailgym-physics/tests/conformance.rs` reads them back — and a
//! physics test may not depend on the bench crate, which depends on it. One
//! codec, behind the same feature as everything else here, and none of it
//! reaches `wasm-pack build`.

pub mod npy;

use crate::dynamics::{ForceModel, Generalized, Load};
use crate::environment::wind::{ProceduralWind, WindConfig, WindMode};
use crate::environment::WindField;
use crate::parameters::BoatParameters;
use crate::state::{BoatState, Controls};

/// Wraps a [`ForceModel`] and adds a constant external [`Load`], expressed in
/// the boat-fixed frame `B` like every other load (F4.4).
///
/// The sanctioned way to drive tests before the sail exists. **Not compiled
/// into the wasm build.**
pub struct WithExternalLoad<F: ForceModel> {
    pub inner: F,
    pub extra: Load,
}

impl<F: ForceModel> ForceModel for WithExternalLoad<F> {
    fn generalized(&self, st: &BoatState, c: &Controls, p: &BoatParameters, t: f64) -> Generalized {
        let mut g = self.inner.generalized(st, c, p, t);
        g.add(self.extra, st.phi);
        g
    }

    fn boom_moment(&self, st: &BoatState, c: &Controls, p: &BoatParameters, t: f64) -> f64 {
        self.inner.boom_moment(st, c, p, t)
    }
}

/// Zero wind everywhere, at all times.
pub fn still_air() -> impl WindField {
    uniform_wind(0.0, 0.0)
}

/// A constant wind, given as the meteorological FROM bearing the scenario
/// JSON and the UI use. The conversion is `environment::wind_from_bearing`
/// and is never re-derived here (F6.1).
pub fn uniform_wind(speed: f64, bearing_deg: f64) -> impl WindField {
    ProceduralWind::new(
        WindConfig {
            mode: WindMode::Uniform,
            speed,
            bearing_deg,
            ..WindConfig::default()
        },
        0,
    )
}

/// Mirror a state about the boat's centreline (task 4.1, derived from F2).
///
/// ```text
/// x → x        y → −y       psi → −psi     phi → −phi
/// u → u        v → −v       r  → −r        p   → −p
/// beta → −beta beta_dot → −beta_dot         delta_r → −delta_r
/// l_sheet → l_sheet         t → t
/// ```
pub fn mirror_state(st: &BoatState) -> BoatState {
    BoatState {
        x: st.x,
        y: -st.y,
        psi: -st.psi,
        phi: -st.phi,
        u: st.u,
        v: -st.v,
        r: -st.r,
        p: -st.p,
        beta: -st.beta,
        beta_dot: -st.beta_dot,
        delta_r: -st.delta_r,
        l_sheet: st.l_sheet,
        t: st.t,
    }
}

/// The controls that drive the mirrored boat.
///
/// `rudder_rate_cmd` is `+1 = steer bow to starboard` (F3), so it flips with
/// `delta_r`. The sheet commands are a length rate and a release flag, neither
/// of which has a side, so both are unchanged.
pub fn mirror_controls(c: &Controls) -> Controls {
    Controls {
        rudder_rate_cmd: -c.rudder_rate_cmd,
        sheet_rate_cmd: c.sheet_rate_cmd,
        sheet_release: c.sheet_release,
    }
}

/// `mirror_state` applied twice is the identity, bit-identical.
///
/// Lives at file scope rather than inside a `mod tests`, so the test path is
/// literally `testkit::mirror_involution` as task 4.1 names it.
#[cfg(test)]
#[test]
fn mirror_involution() {
    use crate::state::{STATE_FIELDS, STATE_LEN};

    // Deterministic test-local generator; see the note in `state.rs`.
    struct Lcg(u64);
    impl Lcg {
        fn range(&mut self, lo: f64, hi: f64) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            lo + ((self.0 >> 11) as f64) / ((1u64 << 53) as f64) * (hi - lo)
        }
    }

    let mut rng = Lcg(0x4D17_2051_9E37_79B9);
    for k in 0..200 {
        let a: [f64; STATE_LEN] = std::array::from_fn(|_| rng.range(-5.0, 5.0));
        let st = BoatState::from_array(&a);
        let back = mirror_state(&mirror_state(&st));
        for i in 0..STATE_LEN {
            assert_eq!(
                back.to_array()[i].to_bits(),
                a[i].to_bits(),
                "case {k}, field {}",
                STATE_FIELDS[i]
            );
        }
    }

    // And on the controls.
    let c = Controls {
        rudder_rate_cmd: 0.7,
        sheet_rate_cmd: -0.3,
        sheet_release: true,
    };
    assert_eq!(mirror_controls(&mirror_controls(&c)), c);
}
