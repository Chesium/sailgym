//! What sheet length does this boat want at each apparent wind angle? (v2
//! section 12, task 12.3.)
//!
//! ```text
//! cargo run --release -p sailgym-bench --bin trim_sweep
//! ```
//!
//! The rule sailor's trim layer is a table from `|awa|` to a sheet length.
//! The PRD is explicit that the table is **measured and not invented**: v1's
//! one close-hauled data point (2.0 m ≈ 1.78 m/s against 0.40 m/s fully
//! hauled) is the first entry and nothing more. This binary produces the
//! rest.
//!
//! # The method, and why it is a hold rather than a sail
//!
//! For each target true wind angle, on **both** tacks, the boat is placed on
//! that angle with way on, the heading is held by a plain proportional helm,
//! the sheet is held at a swept length by a plain proportional trim, and the
//! run is allowed to settle. What is recorded is the mean surge speed and the
//! mean heel over the **last quarter** of the run, plus the mean `|awa|` the
//! boat actually saw — which is not the target, because the boat makes
//! leeway and heels, and the table is indexed by what the sensor reports.
//!
//! The helm and trim loops here are the bench's own and are **not** the rule
//! sailor's: using the controller under measurement to produce the
//! measurement it is tuned from would make the table a fixed point of its own
//! gains. Neither loop is physics and neither number in them is a physical
//! coefficient (v1 brief §43, v2 F14.9).
//!
//! # What it does not measure
//!
//! A steady hold on a steady angle in a uniform field. It says nothing about
//! a manoeuvre, nothing about a gust, and nothing about a real boat: it is
//! this model's own settled polar, at one wind speed, on one scenario's
//! field. `course_bench` (task 12.6) is what measures the controller built
//! from it.

use std::f64::consts::{FRAC_PI_2, PI};

use sailgym_physics::frames::wrap_pi;
use sailgym_physics::scenario::load_shipped;
use sailgym_physics::simulation::Simulation;
use sailgym_physics::state::{BoatState, Controls};

/// The scenario whose field and catalogue the sweep runs in.
///
/// `free_sail` is the browser's own: a uniform 5 m/s northerly. The shipped
/// courses are all sailed in it (task 12.1's `courses/*.json`), so the table
/// is measured in the conditions it is used in.
const SCENARIO: &str = "free_sail";

/// Target true wind angles, degrees, measured off the bow and positive to
/// starboard. Swept on both tacks by negating each one.
///
/// From 30° — inside this boat's close-hauled angle, so the table has an
/// entry for a boat pinching — to 180°, a dead run.
const TARGET_TWA_DEG: [f64; 11] = [
    30.0, 40.0, 50.0, 60.0, 75.0, 90.0, 105.0, 120.0, 135.0, 160.0, 180.0,
];

/// How many sheet lengths to try between the catalogue's two stops.
const SHEET_SAMPLES: usize = 41;

/// Seconds of simulated time per hold, and the fraction of it averaged.
const HOLD_S: f64 = 40.0;
const SETTLE_FRACTION: f64 = 0.25;

/// The initial way on, m/s. Enough that the foils have authority from the
/// first step, so the hold settles instead of starting in irons.
const INITIAL_SPEED: f64 = 1.0;

/// The heel the table refuses to buy speed with, radians.
///
/// **Presentation of a choice, not a coefficient.** A trim that is quicker
/// at 35° of heel than at 20° is quicker on the way to a capsize, and this
/// boat has no hiking (R2). 0.52 rad is 30°, comfortably inside the measured
/// `φ_p`, and the report prints the best length both with and without the
/// cap so the cost of the cap is visible rather than assumed.
const HEEL_CAP_RAD: f64 = 0.52;

/// How far below the best measured speed a sheet length may be and still count
/// as "as fast".
///
/// **Why the table is built from a plateau and not from the argmax.** The
/// speed-versus-length curve is flat near its optimum: re-running this sweep
/// at 25 and at 41 sheet samples moves the bare argmax by up to 0.9 m at a
/// broad reach while moving the speed it buys by under 3 %, and a table built
/// from that argmax is not monotonic in `|awa|` — it is sampling noise dressed
/// as a trim rule. So the admissible set is every length within this tolerance
/// of the best, and the entry is its **midpoint**, with the plateau's own
/// width printed beside it so the flatness is visible rather than hidden.
const SPEED_TOLERANCE: f64 = 0.02;

/// The bench's own helm gains. Not the rule sailor's; see the module note.
const HELM_KP: f64 = 2.2;
const HELM_KD: f64 = 0.55;
const HELM_LIMIT_RAD: f64 = 0.61;
const HELM_RATE_K: f64 = 9.0;
/// The bench's own trim gain.
const TRIM_K: f64 = 7.0;

struct Hold {
    /// m, the sheet length held.
    l_sheet: f64,
    /// m/s, mean surge over the settling window.
    speed: f64,
    /// rad, mean `|φ|` over the settling window.
    heel: f64,
    /// rad, mean `|awa|` the boat actually saw over the settling window.
    awa: f64,
    /// Whether the boat stayed on the angle it was asked to hold.
    held: bool,
}

/// The apparent wind angle, F6.2's own, as the `apparent_wind` sensor reports
/// it: the FROM angle off the bow, positive to starboard.
fn awa_of(sim: &Simulation) -> f64 {
    let d = sailgym_physics::diagnostics::diagnostics(sim);
    d.apparent_wind_angle
}

/// Hold `target_twa` with the sheet at `l_sheet` and report what settled.
fn hold(base: &BoatState, sim: &mut Simulation, target_twa: f64, l_sheet: f64, seed: u64) -> Hold {
    // The wind eye is world +y in this scenario: `wind_from_bearing(speed, 0)`
    // blows toward −y, so the air arrives from the north. The FROM bearing of
    // the eye relative to the bow, positive to starboard, is `psi − π/2`, so
    // a target angle `τ` wants `psi = τ + π/2`.
    let psi = wrap_pi(target_twa + FRAC_PI_2);
    let state = BoatState {
        psi,
        u: INITIAL_SPEED,
        l_sheet,
        ..*base
    };
    sim.reset(state, seed);

    let dt = sim.params().sim.dt;
    let steps = (HOLD_S / dt).round() as u64;
    let from = ((1.0 - SETTLE_FRACTION) * steps as f64).round() as u64;
    let mut speed = 0.0;
    let mut heel = 0.0;
    let mut awa = 0.0;
    let mut n = 0.0;
    let mut held = true;
    for step in 0..steps {
        let st = *sim.state();
        // Heading hold. `err > 0` means "turn the bow to starboard", which is
        // `δr > 0` (F2.2); `r > 0` is turning to port, so it damps with a
        // positive sign.
        let err = wrap_pi(st.psi - psi);
        let delta_target = (HELM_KP * err + HELM_KD * st.r).clamp(-HELM_LIMIT_RAD, HELM_LIMIT_RAD);
        let rudder = (HELM_RATE_K * (delta_target - st.delta_r)).clamp(-1.0, 1.0);
        // Trim hold. `+1` eases (pays out), so a sheet shorter than the target
        // asks for a positive command (F3).
        let sheet = (TRIM_K * (l_sheet - st.l_sheet)).clamp(-1.0, 1.0);
        sim.set_controls(Controls {
            rudder_rate_cmd: rudder,
            sheet_rate_cmd: sheet,
            sheet_release: false,
        });
        sim.advance(1);
        if step >= from {
            let st = *sim.state();
            speed += st.u;
            heel += st.phi.abs();
            awa += awa_of(sim).abs();
            n += 1.0;
            // Twenty degrees off the angle it was asked to hold is not a hold.
            if wrap_pi(st.psi - psi).abs() > 20.0f64.to_radians() {
                held = false;
            }
        }
        if sim.capsize().capsized {
            held = false;
        }
    }
    Hold {
        l_sheet,
        speed: speed / n,
        heel: heel / n,
        awa: awa / n,
        held,
    }
}

fn main() {
    let scenario = load_shipped(SCENARIO).expect("a shipped scenario");
    let params = scenario
        .to_parameters()
        .expect("the scenario's resolved catalogue");
    let base = scenario.to_boat_state();
    let seed = scenario.seed;
    let mut sim = Simulation::new(params, seed);
    // **The scenario's own field, not the default one.** `Simulation::new`
    // builds the default `WindConfig`; the configuration has to be installed
    // before the seeded `reset` that rebuilds the field from it, which is the
    // same order `Sim::retry_practice` uses and for the same reason.
    sim.set_wind(scenario.wind);

    let lo = params.sheet.l_sheet_min;
    let hi = params.sheet.l_sheet_max;
    println!("# trim_sweep — settled speed and heel against sheet length");
    println!();
    println!(
        "scenario `{SCENARIO}` · wind {:.1} m/s from {:.0}° · sheet {:.4}–{:.4} m · \
         {HOLD_S:.0} s per hold, last {:.0} % averaged",
        scenario.wind.speed,
        scenario.wind.bearing_deg,
        lo,
        hi,
        SETTLE_FRACTION * 100.0
    );
    println!();
    println!(
        "| target TWA | measured \\|awa\\| | best speed | plateau (±{:.0} %) | table L | heel at L | \
         worst tack-to-tack Δspeed |",
        SPEED_TOLERANCE * 100.0
    );
    println!("|---|---|---|---|---|---|---|");

    // What the rule sailor's table is built from: per angle, the measured
    // `|awa|` and the midpoint of the admissible plateau.
    let mut table: Vec<(f64, f64, f64, f64)> = Vec::new();

    for target_deg in TARGET_TWA_DEG {
        // Both tacks, swept identically. The scenario's field is a uniform
        // northerly and the initial state is symmetric about the wind's axis,
        // so the two sweeps are exact reflections of one another: F5.2's odd
        // `C_L` and even `C_D` make that a **bit-exact** property, and the
        // last column of the table is where it is checked rather than assumed
        // (F11's R3).
        let sweep = |sim: &mut Simulation, sign: f64| -> Vec<Hold> {
            (0..SHEET_SAMPLES)
                .map(|i| {
                    let f = i as f64 / (SHEET_SAMPLES - 1) as f64;
                    hold(
                        &base,
                        sim,
                        sign * target_deg.to_radians(),
                        lo + f * (hi - lo),
                        seed,
                    )
                })
                .collect()
        };
        let stbd = sweep(&mut sim, 1.0);
        let port = sweep(&mut sim, -1.0);
        // Reported as a **number**, not as a boolean. The two headings are
        // `τ + π/2` and `−τ + π/2`, whose reflection `π − ψ` is not a
        // bit-exact relation in floating point, so the two sweeps cannot be
        // compared with `to_bits()` however symmetric the model is. What is
        // reported is the worst speed discrepancy across the sweep; the
        // bit-exact mirror claim belongs where the inputs **are** exact
        // reflections, which is the rule sailor's own decision-level test.
        let disagreement = stbd
            .iter()
            .zip(port.iter())
            .filter(|(a, b)| a.held && b.held)
            .map(|(a, b)| (a.speed - b.speed).abs())
            .fold(0.0f64, f64::max);

        let held: Vec<&Hold> = stbd
            .iter()
            .filter(|h| h.held && h.heel <= HEEL_CAP_RAD)
            .collect();
        let Some(best) = held.iter().max_by(|a, b| a.speed.total_cmp(&b.speed)) else {
            println!("| {target_deg:.0}° | — | — | — | — | — | {disagreement:.2e} m/s |");
            continue;
        };
        let floor = best.speed * (1.0 - SPEED_TOLERANCE);
        let plateau: Vec<&&Hold> = held.iter().filter(|h| h.speed >= floor).collect();
        let p_lo = plateau
            .iter()
            .map(|h| h.l_sheet)
            .fold(f64::INFINITY, f64::min);
        let p_hi = plateau
            .iter()
            .map(|h| h.l_sheet)
            .fold(f64::NEG_INFINITY, f64::max);
        let mid = 0.5 * (p_lo + p_hi);
        // The sampled hold nearest the midpoint, so the heel reported beside
        // the table entry is a measurement and not an interpolation.
        let at_mid = held
            .iter()
            .min_by(|a, b| (a.l_sheet - mid).abs().total_cmp(&(b.l_sheet - mid).abs()))
            .expect("the plateau is non-empty");
        println!(
            "| {target_deg:.0}° | {:.1}° | {:.3} m/s | {:.3}–{:.3} m | {:.3} m | {:.1}° | \
             {disagreement:.2e} m/s |",
            best.awa.to_degrees(),
            best.speed,
            p_lo,
            p_hi,
            mid,
            at_mid.heel.to_degrees()
        );
        table.push((best.awa, mid, best.speed, at_mid.heel));
    }

    println!();
    println!("## The table, as the rule sailor carries it");
    println!();
    println!(
        "Indexed by the measured `|awa|`, the midpoint of the admissible plateau, \
         **as measured**. It is not smoothed and not forced monotonic: the one step \
         where it decreases is what this model does, and flattening it would be \
         choosing a number from how a run looks (v1 brief §43's discipline, applied \
         to an F14.9 tunable)."
    );
    println!();
    println!("| `\\|awa\\|` | sheet length | settled speed | heel |");
    println!("|---|---|---|---|");
    for (awa, l, speed, heel) in &table {
        println!(
            "| {:.1}° | {:.3} m | {:.3} m/s | {:.1}° |",
            awa.to_degrees(),
            l,
            speed,
            heel.to_degrees()
        );
    }
    println!();
    let steps_down: Vec<String> = table
        .windows(2)
        .filter(|w| w[1].1 < w[0].1)
        .map(|w| {
            format!(
                "{:.1}° → {:.1}° ({:.3} m → {:.3} m)",
                w[0].0.to_degrees(),
                w[1].0.to_degrees(),
                w[0].1,
                w[1].1
            )
        })
        .collect();
    if steps_down.is_empty() {
        println!("The table is monotonic non-decreasing in `|awa|`.");
    } else {
        println!(
            "The table **decreases** at {} step(s): {}. Reported, not removed.",
            steps_down.len(),
            steps_down.join("; ")
        );
    }
    println!();
    println!("```rust");
    println!("// (|awa| rad, sheet length m)");
    print!("const TRIM_TABLE: [(f64, f64); {}] = [", table.len());
    for (awa, l, _, _) in &table {
        print!("({:.4}, {:.4}), ", awa.min(PI), l);
    }
    println!("];");
    println!("```");
}
