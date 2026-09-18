# Section 09 — Scenario System, Recording and Replay (M8)

**Prerequisite reading:** `docs/00-foundations.md` (F3, F8, F9, F11/R7),
`docs/progress/08-handoff.md`, `docs/brief.md` §32, §33, §34, §45.

## Goal

Serializable scenarios, the six shipped configurations, deterministic episode
recording, replay with timeline scrubbing, and a format designed to become the RL
episode-inspection interface (brief §33).

## Design constraint carried from brief §45

The recording schema is not a debug convenience — it is the forward interface to
`episode inspector` and later RL work. Version it from day one, keep it flat and
typed-array-friendly, and include a `reward` placeholder (brief §33) even though
nothing computes one in v1.

---

## Tasks

### 9.1 — Scenario schema
**P-group: S**
**Owns:** `crates/sailgym-physics/src/scenario.rs`, `web/src/sim/scenarioTypes.ts`

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Scenario {
    pub schema_version: u32,          // 1
    pub name: String,
    pub description: String,
    pub seed: u64,
    /// Sparse dotted-path overrides onto BoatParameters::ilca7(). Not a full copy.
    pub parameter_overrides: BTreeMap<String, f64>,
    pub initial_state: InitialState,
    pub wind: WindConfig,
    pub camera: CameraSuggestion,
    /// Optional initial control values. NOT a script of future inputs (brief §32).
    pub initial_controls: Option<ControlsSpec>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct InitialState {
    pub x: f64, pub y: f64,
    pub heading_deg: f64,      // compass-style, converted once on load
    pub heel_deg: f64,
    pub speed: f64,            // initial forward speed, m/s
    pub boom_deg_to_port: f64, // human-facing; converted to β per F2.1
    pub sheet_length: f64,
}
impl Scenario {
    pub fn load(json: &str) -> Result<Self, ScenarioError>;
    pub fn to_boat_state(&self) -> BoatState;
    pub fn to_parameters(&self) -> Result<BoatParameters, ScenarioError>;
    pub fn validate(&self) -> Result<(), ScenarioError>;
}
```

`BTreeMap` not `HashMap` — F9.3. Overrides are sparse and applied in key order so
the result is deterministic.

Human-facing angle fields (`heading_deg`, `boom_deg_to_port`) are converted in
exactly one place, `to_boat_state`. Do not scatter degree conversions.

**Acceptance criteria**
- `cargo test -p sailgym-physics scenario::`:
  - `round_trip`: serialise → deserialise → serialise is byte-identical for all
    six shipped scenarios.
  - `boom_conversion`: `boom_deg_to_port: 30.0` produces `beta` ≈ `−0.524` rad
    (F2.1's sign flip, applied once and correctly).
  - `heading_conversion`: `heading_deg: 90` (east, compass) produces the correct
    `psi` per F2; assert against a hand-computed value in the test.
  - `override_determinism`: applying the same override map twice from different
    insertion orders yields bit-identical `BoatParameters`.
  - `unknown_override_rejected`: an override for a nonexistent path is `Err`,
    not silently ignored.
  - `validate_rejects_bad`: negative `sheet_length`, `seed` absent, unsupported
    `schema_version` each produce a distinct error variant.
- TS `scenarioTypes.ts` parity asserted the same way as diagnostics in 8.1.

---

### 9.2 — The six shipped scenarios
**P-group: A**
**Owns:** `scenarios/*.json`, `web/src/ui/ScenarioPicker.tsx`
**Depends:** 9.1

Exactly the six of brief §32, no more:

| File | Purpose | Notes |
|---|---|---|
| `beam_reach_capsize.json` | Beam-on wind, oversheeting demonstrates capsize risk | Wind speed chosen so capsize is reachable but not instant — start from the figure recorded in `07-handoff.md` |
| `sheet_release_recovery.json` | Identical setup; easing demonstrates reduced heel | **Must be byte-identical to `beam_reach_capsize` except `name`/`description`** |
| `close_hauled.json` | Stable close-hauled sailing | Wind 3.5 m/s per F7/R2 unless 07's handoff says otherwise |
| `tack.json` | Initial condition suitable for manually tacking | |
| `gybe.json` | Initial condition for controlled/uncontrolled gybe experiments | |
| `free_sail.json` | Neutral sandbox | The default on load |

**These scenarios must not script outcomes** (brief §32 last line). No scripted
future control sequence, no forced capsize, no timed events. A scenario is an
initial condition and an environment, nothing more.

The `sheet_release_recovery` constraint is deliberate: brief §46 demonstrates
capsize and recovery from the *same* setup, differing only in what the human
does. If the two files differ in any physical field, the demonstration proves
nothing. Assert byte-equality of the physical fields in a test.

**Acceptance criteria**
- `cargo test -p sailgym-physics scenario::shipped::`:
  - all six load, validate, and produce finite 60 s runs under neutral controls;
  - `recovery_matches_capsize_setup`: the two files' `seed`, `parameter_overrides`,
    `initial_state` and `wind` are **equal**, asserted field by field;
  - `free_sail_is_default`: the app's default scenario id is `free_sail`;
  - `no_scripted_outcomes`: no scenario JSON contains a key matching
    `script|sequence|events|timeline|forced`.
- `web/tests/e2e/scenarios.spec.ts`: each of the six loads in the browser,
  reaches a finite state, and switching between them takes < 500 ms with no
  console error.

---

### 9.3 — Recorder
**P-group: A**
**Owns:** `crates/sailgym-physics/src/recording.rs`, `crates/sailgym-wasm/src/lib.rs` (recording methods)
**Depends:** 9.1

Recording at a configurable rate, not every substep (brief §33).

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EpisodeHeader {
    pub schema_version: u32,          // 1
    pub scenario: Scenario,
    pub parameters: BoatParameters,   // fully resolved, not the sparse overrides
    pub dt: f64,
    pub log_hz: f64,
    pub toolchain: ToolchainInfo,     // R7: rustc version, target, build profile
    pub created_utc: String,          // metadata only; NEVER read by physics (F9.1)
}

/// One logged sample. Field order is normative; the binary encoding depends on it.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct EpisodeFrame {
    pub t: f64,
    pub state: [f64; STATE_LEN],
    pub controls: [f64; 3],           // rudder_rate, sheet_rate, release as 0.0/1.0
    pub wind_at_boat: [f64; 2],
    pub forces: [f64; 12],            // sail/board/rudder/hull xyz, packed
    pub moments: [f64; 4],            // yaw, heel, righting, boom
    pub sheet_tension: f64,
    pub reward: f64,                  // placeholder, always 0.0 in v1 (brief §33)
    pub capsized: bool,
}

pub struct Recorder { /* … */ }
impl Recorder {
    pub fn start(hz: f64, header: EpisodeHeader) -> Self;
    /// Called after each completed step; records only when the sample interval elapses.
    pub fn observe(&mut self, sim: &Simulation, diag: &Diagnostics);
    pub fn finish(self) -> Episode;
}
```

Two serialisations, both shipped: JSON for inspection, and a typed-array binary
form (header JSON + a `Float64Array` block) for size (brief §33).

**Acceptance criteria**
- `cargo test -p sailgym-physics recording::`:
  - `log_rate_respected`: 60 s at `log_hz = 20` produces `1200 ± 1` frames.
  - `recording_does_not_perturb`: a 60 s run with recording on and one with it
    off produce **bit-identical** final states. The recorder is an observer.
  - `binary_round_trip`: binary encode → decode reproduces every frame bit-identically.
  - `json_round_trip`: same for JSON, within exact `f64` round-trip.
  - `header_captures_toolchain`: `ToolchainInfo` is populated and non-empty (R7).
  - `reward_placeholder_zero`: every frame's `reward` is exactly `0.0` in v1.
  - `no_wall_clock_in_physics`: `created_utc` is set in `start`, and a grep
    asserts no time call exists in `observe` or anything it calls.

---

### 9.4 — Replay player and timeline
**P-group: B**
**Owns:** `web/src/sim/replay.ts`, `web/src/ui/Timeline.tsx`
**Depends:** 9.3

A replay **consumes stored trajectory data; it does not recompute physics**
(brief §33). The renderer reads frames from the episode exactly as it reads
snapshots from the live sim, so the same components work in both modes.

```ts
export interface ReplaySource {
  header: EpisodeHeader
  frameCount: number
  frameAt(i: number): EpisodeFrame
  /** Linear interpolation between frames for smooth scrubbing. Display only. */
  sampleAt(t: number): EpisodeFrame
}
export type PlaybackMode = { kind: 'live' } | { kind: 'replay'; source: ReplaySource }
```

Timeline supports play, pause, scrub, step, playback speed, and reset to episode
start (brief §33).

**Acceptance criteria**
- `pnpm --dir web test:unit replay`:
  - `sampleAt` at an exact frame time returns that frame's values unchanged;
  - `sampleAt` between frames interpolates linearly, verified against a
    hand-computed midpoint;
  - interpolation of `psi` and `beta` handles the `±π` wrap without a spike
    (assert no interpolated angle jumps by more than the true delta).
- `web/tests/e2e/replay.spec.ts`:
  - record 10 s, stop, enter replay, and the rendered boat position at `t = 5 s`
    matches the recorded frame within 1e-6;
  - scrubbing to `t = 2 s` and back to `t = 8 s` shows the recorded states, not
    recomputed ones — asserted by editing one in-memory frame and confirming the
    render follows the edit;
  - "reset to episode start" returns to frame 0;
  - replay works with the physics clock paused.

---

### 9.5 — Export and import
**P-group: B**
**Owns:** `web/src/sim/episodeIo.ts`, `web/src/ui/RecordControls.tsx`
**Depends:** 9.3, 9.4

Start/stop recording controls, download as JSON or binary, and load an episode
file back into the replay player.

**Acceptance criteria**
- `web/tests/e2e/replay.spec.ts`: record → export → re-import → replay produces
  a frame sequence identical to the original (compared in-page, avoiding a real
  file dialog by exercising the underlying blob path directly).
- Exported JSON validates against the `EpisodeHeader`/`EpisodeFrame` schema.
- An episode recorded with a different `schema_version` is rejected with a clear
  message, not a crash.

---

### 9.6 — Golden scenario regression tests
**P-group: C**
**Owns:** `crates/sailgym-physics/tests/regression.rs`, `crates/sailgym-bench/src/bin/gen_golden.rs`, `crates/sailgym-physics/tests/golden/*.json`
**Depends:** 9.2, 9.3

For each of the six scenarios: a fixed 30 s control script (defined **in the
test**, not in the scenario — brief §32 forbids scripted scenarios), producing a
golden trajectory sampled at 5 Hz and committed.

R7 handling is mandatory and must be got right: golden files record the toolchain
that produced them. On mismatch the test **skips with an explicit message**
naming both toolchains — it does not fail, and it does not silently pass.

```rust
/// Regenerate all golden files. Run deliberately, never automatically.
/// cargo run -p sailgym-bench --bin gen_golden
fn main();
```

`gen_golden` must refuse to run if the working tree has uncommitted changes under
`crates/sailgym-physics/src`, so goldens are never regenerated to paper over a
physics change in progress.

**Acceptance criteria**
- `cargo test -p sailgym-physics --test regression` exits 0 with six tests.
- Tolerance: `1e-9` absolute on positions and angles, `1e-10` on body-frame
  velocities, at every sampled point.
- Deliberately perturbing a coefficient by 0.1 % makes at least four of the six
  regressions fail — verify once, then revert. A regression suite that tolerates
  a physics change is worthless.
- Running on a mismatched toolchain produces a skip whose message names the
  expected and actual versions.
- `gen_golden` refuses to run with a dirty physics tree.

---

### 9.7 — Determinism and replay E2E
**P-group: C**
**Owns:** `crates/sailgym-physics/tests/invariants.rs` (adds), `web/tests/e2e/determinism.spec.ts` (extends)
**Depends:** 9.4

Add `deterministic_replay` to the invariant suite (brief §35): the same seed and
control sequence, replayed through the recorder twice, produces identical frames
bit-for-bit.

Extend the browser determinism spec: load `beam_reach_capsize`, play a scripted
key sequence, record; reset; repeat; assert the two episodes are frame-identical.

**Acceptance criteria**
- `cargo test -p sailgym-physics --test invariants` exits 0 with 21 tests.
- `pnpm --dir web test:e2e determinism` exits 0 including the new episode check.

---

## Section acceptance criteria

1. `pwsh scripts/check.ps1` exits 0, now including the regression step.
2. All six brief §32 scenarios ship, load, and script no outcomes
   (`no_scripted_outcomes` passes).
3. `sheet_release_recovery` is physically identical to `beam_reach_capsize` —
   the brief §46 demonstration is therefore honest.
4. Recording does not perturb the simulation (`recording_does_not_perturb`,
   bit-identical).
5. Replay consumes stored data, proven by the frame-edit test in 9.4.
6. Six golden regressions pass, and are proven sensitive to a 0.1 % coefficient change.
7. R7 handled: toolchain mismatch skips with a clear message.
8. `reward` placeholder present and zero; the schema is versioned at 1.
9. `docs/progress/09-handoff.md` written, including the toolchain recorded in the
   golden files and the exact control scripts used.

## Risks touched

- **R7 — resolved here.** The skip-on-mismatch behaviour is the deliverable, not
  a workaround. State clearly in the handoff which platform the committed goldens
  came from.
- **R2** — scenario wind speeds encode the answer from section 07. If
  `close_hauled` needed a wind speed below 3.5 m/s to be sailable, say so.
