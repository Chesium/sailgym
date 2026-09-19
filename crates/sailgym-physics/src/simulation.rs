//! Top-level `Simulation`: state, parameters, controls and the force model
//! in one owner, driven at a fixed timestep.
//!
//! This is the object the WASM wrapper holds. It reads no wall clock, owns its
//! seed explicitly, and `advance(n)` is exactly `n` calls to `advance(1)`
//! (F9.1, F9.2, F9.7).

use crate::environment::wind::{ProceduralWind, WindConfig};
use crate::environment::WindField;
use crate::forces::{evaluate, ForceBreakdown, WindForces};
use crate::parameters::{BoatParameters, ParamError};
use crate::scenario::{Scenario, ScenarioError};
use crate::stability::capsize::CapsizeState;
use crate::stability::hydrostatics::GzCurve;
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
    /// The force evaluation at the **published** state — the one the next
    /// step's first stage will consume, bit for bit.
    ///
    /// `diagnostics::diagnostics` reads this rather than calling `evaluate`
    /// itself (task 8.1): the displayed forces must be the forces that move
    /// the boat, not a second evaluation that could drift if an argument were
    /// ever passed differently. It is refreshed by every method that can
    /// change it and by nothing else — see [`Simulation::refresh_forces`].
    forces: ForceBreakdown,
}

impl Simulation {
    /// A simulation at rest at the origin, driven by the real force model
    /// (F4.4, section 04).
    pub fn new(params: BoatParameters, seed: u64) -> Self {
        let mut sim = Self {
            state: Self::initial_state(&params),
            params,
            controls: Controls::default(),
            wind: ProceduralWind::new(WindConfig::default(), seed),
            seed,
            steps: 0,
            capsize: CapsizeState::default(),
            forces: ForceBreakdown::default(),
        };
        sim.refresh_forces();
        sim
    }

    /// Re-evaluate the force breakdown at the published state.
    ///
    /// `evaluate` is a pure function of `(state, controls, parameters, field,
    /// t)`, so this is exactly the breakdown the next integration step's first
    /// stage computes. Called from every method that can change one of those
    /// five arguments, and from nowhere else; it never touches the state, so
    /// it cannot perturb a trajectory.
    fn refresh_forces(&mut self) {
        let f = evaluate(
            &self.state,
            &self.controls,
            &self.params,
            &self.wind,
            self.state.t,
        );
        self.forces = f;
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
        self.refresh_forces();
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
        self.refresh_forces();
    }

    /// True wind at the boat's position and simulation time, world frame.
    pub fn wind_at_boat(&self) -> Vec2 {
        self.wind.sample(self.state.x, self.state.y, self.state.t)
    }

    pub fn set_controls(&mut self, c: Controls) {
        self.controls = c;
        self.refresh_forces();
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
        // Once per call, not once per step: the cached breakdown depends only
        // on the *final* state, so this is identical to refreshing inside the
        // loop and costs one evaluation instead of `n`. `advance(n)` therefore
        // still equals `n` calls to `advance(1)` in every observable (F9.7).
        self.refresh_forces();
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

    /// The force breakdown at the published state (task 8.1). This is the
    /// evaluation the next step's first stage consumes; the diagnostics
    /// record publishes it unchanged.
    pub fn forces(&self) -> &ForceBreakdown {
        &self.forces
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

    /// Restore the whole catalogue, for the panel's "Reset to ILCA defaults"
    /// (brief §31).
    ///
    /// The defaults live in `BoatParameters::ilca7()` and are fetched from
    /// there, never rebuilt from a copy the browser kept: a run started from a
    /// scenario with non-default parameters must still reset to the *ILCA*,
    /// not to whatever it happened to load with, and no F7 value may be
    /// duplicated in TypeScript (F7, F8).
    pub fn reset_parameters(&mut self) {
        self.params = BoatParameters::ilca7();
        self.refresh_forces();
    }

    /// Load a scenario: parameters, state, seed, wind and the controls in
    /// force at `t = 0`, in that order (section 09, task 9.1).
    ///
    /// The whole load is validated **before** anything is committed, through
    /// `Scenario::validate`, so a rejected scenario leaves the running
    /// simulation exactly as it was. That mirrors `set_parameter`
    /// (section 08 handoff §2.1): an invalid catalogue must never reach the
    /// equations of motion, and a half-applied scenario is worse than none.
    ///
    /// The scenario is an initial condition and an environment. It sets no
    /// future input, and nothing here reads it again after this call
    /// (brief §32).
    pub fn load_scenario(&mut self, sc: &Scenario) -> Result<(), ScenarioError> {
        sc.validate()?;
        let params = sc.to_parameters()?;
        self.params = params;
        self.restart_scenario(sc)
    }

    /// Restart from a scenario's initial condition **keeping the parameter
    /// catalogue currently in force**.
    ///
    /// This is what the Reset button and the `R` key do, and the distinction
    /// from [`Simulation::load_scenario`] is deliberate: brief §31 makes the
    /// whole catalogue live-editable and section 08 made a `sim.*` edit
    /// *reset-required*, so a reset that threw the edit away would make that
    /// edit unreachable. Choosing a scenario in the picker is the other case
    /// — the scenario's own overrides are the point of choosing it — and
    /// that goes through `load_scenario`.
    pub fn restart_scenario(&mut self, sc: &Scenario) -> Result<(), ScenarioError> {
        sc.validate()?;
        let wind = sc.wind;
        wind.validate()
            .map_err(|e| ScenarioError::Wind(e.to_string()))?;

        // `reset` rebuilds the wind field from the seed and refreshes the
        // cached forces; `set_wind` then installs the scenario's own
        // configuration against the same seed.
        self.reset(sc.to_boat_state(), sc.seed);
        self.set_wind(wind);
        self.set_controls(sc.to_controls());
        Ok(())
    }

    /// Replace the whole catalogue, validating first.
    ///
    /// The scenario loader and the WASM wrapper both need this; it is the
    /// bulk sibling of [`Simulation::set_parameter`] and applies the same two
    /// checks (F7 consistency, and that the F6.7 `GZ` curve fits).
    pub fn set_parameters(&mut self, p: BoatParameters) -> Result<(), ParamError> {
        p.validate()?;
        GzCurve::fit(
            p.stability.gm,
            p.stability.phi_peak,
            p.stability.gz_max,
            p.stability.phi_vanish,
        )?;
        self.params = p;
        self.refresh_forces();
        Ok(())
    }

    /// Live parameter editing (F8.2, brief §31). Returns whether the change
    /// requires a reset.
    ///
    /// The edit is applied to a **copy** and only committed once the whole
    /// catalogue still validates and the F6.7 `GZ` curve still fits. That
    /// check is not decoration: `BoatParameters::set_path` has never
    /// validated, `GzCurve::from_params` (which the equations of motion use)
    /// cannot fail, and a `stability` group that `fit` rejects yields the zero
    /// curve — a boat with no righting arm at all, silently (section 07
    /// handoff §8.3). A rejected edit leaves the running simulation exactly as
    /// it was and hands the caller the reason, which brief §31 requires the
    /// panel to show.
    pub fn set_parameter(&mut self, path: &str, value: f64) -> Result<bool, ParamError> {
        let mut probe = self.params;
        let reset_required = probe.set_path(path, value)?;
        probe.validate()?;
        GzCurve::fit(
            probe.stability.gm,
            probe.stability.phi_peak,
            probe.stability.gz_max,
            probe.stability.phi_vanish,
        )?;
        self.params = probe;
        self.refresh_forces();
        Ok(reset_required)
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
