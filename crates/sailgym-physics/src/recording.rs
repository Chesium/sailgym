//! Episode recording (brief §33, task 9.3).
//!
//! The recording schema is not a debug convenience. brief §45 makes it the
//! forward interface to the episode inspector and to later RL work, so it is
//! versioned from day one ([`EPISODE_SCHEMA_VERSION`]), flat, and
//! typed-array-friendly: every sample is [`FRAME_LEN`] `f64`s in a fixed
//! order, which is what makes [`Episode::to_binary`] a header plus one
//! contiguous `Float64Array`.
//!
//! ## The recorder is an observer
//!
//! [`Recorder::observe`] takes `&Simulation`. It cannot perturb a trajectory,
//! and `tests::recording_does_not_perturb` asserts the bit-identical outcome
//! rather than trusting the type. Recording happens at a configurable rate,
//! not every physics substep (brief §33).
//!
//! ## No wall clock
//!
//! F9.1 forbids the physics crate from reading a clock, and
//! `determinism::no_wall_clock` greps this file along with the rest of `src/`.
//! [`EpisodeHeader::created_utc`] is therefore supplied by the **caller** —
//! the browser wrapper or the native bench — and frozen into the header at
//! [`Recorder::start`]. [`iso8601_utc`] is a pure function of the epoch
//! milliseconds it is handed; it reads nothing.

use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostics;
use crate::scenario::Scenario;
use crate::simulation::Simulation;
use crate::state::STATE_LEN;

/// The only episode schema this build writes or reads.
pub const EPISODE_SCHEMA_VERSION: u32 = 1;

/// Scalars per [`EpisodeFrame`]: `t` + state + controls + wind + forces +
/// moments + tension + reward + capsized.
pub const FRAME_LEN: usize = 1 + STATE_LEN + 3 + 2 + 12 + 4 + 1 + 1 + 1;

/// The four bytes that open a binary episode.
const MAGIC: [u8; 4] = *b"SGEP";

/// The toolchain a recording (or a golden trajectory) was produced by.
///
/// R7: recordings and golden files are only valid for the build that made
/// them (F9's guarantee is same-build, same-platform). Storing the toolchain
/// is what lets the regression harness *skip with a clear message* instead of
/// failing confusingly.
///
/// The values come from `build.rs`, which asks the compiler; nothing here is
/// guessed from `cfg!`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolchainInfo {
    /// `rustc --version` output, e.g. `rustc 1.98.1 (48a229cea 2026-09-01)`.
    pub rustc: String,
    /// The target triple the physics crate was compiled for.
    pub target: String,
    /// `debug` or `release`.
    pub profile: String,
}

impl ToolchainInfo {
    /// The toolchain that compiled this crate.
    pub fn current() -> Self {
        Self {
            rustc: env!("SAILGYM_RUSTC").to_string(),
            target: env!("SAILGYM_TARGET").to_string(),
            profile: env!("SAILGYM_PROFILE").to_string(),
        }
    }

    /// A one-line form for skip messages.
    pub fn describe(&self) -> String {
        format!("{} / {} / {}", self.rustc, self.target, self.profile)
    }
}

/// Everything needed to interpret — and to reproduce — an episode.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EpisodeHeader {
    /// Always [`EPISODE_SCHEMA_VERSION`] in this build.
    pub schema_version: u32,
    pub scenario: Scenario,
    /// Fully resolved, not the scenario's sparse overrides.
    pub parameters: crate::parameters::BoatParameters,
    /// s, the fixed physics timestep the episode was produced at.
    pub dt: f64,
    /// Hz, the logging rate — **not** the physics rate (brief §33).
    pub log_hz: f64,
    /// R7.
    pub toolchain: ToolchainInfo,
    /// Metadata only; **never read by physics** (F9.1). Supplied by the
    /// caller, frozen at [`Recorder::start`].
    pub created_utc: String,
}

/// One logged sample.
///
/// **Field order is normative**: [`EpisodeFrame::to_array`] and the binary
/// encoding depend on it, and so will the episode inspector.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EpisodeFrame {
    /// s, simulation time.
    pub t: f64,
    /// The F3 state, in F8.3 order.
    pub state: [f64; STATE_LEN],
    /// `rudder_rate`, `sheet_rate`, `release` as `0.0`/`1.0`.
    pub controls: [f64; 3],
    /// m/s, true wind at the boat, world frame.
    pub wind_at_boat: [f64; 2],
    /// N, sail/board/rudder/hull force in `B`, `xyz` each, packed in that
    /// order.
    pub forces: [f64; 12],
    /// N·m: yaw, heel, righting, boom.
    pub moments: [f64; 4],
    /// N, mainsheet tension.
    pub sheet_tension: f64,
    /// Placeholder, always `0.0` in v1 (brief §33). Nothing computes a reward
    /// and nothing may: the field exists so the format does not have to
    /// change when something does.
    pub reward: f64,
    pub capsized: bool,
}

impl Default for EpisodeFrame {
    fn default() -> Self {
        Self {
            t: 0.0,
            state: [0.0; STATE_LEN],
            controls: [0.0; 3],
            wind_at_boat: [0.0; 2],
            forces: [0.0; 12],
            moments: [0.0; 4],
            sheet_tension: 0.0,
            reward: 0.0,
            capsized: false,
        }
    }
}

impl EpisodeFrame {
    /// The frame as [`FRAME_LEN`] scalars, in the declared field order.
    pub fn to_array(&self) -> [f64; FRAME_LEN] {
        let mut out = [0.0; FRAME_LEN];
        let mut at = 0;
        let mut put = |values: &[f64]| {
            out[at..at + values.len()].copy_from_slice(values);
            at += values.len();
        };
        put(&[self.t]);
        put(&self.state);
        put(&self.controls);
        put(&self.wind_at_boat);
        put(&self.forces);
        put(&self.moments);
        put(&[self.sheet_tension, self.reward]);
        put(&[if self.capsized { 1.0 } else { 0.0 }]);
        out
    }

    /// The exact inverse of [`EpisodeFrame::to_array`].
    pub fn from_array(a: &[f64; FRAME_LEN]) -> Self {
        let mut at = 0;
        let mut take = |n: usize| {
            let slice = &a[at..at + n];
            at += n;
            slice
        };
        let t = take(1)[0];
        let mut state = [0.0; STATE_LEN];
        state.copy_from_slice(take(STATE_LEN));
        let mut controls = [0.0; 3];
        controls.copy_from_slice(take(3));
        let mut wind_at_boat = [0.0; 2];
        wind_at_boat.copy_from_slice(take(2));
        let mut forces = [0.0; 12];
        forces.copy_from_slice(take(12));
        let mut moments = [0.0; 4];
        moments.copy_from_slice(take(4));
        let sheet_tension = take(1)[0];
        let reward = take(1)[0];
        let capsized = take(1)[0] != 0.0;
        Self {
            t,
            state,
            controls,
            wind_at_boat,
            forces,
            moments,
            sheet_tension,
            reward,
            capsized,
        }
    }
}

/// A recorded episode: one header and a flat run of frames.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    pub header: EpisodeHeader,
    pub frames: Vec<EpisodeFrame>,
}

/// Why an episode could not be decoded.
#[derive(Clone, Debug, PartialEq)]
pub enum EpisodeError {
    /// The bytes or the text are not an episode at all.
    Malformed(String),
    /// The document declares a schema this build does not implement.
    UnsupportedSchemaVersion { found: u32, supported: u32 },
}

impl std::fmt::Display for EpisodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(e) => write!(f, "episode is malformed: {e}"),
            Self::UnsupportedSchemaVersion { found, supported } => write!(
                f,
                "episode schema_version {found} is not supported; this build reads {supported}"
            ),
        }
    }
}

impl std::error::Error for EpisodeError {}

impl Episode {
    /// JSON, for inspection (brief §33).
    pub fn to_json(&self) -> Result<String, EpisodeError> {
        serde_json::to_string(self).map_err(|e| EpisodeError::Malformed(e.to_string()))
    }

    /// The inverse of [`Episode::to_json`], with the version checked first so
    /// an episode from another schema gets a message rather than a shape
    /// complaint.
    pub fn from_json(json: &str) -> Result<Self, EpisodeError> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|e| EpisodeError::Malformed(e.to_string()))?;
        let found = value
            .get("header")
            .and_then(|h| h.get("schema_version"))
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                EpisodeError::Malformed("no header.schema_version in the document".to_string())
            })?;
        check_version(found)?;
        serde_json::from_value(value).map_err(|e| EpisodeError::Malformed(e.to_string()))
    }

    /// The typed-array binary form (brief §33): a small JSON header followed
    /// by one contiguous little-endian `f64` block of `frames × FRAME_LEN`.
    ///
    /// ```text
    /// 0   "SGEP"
    /// 4   u32  schema_version
    /// 8   u32  header JSON length, bytes
    /// 12  u32  frame count
    /// 16  u32  scalars per frame
    /// 20  u32  offset of the f64 block (8-byte aligned)
    /// 24  header JSON
    ///     zero padding to the block offset
    ///     frames × FRAME_LEN × f64, little-endian
    /// ```
    pub fn to_binary(&self) -> Result<Vec<u8>, EpisodeError> {
        let header =
            serde_json::to_vec(&self.header).map_err(|e| EpisodeError::Malformed(e.to_string()))?;
        let prefix = 24 + header.len();
        let offset = prefix.next_multiple_of(8);

        let mut out = Vec::with_capacity(offset + self.frames.len() * FRAME_LEN * 8);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&EPISODE_SCHEMA_VERSION.to_le_bytes());
        out.extend_from_slice(&(header.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.frames.len() as u32).to_le_bytes());
        out.extend_from_slice(&(FRAME_LEN as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        out.extend_from_slice(&header);
        out.resize(offset, 0);
        for frame in &self.frames {
            for v in frame.to_array() {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        Ok(out)
    }

    /// The exact inverse of [`Episode::to_binary`].
    pub fn from_binary(bytes: &[u8]) -> Result<Self, EpisodeError> {
        let bad = |why: &str| EpisodeError::Malformed(why.to_string());
        if bytes.len() < 24 || bytes[..4] != MAGIC {
            return Err(bad("not a sailgym episode (bad magic)"));
        }
        let u32_at = |at: usize| -> u32 {
            u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        check_version(u64::from(u32_at(4)))?;
        let header_len = u32_at(8) as usize;
        let frame_count = u32_at(12) as usize;
        let scalars = u32_at(16) as usize;
        let offset = u32_at(20) as usize;

        if scalars != FRAME_LEN {
            return Err(EpisodeError::Malformed(format!(
                "frame layout is {scalars} scalars; this build reads {FRAME_LEN}"
            )));
        }
        if 24 + header_len > bytes.len() || offset < 24 + header_len {
            return Err(bad("header block does not fit the file"));
        }
        let header: EpisodeHeader = serde_json::from_slice(&bytes[24..24 + header_len])
            .map_err(|e| EpisodeError::Malformed(e.to_string()))?;

        let needed = offset + frame_count * FRAME_LEN * 8;
        if bytes.len() < needed {
            return Err(EpisodeError::Malformed(format!(
                "frame block is short: {} bytes, expected {needed}",
                bytes.len()
            )));
        }
        let mut frames = Vec::with_capacity(frame_count);
        for f in 0..frame_count {
            let mut a = [0.0; FRAME_LEN];
            for (i, slot) in a.iter_mut().enumerate() {
                let at = offset + (f * FRAME_LEN + i) * 8;
                let mut word = [0u8; 8];
                word.copy_from_slice(&bytes[at..at + 8]);
                *slot = f64::from_le_bytes(word);
            }
            frames.push(EpisodeFrame::from_array(&a));
        }
        Ok(Self { header, frames })
    }
}

fn check_version(found: u64) -> Result<(), EpisodeError> {
    if found == u64::from(EPISODE_SCHEMA_VERSION) {
        Ok(())
    } else {
        Err(EpisodeError::UnsupportedSchemaVersion {
            found: found.min(u64::from(u32::MAX)) as u32,
            supported: EPISODE_SCHEMA_VERSION,
        })
    }
}

/// Samples the simulation at a fixed logging rate (brief §33).
///
/// It holds no reference to the simulation and is handed one per observation,
/// so a recorder cannot be the reason a trajectory changed.
pub struct Recorder {
    header: EpisodeHeader,
    frames: Vec<EpisodeFrame>,
    /// `1 / log_hz`, precomputed once so the due-time arithmetic is a
    /// multiply and not a divide per step.
    interval: f64,
    /// The index of the next sample, so due times are `n · interval` and
    /// never an accumulated sum (which would drift).
    next: u64,
}

impl Recorder {
    /// Begin recording at `hz` samples per simulated second.
    ///
    /// A non-positive or non-finite `hz` records every observation.
    pub fn start(hz: f64, header: EpisodeHeader) -> Self {
        let interval = if hz.is_finite() && hz > 0.0 {
            1.0 / hz
        } else {
            0.0
        };
        Self {
            header,
            frames: Vec::new(),
            interval,
            next: 0,
        }
    }

    /// Whether the next completed step's state is due to be logged.
    ///
    /// Exposed so a caller can avoid building a whole [`Diagnostics`] record
    /// on the 199 steps out of 200 that will not be kept. `observe` applies
    /// the same test itself, so skipping this is only slower, never wrong.
    pub fn due(&self, t: f64) -> bool {
        t >= self.next as f64 * self.interval
    }

    /// Log the simulation's published state, if the sample interval has
    /// elapsed. Call after each completed step.
    pub fn observe(&mut self, sim: &Simulation, diag: &Diagnostics) {
        let t = sim.state().t;
        if !self.due(t) {
            return;
        }
        // Advance past every interval this sample covers, so a large `dt` (or
        // a very high `log_hz`) cannot make the recorder fall permanently
        // behind and log every step for ever.
        self.next = if self.interval > 0.0 {
            (t / self.interval).floor() as u64 + 1
        } else {
            self.next + 1
        };

        let c = sim.controls();
        let f = sim.forces();
        let wind = sim.wind_at_boat();
        self.frames.push(EpisodeFrame {
            t,
            state: sim.state().to_array(),
            controls: [
                c.rudder_rate_cmd,
                c.sheet_rate_cmd,
                if c.sheet_release { 1.0 } else { 0.0 },
            ],
            wind_at_boat: [wind.x, wind.y],
            forces: [
                f.sail.f.x,
                f.sail.f.y,
                f.sail.f.z,
                f.board.f.x,
                f.board.f.y,
                f.board.f.z,
                f.rudder.f.x,
                f.rudder.f.y,
                f.rudder.f.z,
                f.hull.f.x,
                f.hull.f.y,
                f.hull.f.z,
            ],
            moments: [
                diag.yaw_moment,
                diag.heeling_moment,
                diag.righting_moment,
                diag.boom_moment.total(),
            ],
            sheet_tension: diag.sheet_tension,
            // brief §33: the hook, and only the hook. Nothing computes a
            // reward in v1.
            reward: 0.0,
            capsized: diag.capsize.capsized,
        });
    }

    /// Frames logged so far.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn header(&self) -> &EpisodeHeader {
        &self.header
    }

    /// Close the recording.
    pub fn finish(self) -> Episode {
        Episode {
            header: self.header,
            frames: self.frames,
        }
    }
}

/// Format epoch milliseconds as an ISO-8601 UTC instant.
///
/// A **pure function of its argument**: it reads no clock, which is what lets
/// it live in the physics crate at all (F9.1). Callers that want "now" bring
/// their own: the browser reads `Date`, the native bench reads the standard
/// library's clock. Neither of those names may appear in this crate, and
/// `determinism::no_wall_clock` is what says so.
pub fn iso8601_utc(epoch_millis: f64) -> String {
    if !epoch_millis.is_finite() {
        return String::new();
    }
    let total_millis = epoch_millis.floor() as i64;
    let (mut days, mut rem) = (
        total_millis.div_euclid(86_400_000),
        total_millis.rem_euclid(86_400_000),
    );
    let millis = rem % 1000;
    rem /= 1000;
    let (hour, minute, second) = (rem / 3600, (rem / 60) % 60, rem % 60);

    // Howard Hinnant's civil_from_days, shifted to an era starting 0000-03-01.
    days += 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::diagnostics;
    use crate::parameters::BoatParameters;
    use crate::scenario::load_shipped;
    use crate::state::{Controls, STATE_FIELDS};

    fn header(scenario: &Scenario, params: &BoatParameters, hz: f64) -> EpisodeHeader {
        EpisodeHeader {
            schema_version: EPISODE_SCHEMA_VERSION,
            scenario: scenario.clone(),
            parameters: *params,
            dt: params.sim.dt,
            log_hz: hz,
            toolchain: ToolchainInfo::current(),
            created_utc: iso8601_utc(1_758_326_400_000.0),
        }
    }

    /// A simulation on `beam_reach_capsize`, which loads the sail, the sheet
    /// and the roll DOF — so every packed force and moment is non-zero.
    fn fixture(hz: f64) -> (Simulation, Recorder) {
        let sc = load_shipped("beam_reach_capsize").expect("shipped scenario");
        let params = sc.to_parameters().expect("valid parameters");
        let mut sim = Simulation::new(params, sc.seed);
        sim.load_scenario(&sc).expect("scenario loads");
        let rec = Recorder::start(hz, header(&sc, &params, hz));
        (sim, rec)
    }

    fn record_seconds(sim: &mut Simulation, rec: &mut Recorder, seconds: f64) {
        let steps = (seconds / sim.params().sim.dt).round() as u32;
        for _ in 0..steps {
            sim.advance(1);
            if rec.due(sim.state().t) {
                let d = diagnostics(sim);
                rec.observe(sim, &d);
            }
        }
    }

    #[test]
    fn log_rate_respected() {
        // 60 s at 20 Hz is 1200 intervals; the boundary sample makes it 1201.
        let (mut sim, mut rec) = fixture(20.0);
        record_seconds(&mut sim, &mut rec, 60.0);
        let n = rec.len() as i64;
        assert!(
            (n - 1200).abs() <= 1,
            "logged {n} frames, expected 1200 ± 1"
        );

        // …and the samples really are one interval apart, not clustered.
        let episode = rec.finish();
        for pair in episode.frames.windows(2) {
            let gap = pair[1].t - pair[0].t;
            assert!(
                (gap - 0.05).abs() < sim.params().sim.dt + 1e-12,
                "samples {} and {} are {gap} s apart",
                pair[0].t,
                pair[1].t
            );
        }
    }

    #[test]
    fn recording_does_not_perturb() {
        // The recorder is an observer: a 60 s run with it on and one with it
        // off must end on bit-identical states.
        let (mut recorded, mut rec) = fixture(20.0);
        record_seconds(&mut recorded, &mut rec, 60.0);

        let (mut plain, _) = fixture(20.0);
        let steps = (60.0 / plain.params().sim.dt).round() as u32;
        for _ in 0..steps {
            plain.advance(1);
        }

        let (a, b) = (recorded.state().to_array(), plain.state().to_array());
        for (i, name) in STATE_FIELDS.iter().enumerate() {
            assert_eq!(
                a[i].to_bits(),
                b[i].to_bits(),
                "field {name}: recorded {} vs plain {}",
                a[i],
                b[i]
            );
        }
        assert!(rec.len() > 1000, "the recorded run must have logged");
        // The run has to have gone somewhere, or this proves nothing.
        assert!(a[0].abs() + a[1].abs() > 1.0, "the boat never moved");
    }

    #[test]
    fn binary_round_trip() {
        let (mut sim, mut rec) = fixture(10.0);
        sim.set_controls(Controls {
            rudder_rate_cmd: 0.3,
            sheet_rate_cmd: -0.5,
            sheet_release: true,
        });
        record_seconds(&mut sim, &mut rec, 20.0);
        let episode = rec.finish();
        assert!(episode.frames.len() > 100);

        let bytes = episode.to_binary().expect("encodes");
        let back = Episode::from_binary(&bytes).expect("decodes");
        assert_eq!(back.header, episode.header);
        assert_eq!(back.frames.len(), episode.frames.len());
        for (i, (a, b)) in episode.frames.iter().zip(back.frames.iter()).enumerate() {
            let (x, y) = (a.to_array(), b.to_array());
            for k in 0..FRAME_LEN {
                assert_eq!(x[k].to_bits(), y[k].to_bits(), "frame {i}, scalar {k}");
            }
            assert_eq!(a.capsized, b.capsized);
        }

        // A file from another schema is refused with a message, not a panic.
        let mut wrong = bytes.clone();
        wrong[4] = 9;
        assert_eq!(
            Episode::from_binary(&wrong).unwrap_err(),
            EpisodeError::UnsupportedSchemaVersion {
                found: 9,
                supported: EPISODE_SCHEMA_VERSION
            }
        );
        assert!(matches!(
            Episode::from_binary(b"nope"),
            Err(EpisodeError::Malformed(_))
        ));
    }

    #[test]
    fn json_round_trip() {
        let (mut sim, mut rec) = fixture(10.0);
        record_seconds(&mut sim, &mut rec, 20.0);
        let episode = rec.finish();

        let json = episode.to_json().expect("encodes");
        let back = Episode::from_json(&json).expect("decodes");
        assert_eq!(back.header, episode.header);
        for (i, (a, b)) in episode.frames.iter().zip(back.frames.iter()).enumerate() {
            let (x, y) = (a.to_array(), b.to_array());
            for k in 0..FRAME_LEN {
                // Exact `f64` round trip — the workspace enables serde_json's
                // `float_roundtrip` feature precisely for this.
                assert_eq!(x[k].to_bits(), y[k].to_bits(), "frame {i}, scalar {k}");
            }
        }
        assert_eq!(back.to_json().unwrap(), json);

        let mut future: serde_json::Value = serde_json::from_str(&json).unwrap();
        future["header"]["schema_version"] = serde_json::json!(7);
        assert_eq!(
            Episode::from_json(&future.to_string()).unwrap_err(),
            EpisodeError::UnsupportedSchemaVersion {
                found: 7,
                supported: EPISODE_SCHEMA_VERSION
            }
        );
    }

    #[test]
    fn header_captures_toolchain() {
        // R7: the header says which build produced the episode.
        let info = ToolchainInfo::current();
        assert!(!info.rustc.is_empty(), "rustc version must be captured");
        assert!(info.rustc.contains("rustc"), "got {:?}", info.rustc);
        assert!(!info.target.is_empty(), "target triple must be captured");
        assert!(
            info.profile == "debug" || info.profile == "release",
            "profile must be a cargo profile, got {:?}",
            info.profile
        );
        assert!(info.describe().len() > 10);

        let (_, rec) = fixture(20.0);
        assert_eq!(rec.header().toolchain, info);
        assert_eq!(rec.header().schema_version, EPISODE_SCHEMA_VERSION);
        assert!(!rec.header().created_utc.is_empty());
    }

    #[test]
    fn reward_placeholder_zero() {
        let (mut sim, mut rec) = fixture(20.0);
        record_seconds(&mut sim, &mut rec, 30.0);
        let episode = rec.finish();
        assert!(!episode.frames.is_empty());
        for (i, frame) in episode.frames.iter().enumerate() {
            // Exactly zero, bit for bit.
            assert_eq!(frame.reward.to_bits(), 0.0_f64.to_bits(), "frame {i}");
        }
    }

    #[test]
    fn no_wall_clock_in_physics() {
        // F9.1. `created_utc` is set once, in `start`, from a value the
        // caller supplies; `observe` and everything it calls read no clock.
        let source = include_str!("recording.rs");
        let body = source
            .split_once("pub fn observe(")
            .expect("observe must be declared here")
            .1
            .split_once("\n    }")
            .expect("observe must close")
            .0;
        // Built from fragments: the crate-wide grep in
        // `determinism::no_wall_clock` scans this file too, test code
        // included, so the needles must not be spelled out in the source.
        let banned = [
            ["Ins", "tant"].concat(),
            ["System", "Time"].concat(),
            ["no", "w("].concat(),
            "created_utc".to_string(),
            "iso8601".to_string(),
        ];
        for needle in &banned {
            assert!(
                !body.contains(needle.as_str()),
                "`observe` mentions `{needle}`; the recorder must read no clock"
            );
        }

        // `start` is where the header — and with it `created_utc` — is
        // frozen, and it takes the value rather than producing one.
        let start = source
            .split_once("pub fn start(hz: f64, header: EpisodeHeader)")
            .expect("start must be declared here")
            .1;
        assert!(start.starts_with(" -> Self {"));

        // The crate-wide guard is `determinism::no_wall_clock`; this is the
        // narrow one for the file that would be most tempted.
        let code: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for needle in &banned[..3] {
            assert!(
                !code.contains(needle.as_str()),
                "recording.rs uses `{needle}`"
            );
        }
        assert!(!code.contains(&["std", "::time"].concat()));
    }

    #[test]
    fn frame_layout_is_the_declared_order() {
        let frame = EpisodeFrame {
            t: 1.0,
            state: std::array::from_fn(|i| 10.0 + i as f64),
            controls: [100.0, 101.0, 1.0],
            wind_at_boat: [200.0, 201.0],
            forces: std::array::from_fn(|i| 300.0 + i as f64),
            moments: std::array::from_fn(|i| 400.0 + i as f64),
            sheet_tension: 500.0,
            reward: 0.0,
            capsized: true,
        };
        let a = frame.to_array();
        assert_eq!(a.len(), FRAME_LEN);
        assert_eq!(a[0], 1.0);
        assert_eq!(a[1], 10.0);
        assert_eq!(a[1 + STATE_LEN], 100.0);
        assert_eq!(a[1 + STATE_LEN + 3], 200.0);
        assert_eq!(a[1 + STATE_LEN + 5], 300.0);
        assert_eq!(a[1 + STATE_LEN + 17], 400.0);
        assert_eq!(a[FRAME_LEN - 3], 500.0);
        assert_eq!(a[FRAME_LEN - 2], 0.0);
        assert_eq!(a[FRAME_LEN - 1], 1.0);
        assert_eq!(EpisodeFrame::from_array(&a), frame);
    }

    #[test]
    fn iso8601_is_a_pure_function_of_its_argument() {
        assert_eq!(iso8601_utc(0.0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso8601_utc(1_000.0), "1970-01-01T00:00:01.000Z");
        // 2026-09-20T00:00:00Z
        assert_eq!(iso8601_utc(1_789_862_400_000.0), "2026-09-20T00:00:00.000Z");
        assert_eq!(
            iso8601_utc(1_789_862_400_000.0 + 3_661_123.0),
            "2026-09-20T01:01:01.123Z"
        );
        // A leap day, and the same input twice.
        assert_eq!(iso8601_utc(1_709_164_800_000.0), "2024-02-29T00:00:00.000Z");
        assert_eq!(iso8601_utc(f64::NAN), "");
        assert_eq!(iso8601_utc(12_345.0), iso8601_utc(12_345.0));
    }
}
