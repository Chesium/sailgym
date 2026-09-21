# The episode recording format

**Normative for the recording schema.** It records what
`crates/sailgym-physics/src/recording.rs` implements; that file is the
implementation and this document is its contract. Conventions, frames, units
and the state layout are `docs/v1/00-foundations.md`'s and are not restated
here — F1 for units, F3 and F8.3 for the state, F6 for the quantities.

Written by v2 section 10 task 10.1. v2 F18.3 is the normative delta it
implements.

---

## 1. Versions

| Constant | Value | Meaning |
|---|---|---|
| `EPISODE_SCHEMA_VERSION` | **2** | the schema this build *writes* |
| `SUPPORTED_SCHEMA_VERSIONS` | `[1, 2]` | the schemas this build *reads* |
| `IDENTITY_VERSION` | 1 | the version of the canonical `ExperimentIdentity` record |
| `PRACTICE_ENVELOPE_VERSION` | 1 | section 11's reserved envelope |
| `SCENARIO_SCHEMA_VERSION` | 1 | unchanged; the scenario document inside the header |

The TypeScript mirror of all four is `web/src/sim/scenarioTypes.ts`, and
`web/tests/unit/scenarioTypes.test.ts` compares the two declarations field for
field, in order, in both directions.

**An episode is re-encoded in the schema its own header declares.** Reading a
schema-1 file and writing it out again produces a schema-1 file, in either
codec. Nothing is silently upgraded; nothing is silently filled in.

An unsupported version is refused with

```
episode schema_version 7 is not supported; this build reads 1 and 2
```

and the episode already loaded is left exactly as it was.

---

## 2. What schema 2 added, and why

Schema 1 recorded the F3 state, four component forces, four moments, the sheet
tension and the wind vector at the boat. That is enough to draw the boat and
not enough to drive the HUD, the force overlay, the charts or the capsize
readout — so a replay built on it had to fall back on the **live** simulator
for everything else, which is the defect v2 F18.3 exists to close.

Schema 2 adds exactly two things:

1. **`EpisodeFrame.diag`** — a [`FrameDiagnostics`](#4-the-frame) block
   captured at the same state and time as the sample it sits beside.
2. **Seven header fields** carrying the canonical
   [identity](#5-experiment-identity).

Both are `#[serde(default)]`, so a schema-1 document decodes into the same
types with `diag = None` and every added header field `Recorded::Unknown`.

---

## 3. The header

```
schema_version    u32     1 or 2
scenario          Scenario           the scenario document, verbatim (schema 1)
parameters        BoatParameters     the fully RESOLVED F7 catalogue
dt                f64     s, the fixed physics timestep
log_hz            f64     Hz of SIMULATED time — metadata, not identity
toolchain         ToolchainInfo      rustc / target / profile (R7) — metadata
created_utc       String  ISO-8601, supplied by the caller (F9.1) — metadata
--- schema 2 ------------------------------------------------------------------
identity_version  u32     IDENTITY_VERSION, or 0 in a pre-identity document
model             Recorded<ModelIdentity>          F18.1d
initial_state     Recorded<BoatState>              the FIRST SAMPLE's state
initial_controls  Recorded<Controls>
practice          Option<PracticeEnvelope>         section 11
action            Recorded<ActionIdentity>
observation       Recorded<ObservationIdentity>
```

`parameters` is the catalogue **in force**, with the scenario's
`parameter_overrides` already applied and any live brief §31 edit included. The
sparse overrides travel beside it, in `scenario.parameter_overrides`, so a
reader can see both what was asked for and what it resolved to. A scenario
*name* is not an initial condition and is never treated as one.

`initial_state` is the state at the **first recorded sample**, not the state
the scenario document describes. A recording started mid-run, after an ad-hoc
reset, or after a parameter edit therefore describes the episode that exists.

`log_hz`, `toolchain` and `created_utc` are **metadata**: a different logging
rate changes the sampling resolution and a rebuilt compiler changes neither the
equations nor the boat, so none of the three is part of identity (v2 F18.3).

### `Recorded<T>` — the three-valued field

```
{ "value": … }      the episode carries it
"unknown"           the concept applies; this document does not say
"not_applicable"    the concept does not exist for this episode
```

The distinction is load-bearing. A hand-flown browser episode has no action
adapter, so `action` is `not_applicable` — that is information. A schema-1 file
cannot say whether it had one, so `action` is `unknown` — that is a gap.
**Unknown is not equal to anything, including another unknown.**

---

## 4. The frame

Field order is normative: it is the binary layout.

| # | field | scalars | unit / meaning |
|---|---|---|---|
| 0 | `t` | 1 | s, simulation time |
| 1 | `state` | 13 | the F3 state in F8.3 order |
| 2 | `controls` | 3 | `rudder_rate`, `sheet_rate`, `release` as 0/1 |
| 3 | `wind_at_boat` | 2 | m/s, true wind, world frame |
| 4 | `forces` | 12 | N, sail / board / rudder / hull, `xyz` each, in `B` |
| 5 | `moments` | 4 | N·m: yaw, heel, righting, boom **total** |
| 6 | `sheet_tension` | 1 | N |
| 7 | `reward` | 1 | brief §33's placeholder; always exactly `0.0` |
| 8 | `capsized` | 1 | `0.0` / `1.0` |
| | **schema-1 total** | **`FRAME_LEN_V1` = 38** | |
| 9 | `diag` | 40 | schema 2 only; absent in a schema-1 frame |
| | **schema-2 total** | **`FRAME_LEN` = 78** | |

The layout is a **strict extension**: the first 38 scalars of a schema-2 frame
are exactly a schema-1 frame, which is what lets one decoder read both widths.

### `FrameDiagnostics` — the recorded diagnostic subset

`DIAG_LEN` = 40 scalars, in this order:

| offset | field | scalars | unit |
|---|---|---|---|
| 0 | `wind_speed` | 1 | m/s |
| 1 | `wind_bearing_deg` | 1 | deg, meteorological FROM, CW from north |
| 2 | `true_wind_body` | 2 | m/s, in `H` |
| 4 | `apparent_wind_body` | 3 | m/s, in `B` |
| 7 | `apparent_wind_speed` | 1 | m/s |
| 8 | `apparent_wind_angle` | 1 | rad, FROM off the bow, + to starboard |
| 9 | `speed_over_ground` | 1 | m/s |
| 10 | `acceleration_body` | 2 | m/s², `(u̇, v̇)` in `H` |
| 12 | `total_force_h` | 2 | N, `(ΣX, ΣY)` in `H` |
| 14 | `sheet_force` | 3 | N, the pull on the boom at `P_b`, in `B` |
| 17 | `sail_ce_b` | 3 | m, the sail load's arm |
| 20 | `board_centre_b` | 3 | m, the board load's arm |
| 23 | `rudder_centre_b` | 3 | m, the rudder load's arm |
| 26 | `sheet_attach_b` | 3 | m, `P_b(β)` |
| 29 | `sheet_block_b` | 3 | m, `P_k` |
| 32 | `alpha_sail` | 1 | rad |
| 33 | `alpha_board` | 1 | rad |
| 34 | `alpha_rudder` | 1 | rad |
| 35 | `sheet_rope_length` | 1 | m, `ℓ(β)` |
| 36 | `sheet_extension` | 1 | m, `e = ℓ − L` |
| 37 | `gz` | 1 | m, the righting arm |
| 38 | `capsize_since` | 1 | s |
| 39 | `capsize_max_heel` | 1 | rad |

`wind_speed` and `wind_bearing_deg` come from
`environment::wind_to_bearing`, so the meteorological from/toward conversion
still exists in exactly one place (F6.1) and the browser never derives a
bearing of its own.

### What is **not** recorded, and what a replay must therefore say

These `diagnostics::Diagnostics` fields are deliberately absent. A replay shows
each as **unavailable**; it may not evaluate the model currently loaded to fill
the gap (RV59).

| field | why it is omitted |
|---|---|
| `steps` | a counter of the live run, not a measurement of the boat |
| `course_over_ground` | read by no replay consumer |
| `leeway_angle` | read by no replay consumer |
| `cl_sail`, `cd_sail`, `cl_board`, `cd_board`, `cl_rudder`, `cd_rudder` | six scalars per sample, read by no replay consumer |
| `boom_moment` breakdown | its **total** is `moments[3]`; the four terms are read by no replay consumer |
| `sheet_hull` | the block-end reaction; the boom end is recorded, the pair is a debug-panel row |
| `energy_kinetic`, `energy_roll_potential`, `energy_sheet_elastic` | debug-panel rows |
| `hull_model_warning` | an R6 flag on the live hull model |

`recording::tests::the_omitted_diagnostics_are_named_at_their_source` asserts
that **every** `Diagnostics` field is either carried by a frame or named in
this list, so a field added to `diagnostics.rs` cannot drift into being
silently absent.

Five fields are not in the block because the frame already carries them
verbatim: `velocity_body` is `(state.u, state.v)`, `yaw_rate` is `state.r`,
`roll_rate` is `state.p`, and `beta`/`beta_dot` are `state.beta`/
`state.beta_dot`. Re-reading the recorded state is an index, not a
recomputation. `heel_deg` is `state.phi` in degrees, which F1 permits at a UI
boundary; `capsize` is the frame's `capsized` flag beside the block's
`capsize_since` and `capsize_max_heel`.

---

## 5. Experiment identity

`EpisodeHeader::identity()` builds the canonical record. It is **derived, not
stored**: a second copy of the catalogue in the document would be a second
thing to keep in step.

```
identity_version   u32
model              Recorded<ModelIdentity>      F18.1d: model_version + src tree id
parameters         Recorded<BoatParameters>     the resolved catalogue
integrator         Recorded<Integrator>
dt                 Recorded<f64>
initial_state      Recorded<BoatState>
initial_controls   Recorded<Controls>
scenario           Recorded<String>             the scenario id
wind               Recorded<WindConfig>
seed               Recorded<u64>
task               Recorded<TaskIdentity>       id, version, thresholds
action             Recorded<ActionIdentity>     adapter, version, period_steps
observation        Recorded<ObservationIdentity>  layout, units, normalisation, noise, privilege
```

For a **schema-1** header the scenario, catalogue, integrator, `dt`, wind and
seed are still `value` — they were always in the document — and only `model`,
`initial_state`, `initial_controls`, `task`, `action` and `observation` come
back `unknown`. A legacy episode can therefore be *viewed* in full detail and
still cannot be labelled a same-conditions experiment.

### Comparison

`ExperimentIdentity::compare` returns one of three verdicts and names the
fields behind a negative one, in the fixed order above:

| verdict | meaning |
|---|---|
| `SameConditions` | every compared field is known on both sides and agrees |
| `Different(fields)` | a compared field is known on both sides and differs |
| `Indeterminate(fields)` | a compared field is unknown, or the model identity is dirty or unknown |

Only `SameConditions` licenses the phrase. In particular a changed **equation**
(a different physics source tree id), **seed**, **`dt`**, **initial condition**
or **task threshold** each makes a strict comparison incompatible on its own,
and `recording::tests::same_conditions_needs_every_field_and_a_clean_source`
measures all five.

`model` is compared through `ModelIdentity::is_comparable_with`, so a **dirty**
or **unknown** source tree is `Indeterminate` — including when compared with
itself (F18.1d). A parameter digest cannot see a changed equation and a source
id cannot see a changed parameter; both travel, and both are required.

### No digest is shipped

v2 F16.4 permits an optional compact key but requires an established SHA-256
rather than handwritten cryptography, and states that stable equality of the
canonical records is enough. This crate implements no cryptographic primitive,
so comparison is structural equality and
`ExperimentIdentity::canonical_json()` is the stable text form — one
declaration order, `BTreeMap` for every map (F9.3). If a digest is ever added,
it must cover this whole record and not the parameters alone.

---

## 6. The practice envelope (reserved for section 11)

```
practice: Option<PracticeEnvelope>

PracticeEnvelope { envelope_version: u32, task: TaskIdentity, events: Vec<PracticeEvent> }
TaskIdentity     { id: String, version: u32, thresholds: BTreeMap<String, f64> }
PracticeEvent    { id: String, step: u64, t: f64, value: f64 }
```

This is **one concrete dependent feature, not an extension registry.** Section
11 writes through the existing recorder — `Recorder::set_practice` and
`Recorder::push_practice_event` — rather than opening a second `Episode` type
or a second recorder in the task crate.

An event carries the **physics step index** it was decided on as well as the
simulated time, because a threshold crossing belongs to a step and not to an
interpolated instant (v2 F18.4). `push_practice_event` refuses an event when no
envelope has been set: an event with no task identity is an observation nobody
can interpret.

When the envelope is present, `identity().task` is its `TaskIdentity`. When it
is absent and `identity_version > 0`, `task` is `not_applicable` — the document
is saying there was no task. When `identity_version == 0`, `task` is `unknown`.

---

## 7. The binary form

```
0   "SGEP"
4   u32  schema_version          1 or 2
8   u32  header JSON length, bytes
12  u32  frame count
16  u32  scalars per frame       38 for schema 1, 78 for schema 2
20  u32  offset of the f64 block (8-byte aligned)
24  header JSON
    zero padding to the block offset
    frames × scalars × f64, little-endian
```

JSON is the other shipped form and carries the same content; both codecs live
in Rust and the browser only marshals (F8).

### Rejection

Both decoders refuse, with a message naming the fault, and **without touching
the episode already loaded**:

| fault | message contains |
|---|---|
| bad magic | `bad magic` |
| unsupported version | `not supported; this build reads 1 and 2` |
| frame width not matching the declared schema | `scalars` |
| header length above `MAX_HEADER_BYTES` (64 KiB) | `limit` |
| frame count above `MAX_EPISODE_FRAMES` | `budget` |
| header that does not fit the file | `does not fit` |
| short frame block | `short` |
| a header whose `schema_version` disagrees with the file's | `but its header says` |
| any non-finite scalar in any frame | `non-finite` |
| `dt` not finite and positive | `timestep` |
| a schema-2 frame with no diagnostics block | `no diagnostics` |

---

## 8. The size budget

| constant | value | derivation |
|---|---|---|
| `BYTES_PER_FRAME` | 624 | `FRAME_LEN` (78) × 8 |
| `MAX_EPISODE_BYTES` | 8 MiB | a browser-memory budget; see below |
| `MAX_EPISODE_FRAMES` | 13 443 | `MAX_EPISODE_BYTES / BYTES_PER_FRAME` |

**Provenance of the 8 MiB.** It is a memory budget, not a physical quantity and
not a threshold chosen from how a recording looked: it is the point past which
holding the frame block, its JSON form and the decoded JavaScript objects at
the same time stops being comfortable in a browser tab. It is stated in *bytes*
and the frame cap is *derived*, so the cap follows the frame width
automatically the next time the schema grows.

What the cap is, in the units a user experiences:

| `log_hz` | simulated duration at the cap |
|---|---|
| 5 Hz | 2688 s ≈ 44.8 min |
| 10 Hz | 1344 s ≈ 22.4 min |
| 20 Hz (default) | 672 s ≈ 11.2 min |
| 50 Hz | 269 s ≈ 4.5 min |

`Recorder::is_full()` reports the cap; `Recorder::due()` is `false` once it is
reached, so a capped recording stops paying for a `Diagnostics` evaluation as
well as for memory. The browser surfaces it — see
`web/src/ui/RecordControls.tsx`'s readout — rather than growing until the tab
dies.

The whole binary file is the frame block plus one header, so a maximum-length
episode is at most `MAX_EPISODE_BYTES + MAX_HEADER_BYTES`.

---

## 9. Migration, and the fixtures that prove it

`crates/sailgym-physics/tests/fixtures/episode-schema1.json` and
`episode-schema1.bin` are a checked-in schema-1 episode — twelve samples of
`close_hauled` at 10 Hz, header keys and frame width exactly as the v1 recorder
wrote them. They are RV60's fixture:
`recording::tests::the_checked_in_schema_1_fixtures_still_decode_and_round_trip`
decodes both, asserts that no diagnostics block was invented, and re-encodes
each in **schema 1** with the 38-scalar frame width preserved.

Section 06 and anything else that extends this schema must do so
deliberately — a new optional block and a version bump, with the old width
still readable — and must not create a second `Episode` type in the physics
crate.
