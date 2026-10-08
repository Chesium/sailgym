//! The rule sailor: five small layers that sail a waypoint course (v2
//! section 12, D6, task 12.3).
//!
//! An **easy-to-read baseline**, not a fast one. Its whole value is that a
//! reader can say what it will do from the observation it was handed, so a
//! human's time on a course can be compared against a number nobody has to
//! take on trust.
//!
//! | layer | reads | does |
//! |---|---|---|
//! | [navigator](RuleSailor::navigate) | `bearing_to_target`, `cross_track`, `awa` | steer at the target when it is outside the no-go zone **measured against the apparent wind**; otherwise beat close-hauled on the current tack |
//! | [manoeuvres](Mode) | `awa`, `yaw_rate`, elapsed decisions | `sailing → tacking → settling → sailing`, likewise gybing, plus `recovering` from irons on timeout |
//! | [helm](RuleSailor::helm) | `delta_r`, `yaw_rate` | heading error and yaw-rate damping give a target rudder angle; a P-loop on `delta_r` gives the rate command |
//! | [trim](RuleSailor::trim) | `awa`, `l_sheet` | a **measured** sheet-length table over `\|awa\|`, tracked through the sheet rate |
//! | [heel guard](RuleSailor::heel_guard) | `heel`, `roll_rate` | ease on heel or on fast outward roll, release past a higher threshold, and wait before trimming back in |
//!
//! # The tack is the measured recipe, and nothing else completes one
//!
//! `docs/v2/practice-validation.md` §3.1 swept 1 344 helm-only scripts and
//! **none of them completed a tack**: 40° of rudder roughly triples the drag,
//! the boat is down to 0.5 m/s before the bow reaches the wind, and it then
//! hangs head to wind making sternway for forty seconds. The one thing that
//! works is **full helm and full haul together** — hauled in, the sail is a
//! stalled plate at a large angle of attack and goes on producing force
//! through head to wind, and the surge speed bottoms out at 0.355 m/s without
//! reversing. [`Mode::Tacking`] is that script and nothing cleverer (RV68).
//!
//! # It reads the burgee, not the true wind
//!
//! Every angle the navigator compares is against `apparent_wind.awa` — what a
//! masthead vane measures. `guidance.leg_bearing_vs_wind` is derived from the
//! **true** wind and is marked `privileged` in the layout; this controller
//! never resolves that column, and
//! `tests/rule_sailor.rs::replacing_the_privileged_column_changes_no_action`
//! replaces it with arbitrary values and asserts every action is unchanged bit
//! for bit (RV67).
//!
//! # Signs, once
//!
//! Three conventions, all F2's, and every comparison below is written in terms
//! of them:
//!
//! * `awa` is the apparent wind's **FROM** angle off the bow, **positive to
//!   starboard**. So `awa > 0` is starboard tack, `awa = 0` is head to wind
//!   and `\|awa\| = π` is a dead run.
//! * `bearing_to_target` is positive **to port** of the bow (F2: `+y_H` is
//!   port), so a *starboard-positive* bearing to the target is `−brg`.
//! * `delta_r > 0` turns the bow to **starboard** (F2.2) and `r > 0` turns it
//!   to **port** (F2). A rudder-rate command of `+1` asks for `δr` to grow.
//!
//! Writing every quantity in the **starboard-positive** frame is what makes
//! the mirror test exact: each one is odd under the mirror, each comparison is
//! a product or an absolute value of odd quantities, and no step uses
//! `signum` on a quantity that can be zero — `(0.0).signum()` is `+1` and
//! `(−0.0).signum()` is `−1`, which is precisely how a mirror-symmetric
//! controller acquires a side it prefers (F11's R3, RV72).
//!
//! # What it does not have
//!
//! * **No speed column.** Tier 0 has none and this controller does not get
//!   one. It detects a stalled boat by timeout and by `awa`, as a sailor
//!   without a log does.
//! * **No clock.** Durations are decision counts (F9.1).
//! * **No randomness.** `decide` draws nothing from its `Pcg32`, so the
//!   baseline is a function of the observation sequence alone.
//! * **No reading of [`AgentDebug`].** It is written for observers and read by
//!   no controller — F6.10's discipline, applied to agents.

use crate::spec::{Action, ActionSpace, ActionVec, Agent, AgentDebug, AgentSpec, Cadence};
use sailgym_physics::rng::Pcg32;

/// The adapter width this controller emits: rudder rate, sheet rate, release.
///
/// Spelled here rather than imported from [`crate::actuation::rate::Rate`] so
/// that `pilot/` depends on the **contract** (F14.5's `[−1, 1]^k`) and not on
/// one adapter's type. `tests/rule_sailor.rs` asserts the two agree.
const ACTION_DIM: usize = 3;

/// The columns this controller resolves, by name, at reset.
///
/// **By name, and never by index.** F14.3 makes the layout runtime data, so an
/// index is a guess that a sensor change silently invalidates.
const REQUIRED_COLUMNS: [&str; 8] = [
    "apparent_wind.awa",
    "guidance.bearing_to_target",
    "guidance.cross_track",
    "imu.yaw_rate",
    "imu.heel",
    "imu.roll_rate",
    "actuator_state.delta_r",
    "rig_state.l_sheet",
];

/// The column this controller must **not** read: derived from the true wind
/// and marked `privileged` in the layout (F14.4, RV67).
pub const PRIVILEGED_COLUMN: &str = "guidance.leg_bearing_vs_wind";

// ---------------------------------------------------------------------------
// Tunables
// ---------------------------------------------------------------------------

/// Every number the rule sailor decides from.
///
/// **F14.9 tunables, not physical coefficients.** brief §43 governs
/// `parameters.rs`; it does not govern a controller gain, and the crate
/// boundary is the distinction. Each one carries where it came from, and
/// [`AgentSpec::version`] is bumped by hand whenever a change here changes
/// what the controller decides from the same observation.
///
/// The angles are apparent-wind angles, because that is what the navigator
/// measures. The durations are **decision counts**: at the declared cadence of
/// 10 steps and the shipped `dt` one decision is 0.05 s, so 200 decisions is
/// 10 s — but neither the cadence nor `dt` is read here, and the controller
/// would behave identically at another one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tunables {
    /// rad. The target is sailable when it lies at least this far from the
    /// apparent wind's eye; inside it, the boat beats.
    ///
    /// *Measured.* `trim_sweep` gives this boat's settled VMG against true
    /// wind angle at 5 m/s: 1.22 m/s at 30°, 1.36 at 40°, 1.31 at 50°, 1.15 at
    /// 60°. The best VMG is at a true 40–45°, which the vane reads as an
    /// apparent 26.5–30°. 31.5° is just outside that, so a target
    /// the boat cannot usefully point at is beaten to rather than pinched at.
    pub no_go: f64,
    /// rad, the apparent angle a beat holds. 28.6° is the measured best-VMG
    /// apparent angle above.
    pub close_hauled_awa: f64,
    /// rad, how far onto the new tack the vane must read before a tack counts
    /// as crossed. 9.7° is the same hysteresis band
    /// `docs/v2/practice-validation.md` §3.3 measured for the `complete_tack`
    /// challenge, for the same reason: a boat that merely touches head to wind
    /// has not crossed.
    pub tack_crossed_awa: f64,
    /// rad, how wide a turn **through the wind's eye** is still worth tacking
    /// for.
    ///
    /// *Measured, and the reason this field exists.* A target that is sailable
    /// but on the other side of the wind can be reached two ways: through the
    /// eye, or round through the run. The shorter turn is not the cheaper one
    /// in this model — a tack needs 1.6 m/s and 6–11 s and does not complete
    /// below that, while bearing away through the run always works because the
    /// sail fills throughout. The first `triangle` run turned 156° through the
    /// eye at 0.37 m/s, failed, recovered, and did it again: a limit cycle
    /// that never reached waypoint 2. 110° is comfortably above a
    /// close-hauled-to-close-hauled tack, which is `2 · close_hauled_awa` =
    /// 57°, and below a luff round from a beam reach.
    pub tack_arc_max: f64,
    /// rad, hysteresis on the no-go boundary.
    ///
    /// Beating and fetching ask for nearly the same helm at the boundary, so
    /// the difference is not worth a decision; but the **mode** is what
    /// `run_baseline`'s narration is built from (D5), and without a band the
    /// first `triangle` run changed mode ninety times in one episode. 8° is
    /// a little over a quarter of the close-hauled angle.
    pub no_go_hysteresis: f64,
    /// rad, how far **across the wind's eye** the target must be before the
    /// boat manoeuvres for it.
    ///
    /// *Measured, and the reason this field exists.* Zero would tack the moment
    /// the target drifted a degree onto the other tack, which on a beat is
    /// every few seconds. A tack measured from a settled beat completes in
    /// 5.9–11.2 s **only above about 1.6 m/s** and does not complete at all
    /// below it, so a tack bought for a degree costs the whole beat. 11.5° is a
    /// little under half the close-hauled angle.
    pub tack_margin: f64,
    /// Decisions after a manoeuvre before another may start, **and before the
    /// first one**.
    ///
    /// 300 is 15 s at the shipped cadence. Two measurements set it: the
    /// successful tack settles at 15.6 s
    /// (`docs/v2/practice-validation.md` §3.4), and a tack from a settled beat
    /// needs 1.6 m/s, which this boat takes about 12 s to reach from rest with
    /// the sheet coming in. `reset` starts the counter at **zero** rather than
    /// in the distant past for exactly that reason: the interval that
    /// separates two manoeuvres is the same interval that has to separate the
    /// start of an episode from the first one.
    pub min_tack_interval: u32,
    /// m. Beating, the boat tacks when it is this far from the leg **and**
    /// still sailing away from it. 18 m is a little over four hull lengths —
    /// wide enough that a 40 m beat is two tacks and not six, which matters
    /// when each one costs 15 s.
    pub corridor_half_width: f64,
    /// Decisions the sheet stays hauled after a tack has crossed. 60 is 3 s;
    /// §3.3's `settle_hold_s` is 1.5 s and the boat's speed is still building
    /// at that point.
    pub tack_hold: u32,
    /// Decisions before an uncompleted tack is called irons. 500 is 25 s; the
    /// measured tack crosses at 6.0 s and settles at 15.6 s.
    pub tack_timeout: u32,
    /// Decisions spent bearing away out of irons. 200 is 10 s.
    pub irons_timeout: u32,
    /// rad. The vane must read this far onto the new gybe before the gybe
    /// counts as crossed, measured off the bow: 140°.
    pub gybe_crossed_awa: f64,
    /// m, the sheet length a gybe hauls to before the crossing.
    ///
    /// *Measured.* The boom crosses under control when the sheet is inside the
    /// dead-run optimum: `trim_sweep`'s run entry is 4.07 m (boom ≈ 88°) and
    /// its broad-reach entry 2.90 m (≈ 57°); 2.60 m is just inside the
    /// tighter of the two, so the boom is already in when the wind crosses the
    /// stern.
    pub gybe_sheet_length: f64,
    /// Decisions held after a gybe has crossed. 40 is 2 s.
    pub gybe_hold: u32,
    /// rad, `|φ|` at which the guard eases: 35°.
    ///
    /// *Measured.* `trim_sweep`'s close-hauled entries settle at 24–29° of
    /// heel, so a guard below 30° would be easing the sheet the trim table had
    /// just asked for, on every beat. 35° is above all of them and well inside
    /// the F7 peak of the righting arm.
    pub heel_ease: f64,
    /// rad, `|φ|` at which the guard releases: 55°, between the 35° ease and
    /// `docs/v2/practice-validation.md` §4's measured point of no return.
    pub heel_release: f64,
    /// rad/s, outward roll rate that eases the sheet before the angle itself
    /// is reached.
    pub roll_rate_ease: f64,
    /// Decisions the guard keeps the sheet from being hauled back in. 40 is
    /// 2 s — the hysteresis that stops the guard and the trim table arguing
    /// at the threshold.
    pub heel_hold: u32,
    /// Heading-error gain, rad of rudder per rad of error.
    pub helm_kp: f64,
    /// Yaw-rate damping, rad of rudder per rad/s.
    pub helm_kd: f64,
    /// rad, the largest rudder angle the helm asks for.
    ///
    /// Deliberately **not** `delta_r_max`: that is an F7 value and may not be
    /// copied here (brief §43's grep). 0.61 rad is 35°, inside the shipped
    /// stop, so the helm's own limit binds first and the controller never
    /// depends on where the catalogue's happens to be.
    pub helm_limit_rad: f64,
    /// Rate-command gain, normalised command per rad of rudder error. Large
    /// enough that the command is saturated for any real error, because a zero
    /// command lets the rudder self-centre (F14.10 §2) and the loop has to
    /// hold it.
    pub helm_rate_k: f64,
    /// Sheet-rate gain, normalised command per metre of sheet error.
    pub trim_rate_k: f64,
}

impl Default for Tunables {
    fn default() -> Self {
        // Every angle is stored in **radians** (F1) and written as the
        // degrees it was chosen in, converted here and nowhere else — the
        // same device `sailgym_task::TaskSpec::shipped` uses, for the same
        // reason: four hand-rounded decimal constants would be four numbers
        // nobody could check against the sentence beside them.
        //
        // Three gains below are written at a value chosen partly so that it
        // is **not** a number in the F7 catalogue: `helm_kd` is 0.6 rather
        // than 0.55, `trim_rate_k` 5.5 rather than 6.0, `roll_rate_ease` 0.8
        // rather than 0.85. A gain has no preferred value — nothing measured
        // picks one over its neighbour — and a coincidental collision with an
        // unrelated catalogue number would have cost
        // `no_f7_literal_appears_in_the_pilot_module` an exemption entry, and
        // an audit with exemptions is an audit nobody reads. Recorded here and
        // in `docs/v2/progress/12-handoff.md` rather than left to be noticed.
        Self {
            no_go: 31.5f64.to_radians(),
            close_hauled_awa: 28.6f64.to_radians(),
            tack_crossed_awa: 9.7f64.to_radians(),
            tack_arc_max: 110.0f64.to_radians(),
            no_go_hysteresis: 8.0f64.to_radians(),
            tack_margin: 11.5f64.to_radians(),
            min_tack_interval: 300,
            corridor_half_width: 18.0,
            tack_hold: 60,
            tack_timeout: 500,
            irons_timeout: 200,
            gybe_crossed_awa: 140.0f64.to_radians(),
            gybe_sheet_length: 2.6,
            gybe_hold: 40,
            heel_ease: 35.0f64.to_radians(),
            heel_release: 55.0f64.to_radians(),
            roll_rate_ease: 0.8,
            heel_hold: 40,
            helm_kp: 1.6,
            helm_kd: 0.6,
            helm_limit_rad: 35.0f64.to_radians(),
            helm_rate_k: 8.0,
            trim_rate_k: 5.5,
        }
    }
}

/// The measured sheet-length table: `(|awa| rad, sheet length m)`.
///
/// **Measured, not invented.** Produced by
/// `cargo run --release -p sailgym-bench --bin trim_sweep` on `free_sail`'s
/// own field — a uniform 5 m/s northerly, the conditions all three shipped
/// courses are sailed in. For each target true wind angle, on **both** tacks,
/// the boat is held on that angle with the sheet pinned at each of 41 lengths
/// between the catalogue's two stops, and the settled surge speed, heel and
/// `|awa|` are averaged over the last quarter of a 40 s hold.
///
/// Each entry is the **midpoint of the admissible plateau**: the lengths
/// within 2 % of the best settled speed whose heel stays under 30°. The bare
/// argmax was not used, and the reason is recorded rather than assumed — the
/// speed-versus-length curve is flat near its optimum, so re-running the sweep
/// at 25 and at 41 samples moved the argmax by up to 0.9 m while moving the
/// speed it bought by under 3 %, and a table built from it was not monotonic.
///
/// The two tacks agreed to within **5.1e-5 m/s** of settled speed at every
/// point, which is a measured number and not a symmetry anyone asserted.
///
/// | `\|awa\|` | sheet | settled speed | heel |
/// |---|---|---|---|
/// | 21.9° | 1.213 m | 1.412 m/s | 23.7° |
/// | 26.5° | 1.300 m | 1.771 m/s | 28.9° |
/// | 32.3° | 1.473 m | 2.043 m/s | 28.8° |
/// | 38.1° | 1.646 m | 2.306 m/s | 28.7° |
/// | 49.0° | 2.078 m | 2.553 m/s | 22.3° |
/// | 60.3° | 2.511 m | 2.711 m/s | 16.4° |
/// | 74.6° | 3.116 m | 2.580 m/s | 8.2° |
/// | 90.6° | 3.722 m | 2.393 m/s | 2.0° |
/// | 109.1° | 2.900 m | 2.327 m/s | 10.8° |
/// | 144.3° | 3.505 m | 2.236 m/s | 4.8° |
/// | 177.7° | 4.068 m | 2.190 m/s | 0.3° |
///
/// **It decreases once**, at 90.6° → 109.1° (3.722 m → 2.900 m), and it is
/// left that way. That is what this model does: at an apparent 91° the quick
/// trim is a boom at 78° with attached flow, and at 109° it is a boom at 57°
/// working as a stalled plate. Flattening the step would be choosing a number
/// from how a run looks, which is brief §43's discipline whether or not the
/// number is a coefficient.
///
/// v1's one close-hauled measurement — 2.0 m giving about 1.78 m/s against
/// 0.40 m/s fully hauled — was taken on `close_hauled`'s 3.5 m/s and is the
/// first data point this replaces, not an entry in it.
const TRIM_TABLE: [(f64, f64); 11] = [
    (0.3818, 1.2134),
    (0.4630, 1.2999),
    (0.5644, 1.4729),
    (0.6642, 1.6459),
    (0.8557, 2.0783),
    (1.0518, 2.5107),
    (1.3015, 3.1162),
    (1.5820, 3.7216),
    (1.9043, 2.9000),
    (2.5191, 3.5054),
    (3.1020, 4.0676),
];

/// Linear interpolation into [`TRIM_TABLE`], clamped at both ends.
///
/// `awa_abs` is `|awa|` and is therefore **even** under the mirror, so the
/// sheet length this returns is even too — which is what "sheet and release
/// unchanged" in the mirror contract means.
pub fn trim_table(awa_abs: f64) -> f64 {
    let first = TRIM_TABLE[0];
    let last = TRIM_TABLE[TRIM_TABLE.len() - 1];
    if !awa_abs.is_finite() || awa_abs <= first.0 {
        return first.1;
    }
    if awa_abs >= last.0 {
        return last.1;
    }
    for pair in TRIM_TABLE.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if awa_abs <= b.0 {
            let f = (awa_abs - a.0) / (b.0 - a.0);
            return a.1 + f * (b.1 - a.1);
        }
    }
    last.1
}

// ---------------------------------------------------------------------------
// Modes
// ---------------------------------------------------------------------------

/// What the rule sailor is doing, for an observer.
///
/// Reported through [`AgentDebug`] and turned into `run_baseline`'s narration
/// (D5). It is **never** read by a controller, this one included: the mode is
/// a field of the controller's own state, and the debug record is a copy of it
/// for somebody watching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Beating: the target is inside the no-go zone, so the boat holds
    /// close-hauled on one tack.
    Beating,
    /// Fetching: the target is sailable, so the boat steers at it.
    Fetching,
    /// Through the wind, on the measured recipe: full helm and full haul.
    Tacking,
    /// Hauled in, steadying on the new tack.
    Settling,
    /// Through the stern, with the boom hauled in first.
    Gybing,
    /// Steadying on the new gybe.
    GybeSettling,
    /// Stuck head to wind, bearing away with the sheet eased.
    Recovering,
}

impl Mode {
    /// Every mode, in declaration order, so [`Mode::from_code`] and this
    /// agree by construction.
    pub const ALL: [Mode; 7] = [
        Self::Beating,
        Self::Fetching,
        Self::Tacking,
        Self::Settling,
        Self::Gybing,
        Self::GybeSettling,
        Self::Recovering,
    ];

    /// The stable id, used in a debug note, a log and a narration.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Beating => "beating",
            Self::Fetching => "fetching",
            Self::Tacking => "tacking",
            Self::Settling => "settling",
            Self::Gybing => "gybing",
            Self::GybeSettling => "gybe_settling",
            Self::Recovering => "recovering",
        }
    }

    /// The numeric code an [`AgentDebug`] note carries.
    ///
    /// `AgentDebug::notes` is `Vec<(String, f64)>` — section 05's record, which
    /// this section may not widen — so a mode crosses it as a number and
    /// [`Mode::from_code`] brings it back. The code is the index into
    /// [`Mode::ALL`], which makes the round trip a property of one list.
    pub fn code(self) -> f64 {
        Self::ALL
            .iter()
            .position(|m| *m == self)
            .expect("every mode is in ALL") as f64
    }

    /// The inverse of [`Mode::code`], for `run_baseline`'s narration.
    pub fn from_code(code: f64) -> Option<Self> {
        if code < 0.0 || code.fract() != 0.0 {
            return None;
        }
        Self::ALL.get(code as usize).copied()
    }

    /// Whether this mode is a manoeuvre rather than a way of sailing.
    pub fn is_manoeuvre(self) -> bool {
        !matches!(self, Self::Beating | Self::Fetching)
    }
}

// ---------------------------------------------------------------------------
// Columns
// ---------------------------------------------------------------------------

/// Where each column the controller reads sits in this episode's layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Columns {
    awa: usize,
    bearing: usize,
    cross_track: usize,
    yaw_rate: usize,
    heel: usize,
    roll_rate: usize,
    delta_r: usize,
    l_sheet: usize,
}

impl Columns {
    /// Resolve every required column by name, or report the ones missing.
    fn resolve(fields: &[String]) -> Result<Self, Vec<String>> {
        let at = |name: &str| fields.iter().position(|f| f == name);
        let missing: Vec<String> = REQUIRED_COLUMNS
            .iter()
            .filter(|name| at(name).is_none())
            .map(|name| (*name).to_string())
            .collect();
        if !missing.is_empty() {
            return Err(missing);
        }
        Ok(Self {
            awa: at("apparent_wind.awa").expect("checked"),
            bearing: at("guidance.bearing_to_target").expect("checked"),
            cross_track: at("guidance.cross_track").expect("checked"),
            yaw_rate: at("imu.yaw_rate").expect("checked"),
            heel: at("imu.heel").expect("checked"),
            roll_rate: at("imu.roll_rate").expect("checked"),
            delta_r: at("actuator_state.delta_r").expect("checked"),
            l_sheet: at("rig_state.l_sheet").expect("checked"),
        })
    }
}

/// What the navigator decided, before the manoeuvre layer sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Plan {
    /// The target is inside the no-go zone, so the boat beats for it.
    beating: bool,
    /// rad, the target's bearing from the apparent wind's eye, + to starboard.
    theta: f64,
    /// rad, the arc from the bow to the target **through the eye**.
    via_eye: f64,
}

/// One decision's inputs, in the signs this file reasons in.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sensed {
    /// rad, apparent wind FROM angle off the bow, **+ to starboard**.
    awa: f64,
    /// rad, bearing to the guidance target, + to **port** of the bow.
    brg: f64,
    /// m, cross-track error, + to port of the leg's direction of travel.
    cross_track: f64,
    /// rad/s, yaw rate, + to port.
    yaw_rate: f64,
    /// rad, heel, + starboard down.
    heel: f64,
    /// rad/s, roll rate, + toward starboard.
    roll_rate: f64,
    /// rad, rudder angle, + bow to starboard.
    delta_r: f64,
    /// m, available mainsheet length.
    l_sheet: f64,
}

impl Sensed {
    fn read(obs: &[f64], c: &Columns) -> Option<Self> {
        let get = |i: usize| obs.get(i).copied();
        Some(Self {
            awa: get(c.awa)?,
            brg: get(c.bearing)?,
            cross_track: get(c.cross_track)?,
            yaw_rate: get(c.yaw_rate)?,
            heel: get(c.heel)?,
            roll_rate: get(c.roll_rate)?,
            delta_r: get(c.delta_r)?,
            l_sheet: get(c.l_sheet)?,
        })
    }

    /// The target's bearing in the **starboard-positive** frame every
    /// comparison in this file uses.
    fn target_stbd(&self) -> f64 {
        -self.brg
    }

    /// The tack the vane says the boat is on: `+1` starboard, `−1` port, `0`
    /// exactly head to wind or dead downwind-with-no-side.
    ///
    /// A **three-way** comparison, not `signum`: `(0.0).signum()` is `+1` and
    /// `(−0.0).signum()` is `−1`, so a mirrored head-to-wind observation would
    /// choose the opposite tack and the controller would prefer one side of
    /// the wind (RV72). An `i8` rather than an `f64` so the mirror of `0` is
    /// `0` and not `−0.0`.
    fn tack(&self) -> i8 {
        if self.awa > 0.0 {
            1
        } else if self.awa < 0.0 {
            -1
        } else {
            0
        }
    }
}

// ---------------------------------------------------------------------------
// The controller
// ---------------------------------------------------------------------------

/// The rule sailor.
///
/// Built with [`RuleSailor::new`], reset once per episode, and asked for an
/// action at its declared cadence. Its whole state is below, and every odd
/// field of it mirrors: see the module note on signs.
#[derive(Clone, Debug)]
pub struct RuleSailor {
    tun: Tunables,
    cadence: Cadence,
    /// `None` until a layout has been resolved, and after a layout that was
    /// missing a column.
    cols: Option<Columns>,
    /// The columns the layout did not carry, for the debug note.
    missing: Vec<String>,
    mode: Mode,
    /// The tack the controller is committed to: `+1` starboard, `−1` port,
    /// `0` before the first decision. Mirrors by integer negation.
    side: i8,
    /// Decisions in the current mode.
    since_mode: u32,
    /// Decisions since the last manoeuvre ended.
    since_manoeuvre: u32,
    /// Decisions the heel guard still forbids hauling for.
    heel_wait: u32,
    /// rad, the rudder angle the helm last asked for. An observer's number.
    target_delta_r: f64,
    /// m, the sheet length the trim last asked for. An observer's number.
    target_l_sheet: f64,
    /// Decisions taken. Even under the mirror.
    decisions: u64,
}

/// The version recorded in [`AgentSpec`].
///
/// **Bumped by hand on any change to what the controller decides from the same
/// observation** — a gain, a threshold, a table entry or a branch. Two runs
/// whose baselines differ only in this number are not the same baseline.
pub const RULE_SAILOR_VERSION: u32 = 1;

/// The stable id.
pub const RULE_SAILOR_ID: &str = "rule_sailor";

impl RuleSailor {
    /// The shipped baseline: the default tunables at a cadence of 10 steps
    /// (20 Hz over the F7 default `dt`, F14.6's own example).
    pub fn new() -> Self {
        Self::with_tunables(Tunables::default(), Cadence::new(10))
    }

    /// The same, with the tunables and cadence spelled out. For tests and for
    /// a sensitivity study; the browser and the bench use [`RuleSailor::new`].
    pub fn with_tunables(tun: Tunables, cadence: Cadence) -> Self {
        Self {
            tun,
            cadence,
            cols: None,
            missing: Vec::new(),
            mode: Mode::Fetching,
            side: 0,
            since_mode: 0,
            since_manoeuvre: 0,
            heel_wait: 0,
            target_delta_r: 0.0,
            target_l_sheet: 0.0,
            decisions: 0,
        }
    }

    /// What it is doing, for an observer.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The tack it is committed to: `+1` starboard, `−1` port, `0` before the
    /// first decision.
    pub fn side(&self) -> i8 {
        self.side
    }

    /// The columns a layout did not carry. Empty when the layout was complete.
    pub fn missing_fields(&self) -> &[String] {
        &self.missing
    }

    // -- the layers --------------------------------------------------------

    /// **Navigator.** What the boat wants, in one record.
    ///
    /// Every angle is measured from the apparent wind's **eye**, which sits at
    /// starboard-positive bearing `awa`; the bow is therefore at `−awa` from
    /// it and the target at `−brg − awa`. The no-go test is on the target's own
    /// angle from the eye and on nothing privileged.
    fn navigate(&self, s: &Sensed) -> Plan {
        let theta = wrap_pi(s.target_stbd() - s.awa);
        let theta_bow = -s.awa;
        // Sticky: a boundary crossed by a degree is not a change of plan, and
        // the mode is what the narration is built from.
        let bound = if self.mode == Mode::Beating {
            self.tun.no_go + self.tun.no_go_hysteresis
        } else {
            self.tun.no_go
        };
        Plan {
            beating: theta.abs() < bound,
            theta,
            // The arc from the bow to the target **through the eye**. Its
            // complement through the run is `2π` minus it.
            via_eye: theta_bow.abs() + theta.abs(),
        }
    }

    /// The turn that reaches the target **round the other side**: through the
    /// run where the direct turn goes through the eye, and the reverse.
    ///
    /// A three-way test rather than `signum`: `(0.0).signum()` is `+1` and
    /// `(−0.0).signum()` is `−1`, and a zero turn has no long way round.
    fn the_long_way(err: f64) -> f64 {
        const TWO_PI: f64 = 2.0 * std::f64::consts::PI;
        if err > 0.0 {
            err - TWO_PI
        } else if err < 0.0 {
            err + TWO_PI
        } else {
            err
        }
    }

    /// The turn to the target that goes **through the run** rather than
    /// through the eye.
    ///
    /// `target_stbd` is already the *shortest* turn, because the sensor wraps
    /// the bearing: it goes through the eye when the eye-arc is under half a
    /// turn and through the run when it is over. So which of the two to ask for
    /// is decided by `via_eye` and never by the sign of anything (RV72 again —
    /// the first `windward_leeward` run gybed the wrong way round for exactly
    /// this reason, turning 225° through the eye instead of 135° through the
    /// run).
    fn through_the_run(target_stbd: f64, via_eye: f64) -> f64 {
        if via_eye < std::f64::consts::PI {
            Self::the_long_way(target_stbd)
        } else {
            target_stbd
        }
    }

    /// **Helm.** Heading error and yaw-rate damping give a target rudder
    /// angle; a P-loop on `delta_r` gives the rate command.
    ///
    /// The P-loop is not decoration. A zero rudder-rate command means
    /// *released, self-centring* (F14.10 §2), so a controller that commanded
    /// zero once it liked the angle would watch the rudder return to the
    /// middle. `r > 0` turns the bow to port and `err` is positive to
    /// starboard, so the damping term enters with a **positive** sign.
    fn helm(&mut self, err: f64, delta_r: f64, yaw_rate: f64) -> f64 {
        let limit = self.tun.helm_limit_rad;
        let target = (self.tun.helm_kp * err + self.tun.helm_kd * yaw_rate).clamp(-limit, limit);
        self.target_delta_r = target;
        (self.tun.helm_rate_k * (target - delta_r)).clamp(-1.0, 1.0)
    }

    /// **Trim.** The measured table, tracked through the sheet rate.
    ///
    /// `+1` eases and `−1` hauls (F3), so a sheet shorter than the target asks
    /// for a positive command. Both the lookup and the error are even under
    /// the mirror.
    fn trim(&mut self, awa: f64, l_sheet: f64) -> f64 {
        let target = trim_table(awa.abs());
        self.target_l_sheet = target;
        (self.tun.trim_rate_k * (target - l_sheet)).clamp(-1.0, 1.0)
    }

    /// **Heel guard.** It overrides the trim and never the helm.
    ///
    /// Three rules, in order: release past `heel_release`; ease at `heel_ease`
    /// or on a fast **outward** roll — one the boat is already leaning into,
    /// which is `heel · roll_rate > 0` and therefore even under the mirror;
    /// and, having eased, refuse to haul back in for `heel_hold` decisions.
    /// The hysteresis is what stops the guard and the table arguing at the
    /// threshold.
    fn heel_guard(&mut self, s: &Sensed, sheet: f64) -> (f64, bool) {
        let heel = s.heel.abs();
        if heel >= self.tun.heel_release {
            self.heel_wait = self.tun.heel_hold;
            return (1.0, true);
        }
        let outward = s.heel * s.roll_rate > 0.0 && s.roll_rate.abs() >= self.tun.roll_rate_ease;
        if heel >= self.tun.heel_ease || outward {
            self.heel_wait = self.tun.heel_hold;
            return (1.0, false);
        }
        if self.heel_wait > 0 {
            self.heel_wait -= 1;
            // Easing is still allowed; hauling is not.
            return (sheet.max(0.0), false);
        }
        (sheet, false)
    }

    /// The whole decision, as the three normalised scalars the `rate` adapter
    /// takes: rudder rate, sheet rate, and the release flag as a sign.
    fn step(&mut self, s: &Sensed) -> [f64; ACTION_DIM] {
        self.decisions += 1;
        self.since_mode = self.since_mode.saturating_add(1);
        self.since_manoeuvre = self.since_manoeuvre.saturating_add(1);
        // The vane decides the tack while the boat is sailing; a manoeuvre
        // keeps the tack it started on until it has crossed.
        //
        // **With the same hysteresis band a tack uses.** The bare sign of
        // `awa` is not enough: a boat hanging head to wind has a vane that
        // crosses zero every few decisions, and a committed tack that followed
        // it would make the beating set-point jump by `2 · close_hauled_awa`
        // each time — which is what the first `triangle` run actually did
        // while making sternway. Inside the band the previous tack stands,
        // which is also what a sailor would say: you are on the tack you came
        // from until you have crossed.
        if !self.mode.is_manoeuvre() {
            if s.awa > self.tun.tack_crossed_awa {
                self.side = 1;
            } else if s.awa < -self.tun.tack_crossed_awa {
                self.side = -1;
            } else if self.side == 0 {
                // Nothing committed yet, and the vane is inside the band:
                // take whatever side it does report, which is still a
                // three-way test and still has no preferred side at zero.
                self.side = s.tack();
            }
        }
        let plan = self.navigate(s);
        let side = f64::from(self.side);
        // The target is across the wind's eye from the committed tack, by
        // enough to be worth a manoeuvre. `side · theta` is a product of two
        // odd quantities and is therefore even under the mirror (RV72).
        let across = side * plan.theta >= self.tun.tack_margin;
        // Through the eye when that arc is short enough to be worth a tack in
        // this model; otherwise round through the run, which this boat can
        // always do (see `Tunables::tack_arc_max`).
        let tack_for_it = across && plan.via_eye < self.tun.tack_arc_max;
        let gybe_for_it = across && !tack_for_it;
        // Beyond the corridor and still sailing away from the leg. The side the
        // boat is heading to has the sign of `−brg` in cross-track units, so
        // the product is the test and it is even.
        let running_out = plan.beating
            && s.cross_track.abs() > self.tun.corridor_half_width
            && s.target_stbd() * s.cross_track > 0.0;

        // **The helm's set-point, chosen once.** Steering at the target is
        // right only when the target is sailable *and* on this side of the
        // wind. The first `windward_leeward` run did it unconditionally and
        // luffed from 1.74 m/s to 0.82 m/s waiting for a tack it was not yet
        // allowed to make; the tack then failed, which is the whole of RV68.
        let hold_close_hauled = plan.beating || tack_for_it;
        let mut err = if hold_close_hauled {
            // Hold the vane at the close-hauled angle on the committed tack.
            // Turning the bow to starboard by `δ` moves the apparent wind aft
            // on the port side, so `awa_new = awa − δ` and the turn that
            // reaches `awa_target` is `awa − awa_target`.
            s.awa - side * self.tun.close_hauled_awa
        } else if gybe_for_it {
            Self::through_the_run(s.target_stbd(), plan.via_eye)
        } else {
            s.target_stbd()
        };

        let (rudder, sheet_override) = match self.mode {
            Mode::Beating | Mode::Fetching => {
                self.mode = if hold_close_hauled {
                    Mode::Beating
                } else {
                    Mode::Fetching
                };
                let ready = self.since_manoeuvre >= self.tun.min_tack_interval;
                if ready && (tack_for_it || running_out) {
                    self.enter(Mode::Tacking);
                } else if ready && gybe_for_it {
                    self.enter(Mode::Gybing);
                }
                match self.mode {
                    // Entered a manoeuvre this very decision: fall through to
                    // its own commands rather than waiting a decision.
                    Mode::Tacking | Mode::Gybing => self.manoeuvre(s, err),
                    _ => (self.helm(err, s.delta_r, s.yaw_rate), None),
                }
            }
            // A gybe keeps turning the way it started: while the target is
            // still across the wind's axis the direct turn is the one that goes
            // back through the eye, and taking it would undo the gybe.
            Mode::Gybing => {
                err = if across {
                    Self::through_the_run(s.target_stbd(), plan.via_eye)
                } else {
                    s.target_stbd()
                };
                self.manoeuvre(s, err)
            }
            _ => self.manoeuvre(s, err),
        };

        let sheet = match sheet_override {
            Some(v) => {
                // A manoeuvre's sheet command still has to be published as the
                // trim target an observer reads, or the debug note would
                // describe a trim the boat is not holding.
                self.target_l_sheet = trim_table(s.awa.abs());
                v
            }
            None => self.trim(s.awa, s.l_sheet),
        };
        let (sheet, release) = self.heel_guard(s, sheet);
        [rudder, sheet, if release { 1.0 } else { -1.0 }]
    }

    /// Enter `mode`, resetting its own decision counter.
    fn enter(&mut self, mode: Mode) {
        self.mode = mode;
        self.since_mode = 0;
    }

    /// The manoeuvre state machine. Returns the rudder command and, when the
    /// manoeuvre owns the sheet, the sheet command.
    fn manoeuvre(&mut self, s: &Sensed, err: f64) -> (f64, Option<f64>) {
        let side = f64::from(self.side);
        match self.mode {
            // **The measured recipe**: full helm toward the wind and a full
            // haul, together. Luffing up turns the bow toward the side the
            // wind is on, so the command is the tack's own sign (F2.2).
            Mode::Tacking => {
                let crossed = -side * s.awa >= self.tun.tack_crossed_awa;
                if crossed {
                    self.enter(Mode::Settling);
                    self.side = s.tack();
                    (self.helm(err, s.delta_r, s.yaw_rate), Some(-1.0))
                } else if self.since_mode >= self.tun.tack_timeout {
                    self.enter(Mode::Recovering);
                    // Bear away back onto the tack the boat came from, with
                    // the sheet eased: the tack did not happen, and a boat in
                    // irons gets out by letting the bow fall off the way it
                    // already wants to.
                    (-side, Some(1.0))
                } else {
                    (side, Some(-1.0))
                }
            }
            // Hauled in, steadying on the new tack.
            Mode::Settling => {
                if self.since_mode >= self.tun.tack_hold {
                    self.finish_manoeuvre();
                }
                (self.helm(err, s.delta_r, s.yaw_rate), Some(-1.0))
            }
            // Through the stern, boom in first. The helm steers at the target
            // as usual; what makes it a gybe is the sheet.
            Mode::Gybing => {
                let crossed = -side * s.awa >= self.tun.gybe_crossed_awa;
                if crossed || self.since_mode >= self.tun.tack_timeout {
                    self.enter(Mode::GybeSettling);
                    let tack = s.tack();
                    if tack != 0 {
                        self.side = tack;
                    }
                }
                let target = self.tun.gybe_sheet_length;
                self.target_l_sheet = target;
                let sheet = (self.tun.trim_rate_k * (target - s.l_sheet)).clamp(-1.0, 1.0);
                (self.helm(err, s.delta_r, s.yaw_rate), Some(sheet))
            }
            Mode::GybeSettling => {
                if self.since_mode >= self.tun.gybe_hold {
                    self.finish_manoeuvre();
                }
                (self.helm(err, s.delta_r, s.yaw_rate), None)
            }
            // Irons. Bear away with the sheet eased until the timeout, then
            // sail again — and `min_tack_interval` stops it retrying at once.
            Mode::Recovering => {
                if self.since_mode >= self.tun.irons_timeout {
                    self.finish_manoeuvre();
                    return (self.helm(err, s.delta_r, s.yaw_rate), None);
                }
                (-side, Some(1.0))
            }
            // Not a manoeuvre; the caller does not route here.
            Mode::Beating | Mode::Fetching => (self.helm(err, s.delta_r, s.yaw_rate), None),
        }
    }

    /// A manoeuvre has ended: back to sailing, and the interval starts now.
    fn finish_manoeuvre(&mut self) {
        self.mode = Mode::Fetching;
        self.since_mode = 0;
        self.since_manoeuvre = 0;
    }
}

impl Default for RuleSailor {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrap to `[−π, π]` by **one** addition, which is what makes it exactly odd.
///
/// `sailgym_physics::frames::wrap_pi` is the convention and this agrees with it
/// on every value the controller wraps —
/// `tests::the_local_wrap_agrees_with_the_frames_convention` sweeps the whole
/// domain and asserts it. A second implementation is here for exactly one
/// reason, and it is the reason RV72 exists: `frames::wrap_pi`'s slow path is
/// a `%` and a conditional `+2π`, and `fl(a + π)` is **not** the exact
/// negation of `fl(−a + π)`, so a controller built on it could not satisfy a
/// bit-exact mirror test. This form is `a`, `a − 2π` or `a + 2π`, and
/// `(−a) + 2π` *is* the exact negation of `a − 2π` in IEEE arithmetic.
///
/// The two differ at exactly `a = −π`, where `frames::wrap_pi`'s half-open
/// `(−π, π]` returns `+π` and this returns `−π`. Nothing downstream
/// distinguishes them: the only wrapped quantity is `theta`, which is read
/// through `|theta|` — equal at `±π` — and through `side · theta`, which is
/// only consulted while `|theta| < no_go`, and `no_go < π`.
///
/// The domain is `[−3π, 3π]`, which covers every difference of two angles
/// already wrapped to `(−π, π]`.
fn wrap_pi(a: f64) -> f64 {
    use std::f64::consts::PI;
    const TWO_PI: f64 = 2.0 * PI;
    debug_assert!(
        !a.is_finite() || a.abs() <= 3.0 * PI,
        "wrap_pi takes a difference of wrapped angles: {a}"
    );
    if a > PI {
        a - TWO_PI
    } else if a < -PI {
        a + TWO_PI
    } else {
        a
    }
}

impl Agent for RuleSailor {
    fn spec(&self) -> AgentSpec {
        AgentSpec::new(
            RULE_SAILOR_ID,
            RULE_SAILOR_VERSION,
            ActionSpace::Rates,
            self.cadence,
        )
    }

    /// Resolve the columns by name, and forget everything the last episode
    /// knew.
    ///
    /// A layout missing a required column leaves `cols` as `None`, which makes
    /// every later decision the all-zero action and lists what was missing in
    /// the debug note. **Not a panic**: a panic in WASM is a dead page.
    fn reset(&mut self, fields: &[String], _rng: &mut Pcg32) {
        match Columns::resolve(fields) {
            Ok(cols) => {
                self.cols = Some(cols);
                self.missing.clear();
            }
            Err(missing) => {
                self.cols = None;
                self.missing = missing;
            }
        }
        self.mode = Mode::Fetching;
        self.side = 0;
        self.since_mode = 0;
        // **Zero, not the distant past.** See `Tunables::min_tack_interval`:
        // the boat has no way on at `t = 0` and a tack needs 1.6 m/s, so the
        // first manoeuvre waits exactly as long as every later one.
        self.since_manoeuvre = 0;
        self.heel_wait = 0;
        self.target_delta_r = 0.0;
        self.target_l_sheet = 0.0;
        self.decisions = 0;
    }

    /// Decide, from the concatenated observation vector and nothing else.
    ///
    /// The `Pcg32` is **not drawn from**: the baseline is a function of the
    /// observation sequence alone, which is what makes two runs of it under
    /// the same conditions the same run.
    fn decide(&mut self, obs: &[f64], _rng: &mut Pcg32) -> Action {
        let zero = Action::Rates(
            ActionVec::new(&[0.0; ACTION_DIM]).expect("the all-zero action is in bounds"),
        );
        let Some(cols) = self.cols else {
            return zero;
        };
        let Some(sensed) = Sensed::read(obs, &cols) else {
            // A layout that resolved and an observation shorter than it. The
            // same answer as a missing column, for the same reason.
            return zero;
        };
        if !sensed.awa.is_finite()
            || !sensed.brg.is_finite()
            || !sensed.delta_r.is_finite()
            || !sensed.l_sheet.is_finite()
        {
            return zero;
        }
        let values = self.step(&sensed);
        Action::Rates(
            ActionVec::new(&values)
                .expect("every layer clamps to [-1, 1], so the action is in bounds"),
        )
    }

    /// `mode`, `side`, the target rudder angle and the target sheet length.
    ///
    /// Written for observers and read by no controller (F6.10's discipline).
    /// `missing_fields` appears only when a column was missing, and carries
    /// how many — the names are on [`RuleSailor::missing_fields`], because an
    /// `AgentDebug` note is a number.
    fn debug(&self) -> AgentDebug {
        let mut notes = vec![
            ("mode".to_string(), self.mode.code()),
            ("side".to_string(), f64::from(self.side)),
            ("target_delta_r".to_string(), self.target_delta_r),
            ("target_l_sheet".to_string(), self.target_l_sheet),
            ("decisions".to_string(), self.decisions as f64),
        ];
        if !self.missing.is_empty() {
            notes.push(("missing_fields".to_string(), self.missing.len() as f64));
        }
        AgentDebug { notes }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The required list and the privileged name are disjoint, and the
    /// privileged one is spelled the way the layout spells it.
    #[test]
    fn the_privileged_column_is_not_required() {
        assert!(!REQUIRED_COLUMNS.contains(&PRIVILEGED_COLUMN));
        assert_eq!(REQUIRED_COLUMNS.len(), 8);
    }

    #[test]
    fn the_mode_code_round_trips() {
        for m in Mode::ALL {
            assert_eq!(Mode::from_code(m.code()), Some(m));
            assert!(!m.as_str().is_empty());
        }
        assert_eq!(Mode::from_code(-1.0), None);
        assert_eq!(Mode::from_code(0.5), None);
        assert_eq!(Mode::from_code(Mode::ALL.len() as f64), None);
    }

    #[test]
    fn the_trim_table_is_interpolated_and_clamped() {
        let first = TRIM_TABLE[0];
        let last = TRIM_TABLE[TRIM_TABLE.len() - 1];
        assert_eq!(trim_table(0.0), first.1);
        assert_eq!(trim_table(first.0 - 1.0), first.1);
        assert_eq!(trim_table(first.0), first.1);
        assert_eq!(trim_table(last.0), last.1);
        assert_eq!(trim_table(last.0 + 1.0), last.1);
        assert_eq!(trim_table(f64::NAN), first.1);
        // A midpoint is the mean of its neighbours, and the whole table is
        // inside the range it was swept over.
        let (a, b) = (TRIM_TABLE[0], TRIM_TABLE[1]);
        let mid = trim_table(0.5 * (a.0 + b.0));
        assert!((mid - 0.5 * (a.1 + b.1)).abs() < 1e-12, "{mid}");
        for (awa, l) in TRIM_TABLE {
            assert!((0.0..=std::f64::consts::PI).contains(&awa), "{awa}");
            assert!((1.0..=4.5).contains(&l), "{l}");
        }
        // And it is strictly increasing in `|awa|`, which is what makes the
        // interpolation well defined.
        assert!(TRIM_TABLE.windows(2).all(|w| w[0].0 < w[1].0));
    }

    /// The two turn helpers are exactly odd, which is what makes the
    /// decision-level mirror exact (RV72).
    #[test]
    fn the_turn_helpers_are_exactly_odd() {
        use std::f64::consts::PI;
        for err in [-3.0, -2.0, -0.4, -0.0, 0.0, 0.4, 2.0, 3.0, PI, -PI] {
            assert_eq!(
                RuleSailor::the_long_way(-err),
                -RuleSailor::the_long_way(err),
                "the_long_way is not odd at {err}"
            );
            for via_eye in [0.0, 1.0, PI - 0.1, PI, PI + 0.1, 5.0, 2.0 * PI] {
                assert_eq!(
                    RuleSailor::through_the_run(-err, via_eye),
                    -RuleSailor::through_the_run(err, via_eye),
                    "through_the_run is not odd at {err}, {via_eye}"
                );
            }
        }
        // Non-vacuity, and the one property that matters: the turn it returns
        // is the one that does **not** pass through the eye.
        let two_pi = 2.0 * PI;
        // A short eye-arc: the direct turn goes through the eye, so the run
        // route is the complement.
        assert!((RuleSailor::through_the_run(1.0, 1.0) - (1.0 - two_pi)).abs() < 1e-15);
        // A long eye-arc: the direct turn already goes through the run.
        assert_eq!(RuleSailor::through_the_run(1.0, 5.0), 1.0);
        // And a zero turn has no long way round.
        assert_eq!(RuleSailor::the_long_way(0.0), 0.0);
        assert_eq!(RuleSailor::the_long_way(-0.0), -0.0);
    }

    /// The local wrap is the frames convention, and it is exactly odd.
    #[test]
    fn the_local_wrap_agrees_with_the_frames_convention() {
        use std::f64::consts::PI;
        let mut worst = 0.0f64;
        let n = 20_001;
        for i in 0..n {
            let a = -3.0 * PI + 6.0 * PI * (i as f64) / ((n - 1) as f64);
            let mine = wrap_pi(a);
            let theirs = sailgym_physics::frames::wrap_pi(a);
            // Same angle modulo 2π, to the arithmetic's own precision.
            let gap = ((mine - theirs).abs() % (2.0 * PI))
                .min((2.0 * PI) - ((mine - theirs).abs() % (2.0 * PI)));
            worst = worst.max(gap);
            assert!(mine > -PI - 1e-12 && mine <= PI + 1e-12, "{a} -> {mine}");
            // And **exactly** odd, which is the whole reason this exists.
            assert_eq!(
                wrap_pi(-a).to_bits(),
                (-mine).to_bits(),
                "wrap_pi is not exactly odd at {a}"
            );
        }
        assert!(worst < 1e-9, "the two wraps disagree by {worst}");
        // The one documented difference, at exactly −π.
        assert_eq!(wrap_pi(-PI), -PI);
        assert_eq!(sailgym_physics::frames::wrap_pi(-PI), PI);
        assert_eq!(wrap_pi(PI), PI);
    }

    #[test]
    fn a_three_way_tack_test_has_no_preferred_side() {
        let mut s = Sensed {
            awa: 0.0,
            brg: 0.0,
            cross_track: 0.0,
            yaw_rate: 0.0,
            heel: 0.0,
            roll_rate: 0.0,
            delta_r: 0.0,
            l_sheet: 2.0,
        };
        assert_eq!(s.tack(), 0);
        s.awa = -0.0;
        assert_eq!(s.tack(), 0, "a mirrored head-to-wind must choose no tack");
        s.awa = 0.3;
        assert_eq!(s.tack(), 1);
        s.awa = -0.3;
        assert_eq!(s.tack(), -1);
    }
}
