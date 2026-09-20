#![allow(dead_code)]
//! The golden control scripts, and the runner that turns one into a
//! trajectory (task 9.6).
//!
//! **The scripts live here, not in the scenarios.** brief §32 forbids a
//! scenario from scripting anything; a regression test, on the other hand,
//! needs a fixed input sequence or it is not a regression test. Keeping the
//! two apart is the whole point of this file.
//!
//! It is `#[path]`-included by exactly two consumers, so there is one
//! definition and not two that can drift:
//!
//! * `crates/sailgym-physics/tests/regression.rs` — compares against the
//!   committed goldens;
//! * `crates/sailgym-bench/src/bin/gen_golden.rs` — writes them.
//!
//! Cues are indexed by **step**, not by a floating-point time compared with
//! an accumulated clock: `advance` is exact in step count and a rounding
//! difference in a cue boundary would be a silent one-step trajectory change.

use sailgym_physics::identity::ModelIdentity;
use sailgym_physics::recording::ToolchainInfo;
use sailgym_physics::scenario::Scenario;
use sailgym_physics::simulation::Simulation;
use sailgym_physics::state::{Controls, STATE_LEN};

/// Simulated seconds each golden trajectory covers.
pub const GOLDEN_DURATION_S: f64 = 30.0;

/// Samples of simulated time per second in a golden file.
pub const GOLDEN_SAMPLE_HZ: f64 = 5.0;

/// Absolute tolerance on positions and angles (task 9.6).
pub const TOL_POSITION: f64 = 1e-9;

/// Absolute tolerance on body-frame velocities and rates (task 9.6).
pub const TOL_VELOCITY: f64 = 1e-10;

/// A control change, applied at the start of the step at time `t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cue {
    /// s, simulated time at which the command takes effect.
    pub t: f64,
    /// normalised `[−1, 1]`; `+1` steers the bow to starboard (F3).
    pub rudder_rate: f64,
    /// normalised `[−1, 1]`; `+1` eases, `−1` hauls (F3).
    pub sheet_rate: f64,
    /// Ease at the release rate, overriding `sheet_rate` (F3).
    pub release: bool,
}

impl Cue {
    const fn new(t: f64, rudder_rate: f64, sheet_rate: f64, release: bool) -> Self {
        Self {
            t,
            rudder_rate,
            sheet_rate,
            release,
        }
    }

    pub fn to_controls(self) -> Controls {
        Controls {
            rudder_rate_cmd: self.rudder_rate,
            sheet_rate_cmd: self.sheet_rate,
            sheet_release: self.release,
        }
    }
}

/// Haul hard and hold, then a touch of weather helm. brief §46 steps 4–8.
const BEAM_REACH_CAPSIZE: [Cue; 4] = [
    Cue::new(0.0, 0.0, -1.0, false),
    Cue::new(5.0, 0.0, 0.0, false),
    Cue::new(12.0, 0.4, 0.0, false),
    Cue::new(18.0, 0.0, 0.0, false),
];

/// Identical for four seconds, then the human does the other thing:
/// brief §46 steps 11–16.
///
/// **v2 section 08 moved the release cue from 8 s to 4 s.** The corrected
/// `GZ` curve (F18.1a) holds `GZ_max` out to `φ_p = 45°` where v1's had
/// already peaked at 32.5° and was falling, so the boat carries more righting
/// through the middle of the curve and the scenario's wind moved with it:
/// 9 m/s, measured in `docs/v2/physics-validation.md` §4.6. At that wind the
/// window is real rather than notional — released at 4 s the boat recovers
/// from 65° of heel, released at 8 s it is already past 73° and goes over —
/// and a "recovery" fixture that capsized would be a false label on a
/// regression file.
const SHEET_RELEASE_RECOVERY: [Cue; 4] = [
    Cue::new(0.0, 0.0, -1.0, false),
    Cue::new(4.0, 0.0, 0.0, true),
    Cue::new(8.0, 0.0, 0.0, false),
    Cue::new(20.0, 0.0, -0.5, false),
];

/// Settle, then steer up and trim.
const CLOSE_HAULED: [Cue; 5] = [
    Cue::new(0.0, 0.0, 0.0, false),
    Cue::new(6.0, -0.3, 0.0, false),
    Cue::new(10.0, 0.0, 0.0, false),
    Cue::new(16.0, 0.0, -0.4, false),
    Cue::new(20.0, 0.0, 0.0, false),
];

/// Steer through the wind and back again (brief §46's second demonstration).
const TACK: [Cue; 5] = [
    Cue::new(0.0, 0.0, 0.0, false),
    Cue::new(4.0, -1.0, 0.0, false),
    Cue::new(8.0, 0.0, 0.0, false),
    Cue::new(14.0, 0.6, 0.0, false),
    Cue::new(18.0, 0.0, 0.0, false),
];

/// Bear away through dead downwind, then haul and ease across the gybe
/// (brief §46's third demonstration, both handlings).
const GYBE: [Cue; 5] = [
    Cue::new(0.0, 0.0, 0.0, false),
    Cue::new(5.0, 0.8, 0.0, false),
    Cue::new(10.0, 0.0, 0.0, false),
    Cue::new(15.0, 0.0, -1.0, false),
    Cue::new(20.0, 0.0, 1.0, false),
];

/// The sandbox gets one of everything: haul, steer, release.
const FREE_SAIL: [Cue; 6] = [
    Cue::new(0.0, 0.0, -1.0, false),
    Cue::new(6.0, 0.0, 0.0, false),
    Cue::new(10.0, 0.5, 0.0, false),
    Cue::new(15.0, 0.0, 0.0, false),
    Cue::new(20.0, 0.0, 0.0, true),
    Cue::new(24.0, 0.0, 0.0, false),
];

/// The fixed 30 s control script for one scenario.
///
/// Each one is chosen to put the rig through something the scenario is
/// *about*, so a physics change shows up: the two beam-reach scripts load and
/// unload the sheet, `tack` and `gybe` steer the boat through the wind, and
/// `free_sail` exercises haul, steer and release in turn.
pub fn golden_script(scenario: &str) -> &'static [Cue] {
    match scenario {
        "beam_reach_capsize" => &BEAM_REACH_CAPSIZE,
        "sheet_release_recovery" => &SHEET_RELEASE_RECOVERY,
        "close_hauled" => &CLOSE_HAULED,
        "tack" => &TACK,
        "gybe" => &GYBE,
        _ => &FREE_SAIL,
    }
}

/// One golden trajectory: the flat F3 state at every sample point.
///
/// Sample zero is the scenario's own initial state, so a golden file pins the
/// initial condition as well as the dynamics.
pub fn run_golden(sc: &Scenario) -> Vec<[f64; STATE_LEN]> {
    let params = sc
        .to_parameters()
        .unwrap_or_else(|e| panic!("{}: {e}", sc.name));
    let mut sim = Simulation::new(params, sc.seed);
    sim.load_scenario(sc)
        .unwrap_or_else(|e| panic!("{}: {e}", sc.name));

    let dt = params.sim.dt;
    let steps = (GOLDEN_DURATION_S / dt).round() as u64;
    let cues = golden_script(&sc.name);
    let cue_steps: Vec<u64> = cues.iter().map(|c| (c.t / dt).round() as u64).collect();

    // Sampling is counted in **steps**, not in accumulated simulated time:
    // `t >= n/hz` with a tolerance double-counts a sample whose time lands a
    // few ULP short of the boundary, and without one it drops it. The step
    // stride is exact and needs neither.
    let every = (1.0 / (GOLDEN_SAMPLE_HZ * dt)).round().max(1.0) as u64;

    let mut out = Vec::new();
    let mut next_cue = 0usize;

    for i in 0..=steps {
        while next_cue < cues.len() && cue_steps[next_cue] == i {
            sim.set_controls(cues[next_cue].to_controls());
            next_cue += 1;
        }
        if i % every == 0 {
            out.push(sim.state().to_array());
        }
        if i < steps {
            sim.advance(1);
        }
    }
    out
}

/// A committed golden trajectory.
///
/// `toolchain` is R7's whole point: a golden file is only valid for the build
/// that produced it, so the file says which build that was and the comparison
/// **skips with a clear message** on a mismatch rather than failing.
///
/// `identity` and `declared_changes` are v2 F18.1d's half of the same idea, and
/// they answer a different question: not "which compiler" but "which
/// equations". A golden generated from a tree git cannot identify — which is
/// unavoidable for the commit that *introduces* a physics correction, since the
/// goldens and the source land together — says so in the file, and says what
/// was being changed. `regression.rs` prints both and refuses a
/// non-baseline identity that declares nothing.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Golden {
    pub schema_version: u32,
    pub scenario: String,
    pub toolchain: ToolchainInfo,
    /// The model/source identity of the build that wrote this file (F18.1d).
    pub identity: ModelIdentity,
    /// What the generator was told it was changing, when the source tree was
    /// not a baseline. Empty when the tree was clean.
    #[serde(default)]
    pub declared_changes: String,
    pub dt: f64,
    pub duration_s: f64,
    pub sample_hz: f64,
    /// The exact script that produced it, recorded so the file is readable on
    /// its own. The scripts above remain the source of truth.
    pub script: Vec<[f64; 4]>,
    /// Field names of each sample, in F8.3 order.
    pub fields: Vec<String>,
    /// One `[f64; STATE_LEN]` per sample point.
    pub samples: Vec<Vec<f64>>,
}

/// The only golden-file schema this build writes or reads.
///
/// Bumped to 2 by v2 section 08: the file now carries the model/source
/// identity of the build that wrote it (F18.1d), which a schema-1 reader would
/// silently ignore.
pub const GOLDEN_SCHEMA_VERSION: u32 = 2;

/// Build the committed form of a golden trajectory.
pub fn golden_for(sc: &Scenario, declared_changes: &str) -> Golden {
    let params = sc
        .to_parameters()
        .unwrap_or_else(|e| panic!("{}: {e}", sc.name));
    Golden {
        schema_version: GOLDEN_SCHEMA_VERSION,
        scenario: sc.name.clone(),
        toolchain: ToolchainInfo::current(),
        identity: ModelIdentity::current(),
        declared_changes: declared_changes.to_string(),
        dt: params.sim.dt,
        duration_s: GOLDEN_DURATION_S,
        sample_hz: GOLDEN_SAMPLE_HZ,
        script: golden_script(&sc.name)
            .iter()
            .map(|c| {
                [
                    c.t,
                    c.rudder_rate,
                    c.sheet_rate,
                    if c.release { 1.0 } else { 0.0 },
                ]
            })
            .collect(),
        fields: sailgym_physics::state::STATE_FIELDS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        samples: run_golden(sc).iter().map(|s| s.to_vec()).collect(),
    }
}
