#![allow(dead_code)]
//! The conformance bundle: what is in it, and the one definition of how it is
//! computed (v2 F16, section 02 task 2.4).
//!
//! A conformance bundle is a language-neutral data artifact generated from
//! `sailgym-physics` that a **second implementation** can be tested against.
//! This module is its schema *and* its generator, and it is
//! `#[path]`-included by exactly two consumers, so there is one definition
//! and not two that can drift:
//!
//! * `crates/sailgym-bench/src/bin/gen_conformance.rs` — writes the bundle;
//! * `crates/sailgym-physics/tests/conformance.rs` — reads the committed
//!   bundle back, recomputes every output from current source, and compares
//!   bit for bit.
//!
//! That is the same arrangement `tests/golden/script.rs` already has with
//! `gen_golden.rs`, and it is a textual include rather than a crate
//! dependency: a physics test may not depend on the bench crate, which
//! depends on it. The one piece that genuinely has to be a library — the
//! `.npy` codec — lives in `sailgym_physics::testkit::npy` for exactly that
//! reason.
//!
//! ## What the bundle is, in one paragraph
//!
//! Every fixture is a 2-D `<f8` matrix whose leading columns are **inputs**
//! and whose trailing columns are **what this crate produced from them**. A
//! port feeds the inputs to its own implementation and compares its outputs
//! against the recorded ones, to the tolerance its tier carries. The column
//! names are the contract and live in `manifest.json`: a port that reads
//! columns by position is one insertion away from silently comparing the
//! wrong thing.
//!
//! ## What it is not
//!
//! Conformance is **implementation agreement, not physical validation**
//! (F16.2). Two implementations that agree to the last bit may share a
//! modelling mistake. Nothing in this bundle says the boat is right; it says
//! the port is the same boat.
//!
//! ## No equality across stacks
//!
//! F16.1: XLA reassociates, an FMA is not a multiply then an add, GPU
//! reductions are not order-deterministic and JAX defaults to f32. **No
//! conformance test may assert equality across stacks.** The Rust runner is
//! the exception that proves it: Rust against Rust on the same build is F9's
//! territory and is compared exactly, because the thing it is testing is not
//! a port but a **stale bundle**.

pub mod samplers;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use sailgym_physics::aero::apparent::{apparent_wind_at, apparent_wind_cg, true_wind_body};
use sailgym_physics::aero::sail::sail_load;
use sailgym_physics::constants::{RHO_AIR, RHO_WATER};
use sailgym_physics::digest::{
    BundleIdentity, FixtureId, WindFixtureId, BUNDLE_SCHEMA_VERSION, TOLERANCE_CONTRACT_VERSION,
};
use sailgym_physics::dynamics::derivative;
use sailgym_physics::environment::wind::{ProceduralWind, WindConfig, WindMode};
use sailgym_physics::environment::WindField;
use sailgym_physics::foil::{angle_of_attack, cd, cl, foil_force, smoothstep, FoilParams};
use sailgym_physics::forces::WindForces;
use sailgym_physics::frames::wrap_pi;
use sailgym_physics::hydro::centerboard::{foil_hydro_load, local_flow};
use sailgym_physics::hydro::hull::hull_loads;
use sailgym_physics::identity::ModelIdentity;
use sailgym_physics::parameters::BoatParameters;
use sailgym_physics::recording::ToolchainInfo;
use sailgym_physics::rigging::mainsheet::{
    boom_attach_point, drope_dbeta, rope_path_length, sheet_output,
};
use sailgym_physics::scenario::{load_shipped, shipped_names, Scenario};
use sailgym_physics::simulation::Simulation;
use sailgym_physics::stability::hydrostatics::{righting_moment, GzCurve, GzRepresentation};
use sailgym_physics::state::StateDot;
use sailgym_physics::state::{BoatState, Controls, STATE_FIELDS, STATE_LEN};
use sailgym_physics::testkit::npy;
use sailgym_physics::vec::{Vec2, Vec3};

use samplers::{
    actuator_boundaries, around, background_states, halton, halton_span, phi_unwrapped,
    sheet_slack_boundary, stall_blend_edge, wrap_pi_edge, zero_flow_eps, HALTON_BACKGROUND,
    LIMIT_RATE_BOUNDARY, PHI_UNWRAPPED, RUDDER_SELF_CENTRE_ZERO, SHEET_RELEASE_PRECEDENCE,
    SHEET_SLACK_BOUNDARY, STALL_BLEND_EDGE, WRAP_PI_EDGE, ZERO_FLOW_EPS,
};

/// The generator's own version.
///
/// Bumped by hand whenever this module changes **what it emits** — a new
/// fixture, a new column, a different sampler, a different derivation. It is
/// part of the bundle key, so a bump renames the directory and the old bundle
/// stops being accepted, which is the point.
pub const GENERATOR_VERSION: u32 = 1;

/// The command that writes the bundle. Printed in the manifest and in the
/// generated document, so neither can be reproduced by guesswork.
pub const GENERATOR_COMMAND: &str = "cargo run --release -p sailgym-bench --bin gen_conformance";

/// Simulated seconds in each shipped-scenario tier-2 case.
///
/// F16.2 asks for 1–5 s. Two seconds of every shipped scenario plus five of
/// the sheet transient is what fits inside the 2 MB budget of task 2.4 while
/// sampling **every physics step**, which is what makes tier 2 a trajectory
/// fixture rather than a sparse one. The horizon is a budget decision and is
/// recorded as one; nothing physical chose it.
pub const TIER2_SCENARIO_SECONDS: f64 = 2.0;

/// Simulated seconds in the dedicated sheet-transient case.
///
/// The full five, because this is the case the bundle exists to stress:
/// `k_sheet = 2e4 N/m` with `I_b = 12 kg·m²` gives `ω ≈ 41 rad/s`, about 30
/// steps per period at `dt = 0.005`. F11's R1 names it the stiffest mode and
/// the likeliest blow-up, and v2 F18.1b made its tension law discontinuous at
/// take-up — so it is where a lower-precision port degrades first.
pub const TIER2_TRANSIENT_SECONDS: f64 = 5.0;

/// The hard size budget on the whole bundle, in bytes (task 2.4).
///
/// Asserted by the generator itself rather than by review: RV9 is "the
/// committed bundle grows without bound as tiers are added", and a budget
/// nobody checks is a wish.
pub const MAX_BUNDLE_BYTES: u64 = 2_000_000;

// ---------------------------------------------------------------------------
// Context: the one catalogue and the wind fields, shipped as data
// ---------------------------------------------------------------------------

/// One wind field in the bundle, identified by name.
///
/// F16.7: `ProceduralWind::new` draws `κ_k`, `ϕ_k`, `ω_k` once at
/// construction and `sample` is a pure sum over the result, so the drawn
/// table travels in `wind_modes.json` and **no stack other than Rust ever
/// implements PCG32**.
pub struct WindEntry {
    pub name: String,
    pub seed: u64,
    pub config: WindConfig,
    pub field: ProceduralWind,
}

/// The serialised form of one wind field, as `wind_modes.json` carries it.
#[derive(Serialize)]
struct WindEntryOut<'a> {
    name: &'a str,
    seed: u64,
    config: &'a WindConfig,
    modes: &'a [sailgym_physics::environment::wind::WindMode3],
}

/// Everything a fixture's `compute` may read. Pure: nothing here is mutated
/// while fixtures are evaluated.
pub struct Context {
    /// The single catalogue the bundle covers (`ilca7`; no shipped scenario
    /// overrides a parameter, which `gen_conformance` asserts).
    pub params: BoatParameters,
    /// The wind fields, in a fixed order; the `field` input column indexes
    /// this list.
    pub winds: Vec<WindEntry>,
}

impl Context {
    /// Build the context. Deterministic, and allocates the wind tables once.
    pub fn new() -> Self {
        let params = BoatParameters::ilca7();
        let mut winds = Vec::new();

        // Three synthetic fields, one per F6.1 mode, so the bundle exercises
        // the uniform fast path, the frozen perturbation and the gust.
        let base = WindConfig::default();
        winds.push(entry(
            "synthetic_uniform",
            0,
            WindConfig {
                mode: WindMode::Uniform,
                ..base
            },
        ));
        winds.push(entry(
            "synthetic_spatial",
            4242,
            WindConfig {
                mode: WindMode::Spatial,
                ..base
            },
        ));
        winds.push(entry(
            "synthetic_gust",
            7,
            WindConfig {
                mode: WindMode::Gust,
                ..base
            },
        ));

        // And every shipped scenario's own field, so a port can reproduce the
        // tier-2 trajectories without parsing `scenarios/`.
        for name in shipped_names() {
            let sc = load_shipped(name).unwrap_or_else(|e| panic!("{name}: {e}"));
            winds.push(entry(&format!("scenario_{name}"), sc.seed, sc.wind));
        }

        Self { params, winds }
    }

    fn wind(&self, index: f64) -> &WindEntry {
        &self.winds[index as usize]
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

fn entry(name: &str, seed: u64, config: WindConfig) -> WindEntry {
    WindEntry {
        name: name.to_string(),
        seed,
        config,
        field: ProceduralWind::new(config, seed),
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// How a fixture's outputs are produced from its inputs.
///
/// A plain `fn` pointer, not a closure: the runner and the generator have to
/// call literally the same code, and a captured environment would be a place
/// for the two to differ.
pub type ComputeFn = fn(&Context, &[f64], usize) -> Vec<f64>;

/// One `.npy` in the bundle.
pub struct Fixture {
    pub name: &'static str,
    pub tier: u32,
    /// The F16.3 samplers that contributed rows, in order.
    pub samplers: Vec<String>,
    pub input_columns: Vec<String>,
    pub output_columns: Vec<String>,
    /// Flat, `rows × input_columns.len()`, row-major.
    pub inputs: Vec<f64>,
    pub compute: ComputeFn,
}

impl Fixture {
    pub fn rows(&self) -> usize {
        if self.input_columns.is_empty() {
            0
        } else {
            self.inputs.len() / self.input_columns.len()
        }
    }

    /// Recompute the outputs from current source.
    pub fn outputs(&self, ctx: &Context) -> Vec<f64> {
        let out = (self.compute)(ctx, &self.inputs, self.input_columns.len());
        assert_eq!(
            out.len(),
            self.rows() * self.output_columns.len(),
            "{}: compute returned {} values for {} rows × {} columns",
            self.name,
            out.len(),
            self.rows(),
            self.output_columns.len()
        );
        out
    }

    /// The whole matrix: inputs then outputs, row by row.
    pub fn matrix(&self, ctx: &Context) -> (usize, usize, Vec<f64>) {
        let n_in = self.input_columns.len();
        let n_out = self.output_columns.len();
        let out = self.outputs(ctx);
        let rows = self.rows();
        let mut data = Vec::with_capacity(rows * (n_in + n_out));
        for r in 0..rows {
            data.extend_from_slice(&self.inputs[r * n_in..(r + 1) * n_in]);
            data.extend_from_slice(&out[r * n_out..(r + 1) * n_out]);
        }
        (rows, n_in + n_out, data)
    }
}

/// Apply a pure row function over the flat input block.
fn map_rows(inputs: &[f64], n_in: usize, f: impl Fn(&[f64]) -> Vec<f64>) -> Vec<f64> {
    let mut out = Vec::new();
    for row in inputs.chunks_exact(n_in) {
        out.extend(f(row));
    }
    out
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

fn state_columns() -> Vec<String> {
    names(&STATE_FIELDS)
}

fn state_from(row: &[f64]) -> BoatState {
    let mut a = [0.0; STATE_LEN];
    a.copy_from_slice(&row[..STATE_LEN]);
    BoatState::from_array(&a)
}

fn push_vec3(out: &mut Vec<f64>, v: Vec3) {
    out.push(v.x);
    out.push(v.y);
    out.push(v.z);
}

fn dot_to_vec(d: &StateDot) -> Vec<f64> {
    vec![
        d.x, d.y, d.psi, d.phi, d.u, d.v, d.r, d.p, d.beta, d.beta_dot, d.delta_r, d.l_sheet, d.t,
    ]
}

/// The nine `FoilSection` fields, as input columns.
fn foil_param_columns() -> Vec<String> {
    names(&[
        "area",
        "ar",
        "alpha_stall",
        "stall_blend",
        "cn_max",
        "cd0",
        "oswald",
        "alpha_camber",
        "camber_blend",
    ])
}

fn foil_params_from(row: &[f64]) -> FoilParams {
    FoilParams {
        area: row[0],
        ar: row[1],
        alpha_stall: row[2],
        stall_blend: row[3],
        cn_max: row[4],
        cd0: row[5],
        oswald: row[6],
        alpha_camber: row[7],
        camber_blend: row[8],
    }
}

fn foil_params_values(p: &FoilParams) -> [f64; 9] {
    [
        p.area,
        p.ar,
        p.alpha_stall,
        p.stall_blend,
        p.cn_max,
        p.cd0,
        p.oswald,
        p.alpha_camber,
        p.camber_blend,
    ]
}

/// Every fixture in the bundle, in a fixed order (F9.3/F9.4: the order is
/// part of the artifact).
pub fn fixtures(ctx: &Context) -> Vec<Fixture> {
    vec![
        tier0_wrap_pi(),
        tier0_wave(),
        tier0_wind_sample(ctx),
        tier0_apparent(ctx),
        tier0_foil(ctx),
        tier0_sail(ctx),
        tier0_sheet(ctx),
        tier0_gz(ctx),
        tier0_hydro(ctx),
        tier1_derivative(ctx),
        tier2_trajectories(),
    ]
}

// --- tier 0 ----------------------------------------------------------------

fn tier0_wrap_pi() -> Fixture {
    Fixture {
        name: "tier0_wrap_pi",
        tier: 0,
        samplers: names(&[WRAP_PI_EDGE, HALTON_BACKGROUND]),
        input_columns: names(&["a"]),
        output_columns: names(&["wrap_pi"]),
        inputs: wrap_pi_edge(),
        compute: |_, inputs, n_in| map_rows(inputs, n_in, |r| vec![wrap_pi(r[0])]),
    }
}

fn tier0_wave() -> Fixture {
    use std::f64::consts::{PI, TAU};
    let mut theta = Vec::new();
    // Both fold seams, exactly and either side of them.
    for c in [0.0, PI / 2.0, -PI / 2.0, PI, -PI, TAU, -TAU] {
        theta.extend(around(c));
    }
    // The whole reduced domain, and far enough out that a one-step reduction
    // would have lost digits — which is the difference the Cody-Waite split
    // exists for and the first thing a port gets wrong.
    theta.extend(halton_span(700, 2, -6.0 * TAU, 6.0 * TAU));
    theta.extend(halton_span(700, 3, -5.0e3, 5.0e3));
    theta.extend(halton_span(400, 5, -1.0e5, 1.0e5));
    Fixture {
        name: "tier0_wave",
        tier: 0,
        samplers: names(&[HALTON_BACKGROUND]),
        input_columns: names(&["theta"]),
        output_columns: names(&["wave"]),
        inputs: theta,
        compute: |_, inputs, n_in| {
            map_rows(inputs, n_in, |r| {
                vec![sailgym_physics::environment::wind::wave(r[0])]
            })
        },
    }
}

fn tier0_wind_sample(ctx: &Context) -> Fixture {
    let mut inputs = Vec::new();
    for (i, w) in ctx.winds.iter().enumerate() {
        let n = if w.field.mode_count() == 0 { 20 } else { 140 };
        for j in 0..n {
            inputs.push(i as f64);
            inputs.push(-2000.0 + 4000.0 * halton(j, 2));
            inputs.push(-2000.0 + 4000.0 * halton(j, 3));
            inputs.push(600.0 * halton(j, 5));
        }
    }
    Fixture {
        name: "tier0_wind_sample",
        tier: 0,
        samplers: names(&[HALTON_BACKGROUND]),
        input_columns: names(&["field", "x", "y", "t"]),
        output_columns: names(&["wx", "wy"]),
        inputs,
        compute: |ctx, inputs, n_in| {
            map_rows(inputs, n_in, |r| {
                let w = ctx.wind(r[0]).field.sample(r[1], r[2], r[3]);
                vec![w.x, w.y]
            })
        },
    }
}

fn tier0_apparent(ctx: &Context) -> Fixture {
    let mut inputs = Vec::new();
    let states = background_states(360, &ctx.params);
    let mut extra: Vec<BoatState> = Vec::new();
    // Heel past inversion, where `R_x(−φ)` changes the sign of the lateral
    // component and a port that fudged a `cos φ` diverges (F6.4).
    for phi in phi_unwrapped() {
        extra.push(BoatState {
            u: 3.0,
            v: 0.4,
            r: 0.2,
            p: 0.3,
            phi,
            ..BoatState::default()
        });
    }
    for (i, st) in states.iter().chain(extra.iter()).enumerate() {
        inputs.extend_from_slice(&st.to_array());
        // Wind and the application point vary with the same low-discrepancy
        // sweep, so every row is a different geometry.
        inputs.push(-12.0 + 24.0 * halton(i, 43));
        inputs.push(-12.0 + 24.0 * halton(i, 47));
        inputs.push(-2.5 + 5.0 * halton(i, 53));
        inputs.push(-1.5 + 3.0 * halton(i, 59));
        inputs.push(-1.0 + 4.0 * halton(i, 61));
    }
    let mut input_columns = state_columns();
    input_columns.extend(names(&["wind_x", "wind_y", "rb_x", "rb_y", "rb_z"]));
    Fixture {
        name: "tier0_apparent",
        tier: 0,
        samplers: names(&[HALTON_BACKGROUND, PHI_UNWRAPPED]),
        input_columns,
        output_columns: names(&[
            "tw_x", "tw_y", "aw_x", "aw_y", "aw_z", "cg_x", "cg_y", "cg_z",
        ]),
        inputs,
        compute: |_, inputs, n_in| {
            map_rows(inputs, n_in, |r| {
                let st = state_from(r);
                let wind = Vec2::new(r[STATE_LEN], r[STATE_LEN + 1]);
                let rb = Vec3::new(r[STATE_LEN + 2], r[STATE_LEN + 3], r[STATE_LEN + 4]);
                let tw = true_wind_body(&st, wind);
                let aw = apparent_wind_at(&st, wind, rb);
                let cg = apparent_wind_cg(&st, wind);
                let mut out = vec![tw.x, tw.y];
                push_vec3(&mut out, aw);
                push_vec3(&mut out, cg);
                out
            })
        },
    }
}

fn tier0_foil(ctx: &Context) -> Fixture {
    let sections = [
        ctx.params.sail.section,
        ctx.params.board.section,
        ctx.params.rudder.section,
    ];
    let mut inputs = Vec::new();
    let mut push =
        |edge0: f64, edge1: f64, x: f64, v: Vec2, c: Vec2, rho: f64, alpha: f64, p: &FoilParams| {
            inputs.extend_from_slice(&[edge0, edge1, x, v.x, v.y, c.x, c.y, rho, alpha]);
            inputs.extend_from_slice(&foil_params_values(p));
        };

    for (k, p) in sections.iter().enumerate() {
        let rho = if k == 0 { RHO_AIR } else { RHO_WATER };
        // The stall joins, at, around and exactly on the edges.
        for alpha in stall_blend_edge(p.alpha_stall, p.stall_blend) {
            let (s, c) = alpha.sin_cos();
            push(
                p.alpha_stall,
                p.alpha_stall + p.stall_blend,
                alpha.abs(),
                Vec2::new(5.0 * c, 5.0 * s),
                Vec2::new(-1.0, 0.0),
                rho,
                alpha,
                p,
            );
        }
        // The zero-flow guard, where `0/0` is a NaN in one stack and a
        // guarded zero in the other.
        for m in zero_flow_eps() {
            push(
                0.0,
                1.0,
                m,
                Vec2::new(m, 0.0),
                Vec2::new(-1.0, 0.0),
                rho,
                0.0,
                p,
            );
            push(
                0.0,
                1.0,
                m,
                Vec2::new(0.0, m),
                Vec2::new(-1.0, 0.0),
                rho,
                0.0,
                p,
            );
        }
        // `smoothstep`'s own two clamps, which are edges of their own.
        for x in around(0.0).into_iter().chain(around(1.0)) {
            push(
                0.0,
                1.0,
                x,
                Vec2::new(4.0, 0.3),
                Vec2::new(-1.0, 0.0),
                rho,
                0.1,
                p,
            );
        }
        // The background sweep: every angle of attack on `[−π, π]`, with the
        // flow and the chord both free.
        for i in 0..120 {
            let a = -std::f64::consts::PI + 2.0 * std::f64::consts::PI * halton(i, 2);
            let speed = 0.05 + 12.0 * halton(i, 3);
            let dir = -std::f64::consts::PI + 2.0 * std::f64::consts::PI * halton(i, 5);
            let chord = -std::f64::consts::PI + 2.0 * std::f64::consts::PI * halton(i, 7);
            push(
                p.alpha_stall,
                p.alpha_stall + p.stall_blend,
                -0.5 + 2.0 * halton(i, 11),
                Vec2::new(speed * dir.cos(), speed * dir.sin()),
                Vec2::new(chord.cos(), chord.sin()),
                rho,
                a,
                p,
            );
        }
    }

    let mut input_columns = names(&[
        "edge0", "edge1", "x", "vx", "vy", "cx", "cy", "rho", "alpha",
    ]);
    input_columns.extend(foil_param_columns());
    Fixture {
        name: "tier0_foil",
        tier: 0,
        samplers: names(&[STALL_BLEND_EDGE, ZERO_FLOW_EPS, HALTON_BACKGROUND]),
        input_columns,
        output_columns: names(&["smoothstep", "angle_of_attack", "cl", "cd", "fx", "fy"]),
        inputs,
        compute: |_, inputs, n_in| {
            map_rows(inputs, n_in, |r| {
                let v = Vec2::new(r[3], r[4]);
                let chord = Vec2::new(r[5], r[6]);
                let rho = r[7];
                let alpha = r[8];
                let p = foil_params_from(&r[9..18]);
                let f = foil_force(v, chord, rho, &p);
                vec![
                    smoothstep(r[0], r[1], r[2]),
                    angle_of_attack(v, chord),
                    cl(alpha, &p),
                    cd(alpha, &p),
                    f.x,
                    f.y,
                ]
            })
        },
    }
}

fn tier0_sail(ctx: &Context) -> Fixture {
    let mut inputs = Vec::new();
    let states = background_states(320, &ctx.params);
    let mut extra: Vec<BoatState> = Vec::new();
    for phi in phi_unwrapped() {
        extra.push(BoatState {
            u: 3.5,
            phi,
            beta: 0.6,
            ..BoatState::default()
        });
    }
    // The boom exactly on the centreline and exactly abeam, where the chord
    // and the flow are parallel or perpendicular.
    for beta in around(0.0)
        .into_iter()
        .chain(around(std::f64::consts::FRAC_PI_2))
        .chain(around(-std::f64::consts::FRAC_PI_2))
    {
        extra.push(BoatState {
            u: 3.0,
            beta,
            ..BoatState::default()
        });
    }
    for (i, st) in states.iter().chain(extra.iter()).enumerate() {
        inputs.extend_from_slice(&st.to_array());
        inputs.push(-14.0 + 28.0 * halton(i, 43));
        inputs.push(-14.0 + 28.0 * halton(i, 47));
    }
    let mut input_columns = state_columns();
    input_columns.extend(names(&["wind_x", "wind_y"]));
    Fixture {
        name: "tier0_sail",
        tier: 0,
        samplers: names(&[HALTON_BACKGROUND, PHI_UNWRAPPED]),
        input_columns,
        output_columns: names(&[
            "f_x", "f_y", "f_z", "r_x", "r_y", "r_z", "alpha", "cl", "cd", "m_beta", "aw_x",
            "aw_y", "aw_z", "q", "ce_x", "ce_y", "ce_z",
        ]),
        inputs,
        compute: |ctx, inputs, n_in| {
            map_rows(inputs, n_in, |r| {
                let st = state_from(r);
                let wind = Vec2::new(r[STATE_LEN], r[STATE_LEN + 1]);
                let s = sail_load(&st, wind, &ctx.params);
                let mut out = Vec::new();
                push_vec3(&mut out, s.load.f);
                push_vec3(&mut out, s.load.r);
                out.extend_from_slice(&[s.alpha, s.cl, s.cd, s.m_beta]);
                push_vec3(&mut out, s.aw_b);
                out.push(s.q);
                push_vec3(&mut out, s.ce_b);
                out
            })
        },
    }
}

fn tier0_sheet(ctx: &Context) -> Fixture {
    let mut inputs = Vec::new();
    let mut push = |st: &BoatState, rate: f64| {
        inputs.extend_from_slice(&st.to_array());
        inputs.push(rate);
    };
    for (st, rate) in sheet_slack_boundary(&ctx.params) {
        push(&st, rate);
    }
    for (i, st) in background_states(160, &ctx.params).iter().enumerate() {
        push(st, -3.0 + 9.0 * halton(i, 43));
    }
    let mut input_columns = state_columns();
    input_columns.push("l_sheet_dot".to_string());
    Fixture {
        name: "tier0_sheet",
        tier: 0,
        samplers: names(&[SHEET_SLACK_BOUNDARY, HALTON_BACKGROUND]),
        input_columns,
        output_columns: names(&[
            "attach_x",
            "attach_y",
            "attach_z",
            "rope_path_length",
            "drope_dbeta",
            "tension",
            "m_beta",
            "boom_f_x",
            "boom_f_y",
            "boom_f_z",
            "boom_r_x",
            "boom_r_y",
            "boom_r_z",
            "hull_f_x",
            "hull_f_y",
            "hull_f_z",
            "hull_r_x",
            "hull_r_y",
            "hull_r_z",
            "rope_length",
            "extension",
        ]),
        inputs,
        compute: |ctx, inputs, n_in| {
            map_rows(inputs, n_in, |r| {
                let st = state_from(r);
                let rate = r[STATE_LEN];
                let p = &ctx.params;
                let s = sheet_output(&st, rate, p);
                let mut out = Vec::new();
                push_vec3(&mut out, boom_attach_point(st.beta, p));
                out.push(rope_path_length(st.beta, p));
                out.push(drope_dbeta(st.beta, p));
                out.push(s.tension);
                out.push(s.m_beta);
                push_vec3(&mut out, s.boom_load.f);
                push_vec3(&mut out, s.boom_load.r);
                push_vec3(&mut out, s.hull_load.f);
                push_vec3(&mut out, s.hull_load.r);
                out.push(s.rope_length);
                out.push(s.extension);
                out
            })
        },
    }
}

fn tier0_gz(ctx: &Context) -> Fixture {
    use std::f64::consts::PI;
    let s = ctx.params.stability;
    let limit = GzCurve::gz_envelope(&ctx.params);
    let mass = ctx.params.total_mass();

    // The heel angles: the named places, the whole supported domain, and
    // past it, where the curve is extended by its own 2π periodicity.
    let mut phis: Vec<f64> = Vec::new();
    for c in [0.0, s.phi_peak, s.phi_vanish, PI, PI / 2.0] {
        phis.extend(around(c));
        phis.extend(around(-c));
    }
    phis.extend(halton_span(180, 2, -PI, PI));
    phis.extend(phi_unwrapped());

    // Mostly the shipped tunables, so the rows carry a real curve; then a
    // sweep across the admissible `GM` window and outside it, which is where
    // `fit`'s accept/reject branch lives.
    let mut sets: Vec<[f64; 5]> = vec![[s.gm, s.phi_peak, s.gz_max, s.phi_vanish, limit]];
    for gm in [0.50, 0.535, 0.545, 0.55, 0.56, 0.561, 0.60, 1.00] {
        sets.push([gm, s.phi_peak, s.gz_max, s.phi_vanish, limit]);
    }
    for phi_p in [0.6, 0.785, 0.9] {
        sets.push([s.gm, phi_p, s.gz_max, s.phi_vanish, limit]);
    }

    let mut inputs = Vec::new();
    for (k, set) in sets.iter().enumerate() {
        // The default set gets the whole heel sweep; the variations get a
        // coarse one, because what they are there to exercise is the fit.
        let take: Vec<f64> = if k == 0 {
            phis.clone()
        } else {
            halton_span(12, 2, -PI, PI)
        };
        for phi in take {
            inputs.extend_from_slice(set);
            inputs.push(phi);
            inputs.push(mass);
        }
    }

    let mut output_columns = names(&["fit_ok"]);
    for n in 1..=GzCurve::default().coefficients().len() {
        output_columns.push(format!("c_{n}"));
    }
    output_columns.extend(names(&["gz", "dgz", "gz_integral", "righting_moment"]));

    Fixture {
        name: "tier0_gz",
        tier: 0,
        samplers: names(&[PHI_UNWRAPPED, HALTON_BACKGROUND]),
        input_columns: names(&[
            "gm",
            "phi_peak",
            "gz_max",
            "phi_vanish",
            "gz_limit",
            "phi",
            "total_mass",
        ]),
        output_columns,
        inputs,
        compute: |_, inputs, n_in| {
            map_rows(inputs, n_in, |r| {
                let fit = GzCurve::fit(r[0], r[1], r[2], r[3], r[4]);
                let ok = fit.is_ok();
                // A rejected set yields the zero curve, which is what
                // `GzCurve::default()` is and what a consumer that ignored
                // the rejection would get. The `fit_ok` column is the branch
                // a port has to reproduce; the rest of the row says what the
                // rejection costs.
                let curve = fit.unwrap_or_default();
                let phi = r[5];
                let mut out = vec![if ok { 1.0 } else { 0.0 }];
                out.extend_from_slice(&curve.coefficients());
                out.push(curve.gz(phi));
                out.push(curve.dgz(phi));
                out.push(curve.gz_integral(phi));
                out.push(righting_moment(phi, &curve, r[6]));
                out
            })
        },
    }
}

fn tier0_hydro(ctx: &Context) -> Fixture {
    let mut inputs = Vec::new();
    let sections = [
        (
            ctx.params.board.pos_b,
            Vec2::new(-1.0, 0.0),
            ctx.params.board.section,
        ),
        (
            ctx.params.rudder.pos_b,
            Vec2::new(-1.0, 0.0),
            ctx.params.rudder.section,
        ),
    ];
    let states = background_states(200, &ctx.params);
    let mut extra: Vec<BoatState> = Vec::new();
    // At rest: `local_flow` is exactly zero and the foil must produce exactly
    // zero force, not a NaN (brief §14, F5.3's `EPS_FLOW` guard).
    extra.push(BoatState::default());
    for phi in phi_unwrapped() {
        extra.push(BoatState {
            u: 2.5,
            v: 0.3,
            r: 0.15,
            p: 0.4,
            phi,
            ..BoatState::default()
        });
    }
    for m in zero_flow_eps() {
        extra.push(BoatState {
            u: m,
            ..BoatState::default()
        });
    }
    for (i, st) in states.iter().chain(extra.iter()).enumerate() {
        let (pos, chord, section) = sections[i % sections.len()];
        // The rudder's chord turns with `δr` (F2.2); the board's is fixed.
        let chord = if i % sections.len() == 1 {
            let c = sailgym_physics::frames::rudder_chord(st.delta_r);
            Vec2::new(c.x, c.y)
        } else {
            chord
        };
        inputs.extend_from_slice(&st.to_array());
        inputs.extend_from_slice(&[pos.x, pos.y, pos.z, chord.x, chord.y]);
        inputs.extend_from_slice(&foil_params_values(&section));
    }
    let mut input_columns = state_columns();
    input_columns.extend(names(&["rb_x", "rb_y", "rb_z", "chord_x", "chord_y"]));
    input_columns.extend(foil_param_columns());
    Fixture {
        name: "tier0_hydro",
        tier: 0,
        samplers: names(&[HALTON_BACKGROUND, PHI_UNWRAPPED, ZERO_FLOW_EPS]),
        input_columns,
        output_columns: names(&[
            "flow_x",
            "flow_y",
            "f_x",
            "f_y",
            "f_z",
            "r_x",
            "r_y",
            "r_z",
            "alpha",
            "v_local_x",
            "v_local_y",
            "hull_f_x",
            "hull_f_y",
            "hull_n_yaw",
            "hull_k_roll",
        ]),
        inputs,
        compute: |ctx, inputs, n_in| {
            map_rows(inputs, n_in, |r| {
                let st = state_from(r);
                let rb = Vec3::new(r[STATE_LEN], r[STATE_LEN + 1], r[STATE_LEN + 2]);
                let chord = Vec2::new(r[STATE_LEN + 3], r[STATE_LEN + 4]);
                let p = foil_params_from(&r[STATE_LEN + 5..STATE_LEN + 14]);
                let flow = local_flow(&st, rb);
                let load = foil_hydro_load(&st, rb, chord, &p);
                let hull = hull_loads(&st, &ctx.params);
                let mut out = vec![flow.x, flow.y];
                push_vec3(&mut out, load.load.f);
                push_vec3(&mut out, load.load.r);
                out.push(load.alpha);
                out.push(load.v_local.x);
                out.push(load.v_local.y);
                out.push(hull.load.f.x);
                out.push(hull.load.f.y);
                out.push(hull.n_yaw);
                out.push(hull.k_roll);
                out
            })
        },
    }
}

// --- tier 1 ----------------------------------------------------------------

fn tier1_derivative(ctx: &Context) -> Fixture {
    let mut inputs = Vec::new();
    let mut push = |st: &BoatState, c: &Controls, field: usize, t: f64| {
        inputs.extend_from_slice(&st.to_array());
        inputs.push(c.rudder_rate_cmd);
        inputs.push(c.sheet_rate_cmd);
        inputs.push(if c.sheet_release { 1.0 } else { 0.0 });
        inputs.push(field as f64);
        inputs.push(t);
    };

    // Every actuator branch: `limit_rate`'s strict inequality, the three-way
    // self-centring comparison, and release precedence.
    for (i, (st, c)) in actuator_boundaries(&ctx.params).iter().enumerate() {
        push(st, c, i % ctx.winds.len(), 0.25 * (i as f64));
    }
    // The sheet take-up boundary, reached through the full derivative rather
    // than through `sheet_output` alone.
    for (i, (st, rate)) in sheet_slack_boundary(&ctx.params)
        .iter()
        .enumerate()
        .step_by(7)
    {
        let cmd = (rate / ctx.params.sheet.sheet_ease_rate).clamp(-1.0, 1.0);
        push(
            st,
            &Controls {
                rudder_rate_cmd: 0.0,
                sheet_rate_cmd: cmd,
                sheet_release: false,
            },
            2,
            0.1 * (i as f64),
        );
    }
    // And the background sweep, over every wind field in the bundle.
    for (i, st) in background_states(420, &ctx.params).iter().enumerate() {
        push(
            st,
            &Controls {
                rudder_rate_cmd: -1.5 + 3.0 * halton(i, 43),
                sheet_rate_cmd: -1.5 + 3.0 * halton(i, 47),
                sheet_release: i % 5 == 0,
            },
            i % ctx.winds.len(),
            120.0 * halton(i, 53),
        );
    }

    let mut input_columns = state_columns();
    // `eval_t`, not `t`: `derivative`'s time argument and `BoatState::t` are
    // equal in a running simulation but are sampled independently here, and
    // **the column names are the contract** — two columns called `t` would
    // make a name-keyed reader ambiguous, which is the exact failure the
    // names exist to prevent.
    input_columns.extend(names(&[
        "rudder_rate_cmd",
        "sheet_rate_cmd",
        "sheet_release",
        "field",
        "eval_t",
    ]));
    Fixture {
        name: "tier1_derivative",
        tier: 1,
        samplers: names(&[
            LIMIT_RATE_BOUNDARY,
            RUDDER_SELF_CENTRE_ZERO,
            SHEET_RELEASE_PRECEDENCE,
            SHEET_SLACK_BOUNDARY,
            HALTON_BACKGROUND,
        ]),
        input_columns,
        output_columns: STATE_FIELDS.iter().map(|f| format!("d_{f}")).collect(),
        inputs,
        compute: |ctx, inputs, n_in| {
            map_rows(inputs, n_in, |r| {
                let st = state_from(r);
                let c = Controls {
                    rudder_rate_cmd: r[STATE_LEN],
                    sheet_rate_cmd: r[STATE_LEN + 1],
                    sheet_release: r[STATE_LEN + 2] != 0.0,
                };
                let entry = ctx.wind(r[STATE_LEN + 3]);
                let fm = WindForces { wind: &entry.field };
                dot_to_vec(&derivative(&st, &c, &ctx.params, &fm, r[STATE_LEN + 4]))
            })
        },
    }
}

// --- tier 2 ----------------------------------------------------------------

/// A control change, applied at the start of the step at time `t`.
///
/// The same shape as `tests/golden/script.rs`'s `Cue`, and the shipped cases
/// below reuse that file's cue pattern. It is duplicated here rather than
/// included because the two files are owned by different tasks and the cue
/// *values* for the sheet transient are this bundle's own.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Cue {
    pub t: f64,
    pub rudder_rate: f64,
    pub sheet_rate: f64,
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
    fn controls(self) -> Controls {
        Controls {
            rudder_rate_cmd: self.rudder_rate,
            sheet_rate_cmd: self.sheet_rate,
            sheet_release: self.release,
        }
    }
}

/// One tier-2 trajectory, described well enough to be reproduced from the
/// manifest alone.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TrajectoryCase {
    pub name: String,
    /// The shipped scenario the initial condition and the wind come from.
    pub scenario: String,
    pub seconds: f64,
    pub cues: Vec<Cue>,
    /// The resolved F3 initial state, so a port need not parse `scenarios/`.
    pub initial_state: Vec<f64>,
    /// The name of this case's field in `wind_modes.json`.
    pub wind: String,
}

/// The cue pattern of `tests/golden/script.rs`, truncated to what fits inside
/// the tier-2 horizon, plus the dedicated sheet transient.
fn trajectory_cues(scenario: &str) -> Vec<Cue> {
    match scenario {
        // Haul hard and hold (brief §46 steps 4–8).
        "beam_reach_capsize" | "sheet_release_recovery" | "free_sail" => {
            vec![Cue::new(0.0, 0.0, -1.0, false)]
        }
        // Settle, then steer: the cue lands inside the horizon rather than at
        // the 6 s the 30 s golden uses, because a 2 s fixture in which
        // nothing is commanded tests only the initial transient.
        "close_hauled" => vec![
            Cue::new(0.0, 0.0, 0.0, false),
            Cue::new(0.8, -0.3, 0.0, false),
        ],
        "tack" => vec![
            Cue::new(0.0, 0.0, 0.0, false),
            Cue::new(0.6, -1.0, 0.0, false),
        ],
        "gybe" => vec![
            Cue::new(0.0, 0.0, 0.0, false),
            Cue::new(0.6, 0.8, 0.0, false),
        ],
        _ => vec![Cue::new(0.0, 0.0, 0.0, false)],
    }
}

/// The tier-2 cases, in a fixed order.
pub fn trajectory_cases() -> Vec<TrajectoryCase> {
    let mut out = Vec::new();
    for name in shipped_names() {
        let sc = load_shipped(name).unwrap_or_else(|e| panic!("{name}: {e}"));
        out.push(TrajectoryCase {
            name: name.to_string(),
            scenario: name.to_string(),
            seconds: TIER2_SCENARIO_SECONDS,
            cues: trajectory_cues(name),
            initial_state: initial_state_of(&sc).to_array().to_vec(),
            wind: format!("scenario_{name}"),
        });
    }
    // The dedicated sheet transient (task 2.4, F11 R1). `free_sail` starts
    // with the sheet fully eased at `l_sheet_max`, so hauling flat out drives
    // the rope through take-up under load — the discontinuity v2 F18.1b
    // introduced and the stiffest mode in the model.
    let sc = load_shipped("free_sail").expect("free_sail is shipped");
    out.push(TrajectoryCase {
        name: "sheet_transient".to_string(),
        scenario: "free_sail".to_string(),
        seconds: TIER2_TRANSIENT_SECONDS,
        cues: vec![
            Cue::new(0.0, 0.0, -1.0, false),
            Cue::new(2.0, 0.0, 1.0, false),
            Cue::new(3.0, 0.0, -1.0, false),
        ],
        initial_state: initial_state_of(&sc).to_array().to_vec(),
        wind: "scenario_free_sail".to_string(),
    });
    out
}

fn initial_state_of(sc: &Scenario) -> BoatState {
    let params = sc
        .to_parameters()
        .unwrap_or_else(|e| panic!("{}: {e}", sc.name));
    let mut sim = Simulation::new(params, sc.seed);
    sim.load_scenario(sc)
        .unwrap_or_else(|e| panic!("{}: {e}", sc.name));
    *sim.state()
}

/// Run one case at a given timestep, returning the state at every multiple of
/// `stride` steps.
///
/// `dt_scale` is 1, 2 or 4: the reference study runs the same case at `dt`,
/// `dt/2` and `dt/4` and compares at the **same simulated times**, which is
/// what `stride` is for.
pub fn run_case(case: &TrajectoryCase, dt_scale: u32) -> Vec<[f64; STATE_LEN]> {
    let sc = load_shipped(&case.scenario).unwrap_or_else(|e| panic!("{}: {e}", case.scenario));
    let mut params = sc
        .to_parameters()
        .unwrap_or_else(|e| panic!("{}: {e}", case.scenario));
    let base_dt = params.sim.dt;
    params.sim.dt = base_dt / (dt_scale as f64);
    let mut sim = Simulation::new(params, sc.seed);
    sim.load_scenario(&sc)
        .unwrap_or_else(|e| panic!("{}: {e}", case.scenario));
    // `load_scenario` re-resolves the catalogue from the scenario, so the
    // refined timestep has to be reapplied afterwards.
    sim.set_parameters(params)
        .unwrap_or_else(|e| panic!("{}: {e}", case.scenario));

    let dt = params.sim.dt;
    let steps = (case.seconds / dt).round() as u64;
    let cue_steps: Vec<u64> = case
        .cues
        .iter()
        .map(|c| (c.t / dt).round() as u64)
        .collect();
    let stride = dt_scale as u64;

    let mut out = Vec::new();
    let mut next_cue = 0usize;
    for i in 0..=steps {
        while next_cue < case.cues.len() && cue_steps[next_cue] == i {
            sim.set_controls(case.cues[next_cue].controls());
            next_cue += 1;
        }
        if i % stride == 0 {
            out.push(sim.state().to_array());
        }
        if i < steps {
            sim.advance(1);
        }
    }
    out
}

fn tier2_trajectories() -> Fixture {
    let cases = trajectory_cases();
    let dt = BoatParameters::ilca7().sim.dt;
    let mut inputs = Vec::new();
    for (k, case) in cases.iter().enumerate() {
        let steps = (case.seconds / dt).round() as u64;
        for step in 0..=steps {
            inputs.push(k as f64);
            inputs.push(step as f64);
        }
    }
    Fixture {
        name: "tier2_trajectories",
        tier: 2,
        samplers: vec![
            "golden_script_cues".to_string(),
            "sheet_transient".to_string(),
        ],
        input_columns: names(&["case", "step"]),
        output_columns: state_columns(),
        inputs,
        compute: |_, inputs, n_in| {
            let cases = trajectory_cases();
            // Each case is run once and its samples laid out in row order.
            // The input column `step` is asserted against the row's position
            // rather than trusted, so a hand-edited fixture cannot silently
            // reorder the trajectory.
            let mut by_case: Vec<Vec<[f64; STATE_LEN]>> = Vec::with_capacity(cases.len());
            for case in &cases {
                by_case.push(run_case(case, 1));
            }
            map_rows(inputs, n_in, |r| {
                let k = r[0] as usize;
                let step = r[1] as usize;
                by_case[k][step].to_vec()
            })
        },
    }
}

// ---------------------------------------------------------------------------
// The tolerance contract (F16.2), derived rather than typed
// ---------------------------------------------------------------------------

/// The ULP budget F16.2 proposes for well-scaled tier-0 and tier-1 values.
///
/// **A proposal, not a universal pass threshold.** F16.2 is explicit that an
/// ULP distance and a relative error are distinct metrics and that neither
/// means anything near cancellation, which is why every entry below carries
/// an absolute floor derived from its own measured scale as well.
pub const ULP_BUDGET: u32 = 32;

/// The tier-2 port budget: a port's error must stay under this fraction of
/// the **measured** discretization error of the reference itself (F16.2).
pub const TIER2_FRACTION_OF_DISCRETIZATION: f64 = 0.10;

/// ULPs of accumulated rounding allowed per step of an RK2 trajectory, used
/// as the tier-2 numerical floor.
///
/// Two derivative evaluations per step, each a few dozen dependent
/// floating-point operations, and no compensation anywhere: a few ULP of the
/// running scale per step is the floor below which a difference says nothing
/// about the port. Eight is generous by a small factor and is stated so that
/// the floor can be argued with rather than guessed at.
pub const TIER2_FLOOR_ULPS_PER_STEP: f64 = 8.0;

/// One quantity's tolerance.
///
/// The *rule* that produced it is stated once per tier in
/// [`TierContract::rule`] — F16.2 asks for "the tolerance **and its
/// justification**, as a string, **per tier**" — and the measured numbers
/// that instantiate it for this quantity are here. A bound without the
/// measurement behind it is how a tolerance gets widened three months later
/// by someone who does not know what it meant.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QuantityTolerance {
    pub fixture: String,
    /// The output column, or `case::column` for tier 2.
    pub quantity: String,
    pub unit: String,
    /// The largest `|value|` this quantity reached over its fixture's domain.
    pub measured_scale: f64,
    /// The bound a consumer applies near zero, in `unit`.
    pub absolute: f64,
    /// The bound a consumer applies away from zero. Zero where the tier's
    /// rule is purely absolute.
    pub relative: f64,
    /// The ULP form of `relative`, where one applies.
    pub ulp: Option<u32>,
    /// Tier 2 only: the reference study this bound is a fraction of.
    pub measured: Option<ReferenceError>,
}

/// The tier-2 dt/dt2/dt4 measurement behind one quantity's bound.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ReferenceError {
    /// `max |x(dt) − x(dt/4)|` over the trajectory, in the quantity's unit.
    pub reference_error_dt: f64,
    /// The same at `dt/2`. Printed so the refinement is visible; **no order
    /// is claimed from it**.
    pub reference_error_half_dt: f64,
    /// The numerical floor, below which a difference says nothing.
    pub floor: f64,
    pub steps: f64,
}

/// One tier of the F16.2 contract: the rule, stated once, and the quantities
/// it was instantiated for.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TierContract {
    pub tier: u32,
    /// The derivation. One string for the whole tier, per F16.2.
    pub rule: String,
    /// The finite input domain each fixture in this tier was sampled over.
    pub domains: BTreeMap<String, String>,
    pub quantities: Vec<QuantityTolerance>,
}

/// The measured gap between the hand-rolled kernel and the host library, and
/// between the fixed pairwise summation and a compensated one (F16.6).
///
/// Reported as numbers, never as a pass or a fail: a port that calls its
/// host's cosine is held to `kernel_vs_libm_absolute`, and a port that
/// reassociates the mode sum is held to `summation_vs_compensated_absolute`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WindKernelGap {
    pub domain: String,
    /// max `|wave(θ) − θ.cos()|` over the sampled `θ`.
    pub kernel_vs_libm_absolute: f64,
    /// The `θ` at which that maximum occurred.
    pub kernel_vs_libm_at: f64,
    /// max `|sample − compensated_sample|` over the sampled points, m/s.
    pub summation_vs_compensated_absolute: f64,
    /// The same, relative to the field's base speed.
    pub summation_vs_compensated_relative: f64,
    pub derivation: String,
}

/// The whole F16.2 contract, as the manifest carries it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Tolerances {
    pub contract_version: u32,
    pub tiers: Vec<TierContract>,
    pub wind_kernel: WindKernelGap,
    /// Restated in the artifact itself, because a bundle outlives the
    /// document that described it.
    pub notes: Vec<String>,
}

/// The unit of one tier-1 derivative component, in the component's own terms.
///
/// F16.2 forbids "dimensionally invalid reuse of a state-error tolerance":
/// `d_u` is an acceleration and `d_psi` is a rate, and a single number cannot
/// be a bound on both.
fn derivative_unit(field: &str) -> &'static str {
    match field {
        "x" | "y" | "l_sheet" => "m/s",
        "psi" | "phi" | "beta" | "delta_r" => "rad/s",
        "u" | "v" => "m/s^2",
        "r" | "p" | "beta_dot" => "rad/s^2",
        _ => "s/s",
    }
}

fn state_unit(field: &str) -> &'static str {
    match field {
        "x" | "y" | "l_sheet" => "m",
        "psi" | "phi" | "beta" | "delta_r" => "rad",
        "u" | "v" => "m/s",
        "r" | "p" | "beta_dot" => "rad/s",
        _ => "s",
    }
}

/// The unit of a tier-0 output column. Unknown columns read
/// `dimensionless`, which is the honest answer for a coefficient and a
/// visible gap if a column is ever added without one.
fn tier0_unit(fixture: &str, column: &str) -> &'static str {
    match fixture {
        "tier0_wrap_pi" => return "rad",
        "tier0_wave" => return "dimensionless",
        "tier0_wind_sample" | "tier0_apparent" => return "m/s",
        _ => {}
    }
    match column {
        "fx" | "fy" | "tension" => "N",
        "m_beta" | "righting_moment" | "hull_n_yaw" | "hull_k_roll" => "N*m",
        "alpha" | "angle_of_attack" => "rad",
        "q" => "Pa",
        "gz" | "rope_length" | "extension" | "rope_path_length" => "m",
        "dgz" | "drope_dbeta" => "m/rad",
        "gz_integral" => "m*rad",
        "cl" | "cd" | "smoothstep" | "fit_ok" => "dimensionless",
        c if c.starts_with("c_") => "m",
        c if c.starts_with("flow_") || c.starts_with("v_local_") || c.starts_with("aw_") => "m/s",
        c if c.starts_with("f_") || c.starts_with("boom_f") || c.starts_with("hull_f") => "N",
        c if c.ends_with("_x") || c.ends_with("_y") || c.ends_with("_z") => "m",
        _ => "dimensionless",
    }
}

fn max_abs(values: &[f64]) -> f64 {
    values
        .iter()
        .filter(|v| v.is_finite())
        .fold(0.0_f64, |m, v| m.max(v.abs()))
}

/// Column `c` of a flat `rows × cols` block.
fn column(data: &[f64], cols: usize, c: usize) -> Vec<f64> {
    data.iter().skip(c).step_by(cols).copied().collect()
}

/// Derive the whole contract from the fixtures as generated (F16.2).
///
/// Everything here is **measured on this build** — no number is retyped from
/// `docs/v1/convergence.md`, which describes a different study of a
/// superseded model. That is a deliberate reading of RV11: the risk is "a
/// float from that table appears as a literal in `crates/`", and the
/// strongest mitigation is to measure rather than to read.
pub fn derive_tolerances(
    ctx: &Context,
    built: &[(&Fixture, usize, usize, Vec<f64>)],
) -> Tolerances {
    let eps = f64::EPSILON;
    let relative = (ULP_BUDGET as f64) * eps;
    let mut tier0 = TierContract {
        tier: 0,
        rule: format!(
            "Pure functions. A port is conformant on a column when, for every row, \
             |port - recorded| <= max(absolute, relative * |recorded|). `relative` is \
             {ULP_BUDGET} ULP = {ULP_BUDGET} * DBL_EPSILON = {relative:e}, which F16.2 \
             proposes for well-scaled tier-0 values and which is a proposal, not a \
             universal pass threshold — an ULP distance and a relative error are distinct \
             metrics, and neither means anything near cancellation. `absolute` is that \
             same budget against the column's **own measured scale** over the domain \
             below: relative * measured_scale. A column whose measured scale is **zero** \
             is structurally zero over the whole domain — the sail force has no z \
             component because F6.3 drops the spanwise flow, and a `Load` applied at the \
             CG has no arm — and its bound is zero: a port must produce zero there too, \
             and `the_bundle_is_not_vacuous` checks that such a column really is all \
             zeros rather than merely unsampled. The custom wind kernel and the \
             fixed-order mode reduction are measured separately and reported under \
             `wind_kernel`, because a port that calls its host's cosine cannot meet the \
             ULP bound and should not be asked to. Every number here was measured on the \
             build that wrote this bundle."
        ),
        domains: BTreeMap::new(),
        quantities: Vec::new(),
    };
    let mut tier1 = TierContract {
        tier: 1,
        rule: format!(
            "Derivative components. Each bound is derived **directly, in that \
             component's own units** — m/s, rad/s, m/s^2, rad/s^2 — and never borrowed \
             from a state-error tolerance, which would be dimensionally invalid (F16.2). \
             The form is the same as tier 0: max(absolute, relative * |recorded|) with \
             relative = {ULP_BUDGET} ULP = {relative:e} and absolute = relative * the \
             component's own measured scale. Measured on the build that wrote this bundle."
        ),
        domains: BTreeMap::new(),
        quantities: Vec::new(),
    };

    for (fx, _rows, cols, data) in built {
        if fx.tier > 1 {
            continue;
        }
        let tier = if fx.tier == 0 { &mut tier0 } else { &mut tier1 };
        tier.domains.insert(fx.name.to_string(), domain_of(fx));
        let n_in = fx.input_columns.len();
        for (j, name) in fx.output_columns.iter().enumerate() {
            let values = column(data, *cols, n_in + j);
            let scale = max_abs(&values);
            let unit = if fx.tier == 0 {
                tier0_unit(fx.name, name).to_string()
            } else {
                derivative_unit(name.strip_prefix("d_").unwrap_or(name)).to_string()
            };
            tier.quantities.push(QuantityTolerance {
                fixture: fx.name.to_string(),
                quantity: name.clone(),
                unit,
                measured_scale: scale,
                absolute: relative * scale,
                relative,
                ulp: Some(ULP_BUDGET),
                measured: None,
            });
        }
    }

    Tolerances {
        contract_version: TOLERANCE_CONTRACT_VERSION,
        tiers: vec![tier0, tier1, tier2_contract()],
        wind_kernel: measure_wind_kernel(ctx),
        notes: vec![
            "Conformance is implementation agreement, not physical validation (F16.2). Two \
             implementations that agree to the last bit may share a modelling mistake."
                .to_string(),
            "No conformance test may assert equality across stacks (F16.1). These are \
             tolerances; the Rust runner's bit-for-bit comparison is a staleness check on \
             this bundle, not a cross-stack claim."
                .to_string(),
            "An ULP distance and a relative error are distinct metrics. Both are given; \
             neither is meaningful near cancellation, which is what the absolute bound is \
             for (F16.2)."
                .to_string(),
            "A tolerance change requires a new derivation, never merely a green port test \
             (F16.2). Regenerate the bundle; do not edit a number here."
                .to_string(),
            "There is no tier 3 in this bundle. The brief section 35 invariants exist for \
             Rust in crates/sailgym-physics/tests/invariants.rs; porting them is a later \
             section's work, and agreement here is scoped to the regimes sampled below."
                .to_string(),
        ],
    }
}

fn domain_of(fx: &Fixture) -> String {
    let n_in = fx.input_columns.len();
    let rows = fx.rows();
    let mut parts = Vec::new();
    for (j, name) in fx.input_columns.iter().enumerate() {
        let values: Vec<f64> = (0..rows).map(|r| fx.inputs[r * n_in + j]).collect();
        let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if lo == hi {
            parts.push(format!("{name} = {lo:e}"));
        } else {
            parts.push(format!("{name} in [{lo:e}, {hi:e}]"));
        }
    }
    format!("{rows} rows; {}", parts.join("; "))
}

/// The fresh dt / dt2 / dt4 study F16.2 tier 2 requires, per case and per
/// state quantity.
///
/// Three runs of every tier-2 case at `dt`, `dt/2` and `dt/4`, compared at the
/// same simulated times. `|x(dt) − x(dt/4)|` is the reference's **own**
/// discretization error; a port is asked to stay inside a tenth of it.
///
/// **No order is asserted.** v2 F18.1b's tension law is discontinuous at
/// take-up, and across an event RK2 has no order of accuracy at all
/// (`docs/v2/progress/08-handoff.md` §3). The study measures the error; it
/// does not claim a slope.
fn tier2_contract() -> TierContract {
    let eps = f64::EPSILON;
    let dt = BoatParameters::ilca7().sim.dt;
    let mut quantities = Vec::new();
    let mut domains = BTreeMap::new();
    for case in trajectory_cases() {
        let coarse = run_case(&case, 1);
        let medium = run_case(&case, 2);
        let fine = run_case(&case, 4);
        let n = coarse.len().min(medium.len()).min(fine.len());
        let steps = (case.seconds / dt).round();
        domains.insert(
            case.name.clone(),
            format!(
                "scenario {}, {} s at dt = {dt} ({n} samples), cues at {:?} s",
                case.scenario,
                case.seconds,
                case.cues.iter().map(|c| c.t).collect::<Vec<_>>()
            ),
        );
        for (j, field) in STATE_FIELDS.iter().enumerate() {
            let mut err_coarse = 0.0_f64;
            let mut err_medium = 0.0_f64;
            let mut scale = 0.0_f64;
            for k in 0..n {
                err_coarse = err_coarse.max((coarse[k][j] - fine[k][j]).abs());
                err_medium = err_medium.max((medium[k][j] - fine[k][j]).abs());
                scale = scale.max(coarse[k][j].abs());
            }
            let floor = TIER2_FLOOR_ULPS_PER_STEP * steps * eps * scale.max(1.0);
            let tolerance = (TIER2_FRACTION_OF_DISCRETIZATION * err_coarse).max(floor);
            quantities.push(QuantityTolerance {
                fixture: "tier2_trajectories".to_string(),
                quantity: format!("{}::{field}", case.name),
                unit: state_unit(field).to_string(),
                measured_scale: scale,
                absolute: tolerance,
                relative: 0.0,
                ulp: None,
                measured: Some(ReferenceError {
                    reference_error_dt: err_coarse,
                    reference_error_half_dt: err_medium,
                    floor,
                    steps,
                }),
            });
        }
    }
    TierContract {
        tier: 2,
        rule: format!(
            "Trajectories, 1-5 s, sampled every physics step. A **fresh** dt/dt2/dt4 study \
             on this build's model, per case and per state quantity: each case is run at \
             dt, dt/2 and dt/4 and compared at the same simulated times, and \
             `reference_error_dt` = max |x(dt) - x(dt/4)| is the reference's own \
             discretization error. The bound is \
             {TIER2_FRACTION_OF_DISCRETIZATION} of that, floored at \
             {TIER2_FLOOR_ULPS_PER_STEP} ULP/step * steps * DBL_EPSILON * max(scale, 1) — \
             below which a difference says nothing about the port rather than something \
             about the model. RK2's local state error is O(dt^3) and its global error \
             O(dt^2) **in smooth regimes**, but this model's sheet tension is \
             discontinuous at take-up (v2 F18.1b), so **no order is asserted and none may \
             be inferred from the dt/2 column**: `reference_error_half_dt` is printed \
             because a refinement that did not shrink would be worth knowing, not because \
             a slope is being claimed. Long divergent capsize trajectories are not a \
             pointwise oracle (F16.2) and none is included here. Nothing in this tier is \
             taken from docs/v1/convergence.md, which studies a superseded model."
        ),
        domains,
        quantities,
    }
}

/// Measure the two F16.6 gaps: kernel against `libm`, and the fixed pairwise
/// summation against a compensated one.
fn measure_wind_kernel(ctx: &Context) -> WindKernelGap {
    use sailgym_physics::environment::wind::wave;
    use std::f64::consts::TAU;

    let mut worst = 0.0_f64;
    let mut worst_at = 0.0_f64;
    let mut check = |theta: f64| {
        let e = (wave(theta) - theta.cos()).abs();
        if e > worst {
            worst = e;
            worst_at = theta;
        }
    };
    let n = 400_000;
    for i in 0..=n {
        check(-6.0 * TAU + 12.0 * TAU * (i as f64) / (n as f64));
    }
    for i in 0..200_000 {
        check(-5.0e3 + 1.0e4 * halton(i, 2));
    }
    for i in 0..100_000 {
        check(-1.0e5 + 2.0e5 * halton(i, 3));
    }

    // The summation order. `sample_inner` accumulates two modes at a time
    // into four slots and adds the pairs at the end (F9.4); a port that sums
    // straight through, or vectorises the reduction, lands somewhere else in
    // the last bits. Kahan is the stand-in for "a better-conditioned sum".
    let mut sum_gap = 0.0_f64;
    let mut base_speed = 1.0_f64;
    for entry in &ctx.winds {
        if entry.field.mode_count() == 0 {
            continue;
        }
        base_speed = base_speed.max(entry.config.speed);
        for i in 0..4000 {
            let x = -2000.0 + 4000.0 * halton(i, 2);
            let y = -2000.0 + 4000.0 * halton(i, 3);
            let t = 600.0 * halton(i, 5);
            let got = entry.field.sample(x, y, t);
            let (kx, ky) = kahan_sample(entry, x, y, t);
            sum_gap = sum_gap.max((got.x - kx).abs()).max((got.y - ky).abs());
        }
    }

    WindKernelGap {
        domain: "theta in [-1e5, 1e5] rad (700 000 points); wind sampled over \
                 x, y in [-2000, 2000] m and t in [0, 600] s (4 000 points per non-uniform field)"
            .to_string(),
        kernel_vs_libm_absolute: worst,
        kernel_vs_libm_at: worst_at,
        summation_vs_compensated_absolute: sum_gap,
        summation_vs_compensated_relative: sum_gap / base_speed,
        derivation: format!(
            "F16.6, measured on this build. `environment::wind::wave` is a hand-rolled \
             Cody-Waite reduction and Estrin-evaluated polynomial, not `libm`: over the \
             domain above it differs from `f64::cos` by at most {worst:e} (at theta = \
             {worst_at}). `sample_inner` sums the modes in a fixed two-slot pairwise order \
             required by F9.4; against a Kahan-compensated sum of the same modes the field \
             differs by at most {sum_gap:e} m/s. A port that reproduces the kernel and the \
             order verbatim is held to the tier-0 bound for `tier0_wave` and \
             `tier0_wind_sample`; a port that calls its host's cosine, or reassociates the \
             sum, is held to these measured numbers instead and reports the divergence as a \
             number, not as a pass or a fail."
        ),
    }
}

/// The same sum as `sample_inner`, compensated. Used only to measure the
/// reduction error; nothing in the physics path calls it.
fn kahan_sample(entry: &WindEntry, x: f64, y: f64, t: f64) -> (f64, f64) {
    use sailgym_physics::environment::wind_from_bearing;
    let base = wind_from_bearing(entry.config.speed, entry.config.bearing_deg);
    let mut sum = [0.0_f64; 2];
    let mut comp = [0.0_f64; 2];
    for m in entry.field.modes() {
        let c = (m.k().x * x + m.k().y * y + m.omega() * t + m.phase()).cos();
        for (i, a) in [m.amp().x * c, m.amp().y * c].into_iter().enumerate() {
            let yv = a - comp[i];
            let tv = sum[i] + yv;
            comp[i] = (tv - sum[i]) - yv;
            sum[i] = tv;
        }
    }
    (base.x + sum[0], base.y + sum[1])
}

// ---------------------------------------------------------------------------
// The manifest and the built bundle
// ---------------------------------------------------------------------------

/// `manifest.json`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub bundle_schema_version: u32,
    /// The command that produced this bundle.
    pub generator: String,
    pub generator_version: u32,
    /// The bundle's compact key, under F16.4's name for it. Equals the
    /// containing directory's name, and is exactly
    /// [`BundleIdentity::key`] — a SHA-256 over the bundle's *contract*,
    /// which is everything in `identity` except `model.source`.
    pub digest: String,
    /// R7: a bundle is evidence about the build that produced it.
    pub toolchain: ToolchainInfo,
    /// The full canonical record (F16.4).
    pub identity: BundleIdentity,
    /// What `--allow-dirty --declare` was told, when the source tree was not
    /// a baseline. Empty when the tree was clean.
    pub declared_changes: String,
    pub tolerances: Tolerances,
    /// The `GZ` representation as data (task 2.3), so a port need not solve
    /// the F18.1a system to evaluate the curve.
    pub gz: GzRepresentation,
    /// The tier-2 cases, described well enough to be reproduced.
    pub trajectory_cases: Vec<TrajectoryCase>,
    /// Every **data** file in the bundle and its size, so the budget is
    /// auditable without a directory listing.
    ///
    /// `manifest.json` itself is deliberately absent: a record that stated
    /// its own length would have to be a fixed point of its own
    /// serialisation. The generator measures the real directory — manifest
    /// included — against [`MAX_BUNDLE_BYTES`] and prints both.
    pub file_bytes: BTreeMap<String, u64>,
    /// The sum of [`Manifest::file_bytes`]; see the note there.
    pub data_bytes: u64,
}

/// A bundle in memory: the files to write, and the manifest describing them.
pub struct Bundle {
    pub manifest: Manifest,
    /// `(relative path, bytes)`, in write order.
    pub files: Vec<(String, Vec<u8>)>,
}

/// Build the whole bundle from current source.
///
/// `declared_changes` is `gen_golden`'s convention: empty for a clean tree,
/// and the `--declare` text plus the dirty file list otherwise.
pub fn build(ctx: &Context, declared_changes: &str) -> Bundle {
    let fixtures = fixtures(ctx);
    let built: Vec<(&Fixture, usize, usize, Vec<f64>)> = fixtures
        .iter()
        .map(|fx| {
            let (rows, cols, data) = fx.matrix(ctx);
            (fx, rows, cols, data)
        })
        .collect();

    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut fixture_ids = Vec::new();
    for (fx, rows, cols, data) in &built {
        let bytes = npy::encode_f64_2d(*rows, *cols, data);
        // The key covers the payload, not the header: a header rewrite that
        // preserved every number would not be a physics change.
        let payload: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        fixture_ids.push(FixtureId {
            name: fx.name.to_string(),
            tier: fx.tier,
            samplers: fx.samplers.clone(),
            rows: *rows,
            input_columns: fx.input_columns.clone(),
            output_columns: fx.output_columns.clone(),
            data_key: sailgym_physics::digest::sha256_hex(&payload),
        });
        files.push((format!("{}.npy", fx.name), bytes));
    }

    // The wind modes, as data (F16.7).
    let entries: Vec<WindEntryOut> = ctx
        .winds
        .iter()
        .map(|w| WindEntryOut {
            name: &w.name,
            seed: w.seed,
            config: &w.config,
            modes: w.field.modes(),
        })
        .collect();
    let mut wind_ids = Vec::new();
    for e in &entries {
        let text = serde_json::to_string(e).expect("a wind entry must serialise");
        wind_ids.push(WindFixtureId {
            name: e.name.to_string(),
            seed: e.seed,
            mode_count: e.modes.len(),
            modes_key: sailgym_physics::digest::sha256_hex(text.as_bytes()),
        });
    }
    let mut wind_json =
        serde_json::to_string_pretty(&entries).expect("the wind modes must serialise");
    wind_json.push('\n');
    files.push(("wind_modes.json".to_string(), wind_json.into_bytes()));

    // The F7 catalogue, verbatim.
    let mut params_json =
        serde_json::to_string_pretty(&ctx.params).expect("the catalogue must serialise");
    params_json.push('\n');
    files.push(("parameters.json".to_string(), params_json.into_bytes()));

    let identity = BundleIdentity {
        bundle_schema_version: BUNDLE_SCHEMA_VERSION,
        generator_version: GENERATOR_VERSION,
        tolerance_contract_version: TOLERANCE_CONTRACT_VERSION,
        model: ModelIdentity::current(),
        parameters: ctx.params,
        integrator: ctx.params.sim.integrator,
        dt: ctx.params.sim.dt,
        wind: wind_ids,
        fixtures: fixture_ids,
    };

    let file_bytes: BTreeMap<String, u64> = files
        .iter()
        .map(|(name, bytes)| (name.clone(), bytes.len() as u64))
        .collect();
    let data_bytes = file_bytes.values().sum();

    let manifest = Manifest {
        bundle_schema_version: BUNDLE_SCHEMA_VERSION,
        generator: GENERATOR_COMMAND.to_string(),
        generator_version: GENERATOR_VERSION,
        digest: identity.key(),
        toolchain: ToolchainInfo::current(),
        identity,
        declared_changes: declared_changes.to_string(),
        tolerances: derive_tolerances(ctx, &built),
        gz: GzCurve::from_params(&ctx.params).representation(),
        trajectory_cases: trajectory_cases(),
        file_bytes,
        data_bytes,
    };

    files.push(("manifest.json".to_string(), manifest_bytes(&manifest)));

    Bundle { manifest, files }
}

/// The manifest's canonical on-disk text: pretty JSON, newline-terminated.
pub fn manifest_bytes(m: &Manifest) -> Vec<u8> {
    let mut text = serde_json::to_string_pretty(m).expect("the manifest must serialise");
    text.push('\n');
    text.into_bytes()
}
