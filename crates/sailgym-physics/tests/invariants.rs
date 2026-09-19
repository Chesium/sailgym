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
/// `Simulation::initial_state` starts at `l_sheet_min`, which since section 06
/// is a loaded rope. Fixtures that mean "free boom" say so with this.
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

#[test]
fn sheet_does_no_negative_work() {
    // Zero wind, no sheet command, kinetic energy put into the boom. The rope
    // and the gooseneck may take energy out; nothing may put any in.
    //
    // The sheet lengths are drawn above `ℓ(0) = 1.0404 m`, so the rope is
    // slack with the boom on the centreline and every load it carries comes
    // with the damping that accompanies `dℓ/dβ ≠ 0`. `l_sheet_min = 0.90 m`
    // is *below* that, and a sheet hauled to it is permanently stretched at
    // the one angle where the element has no damping at all — an undamped
    // 43 rad/s mode that RK2 cannot integrate to this tolerance at any `dt`
    // the project uses. That is an integrator limit, not an accounting error:
    // the per-step rise falls off as `dt³` (0.138 J at `dt = 0.01`, 3.1e-6 J
    // at `dt = 0.00125`). The measurements and what they mean for R1 are in
    // `docs/progress/06-handoff.md`; the bound below is unchanged.
    let p = params();
    let c = Controls::default();
    let mut rng = Lcg(0x0E0E_5555_6666_7777);
    let free_at_centre = rope_path_length(0.0, &p);
    for episode in 0..10 {
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
        let first = previous;
        let mut rope_took_up = false;
        assert!(previous > 0.0);
        for i in 0..steps_for(20.0, &p) {
            sim.advance(1);
            let e = mechanical_energy(sim.state(), &p);
            assert!(
                e <= previous + 1e-9,
                "episode {episode}, step {i}: energy rose {previous} -> {e}"
            );
            previous = e;
            rope_took_up |= sheet_tension(sim.state(), &c, &p) > 0.0;
        }
        // The elastic term has to be exercised, or the test proves nothing.
        assert!(rope_took_up, "episode {episode}: the rope never took up");
        assert!(
            previous < first,
            "episode {episode}: nothing was dissipated"
        );
    }
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
