//! The rule sailor, layer by layer and mirror for mirror (v2 section 12,
//! task 12.3).
//!
//! # What is asserted here, and what is asserted in `sailgym-env`
//!
//! This crate has `sailgym-physics` and `sailgym-course` and nothing else
//! (F14.1), so everything here runs a `Simulation` with a `Tracker` directly —
//! the same shape `tests/determinism.rs` already uses. The **closed loop** —
//! does it finish the three shipped courses? — needs an `Episode`, so it lives
//! in `crates/sailgym-env/tests/rule_sailor.rs`.
//!
//! # The mirror test is the point of this file
//!
//! F11's R3 names sign-convention drift as the highest-probability defect class
//! in this repository, and RV72 names its symptom: "sometimes it sails badly to
//! port". A tolerance would hide exactly that, so the assertion is **bit for
//! bit**, with one documented carve-out — `−(+0.0)` is `−0.0`, whose bits
//! differ from the `+0.0` a mirrored zero produces, so at zero IEEE equality is
//! asserted instead. That is section 04's own carve-out (F15.5 §4), written the
//! same way and for the same reason.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sailgym_agent::actuation::{apply, rate::Rate, Actuation};
use sailgym_agent::observation::{observe, sensor_streams, ObsLayout};
use sailgym_agent::pilot::rule_sailor::{Mode, RuleSailor, Tunables, PRIVILEGED_COLUMN};
use sailgym_agent::sensor::{Sensor, SensorRegistry};
use sailgym_agent::spec::{agent_rng, Action, ActionSpace, Agent, Cadence};
use sailgym_agent::worldview::WorldView;

use sailgym_course::guidance::{guidance, CourseParams};
use sailgym_course::{CourseId, Tracker};

use sailgym_physics::rng::Pcg32;
use sailgym_physics::scenario::load_shipped;
use sailgym_physics::simulation::Simulation;

/// The tier-0 suite, in the order section 05 registered it.
const SUITE: [&str; 5] = [
    "imu",
    "apparent_wind",
    "rig_state",
    "actuator_state",
    "guidance",
];

// ---------------------------------------------------------------------------
// The parity table
// ---------------------------------------------------------------------------

/// How each observation column behaves under the **port/starboard mirror**.
///
/// The mirror reflects the world about the wind's axis: every quantity measured
/// *to port* or *to starboard* changes sign, and every magnitude does not. The
/// table is stated here, by column name, because the mirror test is only as
/// good as it is — and because a sensor column added without an entry makes
/// this file fail rather than silently go untested.
///
/// | column | parity | why |
/// |---|---|---|
/// | `imu.roll_rate` | odd | `p > 0` rolls toward starboard (F2) |
/// | `imu.yaw_rate` | odd | `r > 0` turns to port (F2) |
/// | `imu.heel` | odd | `φ > 0` is starboard down (F2) |
/// | `imu.accel_surge` | even | along the bow; the mirror does not touch it |
/// | `imu.accel_sway` | odd | `+y_H` is port (F2) |
/// | `apparent_wind.awa` | odd | the FROM angle, positive to starboard |
/// | `apparent_wind.aws` | even | a speed |
/// | `rig_state.beta` | odd | `β > 0` is the boom to starboard (F2.1) |
/// | `rig_state.beta_dot` | odd | its derivative |
/// | `rig_state.l_sheet` | even | a length |
/// | `rig_state.sheet_slack` | even | a normalised length |
/// | `actuator_state.delta_r` | odd | `δr > 0` turns the bow to starboard (F2.2) |
/// | `actuator_state.rudder_rate_cmd` | odd | its command |
/// | `guidance.cross_track` | odd | positive to port of the leg (F15.5 §4) |
/// | `guidance.bearing_to_target` | odd | positive to port of the bow |
/// | `guidance.distance_to_target` | even | a distance |
/// | `guidance.leg_bearing_vs_wind` | odd | positive when the leg lies to port of the eye |
/// | `guidance.rounding_side` | odd | `+1` to starboard, `−1` to port |
const PARITY: [(&str, f64); 18] = [
    ("imu.roll_rate", -1.0),
    ("imu.yaw_rate", -1.0),
    ("imu.heel", -1.0),
    ("imu.accel_surge", 1.0),
    ("imu.accel_sway", -1.0),
    ("apparent_wind.awa", -1.0),
    ("apparent_wind.aws", 1.0),
    ("rig_state.beta", -1.0),
    ("rig_state.beta_dot", -1.0),
    ("rig_state.l_sheet", 1.0),
    ("rig_state.sheet_slack", 1.0),
    ("actuator_state.delta_r", -1.0),
    ("actuator_state.rudder_rate_cmd", -1.0),
    ("guidance.cross_track", -1.0),
    ("guidance.bearing_to_target", -1.0),
    ("guidance.distance_to_target", 1.0),
    ("guidance.leg_bearing_vs_wind", -1.0),
    ("guidance.rounding_side", -1.0),
];

/// The parity of every column of `layout`, in column order.
///
/// Panics on a column the table does not name, which is the point: a new
/// sensor column has to be classified before this file will run again.
fn parities(layout: &ObsLayout) -> Vec<f64> {
    let table: BTreeMap<&str, f64> = PARITY.iter().copied().collect();
    layout
        .names()
        .iter()
        .map(|name| {
            *table
                .get(name.as_str())
                .unwrap_or_else(|| panic!("the parity table does not classify `{name}`"))
        })
        .collect()
}

/// IEEE equality, which is exact for every value except the sign of zero.
///
/// `a == -b` is the carve-out section 04 recorded: negation is exact in IEEE,
/// so this is bit equality away from zero, and at zero it accepts `+0.0`
/// against `−0.0` — which is the one difference a mirrored zero can produce.
fn is_negation(a: f64, b: f64) -> bool {
    a == -b
}

// ---------------------------------------------------------------------------
// The rig
// ---------------------------------------------------------------------------

/// A `Simulation`, a `Tracker` and an agent: the smallest thing that produces
/// a real observation sequence on a real course.
struct Rig {
    sim: Simulation,
    sensors: Vec<Box<dyn Sensor>>,
    streams: Vec<Pcg32>,
    adapter: Box<dyn Actuation>,
    tracker: Tracker,
    course: CourseParams,
    cadence: Cadence,
    rng: Pcg32,
    layout: ObsLayout,
    step: u64,
    obs: Vec<f64>,
    /// Every decision: the observation it was taken from and the action taken.
    log: Vec<Decision>,
}

impl Rig {
    fn new(course: CourseId, seed: u64, agent: &mut dyn Agent) -> Self {
        let doc = course.load().expect("a shipped course");
        let route = doc.route().expect("a valid route");
        let sc = load_shipped(&doc.scenario).expect("a shipped scenario");
        let params = sc.to_parameters().expect("a valid catalogue");
        let mut sim = Simulation::new(params, sc.seed);
        sim.load_scenario(&sc).expect("the scenario loads");
        let tracker = Tracker::start(route, sim.state()).expect("a valid route");

        let sensors = SensorRegistry::tier0()
            .resolve(&SUITE)
            .expect("the tier-0 suite");
        let layout = ObsLayout::of(&sensors);
        let rng = agent_rng(&Pcg32::seed_from_u64(seed));
        let streams = sensor_streams(&rng, &sensors);
        let mut reset_rng = rng.clone();
        agent.reset(&layout.names(), &mut reset_rng);

        let cadence = agent.spec().cadence;
        Self {
            sim,
            sensors,
            streams,
            adapter: Box::new(Rate),
            tracker,
            course: CourseParams::default(),
            cadence,
            rng,
            layout,
            step: 0,
            obs: Vec::new(),
            log: Vec::new(),
        }
    }

    fn decide(&mut self, agent: &mut dyn Agent) {
        let st = *self.sim.state();
        let controls = *self.sim.controls();
        let params = *self.sim.params();
        let g = guidance(
            self.tracker.route(),
            self.tracker.leg_index(),
            &st,
            &self.course,
        );
        let view = WorldView {
            st: &st,
            controls: &controls,
            p: &params,
            wind: self.sim.wind(),
            guidance: g.as_ref(),
            others: &[],
            t: st.t,
        };
        observe(&mut self.sensors, &mut self.streams, &view, &mut self.obs);
        let action = agent.decide(&self.obs, &mut self.rng);
        let c = apply(self.adapter.as_mut(), &action, &st, &params).expect("a valid action");
        self.log.push((self.obs.clone(), action.values().to_vec()));
        self.sim.set_controls(c);
    }

    /// Run `steps` physics steps, deciding at the cadence.
    fn run(&mut self, steps: u64, agent: &mut dyn Agent) {
        for _ in 0..steps {
            if self.cadence.decides_at(self.step) {
                self.decide(agent);
            }
            self.sim.advance(1);
            self.step += 1;
            let st = *self.sim.state();
            self.tracker.observe(&st);
            if self.tracker.finished() {
                break;
            }
        }
    }
}

/// One decision: the observation it was taken from, and the action taken.
type Decision = (Vec<f64>, Vec<f64>);

/// The observation sequence a fresh rule sailor produces on `course`.
fn sequence(course: CourseId, steps: u64) -> (ObsLayout, Vec<Decision>) {
    let mut agent = RuleSailor::new();
    let mut rig = Rig::new(
        course,
        12_000_000 + course.as_str().len() as u64,
        &mut agent,
    );
    rig.run(steps, &mut agent);
    (rig.layout.clone(), rig.log)
}

// ---------------------------------------------------------------------------
// 1. The decision-level mirror (RV72)
// ---------------------------------------------------------------------------

#[test]
fn the_mirrored_observation_sequence_gives_the_mirrored_action() {
    for course in CourseId::ALL {
        let (layout, log) = sequence(course, 12_000);
        assert!(
            log.len() > 200,
            "{}: only {} decisions, so the mirror proves little",
            course.as_str(),
            log.len()
        );
        let parity = parities(&layout);
        assert_eq!(parity.len(), layout.len());

        let mut mirrored = RuleSailor::new();
        let names = layout.names();
        let mut rng = Pcg32::seed_from_u64(7);
        mirrored.reset(&names, &mut rng);
        // The original, replayed open-loop, so the two agents see exactly the
        // sequences being compared and nothing is re-integrated.
        let mut original = RuleSailor::new();
        original.reset(&names, &mut rng);

        let mut nonzero_rudder = 0usize;
        for (i, (obs, _)) in log.iter().enumerate() {
            let flipped: Vec<f64> = obs
                .iter()
                .zip(parity.iter())
                .map(|(v, p)| if *p < 0.0 { -*v } else { *v })
                .collect();
            let a = original.decide(obs, &mut rng);
            let b = mirrored.decide(&flipped, &mut rng);
            let (a, b) = (a.values().to_vec(), b.values().to_vec());
            assert_eq!(a.len(), 3);
            assert!(
                is_negation(b[0], a[0]),
                "{}: decision {i}: rudder {} against {}",
                course.as_str(),
                b[0],
                a[0]
            );
            assert_eq!(
                b[1].to_bits(),
                a[1].to_bits(),
                "{}: decision {i}: the sheet command must be unchanged",
                course.as_str()
            );
            assert_eq!(
                b[2].to_bits(),
                a[2].to_bits(),
                "{}: decision {i}: the release flag must be unchanged",
                course.as_str()
            );
            if a[0] != 0.0 {
                nonzero_rudder += 1;
            }
            // Internal state, mirrored.
            assert_eq!(
                mirrored.side(),
                -original.side(),
                "{}: decision {i}: the tack must mirror",
                course.as_str()
            );
            assert_eq!(
                mirrored.mode(),
                original.mode(),
                "{}: decision {i}: the mode must be the same",
                course.as_str()
            );
            let note = |a: &RuleSailor, key: &str| {
                a.debug()
                    .notes
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| *v)
                    .unwrap_or_else(|| panic!("no debug note `{key}`"))
            };
            assert!(
                is_negation(
                    note(&mirrored, "target_delta_r"),
                    note(&original, "target_delta_r")
                ),
                "{}: decision {i}: the target rudder angle must mirror",
                course.as_str()
            );
            assert_eq!(
                note(&mirrored, "target_l_sheet").to_bits(),
                note(&original, "target_l_sheet").to_bits(),
                "{}: decision {i}: the target sheet length must be unchanged",
                course.as_str()
            );
        }
        assert!(
            nonzero_rudder * 2 > log.len(),
            "{}: the rudder was zero on most decisions, so the mirror proves little",
            course.as_str()
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Privilege (RV67)
// ---------------------------------------------------------------------------

#[test]
fn replacing_the_privileged_column_changes_no_action() {
    for course in CourseId::ALL {
        let (layout, log) = sequence(course, 8_000);
        let names = layout.names();
        let at = names
            .iter()
            .position(|n| n == PRIVILEGED_COLUMN)
            .expect("the tier-0 layout carries the privileged column");
        assert!(
            layout.privileged_columns().contains(&at),
            "the layout must agree that `{PRIVILEGED_COLUMN}` is privileged"
        );

        let mut rng = Pcg32::seed_from_u64(11);
        let mut plain = RuleSailor::new();
        plain.reset(&names, &mut rng);
        let mut perturbed = RuleSailor::new();
        perturbed.reset(&names, &mut rng);

        // Arbitrary values, including the ones most likely to break a reader:
        // the extremes of the declared bounds, zero, and a NaN.
        let arbitrary = [0.0, 3.0, -3.0, f64::NAN, 1.0, -1.0, 2.5];
        let mut perturbed_count = 0usize;
        for (i, (obs, _)) in log.iter().enumerate() {
            let mut edited = obs.clone();
            edited[at] = arbitrary[i % arbitrary.len()];
            if edited[at].to_bits() != obs[at].to_bits() {
                perturbed_count += 1;
            }
            let a = plain.decide(obs, &mut rng).values().to_vec();
            let b = perturbed.decide(&edited, &mut rng).values().to_vec();
            for (k, (x, y)) in a.iter().zip(b.iter()).enumerate() {
                assert_eq!(
                    x.to_bits(),
                    y.to_bits(),
                    "{}: decision {i}, scalar {k}: the baseline read a privileged column",
                    course.as_str()
                );
            }
        }
        // Non-vacuity: the column really was given a different value on
        // (almost) every decision. It is **not** asserted that the recorded
        // value was itself non-zero — a dead beat has a leg bearing of exactly
        // zero against the wind, which is the case `windward_leeward` and
        // `triangle` both start on.
        assert!(
            perturbed_count * 4 > log.len() * 3,
            "{}: only {perturbed_count} of {} decisions were perturbed",
            course.as_str(),
            log.len()
        );
    }
}

// ---------------------------------------------------------------------------
// 3. A layout missing a column
// ---------------------------------------------------------------------------

#[test]
fn a_missing_column_gives_the_all_zero_action_and_a_debug_note() {
    let sensors = SensorRegistry::tier0()
        .resolve(&SUITE)
        .expect("the tier-0 suite");
    let full = ObsLayout::of(&sensors).names();
    let mut rng = Pcg32::seed_from_u64(3);

    // Every required column, dropped one at a time.
    for drop in [
        "apparent_wind.awa",
        "guidance.bearing_to_target",
        "guidance.cross_track",
        "imu.yaw_rate",
        "imu.heel",
        "imu.roll_rate",
        "actuator_state.delta_r",
        "rig_state.l_sheet",
    ] {
        let names: Vec<String> = full.iter().filter(|n| *n != drop).cloned().collect();
        assert_eq!(names.len(), full.len() - 1, "`{drop}` is not in the layout");
        let mut agent = RuleSailor::new();
        agent.reset(&names, &mut rng);
        assert_eq!(agent.missing_fields(), [drop.to_string()]);
        // A whole observation, a short one and an empty one: none panics.
        for obs in [vec![0.5; names.len()], vec![0.5; 3], Vec::new()] {
            let action = agent.decide(&obs, &mut rng);
            assert_eq!(action.values(), &[0.0, 0.0, 0.0], "dropped `{drop}`");
            assert_eq!(action.space(), ActionSpace::Rates);
        }
        let notes: BTreeMap<String, f64> = agent.debug().notes.into_iter().collect();
        assert_eq!(notes.get("missing_fields"), Some(&1.0));
    }

    // An empty layout reports every required column.
    let mut agent = RuleSailor::new();
    agent.reset(&[], &mut rng);
    assert_eq!(agent.missing_fields().len(), 8);
    assert_eq!(
        agent.decide(&[], &mut rng).values(),
        &[0.0, 0.0, 0.0],
        "an empty layout must not panic"
    );

    // …and a complete layout reports none.
    let mut agent = RuleSailor::new();
    agent.reset(&full, &mut rng);
    assert!(agent.missing_fields().is_empty());
    let notes: BTreeMap<String, f64> = agent.debug().notes.into_iter().collect();
    assert!(!notes.contains_key("missing_fields"));

    // A non-finite observation is refused the same way, rather than being
    // multiplied into a NaN command the adapter would reject.
    let mut obs = vec![0.0; full.len()];
    let awa = full
        .iter()
        .position(|n| n == "apparent_wind.awa")
        .expect("awa");
    obs[awa] = f64::NAN;
    assert_eq!(agent.decide(&obs, &mut rng).values(), &[0.0, 0.0, 0.0]);
}

// ---------------------------------------------------------------------------
// 4. One test per layer
// ---------------------------------------------------------------------------

/// A complete tier-0 layout and a reset agent, for the layer tests.
fn ready(tun: Tunables) -> (RuleSailor, Vec<String>, Vec<f64>) {
    let sensors = SensorRegistry::tier0()
        .resolve(&SUITE)
        .expect("the tier-0 suite");
    let names = ObsLayout::of(&sensors).names();
    let mut agent = RuleSailor::with_tunables(tun, Cadence::new(10));
    let mut rng = Pcg32::seed_from_u64(5);
    agent.reset(&names, &mut rng);
    let obs = vec![0.0; names.len()];
    (agent, names, obs)
}

/// Write `value` into the column called `name`.
fn put(obs: &mut [f64], names: &[String], name: &str, value: f64) {
    let at = names
        .iter()
        .position(|n| n == name)
        .unwrap_or_else(|| panic!("no column `{name}`"));
    obs[at] = value;
}

/// Decide `n` times, so that `since_manoeuvre` reaches a chosen value.
///
/// `reset` starts the interval at **zero** — a boat just let go has no way on
/// and a tack needs 1.6 m/s — so a fixture that wants a manoeuvre has to sail
/// first, exactly as the boat does.
fn warm_up(agent: &mut RuleSailor, obs: &[f64], n: u32) {
    let mut rng = Pcg32::seed_from_u64(9);
    for _ in 0..n {
        agent.decide(obs, &mut rng);
    }
}

/// The observation that puts the target `theta` from the apparent wind's eye
/// with the vane reading `awa`: `theta = −brg − awa`, so `brg = −(theta + awa)`.
fn bearing_for(theta: f64, awa: f64) -> f64 {
    -(theta + awa)
}

fn rudder_of(agent: &mut RuleSailor, obs: &[f64]) -> f64 {
    let mut rng = Pcg32::seed_from_u64(1);
    match agent.decide(obs, &mut rng) {
        Action::Rates(a) => a.as_slice()[0],
    }
}

fn sheet_of(agent: &mut RuleSailor, obs: &[f64]) -> (f64, bool) {
    let mut rng = Pcg32::seed_from_u64(1);
    match agent.decide(obs, &mut rng) {
        Action::Rates(a) => (a.as_slice()[1], a.as_slice()[2] > 0.0),
    }
}

/// **The helm.** `δr > 0` turns the bow to starboard (F2.2) and the target is
/// reached by turning toward it, so the rudder command follows the sign of the
/// heading error in all four quadrants.
#[test]
fn the_helms_rudder_sign_follows_f2_in_all_four_quadrants() {
    // A target well outside the no-go zone, so the navigator is fetching and
    // the heading error is the bearing to the target.
    for (bearing_to_port, want_starboard) in [
        (1.2, true),   // target to port  → `brg > 0` → `err = −brg < 0` → to port
        (-1.2, false), // target to starboard
    ] {
        let (mut agent, names, mut obs) = ready(Tunables::default());
        // The wind abeam, so `|theta|` is large and the navigator fetches.
        put(&mut obs, &names, "apparent_wind.awa", 1.5);
        put(
            &mut obs,
            &names,
            "guidance.bearing_to_target",
            bearing_to_port,
        );
        put(&mut obs, &names, "rig_state.l_sheet", 3.0);
        let r = rudder_of(&mut agent, &obs);
        // `want_starboard` is "the target is to port", which asks for a turn
        // to **port**, which is a negative rudder.
        if want_starboard {
            assert!(r < 0.0, "a target to port must turn the bow to port: {r}");
        } else {
            assert!(
                r > 0.0,
                "a target to starboard must turn the bow to starboard: {r}"
            );
        }
    }

    // The other two quadrants: the same two errors with the rudder already
    // over the *wrong* way, which is where the P-loop on `delta_r` matters.
    for (bearing_to_port, delta_r) in [(1.2, 0.5), (-1.2, -0.5)] {
        let (mut agent, names, mut obs) = ready(Tunables::default());
        put(&mut obs, &names, "apparent_wind.awa", 1.5);
        put(
            &mut obs,
            &names,
            "guidance.bearing_to_target",
            bearing_to_port,
        );
        put(&mut obs, &names, "actuator_state.delta_r", delta_r);
        put(&mut obs, &names, "rig_state.l_sheet", 3.0);
        let r = rudder_of(&mut agent, &obs);
        assert!(
            r * delta_r < 0.0,
            "a rudder over the wrong way must be commanded back: δr = {delta_r}, cmd = {r}"
        );
        assert!(r.abs() <= 1.0);
    }

    // And a zero command is never used to **hold** an angle: the rudder
    // self-centres (F14.10 §2), so holding needs a command.
    let (mut agent, names, mut obs) = ready(Tunables::default());
    put(&mut obs, &names, "apparent_wind.awa", 1.5);
    put(&mut obs, &names, "guidance.bearing_to_target", 0.4);
    put(&mut obs, &names, "rig_state.l_sheet", 3.0);
    assert_ne!(rudder_of(&mut agent, &obs), 0.0);
}

/// **Manoeuvres.** No tack inside `min_tack_interval` of the last one, and an
/// uncompleted tack becomes `recovering` on its timeout.
#[test]
fn a_tack_waits_for_its_interval_and_times_out_into_recovering() {
    let tun = Tunables::default();
    let (mut agent, names, mut obs) = ready(tun);
    // Close-hauled on starboard tack, with the target 17° **across** the eye —
    // past `tack_margin`, inside the no-go zone, and an eye-arc of only 46°,
    // well inside `tack_arc_max`. Everything is in place for a tack; the only
    // thing holding it off is the interval.
    let awa = tun.close_hauled_awa;
    put(&mut obs, &names, "apparent_wind.awa", awa);
    put(
        &mut obs,
        &names,
        "guidance.bearing_to_target",
        bearing_for(0.3, awa),
    );
    put(&mut obs, &names, "rig_state.l_sheet", 1.5);

    // Not before the interval has run, however long it is asked.
    warm_up(&mut agent, &obs, tun.min_tack_interval - 1);
    assert_eq!(
        agent.mode(),
        Mode::Beating,
        "a tack inside min_tack_interval"
    );
    let mut rng = Pcg32::seed_from_u64(2);
    agent.decide(&obs, &mut rng);
    assert_eq!(
        agent.mode(),
        Mode::Tacking,
        "the decision the interval runs out on must tack"
    );

    // Now hold the vane where it was: the tack never crosses, so the machine
    // must sit in `Tacking` until the timeout and then **recover**.
    for i in 1..tun.tack_timeout {
        agent.decide(&obs, &mut rng);
        assert_eq!(agent.mode(), Mode::Tacking, "decision {i}");
    }
    agent.decide(&obs, &mut rng);
    assert_eq!(
        agent.mode(),
        Mode::Recovering,
        "an uncompleted tack must become irons recovery"
    );
    // Recovering eases the sheet and bears away.
    let (sheet, release) = sheet_of(&mut agent, &obs);
    assert_eq!(sheet, 1.0, "irons recovery eases the sheet");
    assert!(!release);

    // It leaves recovery on its own timeout, and then may not tack again until
    // the whole interval has run from the moment the manoeuvre ended.
    let mut left_recovery_after = None;
    for i in 0..tun.irons_timeout * 2 {
        agent.decide(&obs, &mut rng);
        if !agent.mode().is_manoeuvre() {
            left_recovery_after = Some(i);
            break;
        }
    }
    left_recovery_after.expect("recovery must end");
    let mut tacked_at = None;
    for i in 0..tun.min_tack_interval * 2 {
        agent.decide(&obs, &mut rng);
        if agent.mode() == Mode::Tacking {
            tacked_at = Some(i);
            break;
        }
    }
    let at = tacked_at.expect("it must eventually try again");
    assert!(
        at + 2 >= tun.min_tack_interval,
        "it tacked again after only {at} decisions, inside an interval of {}",
        tun.min_tack_interval
    );
}

/// **The tack is the measured recipe**: full helm toward the wind and a full
/// haul, together (`practice-validation.md` §3.1).
#[test]
fn a_tack_is_full_helm_and_full_haul_together() {
    let tun = Tunables::default();
    for (awa, want_rudder) in [
        // Starboard tack: the wind is on the starboard bow, so luffing up
        // turns the bow to starboard, which is a positive command (F2.2).
        (tun.close_hauled_awa, 1.0),
        // Port tack: the mirror of it.
        (-tun.close_hauled_awa, -1.0),
    ] {
        let (mut agent, names, mut obs) = ready(tun);
        put(&mut obs, &names, "apparent_wind.awa", awa);
        // The target 17° across the eye, on the side the other tack sails.
        // `theta` therefore has the sign of the committed tack.
        let theta = if awa > 0.0 { 0.3 } else { -0.3 };
        put(
            &mut obs,
            &names,
            "guidance.bearing_to_target",
            bearing_for(theta, awa),
        );
        put(&mut obs, &names, "rig_state.l_sheet", 2.0);
        warm_up(&mut agent, &obs, tun.min_tack_interval - 1);
        let mut rng = Pcg32::seed_from_u64(4);
        let action = agent.decide(&obs, &mut rng);
        assert_eq!(agent.mode(), Mode::Tacking);
        let v = action.values();
        assert_eq!(v[0], want_rudder, "full helm toward the wind");
        assert_eq!(v[1], -1.0, "and a full haul, together");
        assert!(v[2] < 0.0, "the sheet is not released during a tack");
    }
}

/// **The heel guard.** It eases at `heel_ease`, and it refuses to haul back in
/// until its hysteresis has run.
#[test]
fn the_heel_guard_eases_then_waits_before_trimming_back_in() {
    let tun = Tunables::default();
    let (mut agent, names, mut obs) = ready(tun);
    // A reach, with the sheet far shorter than the table wants, so the trim
    // layer on its own would **ease** — which would prove nothing. Make it
    // longer than the table wants, so the trim layer hauls and the guard has
    // something to override.
    put(&mut obs, &names, "apparent_wind.awa", 0.5);
    put(&mut obs, &names, "guidance.bearing_to_target", 0.0);
    put(&mut obs, &names, "rig_state.l_sheet", 4.4);
    let (hauling, _) = sheet_of(&mut agent, &obs);
    assert!(hauling < 0.0, "the trim layer must be hauling: {hauling}");

    // Now heel it just past the ease threshold.
    put(&mut obs, &names, "imu.heel", tun.heel_ease + 0.01);
    let (eased, release) = sheet_of(&mut agent, &obs);
    assert_eq!(eased, 1.0, "the guard eases at heel_ease");
    assert!(!release, "and does not release until heel_release");

    // Upright again: the guard still refuses to haul for its hysteresis.
    put(&mut obs, &names, "imu.heel", 0.0);
    for i in 0..tun.heel_hold {
        let (sheet, _) = sheet_of(&mut agent, &obs);
        assert!(
            sheet >= 0.0,
            "decision {i} hauled back in during the hysteresis: {sheet}"
        );
    }
    let (sheet, _) = sheet_of(&mut agent, &obs);
    assert!(
        sheet < 0.0,
        "the trim layer must resume after the hysteresis: {sheet}"
    );

    // Past `heel_release` it releases, and an outward roll eases before the
    // angle is reached.
    let (mut agent, names, mut obs) = ready(tun);
    put(&mut obs, &names, "apparent_wind.awa", 0.5);
    put(&mut obs, &names, "rig_state.l_sheet", 4.4);
    put(&mut obs, &names, "imu.heel", tun.heel_release + 0.01);
    let (sheet, release) = sheet_of(&mut agent, &obs);
    assert_eq!(sheet, 1.0);
    assert!(release, "past heel_release the sheet is released");

    let (mut agent, names, mut obs) = ready(tun);
    put(&mut obs, &names, "apparent_wind.awa", 0.5);
    put(&mut obs, &names, "rig_state.l_sheet", 4.4);
    // Heeling to starboard and rolling further that way: outward.
    put(&mut obs, &names, "imu.heel", 0.2);
    put(&mut obs, &names, "imu.roll_rate", tun.roll_rate_ease + 0.1);
    let (sheet, release) = sheet_of(&mut agent, &obs);
    assert_eq!(sheet, 1.0, "a fast outward roll eases");
    assert!(!release);

    // …and rolling **back** at the same rate does not.
    let (mut agent, names, mut obs) = ready(tun);
    put(&mut obs, &names, "apparent_wind.awa", 0.5);
    put(&mut obs, &names, "rig_state.l_sheet", 4.4);
    put(&mut obs, &names, "imu.heel", 0.2);
    put(
        &mut obs,
        &names,
        "imu.roll_rate",
        -(tun.roll_rate_ease + 0.1),
    );
    let (sheet, _) = sheet_of(&mut agent, &obs);
    assert!(
        sheet < 0.0,
        "an inward roll is not a reason to ease: {sheet}"
    );
}

/// **The navigator.** The no-go test is against the apparent wind, and the
/// beating set-point is the close-hauled angle on the committed tack.
#[test]
fn the_navigator_beats_inside_the_no_go_zone_and_fetches_outside_it() {
    let tun = Tunables::default();
    // The target dead to windward of a boat already pointing at it: inside the
    // no-go zone, so the boat beats rather than pinching at it.
    let (mut agent, names, mut obs) = ready(tun);
    put(&mut obs, &names, "apparent_wind.awa", 0.1);
    put(&mut obs, &names, "guidance.bearing_to_target", -0.1);
    put(&mut obs, &names, "rig_state.l_sheet", 1.5);
    let mut rng = Pcg32::seed_from_u64(6);
    agent.decide(&obs, &mut rng);
    assert!(
        matches!(agent.mode(), Mode::Beating | Mode::Tacking),
        "a target in the no-go zone must be beaten to: {:?}",
        agent.mode()
    );

    // The same target with the wind abeam: sailable, so it is fetched.
    let (mut agent, names, mut obs) = ready(tun);
    put(&mut obs, &names, "apparent_wind.awa", 1.5);
    put(&mut obs, &names, "guidance.bearing_to_target", 0.0);
    put(&mut obs, &names, "rig_state.l_sheet", 3.0);
    agent.decide(&obs, &mut rng);
    assert_eq!(agent.mode(), Mode::Fetching);
}

// ---------------------------------------------------------------------------
// 5. The greps
// ---------------------------------------------------------------------------

/// Every `.rs` file under `src/pilot/`.
fn pilot_sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src/pilot"),
        &mut out,
    );
    out
}

/// Code lines, with `#[cfg(test)]` items excluded — the same scanner shape the
/// physics, agent and env crates already use.
fn code_lines(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut skipping: Option<i32> = None;
    for (i, line) in source.lines().enumerate() {
        let opens = line.matches('{').count() as i32;
        let closes = line.matches('}').count() as i32;
        if skipping.is_none() && line.trim_start().starts_with("#[cfg(test)]") {
            skipping = Some(depth);
        }
        if skipping.is_none() {
            out.push((i + 1, line.to_string()));
        }
        depth += opens - closes;
        if let Some(at) = skipping {
            if depth <= at && closes > 0 {
                skipping = None;
            }
        }
    }
    out
}

/// Float literals in a line of code.
fn float_literals(line: &str) -> Vec<f64> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'.' {
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                    i += 1;
                }
                let text: String = line[start..i].chars().filter(|c| *c != '_').collect();
                if let Ok(v) = text.parse::<f64>() {
                    out.push(v);
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Values a controller may spell without copying a coefficient.
fn universal(v: f64) -> bool {
    [0.0, 1.0, 0.5, 2.0, 3.0].contains(&v)
}

/// v1 brief §43 and v2 F14.9, as a test: no F7 coefficient has been copied
/// into `pilot/`. The same audit the agent and env crates already run,
/// narrowed to this module.
#[test]
fn no_f7_literal_appears_in_the_pilot_module() {
    let params = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sailgym-physics/src/parameters.rs");
    let catalogue = std::fs::read_to_string(&params).expect("parameters.rs must be readable");
    let f7: Vec<f64> = catalogue
        .lines()
        .flat_map(float_literals)
        .filter(|v| !universal(*v))
        .collect();
    assert!(f7.len() > 40, "the F7 scan found only {} values", f7.len());

    let sources = pilot_sources();
    assert!(sources.len() >= 2, "the pilot scan found {:?}", sources);
    let mut offenders = Vec::new();
    let mut scanned = 0usize;
    for path in &sources {
        let text = std::fs::read_to_string(path).expect("source");
        for (n, line) in code_lines(&text) {
            let code = line.split("//").next().unwrap_or("");
            scanned += 1;
            for v in float_literals(code) {
                if !universal(v) && f7.contains(&v) {
                    offenders.push(format!("{}:{n}: {v} in `{}`", path.display(), code.trim()));
                }
            }
        }
    }
    assert!(scanned > 200, "the pilot scan read only {scanned} lines");
    assert!(
        offenders.is_empty(),
        "an F7 coefficient has been copied into pilot/ (brief §43, F14.9):\n{}",
        offenders.join("\n")
    );
    // …and the scanner would notice one.
    assert!(f7.iter().any(|v| (*v - 0.698).abs() < 1e-12), "delta_r_max");
    assert!(float_literals("let x = 0.698;").contains(&0.698));
}

/// The adapter width this module spells and the `rate` adapter's own agree.
#[test]
fn the_action_width_is_the_rate_adapters() {
    assert_eq!(Rate::DIM, 3);
    let mut rng = Pcg32::seed_from_u64(1);
    let sensors = SensorRegistry::tier0()
        .resolve(&SUITE)
        .expect("the tier-0 suite");
    let names = ObsLayout::of(&sensors).names();
    let mut agent = RuleSailor::new();
    agent.reset(&names, &mut rng);
    let action = agent.decide(&vec![0.0; names.len()], &mut rng);
    assert_eq!(action.values().len(), Rate::DIM);
    let spec = agent.spec();
    assert_eq!(spec.id, "rule_sailor");
    assert_eq!(spec.version, 1);
    assert_eq!(spec.action_space, ActionSpace::Rates);
    assert_eq!(spec.cadence, Cadence::new(10));
    spec.validate().expect("the shipped spec is valid");
}
