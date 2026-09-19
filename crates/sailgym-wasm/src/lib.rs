//! Thin `wasm_bindgen` wrapper over `sailgym-physics` (F8.1).
//!
//! Section 01 ships a **placeholder** `Sim` whose only job is to prove the
//! Rust ⇄ WASM boundary works end to end: construct from a JSON string,
//! advance a counter, hand a flat `f64` buffer back to JS, report a version.
//!
//! There is no physics here and there never will be: every equation lives in
//! `sailgym-physics` (F8. "No physical equation is implemented in TypeScript",
//! and none is implemented in this wrapper either). The methods below match
//! the F8.2 signatures exactly; the F8.2 methods that section 01 does not need
//! are deliberately *not* stubbed.

use wasm_bindgen::prelude::*;

/// Installs the panic hook so a Rust panic surfaces as a JS console error
/// rather than an opaque `unreachable` trap. `wasm_bindgen(start)` makes the
/// glue call it during module init, so JS never has to.
///
/// `js_name` is required, not cosmetic: `wasm-bindgen-test` emits its own
/// `main` export into the integration-test module, and two crates exporting
/// `main` is a hard `wasm-bindgen` error ("the name `main` is exported by
/// multiple crates in this build"). The Rust signature stays `pub fn main()`
/// exactly as task 1.2 specifies; only the JS-side symbol is renamed.
#[wasm_bindgen(start, js_name = sailgymStart)]
pub fn main() {
    console_error_panic_hook::set_once();
}

/// Placeholder simulation handle. The real state machine lands in section 02.
#[wasm_bindgen]
pub struct Sim {
    built: String,
    counter: u32,
}

#[wasm_bindgen]
impl Sim {
    /// Construct from a JSON configuration document.
    ///
    /// Section 01 only validates that the string parses; the configuration
    /// schema itself is defined later. Invalid JSON is an `Err`, so the
    /// boundary's error path is exercised from day one.
    #[wasm_bindgen(constructor)]
    pub fn new(config_json: &str) -> Result<Sim, JsValue> {
        let _config: serde_json::Value = serde_json::from_str(config_json)
            .map_err(|e| JsValue::from_str(&format!("invalid config JSON: {e}")))?;
        Ok(Sim {
            built: env!("CARGO_PKG_VERSION").to_string(),
            counter: 0,
        })
    }

    /// Advance exactly `n` fixed steps. Returns steps actually taken.
    ///
    /// The placeholder takes every requested step, so it always returns `n`.
    pub fn advance(&mut self, n: u32) -> u32 {
        self.counter = self.counter.wrapping_add(n);
        n
    }

    /// Flat `f64` view of the state. Placeholder layout is `[counter]`;
    /// the real F8.3 layout lands in task 2.4.
    pub fn snapshot(&self) -> Box<[f64]> {
        vec![f64::from(self.counter)].into_boxed_slice()
    }

    /// The `sailgym-wasm` crate version this module was built from.
    pub fn version(&self) -> String {
        self.built.clone()
    }
}
