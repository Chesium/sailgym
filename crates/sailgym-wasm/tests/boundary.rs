//! Task 1.2 boundary proof: the `wasm_bindgen` surface behaves in a real
//! browser, not just in `cargo test` on the host.
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
fn advance_returns_requested_steps_and_snapshot_tracks_the_counter() {
    let mut sim = Sim::new("{}").expect("`{}` must construct a Sim");
    assert_eq!(sim.snapshot().as_ref(), &[0.0]);
    assert_eq!(sim.advance(3), 3);
    assert_eq!(sim.advance(4), 4);
    assert_eq!(sim.snapshot().as_ref(), &[7.0]);
}
