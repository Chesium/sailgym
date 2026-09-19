//! Boundary proof: the `wasm_bindgen` surface behaves in a real browser, not
//! just in `cargo test` on the host.
//!
//! Run with: `wasm-pack test --headless --chrome crates/sailgym-wasm`
//!
//! The whole file is gated to `wasm32` because `wasm-bindgen-test` is a
//! wasm-only dev-dependency (see `Cargo.toml`); on the host this compiles to
//! an empty test binary so `cargo clippy --all-targets` stays green.
#![cfg(target_arch = "wasm32")]

use sailgym_wasm::Sim;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn new_accepts_empty_json_object() {
    let sim = Sim::new("{}").expect("`{}` is valid JSON and must construct a Sim");
    assert_eq!(sim.version(), env!("CARGO_PKG_VERSION"));
}

#[wasm_bindgen_test]
fn new_rejects_invalid_json() {
    assert!(
        Sim::new("not json").is_err(),
        "`not json` is not valid JSON and must be rejected"
    );
}

#[wasm_bindgen_test]
fn snapshot_has_the_f8_3_layout() {
    let sim = Sim::new("{}").expect("`{}` must construct a Sim");
    // Task 2.4: the snapshot is the 13-field F8.3 buffer.
    assert_eq!(sim.snapshot().len(), 13);
}

#[wasm_bindgen_test]
fn advance_returns_requested_steps_and_moves_the_clock() {
    let mut sim = Sim::new("{}").expect("`{}` must construct a Sim");
    let dt = sim.dt();
    assert_eq!(sim.snapshot()[12], 0.0);
    assert_eq!(sim.advance(3), 3);
    assert_eq!(sim.advance(4), 4);
    // Index 12 is `t` (F8.3).
    assert!((sim.snapshot()[12] - 7.0 * dt).abs() < 1e-12);

    sim.reset("{}").expect("`{}` must reset");
    assert_eq!(sim.snapshot()[12], 0.0);
}

#[wasm_bindgen_test]
fn set_parameter_is_reflected_in_parameters_json() {
    let mut sim = Sim::new("{}").expect("`{}` must construct a Sim");
    assert_eq!(sim.set_parameter("sail.area", 8.0), Ok(false));
    let json = sim
        .parameters_json()
        .expect("parameters must serialise")
        .as_string()
        .expect("parameters_json returns a JSON string");
    assert!(json.contains("\"area\":8.0"), "{json}");
    assert!(sim.set_parameter("no.such.path", 1.0).is_err());
}

#[wasm_bindgen_test]
fn controls_steer_the_boat() {
    let mut sim = Sim::new("{}").expect("`{}` must construct a Sim");
    // +1 = steer the bow to starboard (F2.2). Index 10 is `delta_r`.
    sim.set_controls(1.0, 0.0, false);
    sim.advance(100);
    assert!(sim.snapshot()[10] > 0.0);
}
