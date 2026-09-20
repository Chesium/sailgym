# Mobile controls — design suggestion

Two touch tracks on the screen edges: a vertical one for the mainsheet, a
horizontal one for the rudder, and — later, once there is a sailor model to
drive — device tilt for hiking. The question this note answers is not "can a
slider produce a number between −1 and 1", which is a morning's work, but **how
the thing feels like rope and tiller rather than like two sliders**: finite
speed, a hand that can let go, and an arm that cannot pull 3.6 m in one stroke.

The short answer is that most of the realism is already in the Rust core and
the wrong design throws it away. `sheet_haul_rate`, `sheet_ease_rate`,
`sheet_release_rate`, `delta_r_rate_max` and `delta_r_return_rate` already
impose every speed limit asked for above. What the touch layer has to do is
present them, not reproduce them.

Status: **suggestion, not a PRD.** Nothing here is normative and nothing here
may be treated as amending `00-foundations.md`.

---

## 0. Scope, first — four of the five ideas here are explicitly deferred

brief §44 lists as explicitly deferred: **mobile controls**, **hand-force
simulation**, **detailed block-and-tackle mechanics**, **hiking/body
movement**, **multiple crew**. brief §4 separately says of the sailor model:
"Do not model: hiking; body motion; sailor lean; crew movement; active
righting-moment control." brief §38 says "Mobile/touch support is out of scope
unless trivial", and brief §45 — unlike the case argued in
`unified-agent-interface.md` — does **not** re-list mobile as intended
direction. The brief outranks everything on scope.

There is one door: §38's *unless trivial*. Plumbing pointer events into the
existing `Controls` funnel is arguably trivial. A hand model is not, and the
brief names it twice. So this note does not claim a ruling; it separates the
work into tiers so the human can rule on each one independently rather than on
a single all-or-nothing question.

| Tier | What it is | Brief position | Needs |
|---|---|---|---|
| **T0** | Touch plumbing: two position tracks, relative grab, no hand model, no Rust change | §38 "unless trivial" — plausibly in | A ruling that it is trivial. Nothing else. |
| **T1** | The hand model: finite grab stroke, ratchet, cleat, flick-to-dump | §44 defers "detailed block-and-tackle mechanics" | Written sign-off. Web-only; no Rust, no foundations delta. |
| **T2** | Tension-limited haul rate in the core — the sailor's arm has finite force | §44 defers "hand-force simulation"; §12 anticipates it as "future versions" | Written sign-off **and** a normative delta to F4.3 plus a new `parameters.rs` entry with provenance. |
| **T3** | Tilt → hiking | §4 forbids it outright in v1; §44 defers it | A new crew-position DOF in the core (F3 state extension). The largest extension here by a wide margin; belongs in its own section, not this one. |

T0 and T1 together are what makes a phone playable, and neither touches the
physics crate. **T2 is the only item that changes a physical model, and it
improves the desktop build too** — it is not really mobile work and could be
sectioned separately on its own merits. T3 should not be started until the
sailor model exists; nothing in T0–T1 should be shaped around it beyond §7
below.

Everything after this section assumes the relevant tier has been signed off.

---

## 1. What the core already does — do not reimplement any of it

| Quantity | Source | Value |
|---|---|---|
| Sheet command is a **rate** | `dynamics.rs::sheet_rate` | `cmd ∈ [−1,1]` → m/s |
| Haul rate | `SheetParams::sheet_haul_rate` | 1.5 m/s |
| Ease rate | `SheetParams::sheet_ease_rate` | 3.0 m/s |
| Release rate | `SheetParams::sheet_release_rate` | 6.0 m/s |
| Sheet travel | `l_sheet_min` … `l_sheet_max` | 0.90 → 4.50 m = **3.60 m** |
| Travel clamp, inside the derivative | `dynamics.rs::sheet_rate` → `limit_rate` | F4.3 |
| Rudder rate limit | `delta_r_rate_max` | 2.09 rad/s = 120°/s |
| Rudder travel | `delta_r_max` | 0.698 rad = ±40° |
| Tiller self-centring on no command | `delta_r_return_rate` | 1.57 rad/s = 90°/s |
| Live rope tension | `Diagnostics::sheet_tension` | N, `≥ 0` always |
| Live rope length | snapshot index 11, `lSheet` | m |

The durations that fall out of those numbers are the whole feel of the boat:

```text
full ease → full haul      3.60 m / 1.5 m/s  =  2.40 s
full haul → full ease      3.60 m / 3.0 m/s  =  1.20 s
full haul → dumped         3.60 m / 6.0 m/s  =  0.60 s
rudder centre → full lock  40° / 120°/s      =  0.33 s
rudder full lock → centre  40° / 90°/s       =  0.44 s   (hands off)
```

Two consequences.

**"Cannot move it infinitely fast" is already solved, in Rust.** The touch layer
must not add a second rate limit on top. It emits a normalised command and lets
`sheet_rate` clamp. A TS-side rate limiter would (a) duplicate a physical
constant outside `parameters.rs`, (b) silently change behaviour the moment
someone edits `sheet_haul_rate` in the Debug Mode parameter panel, and (c) put a
quantity in m/s into TypeScript, which is what F8 exists to prevent.

**The haul/ease asymmetry is free realism.** Easing is twice as fast as hauling
because that is how a loaded sheet behaves, and any position-style control
inherits that asymmetry without a line of code: the track feels heavy one way
and light the other.

---

## 2. The one idea: the thumb is the hand, the rope chases it

`web/src/sim/sheetInput.ts` maps **displacement → rate**: the drag offset from
where the press began becomes `sheetRateCmd`, and releasing returns it to zero.
That is right for a mouse and wrong for a thumb, for one reason: it never tells
you *where the trim is*. On desktop you read the trim off the boom and the rope
in the SVG. On a phone the boat is small, your thumb is over the rail, and the
reason to want a track on the edge in the first place is to see the trim.

So invert the mapping. The thumb is not a rate lever; **the thumb is your hand
on the rope, and the rope chases it, at the speed the core allows**:

```text
h        = thumb position along the track, 0 … 1
L_target = l_sheet_min + h · (l_sheet_max − l_sheet_min)     "where my hand is"
L        = snapshot.lSheet                                    "where the rope is"
gap      = L_target − L
cmd      = clamp(gap / band, −1, +1)                          → sheetRateCmd
```

`band` is a small proportional band — a few percent of travel — so the command
saturates for any real gap and tapers to zero on arrival instead of chattering
across the target. **Express `band` as a fraction of travel, not in metres**,
and take `l_sheet_min`/`l_sheet_max` from `Sim.parameters_json()` at runtime.
Both are `set_parameter`-editable and both are live in the Debug Mode parameter
panel, so a hardcoded 0.90/4.50 in TypeScript would desync the track the moment
anyone drags that slider — and would be exactly the duplicated-constant drift
`CLAUDE.md` warns about.

### Draw both markers

The track carries **two** indicators on one axis: the **hand** (your thumb) and
the **rope** (`lSheet`). The distance between them *is* the rate limit, made
visible. Haul hard and your thumb runs ahead while the rope drags a beat
behind; ease and it snaps up twice as quickly.

This is the load-bearing UX decision in the whole note. A position control with
a rate limit and no visible actual-value reads as **input lag**, and input lag
on a phone reads as a broken app. The same control with both markers drawn
reads as **rope under load**, which is the thing we were trying to convey.
Same code, opposite impression.

### On lift, the hand goes to the rope

```text
on pointer-up / cancel:   L_target := L
```

Your hand cannot be somewhere the rope isn't. Without this rule a lift-off mid-
haul leaves a stale target that keeps hauling after you have stopped touching
the screen — a ghost-command bug that will otherwise be found by a player, in a
gust, at 30° of heel.

### Why not the obvious alternatives

| Mapping | Gives | Costs |
|---|---|---|
| **Rate track** (today's drag, rotated into a widget) | Trivially consistent with `sheetInput.ts`; nothing new to reason about | No sense of trim at the thumb; you must read the boom to know what you have. Defeats the purpose of a visible track. |
| **Position track, no rate limit visible** | Direct, immediate | Reads as lag; players will call it broken |
| **Position track, hand + rope drawn** (proposed) | Trim legible at a glance; rate limit reads as effort; asymmetry free | One more thing to draw; needs the on-lift rule |

---

## 3. The sheet track

### 3.1 Releasing — cleat by default, flick to dump

Dumping the main is the most important gesture in a dinghy and the one that
prevents most capsizes; `sheet_release_recovery` is a named scenario and brief
§46 step 4 builds the demonstration around it. It has to be available in a
panic, one-handed, without aiming.

It must **not** be lift-off. Phones drop touches constantly — a notification, a
palm, a call — and if lifting dumps the sail then every interruption knocks the
boat flat.

- **Lift → holds.** The rope stays where it is. This is a cleat, and cleats are
  real hardware.
- **Flick toward ease past a velocity threshold → dump.** Sets
  `sheetRelease = true`, snaps the hand marker to the eased end, and reuses the
  existing Space path with no Rust change at all. It is also the real gesture:
  you throw the rope away from you.
- **Double-tap the track → toggle cleat/uncleated.** That is precisely how a cam
  cleat works — a flick to engage, a flick to free — so the gesture teaches the
  hardware.

Uncleated is the honest default for realism (most dinghy sailors hand-hold the
main) and the wrong default for a touchscreen (holding a trim would mean
holding your thumb still for a minute at a time). Ship cleated; offer uncleated
as a realism setting where lifting eases at a slip rate.

### 3.2 One hand is not enough — grab stroke and ratchet

3.60 m of rope is roughly five arm pulls. A real sailor does it hand over hand.

Give the hand a **finite grab stroke**: once engaged, the thumb can move the
hand marker at most `stroke` of rope before it runs out of arm. To gain more you
lift and re-grab. A reasonable starting stroke is **0.70 m ≈ 19% of travel**,
which is ~115 px of a 600 px track and **six strokes** end to end.

Two flavours, and the choice matters more than the number:

- **Ratchet block — recommended default.** You keep everything you have gained;
  the stroke only limits how much you gain per pull. The rhythm is
  pull-regrip-pull-regrip, which is satisfying rather than punishing, and it is
  *honest*: a ratchet block holds load in one direction and pays out freely in
  the other, which is standard dinghy hardware and exactly what this models.
- **Bare-handed — realism toggle.** During the re-grab the load pulls rope back
  out, at a rate scaled by `Diagnostics.sheet_tension`. This is genuinely what
  hauling in a breeze feels like, and genuinely infuriating on glass. Opt-in,
  never default.

Both live entirely in the touch reducer. Neither is physics; both are §44
"detailed block-and-tackle mechanics", hence tier T1.

### 3.3 Load you can feel

The piece of realism that **cannot** be faked in the web layer: a sailor cannot
haul at 1.5 m/s against a heavily loaded sheet. `sheet_rate` is currently
load-independent, so hauling in 25 kn costs exactly what hauling at rest costs.

Fixing that properly is tier T2: a `sheet_haul_force_max` (N — the sailor's
arm) in `SheetParams`, capping the haul rate as tension approaches it, inside
`dynamics::sheet_rate` so the clamp stays in the derivative per F4.3. It is a
change to the actuator model, so it needs a normative delta to F4.3, a
provenance note in `parameters.rs`, and the human's sign-off. It is also not
mobile work: it would improve the mouse and keyboard build identically. **Do
not smuggle it into a mobile section, and do not approximate it in
TypeScript.**

What the touch layer *can* do today, for free, from `sheet_tension`:

- rope thickness and colour on the track;
- **haptic feedback: `navigator.vibrate` intensity/pattern scaled by tension.**

The haptics are the single highest-value mobile-only affordance in this note.
A phone can convey load through the hand, which no desktop build can, and it
costs one `Diagnostics` field that is already computed every frame. Note that
iOS Safari does not implement `navigator.vibrate` — treat it as an enhancement
that is simply absent there, never as something the control depends on.

### 3.4 Direction

**Thumb down = haul, thumb up = ease.** This is already fixed by
`sheetInput.ts` and stated in the help overlay, and the two must change
together. Do not introduce a second convention for touch; the mobile track and
the mouse drag are the same hand.

---

## 4. The rudder track

Make this one **direct position**, not rate — a tiller is where your hand is,
and 0.33 s from centre to full lock is fast enough that a position control
never feels sluggish.

```text
x        = thumb position along the track, −1 … +1
δr_target = expo(x) · delta_r_max
cmd      = clamp(gap / band, −1, +1)         → rudderRateCmd, Rust rate-limits
```

Three points.

**Self-centring is already free.** Lift your thumb, send no command, and
`delta_r_return_rate` walks the blade back to centre at 90°/s — which is what a
balanced boat does when you let go of the tiller. `controls.ts` already says in
so many words that self-centring is *not* implemented in the browser. Keep it
that way: the touch layer's "release" is the absence of a command, nothing more.

**Expo, not linear.** Course-keeping corrections are ±3°; a tack is full lock.
On a 300 px track a linear map gives 7.5 px per degree and no one holds a
course. A cubic blend near centre with full travel at the ends fixes it. This is
input shaping, not physics, and the constant belongs in `InputConfig` next to
the existing `rudderDeadZone` — brief §28's requirement that rate, gain,
direction and dead zones be configurable rather than scattered applies
unchanged to the touch path.

**Direction needs a decision.** A real tiller is reversed: push it to port and
the bow goes to starboard. F2.2 and the `Controls` contract define
`rudder_rate_cmd = +1` as *bow to starboard*, and the existing keyboard map
follows that. Default the track to **bow follows thumb** (consistent with the
contract and with `D`/`ArrowRight`) and add a `tillerInvert` flag for the
reversed, realistic behaviour — `sheetInvert` in `InputConfig` is the precedent
for exactly this kind of escape hatch. What must not happen is the track and the
keyboard disagreeing about which way starboard is; R3 (sign-convention drift) is
named as the highest-probability defect class in this repo.

---

## 5. Touch mechanics that will bite

- **Do not use `<input type="range">`.** It teleports the thumb to the tap
  point. On the sheet that is an instantaneous full haul or full dump from a
  stray tap, and it has poor pointer-capture semantics besides.
- **Grab is relative, never absolute.** Touching the track picks the hand up
  where it already is; the hand then follows the *delta*. This is both the
  realistic behaviour (your hand is on the rope where the rope is) and the fix
  for the teleport class of accident.
- **`setPointerCapture` on down, release on up/cancel**, and handle
  `pointercancel` as a lift — a phone will fire it.
- **`touch-action: none` and `overscroll-behavior: none`** on the tracks, or
  Chrome claims the vertical drag as a scroll and Android claims it as
  pull-to-refresh. `BoatSvg.tsx` already sets `touchAction: 'none'` on the world
  SVG; the tracks need the same.
- **Safe-area insets.** The bottom edge is the iOS home indicator and the
  Android gesture bar. A rudder track flush to the bottom means every tack
  risks an app switch. `env(safe-area-inset-bottom)`, and keep the track above
  it.
- **Hit area ≫ drawn area.** Draw a ~40 px track; accept pointers over ~72 px,
  extending inward over the world view. The visible track is the gauge; the
  invisible region is the grab.
- **The mobile arrangement is a mounting decision, not a media query.**
  `Layout.tsx` deliberately decides *what is mounted* rather than what CSS
  hides, because a hidden-but-mounted debug subtree still re-renders every
  frame and makes "Sail Mode shows exactly the §29 set" a statement about
  styling. A phone layout is a third arrangement in that component, under the
  same doctrine.
- **Two thumbs, two pointers.** Sheet and rudder will be dragged
  simultaneously, constantly. Each track tracks its own `pointerId` and ignores
  every other; a shared "is dragging" flag will cross the two controls the
  first time someone tacks while trimming.

---

## 6. Where the code goes

The existing path is short and worth preserving exactly:

```text
pointer events → reduceSheetInput (pure)  → sim.setSheetRate
held keys      →         ┐
                         controlsFromInput (pure) → sim.set_controls(r, s, release)
```

`useSimulation.setSheetRate` pushes the command immediately rather than waiting
for the next frame, for a reason found in section 02. The touch path must reuse
that, and must produce its `Controls` **through `controlsFromInput`** — never a
second `set_controls` call site. One funnel; the `Controls` struct is the
contract.

Shape of the addition:

- `web/src/sim/trackInput.ts` — the pure reducer(s):
  `(state, pointerEvent, cfg, actual) → [state, command]`, no DOM, no React,
  no metres that did not come from `parameters_json`. The hand model (stroke,
  ratchet, cleat, flick threshold) lives here, which is what makes it
  assertable in vitest rather than by hand on a phone.
- `web/src/ui/SheetTrack.tsx`, `web/src/ui/RudderTrack.tsx` — dumb components:
  draw two markers, forward pointer events, hold no logic.
- `web/src/sim/controls.ts` — extend `InputConfig` with the touch constants
  (`stroke` as a fraction of travel, `band`, flick threshold, expo, deadzone,
  `tillerInvert`). Not a new config object; brief §28 asks for one place.
- `web/src/ui/Layout.tsx` — the phone arrangement.

What it must not touch: `crates/` at all (T0/T1), `parameters.rs`, the snapshot
layout, and `sheetInput.ts`'s stated direction convention.

---

## 7. Tilt and hiking — reserve it, do not build it

Tilt is the right instinct and the wrong next step, because the obstacle is not
the sensor, it is that **there is nothing for it to control**. brief §4 models
the sailor as a fixed mass amidships and forbids hiking, lean and active
righting-moment control by name. Tilt-to-hike therefore requires a crew-position
DOF in `BoatState` — an F3 state extension, new righting-moment coupling in the
stability module, new invariants, new regression goldens. That is a physics
section in its own right and it dwarfs everything above.

When it is built, the browser-side facts that will bite:

- iOS requires `DeviceOrientationEvent.requestPermission()` from an explicit
  user gesture, and HTTPS. There is no way to acquire it silently.
- There is no meaningful absolute reference — everyone holds a phone at a
  different angle. It needs an explicit "set neutral" calibration gesture, and
  re-calibration should be one tap away.
- Raw orientation is noisy and the boat's own motion is not in it. Low-pass it,
  and treat the result as a **command**, never as a measurement of anything.

The only thing worth doing now is making sure a third input axis slots into
`Controls` without reshaping the input path — which the single-funnel rule in
§6 already achieves. No speculative `crewPosition` field, no placeholder.

---

## 8. The gate

Two facts about the current chain.

**Step 8 (`pnpm --dir web test:unit`) now runs**, added by v2 section 01. Every
pure reducer proposed in §6 is covered by it for free, alongside the existing
`sheetInput.test.ts` and `controls.test.ts`. This is why the hand model belongs
in a reducer and not in a component.

**Step 9 has no touch project.** `web/playwright.config.ts` defines exactly
three: `chromium`, `firefox`, `msedge`, all `Desktop *` devices, and its header
comment cites brief §38 — "desktop, mouse + keyboard" — as the reason. A mobile
project (`hasTouch: true`, `page.touchscreen`, a `Pixel`/`iPhone` descriptor) is
therefore **a change to a browser target fixed by the brief**, not a test
detail. It has to be recorded the way v2 section 01 recorded the step-8
insertion: named in the PRD's normative deltas, approved with a date, and
reflected in the `CLAUDE.md` gate table if the step count changes.

Note also that no existing task owns `playwright.config.ts` or
`scripts/check.*`; a mobile section must claim them explicitly under F13.2.

Worker count is already capped at 2 because Firefox renders the particle field
in software at ~80 ms a frame. Adding a fourth project adds a quarter more
work to a run that is already host-starved — budget for it, and expect to have
to argue about whether the touch project runs in CI on every section or only on
the sections that touch input.

---

## 9. Acceptance criteria, in the house style

Commands and numbers, never "looks right" (F13.4). Rendering and feel are where
"looks right" is most tempting, so the criteria below are all statements about
a pure function or about a measured quantity.

| Test | Asserts |
|---|---|
| `test:unit trackInput/relative-grab` | a `down` at any point on the track leaves the command at exactly `0`; only subsequent `move` deltas produce a command |
| `test:unit trackInput/lift-snaps` | after `up`, `L_target == L` exactly, and the command is exactly `0` |
| `test:unit trackInput/stroke` | moving the thumb the full length of the track in one drag advances the hand by at most `stroke`; N re-grabs advance at most `N · stroke` |
| `test:unit trackInput/ratchet` | in ratchet mode a re-grab loses exactly `0`; in bare-handed mode the loss is monotone in `sheet_tension` |
| `test:unit trackInput/flick` | a drag above the velocity threshold sets `sheetRelease`; one below it never does; both are exercised at the boundary ±1 px/frame |
| `test:unit trackInput/no-hardcoded-travel` | the reducer given `l_sheet_min/max` from a modified `parameters_json` maps the same thumb position to a different `L_target` |
| `test:unit controls/one-funnel` | touch commands and key commands compose through `controlsFromInput` into a single `Controls`; both tracks driven at once yield both fields |
| `test:unit trackInput/expo` | the rudder map is monotone, odd-symmetric (`f(−x) == −f(x)`), and `f(±1) == ±delta_r_max` |
| `test:e2e mobile/haul-time` | scripted full-track haul moves `lSheet` from max to min in `2.40 s ± tolerance` — i.e. the touch layer added no second rate limit |
| `test:e2e mobile/dump` | flick-to-dump reaches `l_sheet_max` in `0.60 s ± tolerance`, matching the Space path frame for frame |
| `test:e2e mobile/two-finger` | simultaneous sheet and rudder drags produce independent, correct commands |
| `test:e2e mobile/no-scroll` | a vertical drag on the sheet track scrolls the page by exactly `0` px |
| `test:e2e mobile/no-errors` | the §38-style zero-console-error assertion, on the touch project |

The odd-symmetry assertion on the rudder map deserves the same emphasis it gets
in `unified-agent-interface.md` §10: R3 (sign-convention drift) is the
highest-probability defect class here, a touch track is a second place for the
steering sign to be written down, and a symmetry test catches in one line what
playtesting reports as "it turns funny to port".

---

## 10. Risks worth naming (RV candidates; numbers assigned by the PRD)

- **Rate limit reads as broken.** A position track whose actual value is not
  drawn will be reported as input lag. Mitigation: the two-marker rule in §2 is
  not decoration, it is the mitigation, and it should be an acceptance
  criterion rather than a visual nicety.
- **Accidental dump.** Any release gesture that a dropped touch can trigger will
  capsize players at random and be blamed on the physics. Mitigation: cleat by
  default, threshold the flick, and assert the threshold at the boundary.
- **A second sign convention.** See §4 and §9.
- **A second copy of `l_sheet_min/max`.** See §2. A live parameter edit is the
  test that catches it.
- **Gate cost.** A fourth Playwright project against a 2-worker cap. See §8.
- **Scope creep into the core.** The tension-limited haul (T2) is the one
  genuinely physical idea in this note and will be tempting to approximate in
  TypeScript to make a phone demo feel better. That approximation would be both
  an F8 violation and, in effect, tuning a coefficient to make a scenario look
  better (brief §43).

---

## 11. Suggested order

1. **Get the ruling on T0 and T1** (§0). They are separable, and T1 is where
   the interesting design is, so a ruling on T0 alone would produce two
   sliders and none of the feel.
2. **`trackInput.ts` with the position mapping, relative grab and the on-lift
   rule** — no hand model yet. Wire the sheet track only, leave the mouse drag
   untouched, and get the unit tests green. This is the load-bearing step: if
   the two-marker track reads as rope rather than as lag on a real phone, the
   design is right; if it does not, you have found that out before building a
   ratchet.
3. **The rudder track**, with expo and the invert flag.
4. **The hand model** — stroke, ratchet, cleat, flick — one flag at a time,
   each with its reducer test.
5. **Haptics from `sheet_tension`**, as a pure enhancement.
6. **The phone arrangement in `Layout.tsx`**, plus the Playwright touch
   project and the gate paperwork (§8).
7. **T2**, separately, on its own merits, as a physics section — not as part of
   this one.
8. **T3**, after a sailor model exists. Not before.
