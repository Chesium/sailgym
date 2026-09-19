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
use wasm_bindgen::{JsCast, JsValue};
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

/// Task 8.1: the diagnostics record crosses the boundary and survives
/// `JSON.parse` in the browser that will draw it.
#[wasm_bindgen_test]
fn diagnostics_serialise_and_round_trip_through_json_parse() {
    // The default wind is a 5 m/s westerly (section 03), so the sail is
    // loaded and none of the fields below is a structural zero.
    let mut sim = Sim::new("{}").expect("`{}` must construct a Sim");
    sim.set_controls(0.3, -1.0, false);
    sim.advance(400);

    let text = sim
        .diagnostics()
        .expect("diagnostics must serialise")
        .as_string()
        .expect("diagnostics() returns a JSON string");

    let parsed = js_sys::JSON::parse(&text).expect("the browser must parse it");
    let object: &js_sys::Object = parsed.unchecked_ref();
    let key = |k: &str| js_sys::Reflect::get(object, &JsValue::from_str(k)).expect("key");

    // A nested record, a vector, a load and a scalar: one of each shape.
    assert!(key("capsize").is_object());
    assert!(key("apparent_wind_body").is_object());
    assert!(key("sail").is_object());
    assert!(key("t").as_f64().expect("t is a number") > 0.0);

    // The record the page draws is the record the core published.
    let re_encoded = js_sys::JSON::stringify(&parsed)
        .expect("re-encodable")
        .as_string()
        .expect("a string");
    assert_eq!(
        js_sys::JSON::parse(&re_encoded)
            .map(|v| js_sys::JSON::stringify(&v).unwrap().as_string().unwrap())
            .unwrap(),
        re_encoded
    );

    // The parameter catalogue crosses the same way (task 8.5).
    let meta = sim
        .parameter_meta_json()
        .expect("the catalogue must serialise")
        .as_string()
        .expect("parameter_meta_json returns a JSON string");
    let entries: js_sys::Array = js_sys::JSON::parse(&meta).expect("parses").unchecked_into();
    assert!(entries.length() >= 55, "{} entries", entries.length());
}
