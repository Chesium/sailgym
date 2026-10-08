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

// ---------------------------------------------------------------------------
// v2 section 12: the course surface (task 12.7, D5)
// ---------------------------------------------------------------------------
//
// These run under `wasm-pack test --headless --chrome crates/sailgym-wasm` and
// **not** in the gate — the gate proves the browser behaviour through
// `web/tests/e2e/course.spec.ts` (step 9), which drives the real page. They are
// here because this file is where the boundary's own contract is written down,
// and because `run_baseline`'s refusals are easier to read as four assertions
// than as four browser steps.

/// Parse a `JsValue` JSON string.
#[cfg(target_arch = "wasm32")]
fn parse(v: JsValue) -> serde_json::Value {
    serde_json::from_str(&v.as_string().expect("a JSON string")).expect("valid JSON")
}

#[wasm_bindgen_test]
fn practice_tasks_lists_the_skills_then_the_courses() {
    let sim = Sim::new("{}").expect("a Sim");
    let rows = parse(sim.practice_tasks_json().expect("tasks"));
    let rows = rows.as_array().expect("an array");
    assert_eq!(rows.len(), 6, "three skills and three courses");
    let kinds: Vec<&str> = rows.iter().map(|r| r["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        ["skill", "skill", "skill", "course", "course", "course"]
    );
    let courses: Vec<&str> = rows
        .iter()
        .filter(|r| r["kind"] == "course")
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        courses,
        ["course_reach", "course_triangle", "course_windward_leeward"]
    );
    for row in rows.iter().filter(|r| r["kind"] == "course") {
        assert!(row["waypoints"].as_u64().unwrap() >= 2);
        assert!(!row["title"].as_str().unwrap().is_empty());
        assert!(row["thresholds"]["course.half_width"].as_f64().unwrap() > 0.0);
    }
}

#[wasm_bindgen_test]
fn a_course_attempt_publishes_a_course_block() {
    let mut sim = Sim::new("{}").expect("a Sim");
    sim.start_practice("course_reach", 20.0).expect("start");
    let state = parse(sim.practice_state_json().expect("state"));
    let course = &state["course"];
    assert_eq!(course["waypoints"].as_array().unwrap().len(), 3);
    assert_eq!(course["next"], 1);
    assert_eq!(course["waypoints"][0]["state"], "next");
    assert_eq!(course["waypoints"][1]["state"], "pending");
    assert_eq!(course["start"]["x"], 0.0);
    assert!(course["distance_to_next"].as_f64().unwrap() > 0.0);
    // Both posts, from `Rounding::Gate`, and the mark's radius beside them.
    assert_eq!(course["waypoints"][0]["posts"].as_array().unwrap().len(), 2);
    assert_eq!(course["waypoints"][0]["radius"], 5.0);

    // A skill attempt carries no block.
    sim.start_practice("get_moving", 20.0).expect("start");
    let state = parse(sim.practice_state_json().expect("state"));
    assert!(state["course"].is_null());
}

#[wasm_bindgen_test]
fn run_baseline_refuses_what_it_cannot_honour() {
    let mut sim = Sim::new("{}").expect("a Sim");
    // No attempt at all.
    let why = sim.run_baseline(20.0).expect_err("no attempt");
    let why = why.as_string().unwrap();
    assert!(why.contains("no practice attempt"), "{why}");

    // A skill is not a course.
    sim.start_practice("get_moving", 20.0).expect("start");
    let why = sim.run_baseline(20.0).expect_err("a skill has no baseline");
    let why = why.as_string().unwrap();
    assert!(why.contains("not a course"), "{why}");
}

#[wasm_bindgen_test]
fn run_baseline_sails_the_course_and_says_the_conditions_are_the_same() {
    for id in ["course_reach", "course_triangle", "course_windward_leeward"] {
        let mut sim = Sim::new("{}").expect("a Sim");
        sim.start_practice(id, 20.0).expect("start");
        let first = sim
            .run_baseline(20.0)
            .expect("a baseline")
            .as_string()
            .unwrap();
        let run: serde_json::Value = serde_json::from_str(&first).expect("valid JSON");
        // D4: "same conditions, different controller" is the verdict that
        // licenses the ghost and the splits (RV70).
        assert_eq!(run["conditions"]["verdict"], "same_conditions", "{id}");
        assert_eq!(run["outcome"], "finished", "{id}");
        assert!(run["time_s"].as_f64().unwrap() > 10.0, "{id}");
        assert!(!run["narration"].as_array().unwrap().is_empty(), "{id}");
        assert!(!run["episode"]["frames"].as_array().unwrap().is_empty());
        // Two calls, one answer.
        let second = sim
            .run_baseline(20.0)
            .expect("a baseline")
            .as_string()
            .unwrap();
        assert_eq!(first, second, "{id}: two calls disagreed");

        // …and the recorded course comes back from the episode's own identity.
        let episode = serde_json::to_string(&run["episode"]).unwrap();
        let course = parse(sim.episode_course_json(&episode).expect("a course"));
        assert_eq!(
            course["waypoints"].as_array().unwrap().len(),
            run["splits"].as_array().unwrap().len(),
            "{id}: one waypoint per split on a clean run"
        );
    }
}

#[wasm_bindgen_test]
fn episode_course_json_is_null_for_an_episode_with_no_course() {
    let mut sim = Sim::new("{}").expect("a Sim");
    sim.start_recording(20.0);
    sim.advance(10);
    let episode = sim
        .stop_recording()
        .expect("an episode")
        .as_string()
        .unwrap();
    let course = parse(sim.episode_course_json(&episode).expect("no course"));
    assert!(course.is_null(), "a free sail has no course");
}
