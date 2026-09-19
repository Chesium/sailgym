//! sailgym physics core.
//!
//! Pure Rust: this crate must build and test on the host with plain
//! `cargo test`, and must never depend on `wasm-bindgen` or any JS-facing
//! crate (`docs/00-foundations.md` F8.1).
//!
//! Conventions, equations and parameters are normative in
//! `docs/00-foundations.md`; nothing here may redefine them.

pub mod aero;
pub mod constants;
pub mod diagnostics;
pub mod dynamics;
pub mod environment;
pub mod foil;
pub mod forces;
pub mod frames;
pub mod hydro;
pub mod integrator;
pub mod parameters;
pub mod recording;
pub mod rigging;
pub mod rng;
pub mod scenario;
pub mod simulation;
pub mod stability;
pub mod state;
/// Test-only helpers (section 04, task 4.1). Compiled only under `cfg(test)`
/// or the `testkit` feature, so nothing here reaches the wasm build.
#[cfg(any(test, feature = "testkit"))]
pub mod testkit;
pub mod vec;
