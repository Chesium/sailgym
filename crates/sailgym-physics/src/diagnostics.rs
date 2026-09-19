//! The debug record (section 08, brief §30).
//!
//! Everything brief §30 asks to be inspectable lives in one struct, published
//! once per UI frame. `diagnostics::tests::covers_brief_30` holds the literal
//! §30 list and fails if any item stops mapping to a named field, so the
//! struct below **is** the checklist.
//!
//! ## These are the forces that moved the boat
//!
//! [`diagnostics`] does not call `forces::evaluate`. It reads
//! [`Simulation::forces`], the breakdown the simulation cached at the state it
//! publishes — which is bit-for-bit the one the next integration step's first
//! stage consumes. A second evaluation here would be a second argument list to
//! keep in step with the first, and the displayed numbers would eventually
//! disagree with the ones that moved the boat.
//!
//! The two quantities the breakdown does not carry are derived through the
//! **same** functions the equations of motion use, never restated:
//!
//! * accelerations and course come from `dynamics::derivative`, handed the
//!   cached breakdown through [`Frozen`];
//! * `C_L`/`C_D` for the board and the rudder come from `foil::cl`/`foil::cd`
//!   at the `α` the breakdown records, which is the same pure call
//!   `foil::foil_force` made internally.
//!
//! Angles are radians (F1) except `heel_deg`, which F1 permits as a UI
//! boundary quantity and which the section PRD names.

use crate::dynamics::{derivative, ForceModel, Generalized, Load};
use crate::foil::{cd, cl};
use crate::forces::ForceBreakdown;
use crate::parameters::BoatParameters;
use crate::rigging::boom::BoomMoments;
use crate::rigging::mainsheet::boom_attach_point;
use crate::simulation::Simulation;
use crate::stability::capsize::CapsizeState;
use crate::stability::hydrostatics::GzCurve;
use crate::state::{BoatState, Controls};
use crate::vec::{Vec2, Vec3};
use serde::Serialize;

/// m/s. Above this surge speed the F6.6 hull model is extrapolating: it has no
/// planing regime and no wave-making hump, so it over-predicts resistance
/// (R6, brief §15; measured in the section 04 handoff §4 — 90 % of the total
/// is already the quadratic term at 5 m/s).
///
/// Not an F7 coefficient and not tuned: it is the documented validity limit of
/// a model, published so the debug panel can say so rather than letting anyone
/// read a number off a chart as validated data. F11 R6 asks for exactly this.
pub const HULL_MODEL_VALID_TO: f64 = 5.0;

/// Everything brief §30 asks to be inspectable, for one published state.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Diagnostics {
    pub t: f64,
    pub steps: u64,

    // --- environment ------------------------------------------------------
    /// m/s, true wind at the boat, world frame — the direction the air is
    /// blowing *toward* (F6.1).
    pub true_wind_world: Vec2,
    /// m/s, the same vector rotated into the horizontal body frame `H`.
    pub true_wind_body: Vec2,
    /// m/s, apparent wind at the CG, in `B` (F6.2).
    pub apparent_wind_body: Vec3,
    pub apparent_wind_speed: f64,
    /// rad, FROM angle off the bow, positive to starboard.
    pub apparent_wind_angle: f64,

    // --- motion -----------------------------------------------------------
    /// m/s, `(u, v)` in `H`: surge forward, sway to port (F3).
    pub velocity_body: Vec2,
    pub speed_over_ground: f64,
    /// rad, the direction the boat is actually travelling in the world frame,
    /// CCW from world `+x`. `atan2(ẏ, ẋ)` from the F4.1 kinematics.
    pub course_over_ground: f64,
    /// m/s², `(u̇, v̇)` from F4.2. Derived, never integrated (brief §5).
    pub acceleration_body: Vec2,
    pub yaw_rate: f64,
    pub roll_rate: f64,
    /// rad, drift angle: the angle between the heading and the track through
    /// the water, positive when the boat is sliding to starboard. Equal to the
    /// centreboard's `α` when `r = p = 0` (section 04 handoff §2.5).
    pub leeway_angle: f64,

    // --- forces, each with its application point, all in `B` --------------
    pub sail: Load,
    pub board: Load,
    pub rudder: Load,
    pub hull: Load,
    /// The pull on the boom at `P_b` (F6.8). The PRD's field list names only
    /// the hull reaction; both ends are published because the pair is what
    /// makes the sheet a null force system on the hull, and seeing one without
    /// the other is how that gets mistaken for a bug.
    pub sheet: Load,
    /// The reaction `−F_b` on the hull at the block `P_k` (F6.8).
    pub sheet_hull: Load,
    /// N, `(ΣX, ΣY)` in `H`.
    pub total_force_h: Vec2,

    // --- moments ----------------------------------------------------------
    /// N·m, `ΣN` about `+z_H`.
    pub yaw_moment: f64,
    /// N·m, everything in `ΣK` that is **not** the hydrostatic righting: the
    /// sail, the foils and the hull's roll damping. `heeling_moment +
    /// righting_moment = ΣK` exactly.
    pub heeling_moment: f64,
    /// N·m, `K_restore = −Δ·g·GZ(φ)` (F6.7). Negative for starboard-down heel.
    pub righting_moment: f64,
    /// The four F6.9 boom moments about `+z_B` at the mast.
    pub boom_moment: BoomMoments,

    // --- geometry, in `B` -------------------------------------------------
    pub sail_ce_b: Vec3,
    pub board_centre_b: Vec3,
    pub rudder_centre_b: Vec3,
    /// `P_b(β)`, the mainsheet's boom attachment (F6.8).
    pub sheet_attach_b: Vec3,
    /// `P_k`, the block on the hull (F6.8).
    pub sheet_block_b: Vec3,

    // --- coefficients and angles ------------------------------------------
    pub alpha_sail: f64,
    pub cl_sail: f64,
    pub cd_sail: f64,
    pub alpha_board: f64,
    pub cl_board: f64,
    pub cd_board: f64,
    pub alpha_rudder: f64,
    pub cl_rudder: f64,
    pub cd_rudder: f64,

    // --- rig --------------------------------------------------------------
    /// rad, boom angle, positive to starboard (F2.1).
    pub beta: f64,
    pub beta_dot: f64,
    /// N, mainsheet tension, `≥ 0` always (F6.8, brief §11).
    pub sheet_tension: f64,
    /// m, `ℓ(β)` — the geometric rope path length. The renderer draws the rope
    /// from this and `l_sheet`, so no rope geometry is re-derived in
    /// TypeScript (F8).
    pub sheet_rope_length: f64,
    /// m, `e = ℓ − L`. Negative when the rope is slack; the renderer's sag is
    /// `max(0, −e)`.
    pub sheet_extension: f64,

    // --- stability --------------------------------------------------------
    /// m, the righting arm `GZ(φ)` at the published state (F6.7).
    pub gz: f64,
    /// deg, roll. **Not wrapped** (F3): it may exceed `±180°` after an
    /// inversion, and the heel indicator is required to show that.
    pub heel_deg: f64,
    /// The capsize report (F6.10) — informational, read by nothing in the
    /// physics. Nested rather than flattened, so the JSON shape and the Rust
    /// record have the same field list; the parity tests compare the two.
    pub capsize: CapsizeState,

    // --- health -----------------------------------------------------------
    /// J, `½m_x u² + ½m_y v² + ½I_z r² + ½I_x p² + ½I_b β̇²`.
    pub energy_kinetic: f64,
    /// J, `Δ·g·∫₀^φ GZ` — the potential stored in heel (F6.7).
    pub energy_roll_potential: f64,
    /// J, `½k_sheet·max(0, e)²` — the rope's elastic store (F6.8).
    pub energy_sheet_elastic: f64,
    /// R6: `|u|` is above [`HULL_MODEL_VALID_TO`], so the hull resistance the
    /// panel is showing is an extrapolation, not validated data.
    pub hull_model_warning: bool,
}

/// `½m_x u² + ½m_y v² + ½I_z r² + ½I_x p² + ½I_b β̇²` (F4.2 inertias).
pub fn kinetic_energy(st: &BoatState, p: &BoatParameters) -> f64 {
    let m = p.total_mass();
    0.5 * (m + p.inertia.a_x) * st.u * st.u
        + 0.5 * (m + p.inertia.a_y) * st.v * st.v
        + 0.5 * (p.inertia.i_zz + p.inertia.a_psi) * st.r * st.r
        + 0.5 * (p.inertia.i_xx + p.inertia.a_phi) * st.p * st.p
        + 0.5 * p.sail.i_boom * st.beta_dot * st.beta_dot
}

/// `Δ·g·∫₀^φ GZ`, the conservative store hydrostatic righting fills (F6.7).
pub fn roll_potential_energy(st: &BoatState, p: &BoatParameters) -> f64 {
    p.total_mass() * crate::constants::G * GzCurve::from_params(p).gz_integral(st.phi)
}

/// `½k_sheet·max(0, e)²`. Slack rope stores nothing — the same unilateral
/// condition as the tension itself (F6.8), not a separate branch.
pub fn sheet_elastic_energy(extension: f64, p: &BoatParameters) -> f64 {
    let e = extension.max(0.0);
    0.5 * p.sheet.k_sheet * e * e
}

/// A force model that hands back an evaluation already made.
///
/// This is how the accelerations in the record are the accelerations of the
/// published state: `dynamics::derivative` is the one implementation of F4.1
/// and F4.2, and it is given the cached breakdown rather than being allowed to
/// compute a second one.
struct Frozen(ForceBreakdown);

impl ForceModel for Frozen {
    fn generalized(&self, _: &BoatState, _: &Controls, _: &BoatParameters, _: f64) -> Generalized {
        self.0.total
    }
    fn boom_moment(&self, _: &BoatState, _: &Controls, _: &BoatParameters, _: f64) -> f64 {
        self.0.m_beta
    }
}

pub fn diagnostics(sim: &Simulation) -> Diagnostics {
    let st = sim.state();
    let p = sim.params();
    let f = *sim.forces();
    let dot = derivative(st, sim.controls(), p, &Frozen(f), st.t);
    let aw = f.aw_boat;
    Diagnostics {
        t: st.t,
        steps: sim.steps(),

        true_wind_world: sim.wind_at_boat(),
        true_wind_body: f.tw_boat,
        apparent_wind_body: aw,
        apparent_wind_speed: aw.length(),
        apparent_wind_angle: if aw.x == 0.0 && aw.y == 0.0 {
            0.0
        } else {
            aw.y.atan2(-aw.x)
        },

        velocity_body: Vec2::new(st.u, st.v),
        speed_over_ground: st.u.hypot(st.v),
        course_over_ground: if dot.x == 0.0 && dot.y == 0.0 {
            st.psi
        } else {
            dot.y.atan2(dot.x)
        },
        acceleration_body: Vec2::new(dot.u, dot.v),
        yaw_rate: st.r,
        roll_rate: st.p,
        leeway_angle: if st.u == 0.0 && st.v == 0.0 {
            0.0
        } else {
            (-st.v).atan2(st.u)
        },

        sail: f.sail,
        board: f.board,
        rudder: f.rudder,
        hull: f.hull,
        sheet: f.sheet,
        sheet_hull: f.sheet_hull,
        total_force_h: Vec2::new(f.total.x, f.total.y),

        yaw_moment: f.total.n,
        heeling_moment: f.total.k - f.k_restore,
        righting_moment: f.k_restore,
        boom_moment: f.boom,

        sail_ce_b: f.sail.r,
        board_centre_b: p.board.pos_b,
        rudder_centre_b: p.rudder.pos_b,
        sheet_attach_b: boom_attach_point(st.beta, p),
        sheet_block_b: p.sheet.block_pos_b,

        alpha_sail: f.alpha_sail,
        cl_sail: f.cl_sail,
        cd_sail: f.cd_sail,
        alpha_board: f.alpha_board,
        cl_board: cl(f.alpha_board, &p.board.section),
        cd_board: cd(f.alpha_board, &p.board.section),
        alpha_rudder: f.alpha_rudder,
        cl_rudder: cl(f.alpha_rudder, &p.rudder.section),
        cd_rudder: cd(f.alpha_rudder, &p.rudder.section),

        beta: st.beta,
        beta_dot: st.beta_dot,
        sheet_tension: f.sheet_tension,
        sheet_rope_length: f.rope_length,
        sheet_extension: f.sheet_extension,

        gz: f.gz,
        heel_deg: st.phi.to_degrees(),
        capsize: *sim.capsize(),

        energy_kinetic: kinetic_energy(st, p),
        energy_roll_potential: roll_potential_energy(st, p),
        energy_sheet_elastic: sheet_elastic_energy(f.sheet_extension, p),
        hull_model_warning: st.u.abs() > HULL_MODEL_VALID_TO,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::wind::{WindConfig, WindMode};
    use crate::forces::{evaluate, WindForces};
    use crate::state::Controls;

    fn breezy() -> Simulation {
        let mut sim = Simulation::new(BoatParameters::ilca7(), 3);
        sim.set_wind(WindConfig {
            mode: WindMode::Uniform,
            speed: 6.0,
            bearing_deg: 0.0,
            ..Default::default()
        });
        sim.set_controls(Controls {
            rudder_rate_cmd: 0.4,
            sheet_rate_cmd: -1.0,
            sheet_release: false,
        });
        sim
    }

    /// brief §30, item for item.
    ///
    /// The left column is the brief's own wording, copied out; the right is
    /// the field that carries it. Both the mapping and the field's presence in
    /// the struct declaration are asserted, so deleting a field breaks this
    /// test rather than quietly dropping a requirement.
    #[test]
    fn covers_brief_30() {
        const BRIEF_30: [(&str, &[&str]); 20] = [
            ("true wind", &["true_wind_world", "true_wind_body"]),
            (
                "apparent wind",
                &[
                    "apparent_wind_body",
                    "apparent_wind_speed",
                    "apparent_wind_angle",
                ],
            ),
            (
                "boat velocity",
                &["velocity_body", "speed_over_ground", "course_over_ground"],
            ),
            ("acceleration", &["acceleration_body"]),
            ("sail aerodynamic force", &["sail"]),
            ("centerboard force", &["board"]),
            ("rudder force", &["rudder"]),
            ("hull force", &["hull"]),
            ("total force", &["total_force_h"]),
            ("yaw moment", &["yaw_moment"]),
            ("heeling moment", &["heeling_moment"]),
            ("righting moment", &["righting_moment", "gz"]),
            ("sail center of effort", &["sail_ce_b"]),
            (
                "centerboard/rudder centers",
                &["board_centre_b", "rudder_centre_b"],
            ),
            ("boom angular velocity", &["beta", "beta_dot"]),
            (
                "sheet tension",
                &[
                    "sheet_tension",
                    "sheet_rope_length",
                    "sheet_extension",
                    "sheet_attach_b",
                    "sheet_block_b",
                ],
            ),
            ("sail angle of attack", &["alpha_sail"]),
            ("rudder angle of attack", &["alpha_rudder", "alpha_board"]),
            (
                "relevant C_L/C_D",
                &[
                    "cl_sail",
                    "cd_sail",
                    "cl_board",
                    "cd_board",
                    "cl_rudder",
                    "cd_rudder",
                ],
            ),
            ("yaw rate; roll rate", &["yaw_rate", "roll_rate"]),
        ];

        // The struct declaration, so a removed field fails here and not only
        // at the (mechanically generated) JSON.
        let src = include_str!("diagnostics.rs");
        let decl = src
            .split_once("pub struct Diagnostics {")
            .expect("the record must be declared here")
            .1
            .split_once("\n}")
            .expect("the declaration must close")
            .0;
        let declared: Vec<&str> = decl
            .lines()
            .filter_map(|l| l.trim().strip_prefix("pub "))
            .filter_map(|l| l.split_once(':'))
            .map(|(name, _)| name)
            .collect();

        let json = serde_json::to_value(diagnostics(&breezy())).unwrap();
        for (item, fields) in BRIEF_30 {
            for field in fields {
                assert!(
                    declared.contains(field),
                    "brief §30 item {item:?} maps to `{field}`, which is not declared"
                );
                assert!(
                    json.get(field).is_some(),
                    "brief §30 item {item:?} maps to `{field}`, which is not published"
                );
            }
        }
    }

    /// The forces on display are the forces the integrator is handed.
    #[test]
    fn matches_step_forces() {
        let mut sim = breezy();
        sim.advance(200);
        let d = diagnostics(&sim);

        // The evaluation the *next* step's first stage performs, built here
        // from the published state with the published arguments.
        let next = evaluate(
            sim.state(),
            sim.controls(),
            sim.params(),
            sim.wind(),
            sim.state().t,
        );
        let bits = |v: Vec3| (v.x.to_bits(), v.y.to_bits(), v.z.to_bits());
        assert_eq!(bits(d.sail.f), bits(next.sail.f));
        assert_eq!(bits(d.sail.r), bits(next.sail.r));
        assert_eq!(bits(d.board.f), bits(next.board.f));
        assert_eq!(bits(d.rudder.f), bits(next.rudder.f));
        assert_eq!(bits(d.hull.f), bits(next.hull.f));
        assert_eq!(bits(d.sheet.f), bits(next.sheet.f));
        assert_eq!(bits(d.sheet_hull.f), bits(next.sheet_hull.f));
        assert_eq!(d.sheet_tension.to_bits(), next.sheet_tension.to_bits());
        assert_eq!(d.righting_moment.to_bits(), next.k_restore.to_bits());

        // …and that evaluation is literally what reaches the equations of
        // motion, through the same trait object the integrator uses.
        let g = WindForces { wind: sim.wind() }.generalized(
            sim.state(),
            sim.controls(),
            sim.params(),
            sim.state().t,
        );
        assert_eq!(d.total_force_h.x.to_bits(), g.x.to_bits());
        assert_eq!(d.total_force_h.y.to_bits(), g.y.to_bits());
        assert_eq!(d.yaw_moment.to_bits(), g.n.to_bits());
        assert_eq!(
            (d.heeling_moment + d.righting_moment).to_bits(),
            g.k.to_bits()
        );
        assert!(d.sail.f.length() > 1.0, "the fixture must load the sail");
    }

    /// Each energy term is what it claims to be, against a computation written
    /// out here from F4.2, F6.7 and F6.8 rather than shared with the source.
    #[test]
    fn energy_terms_consistent() {
        let mut sim = breezy();
        for _ in 0..12 {
            sim.advance(50);
            let d = diagnostics(&sim);
            let st = sim.state();
            let p = sim.params();

            let m = p.total_mass();
            let kinetic = 0.5 * (m + p.inertia.a_x) * st.u.powi(2)
                + 0.5 * (m + p.inertia.a_y) * st.v.powi(2)
                + 0.5 * (p.inertia.i_zz + p.inertia.a_psi) * st.r.powi(2)
                + 0.5 * (p.inertia.i_xx + p.inertia.a_phi) * st.p.powi(2)
                + 0.5 * p.sail.i_boom * st.beta_dot.powi(2);

            // Δ·g·∫₀^φ GZ by trapezoid, independent of `gz_integral`.
            let curve = GzCurve::from_params(p);
            let n = 200_000;
            let h = st.phi / n as f64;
            let mut integral = 0.0;
            for i in 0..n {
                let a = i as f64 * h;
                integral += 0.5 * (curve.gz(a) + curve.gz(a + h)) * h;
            }
            let potential = m * crate::constants::G * integral;

            let e = (d.sheet_rope_length - st.l_sheet).max(0.0);
            let elastic = 0.5 * p.sheet.k_sheet * e * e;

            let independent = kinetic + potential + elastic;
            let published = d.energy_kinetic + d.energy_roll_potential + d.energy_sheet_elastic;
            assert!(
                (published - independent).abs() < 1e-9,
                "t = {}: published {published} vs independent {independent}",
                st.t
            );
        }
    }

    #[test]
    fn wind_from_angle_and_serialization() {
        let mut sim = Simulation::new(BoatParameters::ilca7(), 0);
        for (bearing, angle) in [
            (90.0, 0.0),
            (180.0, std::f64::consts::FRAC_PI_2),
            (0.0, -std::f64::consts::FRAC_PI_2),
        ] {
            sim.set_wind(WindConfig {
                mode: WindMode::Uniform,
                speed: 5.0,
                bearing_deg: bearing,
                ..Default::default()
            });
            let d = diagnostics(&sim);
            assert!((d.apparent_wind_speed - 5.0).abs() < 1e-12);
            assert!((d.apparent_wind_angle - angle).abs() < 1e-12);
            let json = serde_json::to_value(d).unwrap();
            assert_eq!(
                json["apparent_wind_body"]["x"].as_f64().unwrap(),
                d.apparent_wind_body.x
            );
            assert_eq!(json["sail"]["f"]["x"].as_f64().unwrap(), d.sail.f.x);
            assert!(!json["capsize"]["capsized"].as_bool().unwrap());
        }
    }

    /// R6 is surfaced rather than left for someone to remember.
    #[test]
    fn hull_model_warning_tracks_the_documented_limit() {
        let mut sim = Simulation::new(BoatParameters::ilca7(), 0);
        assert!(!diagnostics(&sim).hull_model_warning);
        let fast = BoatState {
            u: HULL_MODEL_VALID_TO + 0.5,
            ..Simulation::initial_state(sim.params())
        };
        sim.reset(fast, 0);
        assert!(diagnostics(&sim).hull_model_warning);
        // Symmetric in the sign of `u`: sternway extrapolates just as badly.
        sim.reset(
            BoatState {
                u: -(HULL_MODEL_VALID_TO + 0.5),
                ..fast
            },
            0,
        );
        assert!(diagnostics(&sim).hull_model_warning);
    }

    /// The derived motion quantities agree with the state they describe.
    #[test]
    fn motion_terms_agree_with_the_state() {
        let mut sim = breezy();
        sim.advance(400);
        let d = diagnostics(&sim);
        let st = sim.state();
        assert_eq!(d.velocity_body.x, st.u);
        assert_eq!(d.velocity_body.y, st.v);
        assert_eq!(d.yaw_rate, st.r);
        assert_eq!(d.roll_rate, st.p);
        assert_eq!(d.beta, st.beta);
        assert_eq!(d.beta_dot, st.beta_dot);
        assert!((d.heel_deg - st.phi.to_degrees()).abs() < 1e-12);
        assert!((d.speed_over_ground - st.u.hypot(st.v)).abs() < 1e-12);

        // Course over ground is heading plus drift: rotating the body velocity
        // into the world by ψ must land on it.
        let expected = (st.u * st.psi.sin() + st.v * st.psi.cos())
            .atan2(st.u * st.psi.cos() - st.v * st.psi.sin());
        assert!(
            (crate::frames::wrap_pi(d.course_over_ground - expected)).abs() < 1e-12,
            "course {} vs {expected}",
            d.course_over_ground
        );
        assert!((d.leeway_angle - (-st.v).atan2(st.u)).abs() < 1e-12);
        assert!(
            st.v.abs() > 1e-6,
            "the fixture must actually be making leeway"
        );
    }
}
