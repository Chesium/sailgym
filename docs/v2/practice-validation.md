# Practice-task validation

**The evidence behind section 11's task thresholds.** Written by v2 section 11
task 11.1. v2 F18.4 is the normative delta it implements;
`crates/sailgym-task/src/lib.rs` is the implementation and
`crates/sailgym-task/tests/practice.rs` is the measurement, which runs in gate
step 3.

Conventions, frames and units are `docs/v1/00-foundations.md`'s and are not
restated here — F1 for units, F2 for the sign conventions, F3 for the state.
The physics is section 08's corrected baseline
(`docs/v2/physics-validation.md`, `docs/v2/progress/08-handoff.md`); **nothing
in this document changed a coefficient**, and §6 states that as a checkable
property rather than as a promise.

---

## 1. What a threshold is here, and what it is not

A *task threshold* is a speed a challenge asks for, an angle that counts as
settled, a duration that counts as held. It is **task configuration**: it is
versioned with the task ([`TASK_VERSION`], currently 1), it is visible —
`TaskSpec::thresholds()` puts every one in the episode's `TaskIdentity`, and
the page shows them — and it changes nothing about the boat. A *physical
coefficient* lives in `parameters.rs`, is governed by v1 brief §43, and is not
touched by this section.

The distinction is the whole of RV61's mitigation, and it is mechanical rather
than a matter of intent:

| | task threshold | physical coefficient |
|---|---|---|
| lives in | `sailgym-task` | `sailgym-physics/src/parameters.rs` |
| may change to make a challenge fair | **yes**, with a version bump | **no** (brief §43) |
| changes what the boat does | never | always |
| travels with an episode as | `TaskIdentity` (F18.3) | the resolved catalogue |
| audited by | `TaskSpec::validate`, and the runs below | `tests/provenance.rs`, 83 F7 rows |

Two attempts scored under different thresholds are **not** the same
experiment: the version and the whole threshold map are inside
`ExperimentIdentity`, so `compare` refuses a comparison across a change
(`docs/v2/recording-format.md` §5).

## 1.1 How the values were chosen

For each challenge: run the shipped scenario the challenge is set on, at the
section 08 baseline, under a control script that a person could reproduce with
the keyboard and the pads; sweep the script; read the trajectory; pick
thresholds that separate the technique the challenge teaches from the two
things a beginner actually does instead. Every number below came out of such a
run, and each is reproduced by a test in `tests/practice.rs`, so a physics
change that moves it turns the gate red rather than quietly making a challenge
impossible.

The sweeps were run natively with `dt = 0.005 s` (F7) and the scenarios' own
seeds; all three scenarios use `mode: uniform`, so the wind field is constant
in time and the runs are reproducible from the seed alone (F9).

Commands:

```
cargo test -p sailgym-task
cargo test -p sailgym-task --test practice -- --nocapture      # the numbers below
cargo test -p sailgym-task --test practice baseline -- --nocapture
```

---

## 2. Get moving — `free_sail`

**Teaching goal.** Trim for drive: the boat starts at rest with the mainsheet
fully eased, and it will not go anywhere until the sail is sheeted to
something like the right angle for a beam reach.

**Scenario.** `scenarios/free_sail.json`, seed 0. A uniform 5 m/s northerly,
the boat at rest heading east, `l_sheet = 4.50 m` (the stop). The true wind
starts on the port beam at `TWA = −90°`.

### 2.1 The sweep

Full haul (`sheet_rate_cmd = −1`, i.e. `sheet_haul_rate = 1.5 m/s`) for
`trim_s` seconds, then no further input. 25 s of simulated time:

| `trim_s` | `l_sheet` reached | `u` @ 5 s | @ 10 s | @ 15 s | @ 20 s | max `u` | max \|φ\| |
|---|---|---|---|---|---|---|---|
| 0.00 | 4.500 | 0.148 | 0.185 | 0.331 | 0.427 | **0.493** | 17.5° |
| 0.25 | 4.125 | 0.256 | 0.385 | 0.477 | 0.542 | 0.589 | 17.5° |
| 0.50 | 3.750 | 0.597 | 0.651 | 0.691 | 0.721 | 0.743 | 17.5° |
| 0.75 | 3.375 | 1.072 | 1.064 | 1.050 | 1.038 | 1.075 | 17.5° |
| **1.00** | **3.000** | 1.664 | 1.661 | 1.615 | 1.574 | **1.687** | 17.5° |
| **1.25** | **2.618** | 1.576 | 2.395 | 2.369 | 2.333 | **2.396** | 17.5° |
| 1.50 | 2.243 | 1.651 | 2.040 | 2.117 | 2.152 | 2.179 | 18.4° |
| 1.75 | 1.868 | 1.593 | 2.028 | 2.114 | 2.138 | 2.148 | 23.4° |
| 2.00 | 1.493 | 1.410 | 1.795 | 1.868 | 1.872 | 1.874 | 28.2° |
| 2.25 | 1.118 | 1.074 | 1.280 | 1.319 | 1.309 | 1.320 | 31.5° |
| ≥ 2.50 | 1.040 (two-blocked) | 0.93 | 1.06 | 1.08 | 1.06 | **1.076** | 31.9° |

Two facts do the work. Untrimmed, the boat reaches **0.493 m/s** in 25 s and
0.608 m/s in 45 s. Two-blocked, it reaches **1.076 m/s** and then bears away
onto a run with the sail sheeted flat, heeled 32°. Between them there is a
broad band — roughly `l_sheet` 1.1 m to 3.1 m — where the boat sails at
1.3 to 2.4 m/s.

### 2.2 Thresholds, and what each separates

| threshold | value | why this value |
|---|---|---|
| `target_speed_mps` | **1.2 m/s** | above the untrimmed 0.61 and the two-blocked 1.08, below the 1.32 the *worst* correctly-trimmed run reaches. A single number that both wrong answers fail and every right answer passes. |
| `release_speed_mps` | **1.1 m/s** | hysteresis. Without a gap a boat sitting on the target restarts the hold every other step and never completes it; 0.1 m/s is about 8 % of the target and well outside the trimmed band's own variation. |
| `hold_s` | **3.0 s** | the untrimmed boat's speed creeps upward for the whole run, so *touching* a speed proves nothing; three seconds is long enough to be sustaining and short enough that the loop stays about a minute. |
| `backward_speed_mps` | **0.25 m/s** | sternway a player can see on the gauge, well above the ±0.05 m/s a boat wallows at. |
| `backward_hold_s` | **2.0 s** | two seconds of it, so a momentary transient during a manoeuvre is not a failure. |
| `time_limit_s` | **45 s** | the successful script finishes in 5.6 s; 45 s leaves room to get it wrong twice and still recover. |

### 2.3 The scripted runs the gate keeps

| test | script | outcome | measured |
|---|---|---|---|
| `get_moving_succeeds_when_the_sheet_is_trimmed` | haul 1.0 s | **Succeeded** at t = 5.635 s (step 1127) | `speed_reached` t = 2.630 s at 1.2002 m/s; top speed **1.6806 m/s** |
| `get_moving_times_out_when_nothing_is_trimmed` | no input | **TimedOut** at 45.00 s | top speed **0.6075 m/s**, no `speed_reached` |
| `get_moving_times_out_when_the_sheet_is_two_blocked` | haul 4.0 s | **TimedOut** at 45.00 s | top speed **1.0436 m/s** |

**Backward drift is not reachable on `free_sail`.** The scenario starts with
the sheet already at its stop, so there is no over-ease to make, and no swept
script produced sternway. The rule exists because it is the honest thing to
say to a player who *is* going backwards — on `close_hauled`, over-easing
reaches `u = −0.44 m/s` — and it is measured by a table-driven trace
(`get moving: backward drift`) rather than by a scripted run. §5 lists which
failures have which kind of evidence.

---

## 3. Complete a tack — `tack`

**Teaching goal.** Cross head to wind and come out sailing on the other side.
The technique this boat needs is **helm and sheet together**: keep the sail
sheeted in through the turn so the boat is still being driven as the bow
passes through the wind.

**Scenario.** `scenarios/tack.json`, seed 20250903. A uniform 4 m/s northerly,
close-hauled on **port** tack at `TWA = −45°` with 1.8 m/s of way on, boom 35°
to starboard, `l_sheet = 2.00 m`.

### 3.1 What the sweep found, including the thing that nearly sank the challenge

A first sweep drove the helm alone — full rudder to port for 2 to 12 s, with
and without a counter-helm, with and without easing afterwards. **None of it
completes a tack.** The boat crosses head to wind and then stops: 40° of
rudder at 1.8 m/s roughly triples the drag, the boat is down to 0.5 m/s before
the bow reaches the wind, and it then hangs at `TWA` between +5° and +25°
making **sternway** at −0.2 to −0.5 m/s for the next forty seconds. A
closed-loop pilot holding a proportional rudder angle did no better: the
fastest genuine tack any of 1 344 swept scripts produced took **47 s**, nearly
all of it going backwards.

A random search over eight five-second segments of `(rudder, sheet)` command
then found an 11.4 s tack, and the whole difference is the sheet: the winning
script hauls to the stop while it turns. Hauled in, the sail is a stalled
plate at a large angle of attack and goes on producing force through head to
wind; the boat's surge speed bottoms out at **0.355 m/s** and never reverses.

That is a real property of this boat and this model, not a tuning result, and
it is the lesson the challenge exists to teach. A simple two-key script
reproduces it.

### 3.2 The simple scripts

Full helm to port and full haul, both for `n` seconds, then nothing:

| script | approach (\|TWA\| ≤ 30°) | crossing (TWA ≥ +10°) | settled (TWA ≥ +35°, `u` ≥ 0.4) | min `u` |
|---|---|---|---|---|
| helm 5 s + haul 5 s | 1.05 s | 8.47 s | **18.33 s** | +0.306 |
| helm 8 s + haul 8 s | 1.05 s | 6.00 s | **14.09 s** | +0.308 |
| helm 12 s + haul 12 s | 1.05 s | 6.00 s | 27.31 s | +0.216 |
| helm 5 s, **no haul** | 1.02 s | 9.03 s | never | **−0.312** |
| helm 3 s + haul 3 s | 1.05 s | never (falls back at 17.8 s, bears away past −120° at 39.4 s) | never | +0.332 |
| nothing | never | never | never | +0.861 |

### 3.3 Thresholds

| threshold | value | why this value |
|---|---|---|
| `approach_twa_rad` | **30°** (0.5236 rad) | reached at ~1.05 s by every script that steers at all, and never by one that does not. It is the "you are luffing up" phase marker, not a pass mark. |
| `crossing_twa_rad` | **10°** (0.1745 rad) | the hysteresis band. A boat that merely touches head to wind has not crossed; it must be 10° onto the other side. The measured crossings land at 6.0–8.5 s. |
| `settled_twa_rad` | **35°** (0.6109 rad) | past the 30° approach band on the new side by a clear margin, and comfortably inside close-hauled for this boat, whose settled `TWA` runs on to 40–50°. |
| `settle_hold_s` | **1.5 s** | the angle is crossed once; holding it for a second and a half is what distinguishes settling from swinging through. |
| `recover_speed_mps` | **0.4 m/s** | the forward-speed recovery the PRD asks for. The helm-only run sits at −0.3 m/s for the whole attempt and the helm-and-haul run is above 0.4 m/s from the moment it settles, so 0.4 separates them by a wide margin. It is deliberately *low*: this boat's settled speed after a tack is about 0.45 m/s and asking for more would be asking for a boat that does not exist. |
| `wrong_way_twa_rad` | **120°** (2.0944 rad) | well past a beam reach. A boat 120° off the wind is bearing away, not luffing; the 3 s script reaches −122° and is correctly rejected. |
| `max_reversals` | **2** | the machine may fall back a phase twice — a boat wallowing across head to wind genuinely does — and the third is jitter. |
| `time_limit_s` | **45 s** | the successful script finishes at 15.6 s; 45 s allows a slow, over-steered tack (27 s) and still ends an attempt that is going nowhere. |

### 3.4 The scripted runs the gate keeps

| test | script | outcome | measured events |
|---|---|---|---|
| `complete_tack_succeeds_with_helm_and_sheet_together` | helm −1 and sheet −1 for 8 s | **Succeeded** at t = 15.590 s (step 3118) | `approach` 1.050 s (−29.9°) · `crossing` 6.005 s (+10.0°) · `settled` 15.590 s (+40.0°) |
| `complete_tack_times_out_when_the_sheet_is_left_alone` | helm −1 for 8 s, sheet untouched | **TimedOut** at 45.00 s | `approach` 1.015 s · `crossing` 6.665 s · no `settled` |
| `complete_tack_fails_wrong_way_when_the_turn_is_abandoned` | helm −1 and sheet −1 for 3 s | **Failed(WrongWay)** at t = 39.425 s | `approach` 1.050 s · `reversal` 17.765 s · `bore_away` 39.425 s at −120.0° |

The second run is the useful failure: it crossed, and the result says so, and
then says the boat never got going again. That is an observed event, not a
diagnosis.

---

## 4. Recover from excessive heel — `sheet_release_recovery`

**Teaching goal.** Ease or release the sheet **early**. The scenario is
already going over when it starts; the question is how soon the player reacts
and how far the boat goes before it comes back.

**Scenario.** `scenarios/sheet_release_recovery.json`, seed 20250901. A uniform
9 m/s northerly on the beam with the sheet at the geometric minimum
(`l_sheet = 1.0404326023342405 m`, v2 F18.1b). It is byte-identical to
`beam_reach_capsize` in every physical field; the two differ only in what the
player is told to do, which is exactly what the section PRD asks for.

**This challenge does not right a capsized boat** (RV64). Inversion is a stable
equilibrium in the corrected model (section 08 handoff §8 item 7), righting is
deferred, and a declared capsize ends the attempt. Nothing in the lesson text
or the evaluator implies otherwise.

### 4.1 The sweep

Heel with the sheet held, and with `sheet_release` applied for 3 s from `t₀`:

| run | peak \|φ\| | \|φ\| ≥ 25° | ≥ 60° | ≥ 75° | capsize declared | recovered (≤ 20° for 2 s) |
|---|---|---|---|---|---|---|
| sheet held | 169.1° | 0.33 s | 1.63 s | 7.87 s | **9.45 s** | never |
| release at 0 s | 29.7° | 0.33 s | — | — | — | 2.78 s |
| release at 0.5 s | 50.3° | 0.33 s | — | — | — | 3.40 s |
| release at 1 s | 57.4° | 0.33 s | — | — | — | 3.95 s |
| release at 2 s | 62.5° | 0.33 s | 1.63 s | — | — | 5.04 s |
| release at 3 s | 64.6° | 0.33 s | 1.63 s | — | — | 6.09 s |
| release at 4 s | 66.1° | 0.33 s | 1.63 s | — | — | 7.13 s |
| release at 5 s | 68.0° | 0.33 s | 1.63 s | — | — | 8.20 s |
| release at 6 s | 70.3° | 0.33 s | 1.63 s | — | — | 9.31 s |
| release at 6.5 s | 71.9° | 0.33 s | 1.63 s | — | — | 9.89 s |
| **release at 7 s** | **74.2°** | 0.33 s | 1.63 s | — | — | **10.53 s** — the last one that works |
| release at 7.5 s | 175.6° | 0.33 s | 1.63 s | 7.83 s | **9.35 s** | never |
| release at 8 s | 179.0° | 0.33 s | 1.63 s | 7.87 s | 9.43 s | never |

And with a partial ease instead of the release button:

| run | peak \|φ\| | capsize | recovered |
|---|---|---|---|
| `sheet_rate_cmd = +1.0` at 2 s for 3 s | 62.5° | — | 5.26 s |
| `+0.3` at 2 s for 3 s | 62.9° | — | 7.00 s |
| `+0.1` at 2 s for 6 s | 64.3° | — | never in 30 s |
| `+1.0` at 6 s for 3 s | 70.3° | — | 9.48 s |
| `+0.1` at 6 s for 6 s | 158.4° | **8.87 s** | never |

### 4.2 Thresholds

| threshold | value | why this value |
|---|---|---|
| `qualify_heel_rad` | **25°** (0.4363 rad) | the heel passes 25° at 0.33 s in **every** run, including one that releases on the first step (peak 29.7°). Setting it at 30° would have made the fastest correct answer fail to qualify, which measurement caught. |
| `recover_heel_rad` | **20°** (0.3491 rad) | back on its feet. The recovered boat settles at 2–6° and the failing runs oscillate through 40°+, so 20° sits in a gap rather than on a shoulder. |
| `recover_hold_s` | **2.0 s** | the partial-ease runs cross 20° repeatedly and never stay; two seconds is what tells a recovery from a swing through. |
| `late_release_heel_rad` | **75°** (1.3090 rad) | the last release that still recovers is at 7.0 s, when \|φ\| is 71.5°; the sheet-held run passes 75° at 7.87 s and is declared capsized at 9.45 s. 75° is therefore past every recoverable case and still 1.6 s ahead of the capsize, so the result can say *the sheet was still in at 75° of heel* rather than only *you capsized*. |
| `ease_command_min` | **0.05** | a normalised command, so anything a deliberate drag produces counts and nothing else does. `sheet_release` counts whatever this is. |
| `heel_event_step_rad` | **5°** (0.0873 rad) | one `heel_max` event per 5° of new peak. The list stays ordered by step and bounded — the worst measured run emits 14 — and the **last** one is the peak, which is what **Inspect** jumps to. |
| `time_limit_s` | **30 s** | the slowest recovery measured is 11.35 s. |

### 4.3 The scripted runs the gate keeps

| test | script | outcome | measured |
|---|---|---|---|
| `recover_from_heel_succeeds_when_the_sheet_is_released_in_time` | release 4–7 s | **Succeeded** at t = 7.135 s | `heel_qualified` 0.310 s · `release` 4.010 s at 1.1452 rad · `heel_recovered` 7.135 s · peak **1.1536 rad = 66.1°** |
| `recover_from_heel_fails_late_release_when_the_sheet_is_held` | nothing | **Failed(LateRelease)** at t = 7.870 s | peak at the failure 1.3094 rad = 75.0°; no `release` event |
| `recover_from_heel_fails_on_capsize_when_the_release_is_too_late` | release from 7.5 s | **Failed(Capsized)** at t = 9.350 s | `release` 7.510 s at 1.2790 rad; peak 1.8259 rad = 104.6° |
| `releasing_earlier_lowers_the_peak_heel` | release at 0/2/4/6 s | all **Succeeded** | peaks **29.70° < 62.49° < 66.10° < 70.30°**, strictly monotone |

The last one is why the metric is the peak heel and not a pass mark: the
outcome rewards reacting at all, and the number rewards reacting sooner. Two
attempts at the same challenge are then worth comparing.

---

## 5. Coverage: every outcome, and what kind of evidence it has

The section's acceptance asks for a demonstration of each success and each
isolated failure. Table-driven traces isolate one rule at a time with a
hand-written state history; scripted runs prove the rule is reachable on the
shipped boat. Where a row has both, both run in the gate.

| challenge | outcome | trace | scripted run |
|---|---|---|---|
| get moving | Succeeded | ✔ | ✔ haul 1.0 s |
| get moving | TimedOut — insufficient hold | ✔ | — (the trace is the isolated case) |
| get moving | TimedOut — time expiry | ✔ | ✔ no input, and ✔ two-blocked |
| get moving | Failed(BackwardDrift) | ✔ | — not reachable on `free_sail`; see §2.3 |
| get moving | Failed(Capsized) | ✔ | — (`free_sail` at 5 m/s does not capsize) |
| complete a tack | Succeeded | ✔ | ✔ helm + haul 8 s |
| complete a tack | Failed(WrongWay) | ✔ | ✔ helm + haul 3 s |
| complete a tack | Failed(RepeatedJitter) | ✔ | — |
| complete a tack | TimedOut | ✔ ×2 (never approached; crossed without speed) | ✔ helm only |
| recover from heel | Succeeded | ✔ | ✔ release at 0/2/4/6 s |
| recover from heel | Failed(LateRelease) | ✔ | ✔ sheet held |
| recover from heel | Failed(Capsized) | ✔ | ✔ release at 7.5 s |
| recover from heel | TimedOut — insufficient hold | ✔ | — |

Every trace additionally asserts that events are ordered by step and that each
event's `t` is that step's own time to 1e-9 — never an interpolated instant
(v2 F18.4, section 10 handoff §9 item 3).

## 5.1 Two properties the acceptance names directly

**Batching cannot change a score (RV62).** `identical_control_sequences_agree_under_six_batch_sizes`
runs the same control script on each of the three challenges with the
evaluator driven **1, 2, 3, 5, 10 and 37** steps per call, and asserts that the
outcome, the elapsed step count and every event's `(id, step, value)` are
identical. 37 is coprime with the others, so no batch boundary lines up with a
threshold crossing in more than one arm.

**The evaluator is an observer.** `task_evaluation_does_not_perturb_the_physics`
runs each challenge's scenario twice under one control script — once with a
`TaskRun` attached and once without — and compares all **13 state scalars bit
for bit** over 4 000 steps. Measured: identical, all three challenges.

**The dependency points one way.** `physics_does_not_depend_on_the_task_crate`
runs `cargo tree -p sailgym-physics --edges all` and asserts that
`sailgym-task`, `sailgym-wasm` and `wasm-bindgen` appear nowhere in it
(v2 F14.1, F8.1).

---

## 6. Nothing physical changed

| | |
|---|---|
| `crates/sailgym-physics/src/parameters.rs` | untouched; `tests/provenance.rs` compares the same 83 F7 rows |
| the two F18.1c overrides | still the only ones |
| `scenarios/*.json` | untouched |
| equations | untouched; no file under `physics/src` carries a task rule |

`git diff --stat crates/sailgym-physics/src/parameters.rs scenarios/` is empty
for this section. The thresholds above were chosen **from** the baseline, never
by moving it: at no point was a coefficient adjusted because a challenge was
too hard. Where the boat turned out not to do what a challenge assumed — §3.1,
the tack — the challenge's script and thresholds changed and the boat did not.
