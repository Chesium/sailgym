//! Physics invariant tests (brief §35).
//!
//! This file starts here, at M3, and grows through sections 05–07; section 10
//! completes it. Gate step 4 runs this target.
//!
//! Hydrodynamic guards retain explicit still-air fixtures and external loads.
//! M4 adds wind-driven trajectories, diagnostics and continuous tack motion.

use sailgym_physics::dynamics::sheet_rate;
use sailgym_physics::dynamics::Load;
use sailgym_physics::forces::PhysicalForces;
use sailgym_physics::integrator::step;
use sailgym_physics::parameters::BoatParameters;
use sailgym_physics::rigging::mainsheet::{rope_path_length, sheet_output};
use sailgym_physics::simulation::Simulation;
use sailgym_physics::stability::hydrostatics::GzCurve;
use sailgym_physics::state::{BoatState, Controls, STATE_FIELDS, STATE_LEN};
use sailgym_physics::testkit::{mirror_controls, mirror_state, WithExternalLoad};
use sailgym_physics::vec::{Vec2, Vec3};

/// Deterministic test-local generator; see the note in `state.rs`.
struct Lcg(u64);

impl Lcg {
    fn unit(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.unit() * (hi - lo)
    }
}

fn params() -> BoatParameters {
    BoatParameters::ilca7()
}

fn steps_for(seconds: f64, p: &BoatParameters) -> u32 {
    (seconds / p.sim.dt).round() as u32
}

/// At rest with the sheet fully eased, so the boom is unrestrained.
///
/// `Simulation::initial_state` starts at `l_sheet_min`, which under v2 F18.1b
/// is the **geometric** minimum: the boom is two-blocked on the centreline at
/// exactly zero extension, so the rope carries no load but pins the boom.
/// Fixtures that mean "free boom" say so with this.
fn boom_free_state(p: &BoatParameters) -> BoatState {
    BoatState {
        l_sheet: p.sheet.l_sheet_max,
        ..Simulation::initial_state(p)
    }
}

/// `½ m_x u² + ½ m_y v² + ½ I_z r² + ½ I_x p²`.
fn kinetic_energy(st: &BoatState, p: &BoatParameters) -> f64 {
    let m = p.total_mass();
    0.5 * (m + p.inertia.a_x) * st.u * st.u
        + 0.5 * (m + p.inertia.a_y) * st.v * st.v
        + 0.5 * (p.inertia.i_zz + p.inertia.a_psi) * st.r * st.r
        + 0.5 * (p.inertia.i_xx + p.inertia.a_phi) * st.p * st.p
}

// ---------------------------------------------------------------------------
// brief §35 — rest equilibrium
// ---------------------------------------------------------------------------

#[test]
fn rest_equilibrium() {
    let p = params();
    let start = Simulation::initial_state(&p);
    let mut sim = Simulation::new(p, 0);
    sim.set_wind(sailgym_physics::environment::wind::WindConfig {
        speed: 0.0,
        ..Default::default()
    });
    sim.reset(start, 0);
    sim.advance(10_000);

    // `t` is the simulation clock and `ṫ = 1` by F3, so it is the one field
    // that must change; the other twelve are compared bit for bit.
    let now = sim.state().to_array();
    let then = start.to_array();
    for i in 0..STATE_LEN - 1 {
        assert_eq!(
            now[i].to_bits(),
            then[i].to_bits(),
            "field {}: {} vs {}",
            STATE_FIELDS[i],
            now[i],
            then[i]
        );
    }
    assert!(sim.state().t > 0.0);
}

// ---------------------------------------------------------------------------
// brief §35 — port/starboard mirror symmetry
// ---------------------------------------------------------------------------

#[test]
fn mirror_symmetry_trajectory() {
    let p = params();
    let steps = steps_for(20.0, &p);

    // Driven by an initial speed plus a constant external surge load, which is
    // the sanctioned substitute for the sail that does not exist yet. The load
    // is on the centreline, so it is its own mirror image.
    let model = WithExternalLoad {
        inner: PhysicalForces,
        extra: Load {
            f: Vec3::new(150.0, 0.0, 0.0),
            r: Vec3::ZERO,
        },
    };

    let start = BoatState {
        u: 2.5,
        v: 0.2,
        r: 0.05,
        delta_r: 0.12,
        ..Simulation::initial_state(&p)
    };
    let controls = Controls {
        rudder_rate_cmd: 0.4,
        sheet_rate_cmd: -0.2,
        sheet_release: false,
    };

    let mut a = start;
    let mut b = mirror_state(&start);
    let cb = mirror_controls(&controls);
    for i in 0..steps {
        a = step(&a, &controls, &p, &model, p.sim.dt, p.sim.integrator);
        b = step(&b, &cb, &p, &model, p.sim.dt, p.sim.integrator);
        let want = mirror_state(&a).to_array();
        let got = b.to_array();
        for k in 0..STATE_LEN {
            assert!(
                (got[k] - want[k]).abs() < 1e-10,
                "step {i}, field {}: {} vs {}",
                STATE_FIELDS[k],
                got[k],
                want[k]
            );
        }
    }
    // The assertion is only worth something if the boat went somewhere.
    assert!(a.x.abs() + a.y.abs() > 1.0, "the boat never moved");
    assert!(a.y.abs() > 0.1, "the trajectory never left the centreline");
}

// ---------------------------------------------------------------------------
// brief §35 — force sign sanity
// ---------------------------------------------------------------------------

#[test]
fn force_sign_sanity() {
    // Drag opposes motion. The hull terms are the ones with an unambiguous
    // sign for every DOF (F6.6); a foil's lift is perpendicular to its own
    // local flow and has no such relation to `v` on its own, and the F6.7
    // righting moment is a function of `φ` rather than of `p`. So the foils
    // are switched off by zeroing their area and the righting arm by zeroing
    // `GM` and `GZ_max`, which makes every `GZ` coefficient exactly zero.
    // Righting has its own sign guard, `stability::hydrostatics::
    // restoring_moment_sign`, and its own trajectory guard,
    // `roll_mirror_symmetry`.
    let mut p = params();
    p.board.section.area = 0.0;
    p.rudder.section.area = 0.0;
    p.sail.section.area = 0.0;
    p.stability.gm = 0.0;
    p.stability.gz_max = 0.0;

    let mut rng = Lcg(0x5164_0000_1111_2222);
    for k in 0..200 {
        let st = BoatState {
            phi: rng.range(-3.2, 3.2),
            u: rng.range(-8.0, 8.0),
            v: rng.range(-3.0, 3.0),
            r: rng.range(-2.0, 2.0),
            p: rng.range(-2.0, 2.0),
            ..Simulation::initial_state(&p)
        };
        let g = sailgym_physics::forces::evaluate(
            &st,
            &Controls::default(),
            &p,
            &sailgym_physics::testkit::still_air(),
            0.0,
        )
        .total;
        assert!(g.x * st.u <= 0.0, "case {k}: surge");
        assert!(g.y * st.v <= 0.0, "case {k}: sway");
        assert!(g.n * st.r <= 0.0, "case {k}: yaw");
        assert!(g.k * st.p <= 0.0, "case {k}: roll");
    }
}

// ---------------------------------------------------------------------------
// brief §35 — velocity scaling
// ---------------------------------------------------------------------------

#[test]
fn velocity_scaling() {
    // The foils are exactly quadratic in the flow speed at a fixed angle of
    // attack, which is the `F ∝ V²` brief §35 asks for. The hull is
    // deliberately not included: it is linear + quadratic by construction
    // (F6.6) and is not claimed to scale as `V²`.
    let p = params();
    let leeway = 0.08_f64;
    let magnitude = |speed: f64, delta_r: f64| {
        let st = BoatState {
            u: speed * leeway.cos(),
            v: speed * leeway.sin(),
            delta_r,
            ..Simulation::initial_state(&p)
        };
        let board = sailgym_physics::hydro::centerboard::centerboard_load(&st, &p)
            .load
            .f
            .length();
        let rudder = sailgym_physics::hydro::rudder::rudder_load(&st, &p)
            .load
            .f
            .length();
        (board, rudder)
    };

    let speeds: [f64; 4] = [1.0, 2.0, 4.0, 6.0];
    for &delta_r in &[0.0, 0.15, -0.3] {
        for w in speeds.windows(2) {
            let want = (w[1] / w[0]).powi(2);
            let (b1, r1) = magnitude(w[0], delta_r);
            let (b2, r2) = magnitude(w[1], delta_r);
            for (lo, hi, which) in [(b1, b2, "board"), (r1, r2, "rudder")] {
                let got = hi / lo;
                assert!(
                    (got - want).abs() < 0.02 * want,
                    "{which} at δr = {delta_r}: F({})/F({}) = {got}, expected {want}",
                    w[1],
                    w[0]
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// brief §35 — zero-flow foil behaviour
// ---------------------------------------------------------------------------

#[test]
fn zero_flow_foil_behaviour() {
    // Exactly zero, not merely small (F5.3).
    let p = params();
    for &delta_r in &[0.0, 0.3, -0.6] {
        let st = BoatState {
            delta_r,
            ..Simulation::initial_state(&p)
        };
        let board = sailgym_physics::hydro::centerboard::centerboard_load(&st, &p);
        let rudder = sailgym_physics::hydro::rudder::rudder_load(&st, &p);
        assert_eq!(board.load.f, Vec3::ZERO);
        assert_eq!(rudder.load.f, Vec3::ZERO);
        assert_eq!(board.v_local, Vec2::ZERO);
        assert_eq!(rudder.v_local, Vec2::ZERO);
    }
}

// ---------------------------------------------------------------------------
// brief §35 — dissipative behaviour
// ---------------------------------------------------------------------------

#[test]
fn dissipative_behaviour() {
    let p = params();
    let c = Controls::default();
    let mut rng = Lcg(0xD155_0000_3333_4444);
    for k in 0..50 {
        let mut st = BoatState {
            u: rng.range(-6.0, 6.0),
            v: rng.range(-2.0, 2.0),
            r: rng.range(-1.5, 1.5),
            p: rng.range(-1.5, 1.5),
            delta_r: rng.range(-p.rudder.delta_r_max, p.rudder.delta_r_max),
            ..Simulation::initial_state(&p)
        };
        let mut prev = mechanical_energy(&st, &p);
        for i in 0..steps_for(5.0, &p) {
            st = step(&st, &c, &p, &PhysicalForces, p.sim.dt, p.sim.integrator);
            let e = mechanical_energy(&st, &p);
            assert!(
                e <= prev + 1e-9,
                "episode {k}, step {i}: energy rose {prev} -> {e}"
            );
            prev = e;
        }
    }
}

// ---------------------------------------------------------------------------
// brief §35 — coordinate-frame consistency
// ---------------------------------------------------------------------------

#[test]
fn coordinate_frame_consistency() {
    // Rotating the whole setup — initial pose and wind bearing — in world
    // coordinates rotates the trajectory and leaves the intrinsic dynamics
    // alone. The body-frame quantities must be **unchanged**, not rotated;
    // that is the actual content of the test.
    let p = params();
    let steps = steps_for(20.0, &p);
    let controls = Controls {
        rudder_rate_cmd: 0.3,
        ..Controls::default()
    };
    let bearing_deg = 225.0;

    let run = |theta: f64| {
        let mut st = BoatState {
            x: 12.0,
            y: -5.0,
            psi: 0.3,
            u: 3.0,
            v: 0.1,
            ..Simulation::initial_state(&p)
        };
        // Rotate the pose by θ about the world origin.
        let (s, c) = theta.sin_cos();
        st = BoatState {
            x: st.x * c - st.y * s,
            y: st.x * s + st.y * c,
            psi: st.psi + theta,
            ..st
        };
        // Rotate the actual field, so this also exercises the sail in M4.
        let wind = sailgym_physics::testkit::uniform_wind(5.0, bearing_deg - theta.to_degrees());
        let model = WithExternalLoad {
            inner: sailgym_physics::forces::WindForces { wind: &wind },
            extra: Load {
                f: Vec3::new(150.0, 0.0, 0.0),
                r: Vec3::ZERO,
            },
        };
        let mut out = Vec::with_capacity(steps as usize);
        for _ in 0..steps {
            st = step(&st, &controls, &p, &model, p.sim.dt, p.sim.integrator);
            out.push(st);
        }
        out
    };

    let base = run(0.0);
    for theta_deg in [30.0_f64, 90.0, 217.0] {
        let theta = theta_deg.to_radians();
        let (s, c) = theta.sin_cos();
        let rotated = run(theta);
        for (i, (a, b)) in base.iter().zip(rotated.iter()).enumerate() {
            let want_x = a.x * c - a.y * s;
            let want_y = a.x * s + a.y * c;
            assert!(
                (b.x - want_x).abs() < 1e-9 && (b.y - want_y).abs() < 1e-9,
                "θ = {theta_deg}°, step {i}: ({}, {}) vs ({want_x}, {want_y})",
                b.x,
                b.y
            );
            for (got, base_value, name) in [
                (b.u, a.u, "u"),
                (b.v, a.v, "v"),
                (b.r, a.r, "r"),
                (b.p, a.p, "p"),
            ] {
                assert!(
                    (got - base_value).abs() < 1e-11,
                    "θ = {theta_deg}°, step {i}: {name} = {got}, expected {base_value}"
                );
            }
        }
        let last = base.last().expect("a non-empty trajectory");
        assert!(last.x.abs() + last.y.abs() > 1.0, "the boat never moved");
    }
}

// ---------------------------------------------------------------------------
// brief §35 — finite-number invariant
// ---------------------------------------------------------------------------

#[test]
fn finite_number_invariant() {
    let p = params();
    let steps = steps_for(30.0, &p);
    let mut rng = Lcg(0xF171_0000_5555_6666);
    for k in 0..200 {
        let mut st = BoatState {
            x: rng.range(-200.0, 200.0),
            y: rng.range(-200.0, 200.0),
            psi: rng.range(-3.2, 3.2),
            phi: rng.range(-3.2, 3.2),
            u: rng.range(-8.0, 8.0),
            v: rng.range(-3.0, 3.0),
            r: rng.range(-2.0, 2.0),
            p: rng.range(-2.0, 2.0),
            beta: rng.range(-1.7, 1.7),
            beta_dot: rng.range(-1.0, 1.0),
            delta_r: rng.range(-p.rudder.delta_r_max, p.rudder.delta_r_max),
            l_sheet: rng.range(p.sheet.l_sheet_min, p.sheet.l_sheet_max),
            t: 0.0,
        };
        // A randomised external load, re-rolled twice a second, standing in
        // for the sail that does not exist yet.
        let mut model = WithExternalLoad {
            inner: PhysicalForces,
            extra: Load::default(),
        };
        let mut c = Controls::default();
        for i in 0..steps {
            if i % 100 == 0 {
                model.extra = Load {
                    f: Vec3::new(rng.range(-400.0, 400.0), rng.range(-400.0, 400.0), 0.0),
                    r: Vec3::new(rng.range(-2.0, 2.0), 0.0, rng.range(0.0, 2.5)),
                };
                c = Controls {
                    rudder_rate_cmd: rng.range(-1.0, 1.0),
                    sheet_rate_cmd: rng.range(-1.0, 1.0),
                    sheet_release: rng.unit() < 0.1,
                };
            }
            st = step(&st, &c, &p, &model, p.sim.dt, p.sim.integrator);
            assert!(st.is_finite(), "episode {k}, step {i}: {:?}", st.to_array());
        }
    }
}

#[test]
fn sail_mirror_symmetry_trajectory() {
    use sailgym_physics::environment::wind::{WindConfig, WindMode};
    let p = params();
    let start = BoatState {
        psi: 0.2,
        phi: 0.1,
        beta: -0.4,
        u: 2.0,
        v: 0.1,
        r: 0.02,
        p: -0.01,
        ..Simulation::initial_state(&p)
    };
    let mut a = Simulation::new(p, 505);
    let mut b = Simulation::new(p, 505);
    a.set_wind(WindConfig {
        mode: WindMode::Uniform,
        speed: 3.5,
        bearing_deg: 135.0,
        ..Default::default()
    });
    b.set_wind(WindConfig {
        mode: WindMode::Uniform,
        speed: 3.5,
        bearing_deg: 45.0,
        ..Default::default()
    });
    a.reset(start, 505);
    b.reset(mirror_state(&start), 505);
    for i in 0..steps_for(30.0, &p) {
        let c = Controls {
            rudder_rate_cmd: if i < 800 { 0.1 } else { 0.0 },
            ..Default::default()
        };
        a.set_controls(c);
        b.set_controls(mirror_controls(&c));
        a.advance(1);
        b.advance(1);
        for ((actual, expected), name) in b
            .state()
            .to_array()
            .into_iter()
            .zip(mirror_state(a.state()).to_array())
            .zip(STATE_FIELDS)
        {
            assert!(
                (actual - expected).abs() < 1e-9,
                "step {i}, {name}: {actual} vs {expected}"
            );
        }
    }
    assert!(a.state().beta != start.beta && a.state().x.abs() > 1.0);
}

#[test]
fn no_sail_angle_command() {
    fn check(dir: &std::path::Path) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                check(&path);
                continue;
            }
            if path.extension().is_none_or(|x| x != "rs") {
                continue;
            }
            if matches!(
                path.file_name().unwrap().to_str().unwrap(),
                "integrator.rs" | "state.rs"
            ) {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            let compact: String = source.chars().filter(|c| !c.is_whitespace()).collect();
            for (_, tail) in compact
                .match_indices(".beta")
                .map(|(i, _)| (i, &compact[i + 5..]))
            {
                let assignment = (tail.starts_with('=') && !tail.starts_with("=="))
                    || ["+=", "-=", "*=", "/="]
                        .iter()
                        .any(|op| tail.starts_with(op));
                assert!(
                    !assignment,
                    "boom angle assignment outside integrator/state: {}",
                    path.display()
                );
            }
        }
    }
    check(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
}

#[test]
fn apparent_wind_consistency() {
    use sailgym_physics::diagnostics::diagnostics;
    let p = params();
    let mut sim = Simulation::new(p, 506);
    sim.reset(
        BoatState {
            psi: 0.7,
            phi: 0.4,
            u: 2.0,
            v: -0.3,
            r: 0.2,
            p: 0.1,
            beta: -0.5,
            ..Simulation::initial_state(&p)
        },
        506,
    );
    for _ in 0..200 {
        sim.advance(1);
        let st = sim.state();
        let w = sim.wind_at_boat();
        // Independent CG derivation: the angular contribution is zero at CG.
        let ax = w.x * st.psi.cos() + w.y * st.psi.sin() - st.u;
        let ay = -w.x * st.psi.sin() + w.y * st.psi.cos() - st.v;
        let expected = Vec3::new(ax, ay * st.phi.cos(), -ay * st.phi.sin());
        let d = diagnostics(&sim);
        assert!((d.apparent_wind_body - expected).length() < 1e-12);
        assert!((d.apparent_wind_speed - expected.length()).abs() < 1e-12);
        assert!((d.apparent_wind_angle - expected.y.atan2(-expected.x)).abs() < 1e-12);
        assert_eq!(d.t, st.t);
        assert_eq!(d.steps, sim.steps());
    }
}

#[test]
fn tack_through_wind() {
    use sailgym_physics::environment::wind::{WindConfig, WindMode};
    let p = params();
    let mut sim = Simulation::new(p, 507);
    sim.set_wind(WindConfig {
        mode: WindMode::Uniform,
        speed: 3.5,
        bearing_deg: 135.0,
        ..Default::default()
    });
    sim.reset(
        BoatState {
            u: 3.0,
            beta: -0.4,
            // Section 06 made the sheet real, and `initial_state` starts
            // sheeted hard in. This M4 invariant is about the **boom**
            // crossing naturally when the sail unloads, so it keeps the boom
            // free: at `l_sheet_max` the rope is slack over the whole boom
            // range (`ℓ = 4.5 m` at `|β| = 1.757 > beta_max`) and the element
            // contributes exactly nothing. No assertion here was touched.
            ..boom_free_state(&p)
        },
        507,
    );
    let wind_heading = -std::f64::consts::FRAC_PI_4;
    let mut crossed = false;
    let mut changed_side = false;
    let mut max_jump = 0.0_f64;
    for i in 0..steps_for(40.0, &p) {
        // Build rudder deflection for four seconds, then release to self-centre.
        sim.set_controls(Controls {
            rudder_rate_cmd: if i < 800 { 0.1 } else { 0.0 },
            ..Default::default()
        });
        let before = *sim.state();
        sim.advance(1);
        let after = sim.state();
        crossed |= before.psi > wind_heading && after.psi <= wind_heading;
        changed_side |= crossed && after.beta > 0.0;
        max_jump = max_jump.max((after.beta - before.beta).abs());
        assert!(
            (after.beta - before.beta).abs() < 0.3,
            "step {i}: discontinuous boom"
        );
    }
    assert!(
        crossed && changed_side,
        "wind crossed={crossed}, boom crossed={changed_side}"
    );
    eprintln!(
        "tack: max boom step {max_jump:.9} rad, final psi={}, beta={}",
        sim.state().psi,
        sim.state().beta
    );
}

// ---------------------------------------------------------------------------
// brief §35 — sheet unilateral constraint (section 06)
// ---------------------------------------------------------------------------

/// The tension the derivative sees at this state and command, computed exactly
/// the way `forces::evaluate` computes it.
fn sheet_tension(st: &BoatState, c: &Controls, p: &BoatParameters) -> f64 {
    sheet_output(st, sheet_rate(c, st, p), p).tension
}

/// Total mechanical energy: kinetic, plus **the sheet's elastic term**
/// `½·k_sheet·max(0, e)²`, the boom's soft-limit spring, and **the roll
/// potential** `Δ·g·∫₀^φ GZ ds` (F6.7).
///
/// A version that omitted any of these would report energy vanishing into a
/// conservative store and reappearing out of it; the accounting has to be
/// complete or the invariant is meaningless. The roll potential joined the
/// account in section 07, when hydrostatic righting became a real force.
fn mechanical_energy(st: &BoatState, p: &BoatParameters) -> f64 {
    let extension = (rope_path_length(st.beta, p) - st.l_sheet).max(0.0);
    let past_stop = (st.beta.abs() - p.sail.beta_max).max(0.0);
    kinetic_energy(st, p)
        + 0.5 * p.sail.i_boom * st.beta_dot * st.beta_dot
        + 0.5 * p.sheet.k_sheet * extension * extension
        + 0.5 * p.sail.k_lim * past_stop * past_stop
        + roll_potential(st, p)
}

/// `Δ·g·∫₀^φ GZ(s) ds` — the potential energy stored in heel (F6.7).
fn roll_potential(st: &BoatState, p: &BoatParameters) -> f64 {
    p.total_mass() * sailgym_physics::constants::G * GzCurve::from_params(p).gz_integral(st.phi)
}

#[test]
fn sheet_unilateral_constraint() {
    // brief §11, brief §35. 50 full 60 s episodes under randomised control
    // sequences: `T >= 0` at every step, with no exception anywhere.
    let p = params();
    let mut rng = Lcg(0x5EE7_1111_2222_3333);
    for episode in 0..50 {
        let mut sim = Simulation::new(p, episode);
        sim.set_wind(sailgym_physics::environment::wind::WindConfig {
            mode: sailgym_physics::environment::wind::WindMode::Gust,
            speed: rng.range(0.0, 9.0),
            bearing_deg: rng.range(0.0, 360.0),
            ..Default::default()
        });
        sim.reset(
            BoatState {
                psi: rng.range(-3.0, 3.0),
                u: rng.range(-1.0, 4.0),
                beta: rng.range(-1.7, 1.7),
                beta_dot: rng.range(-2.0, 2.0),
                l_sheet: rng.range(p.sheet.l_sheet_min, p.sheet.l_sheet_max),
                ..Simulation::initial_state(&p)
            },
            episode,
        );
        let mut c = Controls::default();
        for i in 0..steps_for(60.0, &p) {
            if i % 200 == 0 {
                c = Controls {
                    rudder_rate_cmd: rng.range(-1.0, 1.0),
                    sheet_rate_cmd: rng.range(-1.0, 1.0),
                    sheet_release: rng.unit() < 0.2,
                };
                sim.set_controls(c);
            }
            sim.advance(1);
            let t = sheet_tension(sim.state(), &c, &p);
            assert!(
                t >= 0.0,
                "episode {episode}, step {i}: T = {t}, {:?}",
                sim.state()
            );
        }
    }
}

/// True when the sheet is carrying load at this state and command — the taut
/// branch of v2 F18.1b, evaluated exactly as `forces::evaluate` evaluates it.
fn sheet_is_taut(st: &BoatState, c: &Controls, p: &BoatParameters) -> bool {
    sheet_output(st, sheet_rate(c, st, p), p).extension > 0.0
}

/// Ten 20 s episodes of the sheet transient, with the take-up steps found
/// rather than avoided. Returns `(worst rise at a take-up or let-go step,
/// worst rise anywhere else, number of transitions)`, in joules.
fn sheet_energy_excursions(dt: f64, episodes: u64, seconds: f64) -> (f64, f64, usize) {
    let mut p = params();
    p.sim = sailgym_physics::parameters::SimParams { dt, ..p.sim };
    let c = Controls::default();
    let mut rng = Lcg(0x0E0E_5555_6666_7777);
    let free_at_centre = rope_path_length(0.0, &p);
    let (mut worst_event, mut worst_quiet) = (0.0_f64, 0.0_f64);
    let mut transitions = 0usize;
    for episode in 0..episodes {
        let mut sim = Simulation::new(p, episode);
        sim.set_wind(sailgym_physics::environment::wind::WindConfig {
            speed: 0.0,
            ..Default::default()
        });
        sim.reset(
            BoatState {
                beta_dot: rng.range(-2.5, 2.5),
                l_sheet: rng.range(free_at_centre + 0.05, 3.0),
                ..Simulation::initial_state(&p)
            },
            episode,
        );
        sim.set_controls(c);
        let mut previous = mechanical_energy(sim.state(), &p);
        let mut was_taut = sheet_is_taut(sim.state(), &c, &p);
        for _ in 0..(seconds / dt) as u32 {
            sim.advance(1);
            let taut = sheet_is_taut(sim.state(), &c, &p);
            let energy = mechanical_energy(sim.state(), &p);
            let rise = energy - previous;
            if taut == was_taut {
                worst_quiet = worst_quiet.max(rise);
            } else {
                transitions += 1;
                worst_event = worst_event.max(rise);
            }
            was_taut = taut;
            previous = energy;
        }
    }
    (worst_event, worst_quiet, transitions)
}

#[test]
fn sheet_does_no_negative_work() {
    // Zero wind, no sheet command, kinetic energy put into the boom. The rope
    // and the gooseneck may take energy out; nothing may put any in.
    //
    // **v2 F18.1b changed what this can claim, and it is claimed precisely.**
    // The corrected law is `T = 0` on the closed slack set and
    // `max(0, k·e + c·ė)` above it, which is *discontinuous* at take-up — the
    // price of a rope that no longer pulls while slack. Across a take-up step
    // RK2's two stages can sit on opposite sides of the boundary, and the step
    // is then not a consistent approximation to anything: it can inject energy.
    //
    // So the invariant is split at the event, which is the honest reading of
    // brief §35 for a non-smooth model:
    //
    // * **away from a transition the energy never rises at all** — the bound is
    //   the same `+1e-9 J` per step the other two dissipation tests use, and
    //   the measured worst rise is exactly `0.0 J`;
    // * **at a transition it may rise, and the rise must vanish with `dt`** —
    //   which is what says the excursion is an integration artefact of the
    //   event and not a hole in the force model.
    //
    // The fixture no longer has to avoid `l_sheet_min`: v2 F18.1b made the
    // shortest sheet the geometric minimum, so the permanently-stretched,
    // undamped `β = 0` mode `docs/v1/progress/06-handoff.md` §2.5 recorded
    // does not exist any more.
    let (worst_event, worst_quiet, transitions) = sheet_energy_excursions(0.005, 10, 20.0);
    assert!(
        worst_quiet <= 1e-9,
        "energy rose by {worst_quiet} J away from any slack/taut transition"
    );
    assert!(
        transitions > 20,
        "only {transitions} slack/taut transitions: the event branch is untested"
    );
    eprintln!(
        "sheet_does_no_negative_work: {transitions} transitions, worst rise at one \
         {worst_event:.6e} J, worst rise elsewhere {worst_quiet:.3e} J"
    );

    // The event excursion, refined. Halving `dt` twice must cut the worst rise
    // by at least four — first order or better — or the excursion is not an
    // event artefact and RV50 has fired.
    let coarse = sheet_energy_excursions(0.01, 4, 10.0).0;
    let medium = sheet_energy_excursions(0.005, 4, 10.0).0;
    let fine = sheet_energy_excursions(0.0025, 4, 10.0).0;
    eprintln!(
        "sheet_does_no_negative_work: worst take-up rise {coarse:.4e} -> {medium:.4e} -> \
         {fine:.4e} J at dt = 0.01, 0.005, 0.0025"
    );
    assert!(coarse > 0.0, "no take-up excursion was produced at all");
    assert!(
        fine < 0.25 * coarse,
        "the take-up energy excursion did not fall with dt: {coarse:.4e} -> {fine:.4e} J. \
         That is RV50: diagnose the law/integrator boundary, do not widen the bound"
    );
}

/// **v2 F18.1b, defect 3, along a trajectory.** A slack rope carries no
/// tension, whatever the extension rate. The old `max(0, k·e + c·ė)` gave a
/// rope hanging 0.5 m loose 5 kN of pull, and the boom rates of a gybe reach
/// that regime; this walks 40 wind-driven episodes with the sheet worked hard
/// and asserts the tension is **exactly** zero at every step the rope is slack.
#[test]
fn slack_sheet_carries_no_tension() {
    let p = params();
    let mut rng = Lcg(0x51AC_0011_2233_4455);
    let mut slack_steps = 0usize;
    let mut taut_steps = 0usize;
    for episode in 0..40 {
        let mut sim = Simulation::new(p, episode);
        sim.set_wind(sailgym_physics::environment::wind::WindConfig {
            mode: sailgym_physics::environment::wind::WindMode::Gust,
            speed: rng.range(2.0, 9.0),
            bearing_deg: rng.range(0.0, 360.0),
            ..Default::default()
        });
        sim.reset(
            BoatState {
                psi: rng.range(-3.0, 3.0),
                u: rng.range(0.0, 4.0),
                beta: rng.range(-1.7, 1.7),
                beta_dot: rng.range(-4.0, 4.0),
                l_sheet: rng.range(p.sheet.l_sheet_min, p.sheet.l_sheet_max),
                ..Simulation::initial_state(&p)
            },
            episode,
        );
        let mut c = Controls::default();
        for i in 0..steps_for(30.0, &p) {
            if i % 120 == 0 {
                // Worked hard on purpose: hauling and releasing are what make
                // `ė` large enough for the old law to fake tension.
                c = Controls {
                    rudder_rate_cmd: rng.range(-1.0, 1.0),
                    sheet_rate_cmd: rng.range(-1.0, 1.0),
                    sheet_release: rng.unit() < 0.35,
                };
                sim.set_controls(c);
            }
            sim.advance(1);
            let st = sim.state();
            let out = sheet_output(st, sheet_rate(&c, st, &p), &p);
            if out.extension > 0.0 {
                taut_steps += 1;
            } else {
                slack_steps += 1;
                assert_eq!(
                    out.tension, 0.0,
                    "episode {episode}, step {i}: slack rope (e = {} m) pulls with {} N",
                    out.extension, out.tension
                );
                assert_eq!(out.m_beta, 0.0);
                assert_eq!(out.boom_load.f, Vec3::ZERO);
                assert_eq!(out.hull_load.f, Vec3::ZERO);
            }
        }
    }
    // Both branches must have been walked, or the assertion is about nothing.
    assert!(slack_steps > 10_000, "only {slack_steps} slack steps");
    assert!(taut_steps > 10_000, "only {taut_steps} taut steps");
    eprintln!(
        "slack_sheet_carries_no_tension: {slack_steps} slack steps, {taut_steps} taut steps, \
         zero tension on every slack one"
    );
}

/// **v2 F18.1b, defect 2, along a trajectory.** A fresh simulation and every
/// shipped scenario start with the rope unloaded. v1's `l_sheet_min = 0.90 m`
/// sat 0.1404 m below the shortest path the rig can take, so the boat began
/// every episode with 2.81 kN of tension nobody had asked for — at the one boom
/// angle where `dℓ/dβ = 0` and the element has no damping at all.
#[test]
fn no_sheet_preload_at_rest() {
    use sailgym_physics::rigging::mainsheet::min_rope_path;

    let p = params();
    let (l_min, beta_min) = min_rope_path(&p);
    assert_eq!(p.sheet.l_sheet_min, l_min);

    let st = Simulation::initial_state(&p);
    let out = sheet_output(&st, 0.0, &p);
    assert_eq!(out.extension, 0.0, "the fresh state is not two-blocked");
    assert_eq!(out.tension, 0.0);

    // And it stays unloaded while nothing disturbs it: no wind, no command.
    let mut sim = Simulation::new(p, 808);
    sim.set_wind(sailgym_physics::environment::wind::WindConfig {
        speed: 0.0,
        ..Default::default()
    });
    sim.reset(st, 808);
    for i in 0..steps_for(10.0, &p) {
        sim.advance(1);
        let out = sheet_output(sim.state(), 0.0, &p);
        assert_eq!(
            out.tension, 0.0,
            "step {i}: the sheet loaded itself from rest, e = {} m",
            out.extension
        );
    }
    assert_eq!(sim.state().beta, beta_min, "the boom left the centreline");

    // No shipped scenario starts stretched either — `tack` alone starts
    // loaded, and it is loaded because its boom is trimmed at 35°.
    for name in sailgym_physics::scenario::shipped_names() {
        let sc = sailgym_physics::scenario::load_shipped(name).expect("a shipped scenario");
        let sp = sc.to_parameters().expect("valid scenario parameters");
        let out = sheet_output(&sc.to_boat_state(), 0.0, &sp);
        let expected = if name == "tack" { 100.0 } else { 0.0 };
        assert!(
            out.tension <= expected,
            "{name} starts with {} N of sheet tension",
            out.tension
        );
    }
}

/// The three v1 defects, reconstructed, each shown **failing** the rule that
/// replaced it.
///
/// v2 section 08's acceptance asks that each original defect fail under the old
/// implementation and pass under the correction. The old implementation is
/// gone, so each half of this test rebuilds the one expression that was wrong —
/// the three-harmonic series, the old tension bracket, the old sheet stop — and
/// measures it against the corrected rule. Without this the three regressions
/// above would be assertions nobody had ever seen fail.
#[test]
fn the_three_v1_defects_are_reproducible() {
    use std::f64::consts::PI;

    let p = params();
    let s = p.stability;

    // --- Defect 1: the three-harmonic fit of F6.7, with v1's GM = 1.00 m ----
    //
    // `GZ = c1 sin φ + c2 sin 2φ + c3 sin 3φ`, solved from the slope at the
    // origin, the value at `φ_p` and the zero at `φ_v` — the whole of v1's
    // system, written out. `gm = 1.00` is v1's shipped value.
    let sines3 = |phi: f64| {
        let (sn, cs) = phi.sin_cos();
        [sn, 2.0 * sn * cs, sn * (3.0 - 4.0 * sn * sn)]
    };
    let det3 = |m: [[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let a3 = [[1.0, 2.0, 3.0], sines3(s.phi_peak), sines3(s.phi_vanish)];
    let d = det3(a3);
    let rhs = [1.00, s.gz_max, 0.0];
    let c3: Vec<f64> = (0..3)
        .map(|k| {
            let mut m = a3;
            for (row, value) in m.iter_mut().zip(rhs) {
                row[k] = value;
            }
            det3(m) / d
        })
        .collect();
    let old_gz = |phi: f64| {
        let sn = sines3(phi);
        c3[0] * sn[0] + c3[1] * sn[1] + c3[2] * sn[2]
    };

    // The defect, measured: positive righting well past the vanishing angle,
    // peaking above `GZ_max` itself.
    let mut worst = (0.0_f64, f64::NEG_INFINITY);
    for i in 1..200_000 {
        let phi = s.phi_vanish + (PI - s.phi_vanish) * (i as f64) / 200_000.0;
        if old_gz(phi) > worst.1 {
            worst = (phi, old_gz(phi));
        }
    }
    assert!(
        worst.1 > 0.7,
        "the v1 three-harmonic curve was reconstructed wrongly: worst GZ past phi_v is {}",
        worst.1
    );
    assert!(
        (worst.0.to_degrees() - 142.2).abs() < 0.5,
        "the v1 peak past phi_v was at 142.2 deg, this reconstruction says {}",
        worst.0.to_degrees()
    );
    // And the corrected curve, on the same tunables, does not.
    let curve = GzCurve::from_params(&p);
    assert!(curve.gz(worst.0) < 0.0);
    // The rule that rejects it: v1's own `gm = 1.00 m` no longer fits at all.
    assert!(
        GzCurve::fit(
            1.00,
            s.phi_peak,
            s.gz_max,
            s.phi_vanish,
            GzCurve::gz_envelope(&p)
        )
        .is_err(),
        "GM = 1.00 m must be rejected by the v2 F18.1a rules"
    );
    eprintln!(
        "v1 defect 1 reproduced: three-harmonic GZ = {:.6} m at {:.2} deg, past phi_v; \
         corrected curve reads {:.6} m there",
        worst.1,
        worst.0.to_degrees(),
        curve.gz(worst.0)
    );

    // --- Defect 2: the v1 sheet stop --------------------------------------
    let (l_min, beta_min) = sailgym_physics::rigging::mainsheet::min_rope_path(&p);
    let v1_stop = 0.90;
    let preload = p.sheet.k_sheet * (l_min - v1_stop);
    assert!(
        (preload - 2808.7).abs() < 1.0,
        "the v1 preload was 2808.7 N, this reconstruction says {preload}"
    );
    assert!(l_min > v1_stop);
    let mut v1_params = p;
    v1_params.sheet.l_sheet_min = v1_stop;
    let why = v1_params
        .validate()
        .expect_err("l_sheet_min = 0.90 m must now be rejected");
    assert!(format!("{why}").contains("l_sheet_min"), "{why}");
    // The corrected stop carries nothing at the minimising angle.
    assert_eq!(
        sheet_output(
            &BoatState {
                beta: beta_min,
                l_sheet: p.sheet.l_sheet_min,
                ..BoatState::ZERO
            },
            0.0,
            &p
        )
        .tension,
        0.0
    );
    eprintln!(
        "v1 defect 2 reproduced: l_sheet_min = {v1_stop} m against a {l_min} m path, \
         {preload:.1} N of preload; the corrected stop carries 0 N"
    );

    // --- Defect 3: the old tension bracket ---------------------------------
    //
    // `max(0, k·e + c·ė)` with no slack branch, evaluated on states the model
    // can actually reach: the sheet command alone supplies `|L̇| ≤ 6 m/s`, and
    // `(dℓ/dβ)·β̇` supplies far more through a gybe.
    let mut faked = 0usize;
    let mut worst_fake = 0.0_f64;
    for slack in [0.01, 0.05, 0.2, 0.5] {
        for beta in [-1.2, -0.4, 0.4, 1.2] {
            let length = rope_path_length(beta, &p);
            for beta_dot in [2.0, 5.0, 10.0] {
                let st = BoatState {
                    beta,
                    beta_dot,
                    l_sheet: length + slack,
                    ..BoatState::ZERO
                };
                let out = sheet_output(&st, 0.0, &p);
                let extension_rate =
                    sailgym_physics::rigging::mainsheet::drope_dbeta(beta, &p) * beta_dot;
                let old_law =
                    (p.sheet.k_sheet * out.extension + p.sheet.c_sheet * extension_rate).max(0.0);
                // The corrected law is exactly zero on every one of these.
                assert_eq!(out.tension, 0.0);
                if old_law > 0.0 {
                    faked += 1;
                    worst_fake = worst_fake.max(old_law);
                }
            }
        }
    }
    assert!(
        faked > 10 && worst_fake > 1e3,
        "the v1 bracket faked tension on only {faked} reachable slack states, worst \
         {worst_fake} N; the reconstruction is wrong"
    );
    eprintln!(
        "v1 defect 3 reproduced: the old bracket pulls on {faked} slack states here, up to \
         {worst_fake:.0} N; the corrected law reads 0 N on every one"
    );
}

/// **v2 F18.1a, defect 1, along a trajectory.** Past the angle of vanishing
/// stability the righting arm is negative everywhere up to inversion, so a boat
/// pushed past `φ_v` keeps going over. v1's curve came back **positive** beyond
/// 81.6° of heel and reached `+0.785 m` at 142° — 2.6× the whole intended
/// righting budget — so the model rolled a capsized boat back upright.
#[test]
fn past_vanishing_the_boat_keeps_going_over() {
    let p = params();
    let curve = GzCurve::from_params(&p);

    // The property, on the curve itself, over the whole supported domain.
    let n = 200_000;
    for i in 1..n {
        let phi = p.stability.phi_vanish
            + (std::f64::consts::PI - p.stability.phi_vanish) * (i as f64) / (n as f64);
        assert!(
            curve.gz(phi) < 0.0,
            "GZ({phi}) = {} is positive past phi_vanish",
            curve.gz(phi)
        );
        assert!(curve.gz(-phi) > 0.0, "the mirror of the same statement");
    }

    // And along a trajectory: no wind, no command, released from just past the
    // unstable equilibrium. The boat must roll on to inversion, not back.
    let mut sim = Simulation::new(p, 909);
    sim.set_wind(sailgym_physics::environment::wind::WindConfig {
        speed: 0.0,
        ..Default::default()
    });
    sim.reset(
        BoatState {
            phi: p.stability.phi_vanish + 0.05,
            ..boom_free_state(&p)
        },
        909,
    );
    let mut lowest = f64::INFINITY;
    for i in 0..steps_for(60.0, &p) {
        sim.advance(1);
        let phi = sim.state().phi;
        assert!(sim.state().is_finite(), "step {i}");
        lowest = lowest.min(phi);
    }
    let end = sim.state().phi;
    assert!(
        lowest > p.stability.phi_vanish,
        "the boat rolled back below phi_vanish (to {lowest} rad) instead of going over"
    );
    assert!(
        (end - std::f64::consts::PI).abs() < 0.2,
        "the boat settled at {end} rad, not inverted"
    );
    eprintln!(
        "past_vanishing_the_boat_keeps_going_over: released at {:.4} rad, settled at {end:.6} rad",
        p.stability.phi_vanish + 0.05
    );
}

#[test]
fn slack_sheet_free_boom() {
    // With the sheet fully eased the rope is slack over the whole boom range,
    // so the boom must decay under the gooseneck alone: `I_b β̈ = −c_β β̇`.
    // The reference is that ODE under the **same** RK2 midpoint map and the
    // same `dt`, so what is compared is the physics, not the integrator.
    let p = params();
    let mut sim = Simulation::new(p, 606);
    sim.set_wind(sailgym_physics::environment::wind::WindConfig {
        speed: 0.0,
        ..Default::default()
    });
    let start = BoatState {
        beta_dot: 0.1,
        l_sheet: p.sheet.l_sheet_max,
        ..Simulation::initial_state(&p)
    };
    sim.reset(start, 606);

    let a = p.sail.c_beta / p.sail.i_boom;
    let dt = p.sim.dt;
    // RK2 midpoint applied to β̈ = −a β̇, written out.
    let rate_map = 1.0 - a * dt + 0.5 * a * a * dt * dt;
    let (mut beta, mut beta_dot) = (start.beta, start.beta_dot);

    for i in 0..steps_for(30.0, &p) {
        sim.advance(1);
        beta += dt * beta_dot * (1.0 - 0.5 * a * dt);
        beta_dot *= rate_map;
        let st = sim.state();
        assert_eq!(sheet_tension(st, &Controls::default(), &p), 0.0);
        assert!(
            (st.beta - beta).abs() < 1e-9 && (st.beta_dot - beta_dot).abs() < 1e-9,
            "step {i}: ({}, {}) vs pure damping ({beta}, {beta_dot})",
            st.beta,
            st.beta_dot
        );
    }
    // The boom really did move, and stayed inside its stops the whole way.
    assert!(sim.state().beta > 0.4 && sim.state().beta < p.sail.beta_max);
}

#[test]
fn sheet_geometry_continuous() {
    // `m_beta` is built from smooth geometry and one `max`, which is C0 at the
    // take-up point, so the moment has no step anywhere.
    //
    // The sweep runs at `l_sheet_max`, the one length at which the boom can
    // actually traverse the whole range: with the sheet hauled the element is
    // still continuous, but so steep that a finite difference over 1e-5 rad
    // reports its *slope*, not a discontinuity — see the section 06 handoff
    // note for the measured numbers.
    let p = params();
    let spacing = 1e-5;
    let mut worst = 0.0_f64;
    let mut previous: Option<f64> = None;
    let steps = (2.0 * std::f64::consts::PI / spacing) as i64;
    for i in 0..=steps {
        let beta = -std::f64::consts::PI + spacing * i as f64;
        let st = BoatState {
            beta,
            l_sheet: p.sheet.l_sheet_max,
            ..BoatState::ZERO
        };
        let m = sheet_output(&st, 0.0, &p).m_beta;
        if let Some(prev) = previous {
            worst = worst.max((m - prev).abs());
            assert!(
                (m - prev).abs() < 1.0,
                "step at beta = {beta}: {prev} -> {m}"
            );
        }
        previous = Some(m);
    }
    eprintln!("sheet_geometry_continuous: worst step {worst:.6} N·m over {spacing} rad");

    // Continuity, proved independently of how steep the element is: halving
    // the sample spacing must halve the largest step. A genuine discontinuity
    // would not shrink at all. This runs at every sheet length, including the
    // hauled ones where the elastic slope alone puts the 1 N·m figure out of
    // reach.
    let worst_step = |l_sheet: f64, h: f64| {
        let mut worst = 0.0_f64;
        let mut previous: Option<f64> = None;
        let n = (2.0 * std::f64::consts::PI / h) as i64;
        for i in 0..=n {
            let st = BoatState {
                beta: -std::f64::consts::PI + h * i as f64,
                l_sheet,
                ..BoatState::ZERO
            };
            let m = sheet_output(&st, 0.0, &p).m_beta;
            if let Some(prev) = previous {
                worst = worst.max((m - prev).abs());
            }
            previous = Some(m);
        }
        worst
    };
    for l_sheet in [p.sheet.l_sheet_min, 1.5, 2.5, 3.5, p.sheet.l_sheet_max] {
        let coarse = worst_step(l_sheet, 1e-3);
        let fine = worst_step(l_sheet, 5e-4);
        assert!(
            fine < 0.6 * coarse,
            "L = {l_sheet}: worst step did not shrink with the spacing, \
             {coarse} -> {fine} — that is a discontinuity, not a slope"
        );
    }
}

// ---------------------------------------------------------------------------
// Roll, righting and capsize (section 07)
// ---------------------------------------------------------------------------

/// Roll moment from one load, through the same `Generalized::add` the EOM uses
/// (F6.4). Nothing is recomputed here.
fn roll_of(l: Load, phi: f64) -> f64 {
    let mut g = sailgym_physics::dynamics::Generalized::default();
    g.add(l, phi);
    g.k
}

#[test]
fn roll_mirror_symmetry() {
    use sailgym_physics::environment::wind::{WindConfig, WindMode};
    // R3's roll guard. The mirror of a boat heeling to starboard is a boat
    // heeling to port by the same angle, at every step, all the way into a
    // capsize — the wind is strong enough here to drive one, so the check
    // covers the nonlinear part of `GZ` and not just the linear root.
    let p = params();
    let start = BoatState {
        psi: 0.35,
        phi: 0.12,
        beta: -0.5,
        u: 1.5,
        r: 0.03,
        p: 0.04,
        ..Simulation::initial_state(&p)
    };
    let mut a = Simulation::new(p, 707);
    let mut b = Simulation::new(p, 707);
    a.set_wind(WindConfig {
        mode: WindMode::Uniform,
        speed: 9.0,
        bearing_deg: 135.0,
        ..Default::default()
    });
    b.set_wind(WindConfig {
        mode: WindMode::Uniform,
        speed: 9.0,
        bearing_deg: 45.0,
        ..Default::default()
    });
    a.reset(start, 707);
    b.reset(mirror_state(&start), 707);

    let mut extreme = 0.0f64;
    for i in 0..steps_for(30.0, &p) {
        a.advance(1);
        b.advance(1);
        let (left, right) = (a.state().phi, b.state().phi);
        assert!(
            (right + left).abs() < 1e-9,
            "step {i}: phi = {left} vs mirrored {right}"
        );
        assert!(
            (a.state().p + b.state().p).abs() < 1e-9,
            "step {i}: roll rate {} vs {}",
            a.state().p,
            b.state().p
        );
        if left.abs() > extreme {
            extreme = left.abs();
        }
    }
    // Worth nothing unless the boat really went over.
    assert!(
        extreme > 1.0,
        "the boat only reached {extreme} rad of heel; the nonlinear part of GZ was never exercised"
    );
    assert_eq!(a.capsize().capsized, b.capsize().capsized);
}

#[test]
fn dissipative_with_roll() {
    // brief §35 dissipative behaviour, with roll live. Zero wind, roll and yaw
    // energy on the clock, and a **complete** account: kinetic, the sheet's
    // elastic term, the boom's soft limit, and the roll potential
    // `Δ·g·∫₀^φ GZ ds`. Hydrostatic righting is conservative, so it may move
    // energy between those stores; nothing may add any.
    let p = params();
    let c = Controls::default();
    let mut rng = Lcg(0x8011_0777_0007_1234);
    for k in 0..40 {
        let mut st = BoatState {
            phi: rng.range(-1.2, 1.2),
            u: rng.range(-3.0, 3.0),
            v: rng.range(-1.0, 1.0),
            r: rng.range(-1.2, 1.2),
            p: rng.range(-1.5, 1.5),
            ..boom_free_state(&p)
        };
        let mut prev = mechanical_energy(&st, &p);
        let first = prev;
        for i in 0..steps_for(20.0, &p) {
            st = step(&st, &c, &p, &PhysicalForces, p.sim.dt, p.sim.integrator);
            let e = mechanical_energy(&st, &p);
            assert!(
                e <= prev + 1e-9,
                "episode {k}, step {i}: energy rose {prev} -> {e}"
            );
            prev = e;
        }
        assert!(prev < first, "episode {k} lost no energy at all");
    }
}

#[test]
fn capsize_finite() {
    use sailgym_physics::environment::wind::{WindConfig, WindMode};
    // brief §17 and brief §35's finite-number invariant, in the regime that
    // stresses them: 100 episodes blown flat and left there for a minute. The
    // simulation is never stopped, nothing is clamped, and no state may go
    // non-finite.
    let p = params();
    let mut rng = Lcg(0xCA95_1234_5678_9ABC);
    let mut capsized = 0usize;
    for episode in 0..100 {
        let mut sim = Simulation::new(p, episode);
        sim.set_wind(WindConfig {
            mode: if episode % 2 == 0 {
                WindMode::Uniform
            } else {
                WindMode::Gust
            },
            speed: rng.range(8.0, 22.0),
            bearing_deg: rng.range(0.0, 360.0),
            ..Default::default()
        });
        sim.reset(
            BoatState {
                psi: rng.range(-3.1, 3.1),
                phi: rng.range(-0.4, 0.4),
                u: rng.range(0.0, 3.0),
                p: rng.range(-1.0, 1.0),
                ..Simulation::initial_state(&p)
            },
            episode,
        );
        for i in 0..steps_for(60.0, &p) {
            sim.advance(1);
            assert!(
                sim.state().is_finite(),
                "episode {episode}, step {i}: {:?}",
                sim.state()
            );
        }
        if sim.capsize().max_heel > p.stability.phi_capsize {
            capsized += 1;
        }
    }
    // The episodes have to actually reach capsize, or this proves nothing.
    assert!(
        capsized > 50,
        "only {capsized} of 100 episodes went past the capsize threshold"
    );
}

#[test]
fn heel_reduces_drive() {
    use sailgym_physics::environment::wind::{WindConfig, WindMode};
    // F6.4 and nothing else. Heel reduces the lateral component of the
    // apparent wind at the sail through `R_x(−φ)` (F6.2 step 6) and tilts the
    // sail force out of the horizontal plane through `Generalized::add`. Both
    // are geometry; there is no empirical `cos φ` anywhere, and this is what
    // says so.
    let p = params();
    let mut sim = Simulation::new(p, 42);
    sim.set_wind(WindConfig {
        mode: WindMode::Uniform,
        speed: 5.0,
        bearing_deg: 0.0,
        ..Default::default()
    });
    sim.reset(
        BoatState {
            l_sheet: 2.0,
            ..Simulation::initial_state(&p)
        },
        42,
    );
    // Let the boat find a steady sailing state, then read the same state twice
    // with only `phi` changed.
    for _ in 0..steps_for(30.0, &p) {
        sim.advance(1);
    }
    let steady = BoatState {
        phi: 0.0,
        ..*sim.state()
    };
    let upright =
        sailgym_physics::forces::evaluate(&steady, &Controls::default(), &p, sim.wind(), steady.t);
    let heeled = sailgym_physics::forces::evaluate(
        &BoatState { phi: 0.5, ..steady },
        &Controls::default(),
        &p,
        sim.wind(),
        steady.t,
    );
    assert!(
        upright.total.x > 0.0,
        "the boat was not being driven forward"
    );
    assert!(
        heeled.total.x < upright.total.x,
        "heel did not reduce drive: {} at phi = 0.5 vs {} upright",
        heeled.total.x,
        upright.total.x
    );
    // The sail itself is what lost the force, not the hull.
    assert!(roll_of(heeled.sail, 0.5).abs() < roll_of(upright.sail, 0.0).abs());
}

#[test]
fn deterministic_replay() {
    use sailgym_physics::diagnostics::diagnostics;
    use sailgym_physics::recording::{
        iso8601_utc, Episode, EpisodeHeader, Recorder, ToolchainInfo, EPISODE_SCHEMA_VERSION,
        FRAME_LEN,
    };
    use sailgym_physics::scenario::load_shipped;

    // brief §35's deterministic-replay invariant, taken through the section
    // 09 recorder rather than only through the state: the same seed and the
    // same control sequence must reproduce every *logged* value bit for bit,
    // not merely a trajectory that looks the same.
    //
    // The scripted commands below are the test's, not the scenario's
    // (brief §32 forbids a scenario from carrying any), and they are indexed
    // by step so the two runs cannot differ in *when* a command landed.
    const SCRIPT: [(u32, f64, f64, bool); 6] = [
        (0, 0.0, -1.0, false),
        (400, 0.6, 0.0, false),
        (900, 0.0, 0.0, false),
        (1500, -0.8, 0.5, false),
        (2200, 0.0, 0.0, true),
        (3000, 0.0, -1.0, false),
    ];
    const LOG_HZ: f64 = 20.0;

    let episode = |scenario: &str| -> Episode {
        let sc = load_shipped(scenario).expect("a shipped scenario");
        let p = sc.to_parameters().expect("valid parameters");
        let mut sim = Simulation::new(p, sc.seed);
        sim.load_scenario(&sc).expect("the scenario loads");

        let mut rec = Recorder::start(
            LOG_HZ,
            EpisodeHeader {
                schema_version: EPISODE_SCHEMA_VERSION,
                scenario: sc.clone(),
                parameters: p,
                dt: p.sim.dt,
                log_hz: LOG_HZ,
                toolchain: ToolchainInfo::current(),
                // A constant, not a clock: physics reads no wall time (F9.1)
                // and neither may a test that asserts reproducibility.
                created_utc: iso8601_utc(0.0),
            },
        );

        let mut next = 0usize;
        for i in 0..steps_for(30.0, &p) {
            while next < SCRIPT.len() && SCRIPT[next].0 == i {
                let (_, rudder_rate_cmd, sheet_rate_cmd, sheet_release) = SCRIPT[next];
                sim.set_controls(Controls {
                    rudder_rate_cmd,
                    sheet_rate_cmd,
                    sheet_release,
                });
                next += 1;
            }
            sim.advance(1);
            if rec.due(sim.state().t) {
                let d = diagnostics(&sim);
                rec.observe(&sim, &d);
            }
        }
        rec.finish()
    };

    for scenario in ["beam_reach_capsize", "gybe"] {
        let a = episode(scenario);
        let b = episode(scenario);

        assert_eq!(a.header, b.header, "{scenario}: headers differ");
        assert_eq!(
            a.frames.len(),
            b.frames.len(),
            "{scenario}: frame counts differ"
        );
        assert!(
            a.frames.len() > 500,
            "{scenario}: only {} frames — 30 s at {LOG_HZ} Hz should be ~600",
            a.frames.len()
        );

        for (k, (x, y)) in a.frames.iter().zip(b.frames.iter()).enumerate() {
            let (p, q) = (x.to_array(), y.to_array());
            for i in 0..FRAME_LEN {
                assert_eq!(
                    p[i].to_bits(),
                    q[i].to_bits(),
                    "{scenario}: frame {k}, scalar {i}: {} vs {}",
                    p[i],
                    q[i]
                );
            }
            assert_eq!(x.capsized, y.capsized, "{scenario}: frame {k}");
        }

        // The episode has to have been an episode: the boat moved, the rig
        // was loaded, and the script really reached the recorder.
        let last = a.frames.last().expect("frames");
        assert!(
            last.state[0].abs() + last.state[1].abs() > 1.0,
            "{scenario}: the boat never moved"
        );
        assert!(
            a.frames.iter().any(|f| f.sheet_tension > 1.0),
            "{scenario}: the sheet never took load"
        );
        assert!(
            a.frames.iter().any(|f| f.controls[2] == 1.0),
            "{scenario}: the release command never reached a frame"
        );

        // …and it survives both serialisations unchanged, which is what the
        // browser replays from.
        let json = a.to_json().expect("json");
        assert_eq!(Episode::from_json(&json).expect("json round trip"), a);
        let bytes = a.to_binary().expect("binary");
        assert_eq!(Episode::from_binary(&bytes).expect("binary round trip"), a);
    }
}

// ---------------------------------------------------------------------------
// Section 10 — completing the brief §35 set, and the audit that keeps
// `docs/v1/invariants.md` honest (task 10.1)
// ---------------------------------------------------------------------------

#[test]
fn tension_never_negative() {
    // brief §35's unilateral constraint stated as a property of the element
    // itself rather than of a trajectory. `sheet_unilateral_constraint` above
    // covers what the simulation actually reaches; this covers what it could
    // reach if a live parameter edit (brief §31) or a hand-written scenario put
    // the rig somewhere the shipped boat never goes.
    //
    // `T = max(0, k·e + c·ė)` is structural (F6.8), so the bound is **exactly
    // zero** and is compared as such — not "within a tolerance", which would be
    // a weaker statement than the model supports. The mirror of this test in
    // `rigging::mainsheet::tests::tension_never_negative` asserts the same
    // property on the same function; keeping one here is what makes gate step 4
    // a complete reading of brief §35 on its own.
    let p = params();
    let mut rng = Lcg(0x7E45_1010_2020_3030);
    let mut ever_taut = false;
    let mut ever_slack = false;
    for k in 0..200_000 {
        // One case in seven is drawn from deliberately impossible ranges: a
        // boom well past its stops, a rope shorter than nothing and payout
        // rates no winch could produce. The element must still be a `max`.
        let wild = k % 7 == 0;
        let (beta, beta_dot, l_sheet, l_dot) = if wild {
            (
                rng.range(-8.0, 8.0),
                rng.range(-500.0, 500.0),
                rng.range(-10.0, 100.0),
                rng.range(-500.0, 500.0),
            )
        } else {
            (
                rng.range(-1.9, 1.9),
                rng.range(-6.0, 6.0),
                rng.range(p.sheet.l_sheet_min, p.sheet.l_sheet_max),
                rng.range(-p.sheet.sheet_release_rate, p.sheet.sheet_release_rate),
            )
        };
        let st = BoatState {
            beta,
            beta_dot,
            l_sheet,
            ..BoatState::ZERO
        };
        let out = sheet_output(&st, l_dot, &p);
        assert!(
            out.tension >= 0.0,
            "case {k}: T = {} at beta = {beta}, beta_dot = {beta_dot}, L = {l_sheet}, L̇ = {l_dot}",
            out.tension
        );
        assert!(
            out.tension.is_finite() && out.m_beta.is_finite(),
            "case {k}: non-finite sheet output"
        );
        if out.tension > 0.0 {
            ever_taut = true;
        } else {
            // A rope that carries no tension carries no force and no torque,
            // through the same expression — there is no slackness branch.
            assert_eq!(out.tension, 0.0, "case {k}: T is negative zero or worse");
            assert_eq!(out.m_beta, 0.0, "case {k}: slack rope produced a torque");
            assert_eq!(out.boom_load.f, Vec3::ZERO, "case {k}");
            assert_eq!(out.hull_load.f, Vec3::ZERO, "case {k}");
            ever_slack = true;
        }
    }
    // Both sides of the `max` have to have been taken, or the test is a
    // statement about one branch of a two-branch expression.
    assert!(
        ever_taut && ever_slack,
        "taut={ever_taut}, slack={ever_slack}"
    );
}

/// `docs/v1/invariants.md` lists every test in this suite, and every test it lists
/// exists (task 10.1).
///
/// The table is the brief §35 audit: an invariant with no row cannot be
/// reviewed, and a row naming a test that does not exist is a claim with
/// nothing behind it. Both directions are checked, so the document cannot
/// drift from the suite in either.
///
/// Rows name their test as `` `<path>::<fn>` `` relative to the repository
/// root, because three of the brief §35 items are proved outside this file:
/// `timestep_convergence` and `error_at_default_dt` live in
/// `tests/convergence.rs` (task 10.2), which gate step 4 also runs.
#[test]
fn documented_invariants_exist() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let doc_path = root.join("docs/v1/invariants.md");
    let doc = std::fs::read_to_string(&doc_path)
        .unwrap_or_else(|e| panic!("{}: {e}", doc_path.display()));

    // Every `` `path::fn` `` in a table row of the document.
    let mut listed: Vec<(String, String)> = Vec::new();
    for line in doc.lines().filter(|l| l.trim_start().starts_with('|')) {
        for cell in line.split('|') {
            for token in cell.split('`').skip(1).step_by(2) {
                if let Some((file, name)) = token.rsplit_once("::") {
                    if file.ends_with(".rs") {
                        listed.push((file.to_string(), name.to_string()));
                    }
                }
            }
        }
    }
    assert!(
        listed.len() >= 20,
        "docs/v1/invariants.md names only {} tests; the brief §35 set is larger than that",
        listed.len()
    );

    // 1. Every test the document names exists, in the file it names.
    let mut sources: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for (file, name) in &listed {
        let source = sources.entry(file.clone()).or_insert_with(|| {
            let path = root.join(file);
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        });
        assert!(
            source.contains(&format!("fn {name}(")),
            "docs/v1/invariants.md names {file}::{name}, which does not exist"
        );
    }

    // 2. Every test in *this* file appears in the document. A new invariant
    //    that nobody documented is exactly what this half catches.
    let here = include_str!("invariants.rs");
    let documented: std::collections::BTreeSet<&str> =
        listed.iter().map(|(_, n)| n.as_str()).collect();
    let mut undocumented = Vec::new();
    for (i, line) in here.lines().enumerate() {
        if line.trim() != "#[test]" {
            continue;
        }
        let Some(decl) = here.lines().nth(i + 1) else {
            continue;
        };
        let Some(name) = decl
            .trim()
            .strip_prefix("fn ")
            .and_then(|s| s.split('(').next())
        else {
            continue;
        };
        if !documented.contains(name) {
            undocumented.push(name.to_string());
        }
    }
    assert!(
        undocumented.is_empty(),
        "tests in tests/invariants.rs with no row in docs/v1/invariants.md: {undocumented:?}"
    );
}
