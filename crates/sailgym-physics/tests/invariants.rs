//! Physics invariant tests (brief §35).
//!
//! This file starts here, at M3, and grows through sections 05–07; section 10
//! completes it. Gate step 4 runs this target.
//!
//! **There is no sail until section 05**, so nothing here may assume the boat
//! can accelerate itself. Every test is driven by an initial velocity or by an
//! explicit external [`Load`] injected through
//! [`sailgym_physics::testkit::WithExternalLoad`] — never by a temporary
//! thrust term (`docs/04-hydro.md`, "the propulsion gap").

use sailgym_physics::dynamics::Load;
use sailgym_physics::environment::wind_from_bearing;
use sailgym_physics::forces::PhysicalForces;
use sailgym_physics::integrator::step;
use sailgym_physics::parameters::BoatParameters;
use sailgym_physics::simulation::Simulation;
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
    // local flow and has no such relation to `v` on its own.
    let mut p = params();
    p.board.section.area = 0.0;
    p.rudder.section.area = 0.0;

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
        let mut prev = kinetic_energy(&st, &p);
        for i in 0..steps_for(5.0, &p) {
            st = step(&st, &c, &p, &PhysicalForces, p.sim.dt, p.sim.integrator);
            let e = kinetic_energy(&st, &p);
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
    let model = WithExternalLoad {
        inner: PhysicalForces,
        extra: Load {
            f: Vec3::new(150.0, 0.0, 0.0),
            r: Vec3::ZERO,
        },
    };
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
        // ...and the wind with it. The sail arrives in section 05; rotating
        // the bearing here is what makes this test still hold then.
        let _wind: Vec2 = wind_from_bearing(5.0, bearing_deg - theta.to_degrees());
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
