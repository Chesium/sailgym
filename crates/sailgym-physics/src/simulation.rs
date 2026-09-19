//! Top-level `Simulation`: state, parameters, controls and the force model
//! in one owner, driven at a fixed timestep.
//!
//! This is the object the WASM wrapper holds. It reads no wall clock, owns its
//! seed explicitly, and `advance(n)` is exactly `n` calls to `advance(1)`
//! (F9.1, F9.2, F9.7).

use crate::environment::wind::{ProceduralWind, WindConfig};
use crate::environment::WindField;
use crate::forces::WindForces;
use crate::parameters::{BoatParameters, ParamError};
use crate::stability::capsize::CapsizeState;
use crate::state::{BoatState, Controls};
use crate::vec::Vec2;

/// The simulator.
pub struct Simulation {
    state: BoatState,
    params: BoatParameters,
    controls: Controls,
    wind: ProceduralWind,
    seed: u64,
    steps: u64,
    capsize: CapsizeState,
}

impl Simulation {
    /// A simulation at rest at the origin, driven by the real force model
    /// (F4.4, section 04).
    pub fn new(params: BoatParameters, seed: u64) -> Self {
        Self {
            state: Self::initial_state(&params),
            params,
            controls: Controls::default(),
            wind: ProceduralWind::new(WindConfig::default(), seed),
            seed,
            steps: 0,
            capsize: CapsizeState::default(),
        }
    }

    /// The state a fresh or reset simulation starts from: at rest at the
    /// origin, sheeted to the shortest available length.
    pub fn initial_state(params: &BoatParameters) -> BoatState {
        BoatState {
            l_sheet: params.sheet.l_sheet_min,
            ..BoatState::ZERO
        }
    }

    /// Restart from `state` with a new seed. The step counter and the
    /// controls are cleared, so a reset is bit-reproducible (brief §34).
    pub fn reset(&mut self, state: BoatState, seed: u64) {
        self.state = state;
        self.seed = seed;
        self.steps = 0;
        self.controls = Controls::default();
        self.capsize = CapsizeState::default();
        // The wind is procedural, not stateful, but it is *seeded*: rebuilding
        // it here is what makes a reset with a new seed give a new field, and
        // a reset with the same seed reproduce the old one bit for bit
        // (brief §34).
        self.wind = ProceduralWind::new(*self.wind.config(), seed);
    }

    /// The wind field. Section 03 samples it for the visualization and the
    /// HUD; the sail starts reading it in section 05.
    pub fn wind(&self) -> &ProceduralWind {
        &self.wind
    }

    /// Replace the wind field, keeping the boat state and the current seed.
    ///
    /// This is how the mode can be switched at runtime without a reload
    /// (section 03 acceptance criterion 5). It is not a parameter edit: the
    /// wind is environment, not boat (F7).
    pub fn set_wind(&mut self, cfg: WindConfig) {
        self.wind = ProceduralWind::new(cfg, self.seed);
    }

    /// True wind at the boat's position and simulation time, world frame.
    pub fn wind_at_boat(&self) -> Vec2 {
        self.wind.sample(self.state.x, self.state.y, self.state.t)
    }

    pub fn set_controls(&mut self, c: Controls) {
        self.controls = c;
    }

    /// Advance exactly `n` fixed steps of `params.sim.dt`. Returns the number
    /// of steps actually taken.
    pub fn advance(&mut self, n: u32) -> u32 {
        let dt = self.params.sim.dt;
        let method = self.params.sim.integrator;
        for _ in 0..n {
            self.state = crate::integrator::step(
                &self.state,
                &self.controls,
                &self.params,
                &WindForces { wind: &self.wind },
                dt,
                method,
            );
            self.steps += 1;
            // Once per **completed** step, and only here (F6.10). The capsize
            // report is an observer of the trajectory: driving it from inside
            // `derivative` would feed it the integrator's intermediate stages,
            // which are not states the boat ever occupies, and would make
            // `advance(n)` differ from `n` calls to `advance(1)` (F9.7).
            // Nothing reads it back — see `stability::capsize`.
            self.capsize.observe(&self.state, &self.params);
        }
        n
    }

    pub fn state(&self) -> &BoatState {
        &self.state
    }

    pub fn params(&self) -> &BoatParameters {
        &self.params
    }

    pub fn controls(&self) -> &Controls {
        &self.controls
    }

    /// The capsize report (F6.10). Informational: written by [`Self::advance`],
    /// read by the diagnostics and the UI, and read by no force model.
    pub fn capsize(&self) -> &CapsizeState {
        &self.capsize
    }

    /// The seed every procedural source in the simulation derives from
    /// (F9.2). The wind field is the first consumer.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Steps taken since the last reset.
    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// Live parameter editing (F8.2, brief §31). Returns whether the change
    /// requires a reset.
    pub fn set_parameter(&mut self, path: &str, value: f64) -> Result<bool, ParamError> {
        self.params.set_path(path, value)
    }
}

/// Asserts that [`BoatState::to_array`] indexes the F8.3 layout by name, and
/// that `web/src/sim/snapshot.ts` mirrors the same list in the same order.
///
/// Lives at file scope rather than inside a `mod tests`, so that the test path
/// is literally `simulation::snapshot_layout`.
#[cfg(test)]
#[test]
fn snapshot_layout() {
    use crate::state::{STATE_FIELDS, STATE_LEN};

    // The documented F8.3 order, written out independently of `state.rs`.
    const DOCUMENTED: [&str; STATE_LEN] = [
        "x", "y", "psi", "phi", "u", "v", "r", "p", "beta", "beta_dot", "delta_r", "l_sheet", "t",
    ];
    assert_eq!(STATE_FIELDS, DOCUMENTED);

    // Each index carries the field it is documented to carry.
    let a: [f64; STATE_LEN] = std::array::from_fn(|i| i as f64);
    let s = BoatState::from_array(&a);
    assert_eq!(s.x, 0.0);
    assert_eq!(s.y, 1.0);
    assert_eq!(s.psi, 2.0);
    assert_eq!(s.phi, 3.0);
    assert_eq!(s.u, 4.0);
    assert_eq!(s.v, 5.0);
    assert_eq!(s.r, 6.0);
    assert_eq!(s.p, 7.0);
    assert_eq!(s.beta, 8.0);
    assert_eq!(s.beta_dot, 9.0);
    assert_eq!(s.delta_r, 10.0);
    assert_eq!(s.l_sheet, 11.0);
    assert_eq!(s.t, 12.0);
    assert_eq!(s.to_array(), a);

    // F8.3: the TypeScript accessor mirrors the same list. The physics crate
    // must still build and test on its own, so a missing web tree skips.
    let ts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/src/sim/snapshot.ts");
    let Ok(src) = std::fs::read_to_string(&ts) else {
        eprintln!("skip: {} not present", ts.display());
        return;
    };
    let list = src
        .split_once("SNAPSHOT_FIELDS = [")
        .expect("snapshot.ts must declare SNAPSHOT_FIELDS")
        .1
        .split_once(']')
        .expect("SNAPSHOT_FIELDS must be a closed array literal")
        .0;
    let ts_fields: Vec<String> = list
        .split(',')
        .map(|s| s.trim().trim_matches('\'').trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .map(|camel| {
            // camelCase -> snake_case
            let mut out = String::new();
            for ch in camel.chars() {
                if ch.is_ascii_uppercase() {
                    out.push('_');
                    out.push(ch.to_ascii_lowercase());
                } else {
                    out.push(ch);
                }
            }
            out
        })
        .collect();
    assert_eq!(
        ts_fields,
        DOCUMENTED.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        "web/src/sim/snapshot.ts disagrees with the F8.3 layout"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_simulation_is_at_rest_and_valid() {
        let sim = Simulation::new(BoatParameters::ilca7(), 7);
        assert_eq!(sim.seed(), 7);
        assert_eq!(sim.steps(), 0);
        assert_eq!(sim.state().u, 0.0);
        assert_eq!(sim.state().t, 0.0);
        assert!(sim.params().validate().is_ok());
    }

    #[test]
    fn advance_counts_and_returns_its_steps() {
        let mut sim = Simulation::new(BoatParameters::ilca7(), 1);
        assert_eq!(sim.advance(10), 10);
        assert_eq!(sim.steps(), 10);
        assert!((sim.state().t - 10.0 * sim.params().sim.dt).abs() < 1e-12);
        assert_eq!(sim.advance(0), 0);
        assert_eq!(sim.steps(), 10);
    }

    #[test]
    fn reset_clears_controls_and_the_step_counter() {
        let mut sim = Simulation::new(BoatParameters::ilca7(), 1);
        sim.set_controls(Controls {
            rudder_rate_cmd: 1.0,
            ..Controls::default()
        });
        sim.advance(100);
        let start = Simulation::initial_state(sim.params());
        sim.reset(start, 99);
        assert_eq!(sim.steps(), 0);
        assert_eq!(sim.seed(), 99);
        assert_eq!(*sim.controls(), Controls::default());
        assert_eq!(sim.state().to_array(), start.to_array());
    }

    #[test]
    fn wind_is_reseeded_by_reset_and_replaceable_at_runtime() {
        use crate::environment::wind::WindMode;

        let mut sim = Simulation::new(BoatParameters::ilca7(), 1);
        let before = sim.wind_at_boat();

        // A reset with the same seed reproduces the same field, bit for bit.
        sim.reset(Simulation::initial_state(sim.params()), 1);
        assert_eq!(sim.wind_at_boat().x.to_bits(), before.x.to_bits());

        // A different seed gives a different field.
        sim.reset(Simulation::initial_state(sim.params()), 2);
        assert_ne!(sim.wind_at_boat(), before);

        // Switching mode at runtime leaves the boat alone.
        let state = *sim.state();
        sim.set_wind(WindConfig {
            mode: WindMode::Uniform,
            ..*sim.wind().config()
        });
        assert_eq!(sim.wind().mode_count(), 0);
        assert_eq!(sim.state().to_array(), state.to_array());
    }

    #[test]
    fn set_parameter_reports_whether_a_reset_is_needed() {
        let mut sim = Simulation::new(BoatParameters::ilca7(), 1);
        assert_eq!(sim.set_parameter("sail.area", 8.0), Ok(false));
        assert_eq!(sim.params().get_path("sail.area"), Ok(8.0));
        assert_eq!(sim.set_parameter("sim.dt", 0.01), Ok(true));
        assert!(sim.set_parameter("nope", 1.0).is_err());
    }
}
