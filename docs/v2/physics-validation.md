# v2 — Physics validation evidence (section 08)

**What this document is.** The measured behaviour of the shipped reduced model
at the revision named below, the three defects
[`discussions/physics-tightening.md`](discussions/physics-tightening.md)
records, the exact corrections proposed for them, and the evidence that the
corrections satisfy the contract in
[`00-foundations.md` F18.1](00-foundations.md).

**What it is not.** It is **not** a validation against a real ILCA. Every
stability and rigging number below is ASSUMED or TUNABLE in the v1 F7 sense
(`docs/v1/00-foundations.md` F7, brief §48). Agreement here is internal
consistency — the model doing what the model says it does — and nothing more.
No towing-tank, VPP, CFD or IMU comparison has been performed and none is
claimed. No approval signature or approval date is recorded here; the human
decision record lives in `docs/v2/brief.md` §5 and in the section handoff.

---

## 0. Provenance of the measurements

| Item | Value |
|---|---|
| Repository revision (baseline) | `83fa53d141e39f5d21bc42d984380e8ef1e20d3f` |
| `git rev-parse HEAD:crates/sailgym-physics/src` (baseline) | `bc83d0dc6bc008de5e7c86452f67b05cbdf5742e` |
| Working tree at measurement | clean (`git status --porcelain` empty) |
| Toolchain | `rustc 1.98.1 (48a229cea 2026-09-01)` |
| Host | `x86_64-unknown-linux-gnu`, release profile |
| Parameters | `BoatParameters::ilca7()`, no overrides (all six shipped scenarios carry `"parameter_overrides": {}`) |
| Integrator / timestep | `Rk2Midpoint`, `dt = 0.005 s` (F7) |
| Units | `GZ`, `ℓ`, `e`, `L` in metres; `GZ'` in m/rad; `φ`, `β` in radians (degrees shown for reading only); `T` in newtons; `M_β` in N·m |

**How the baseline numbers were produced.** A throwaway binary linked against
`sailgym-physics` at the revision above, calling the crate's own public API —
`GzCurve::fit`, `GzCurve::gz`, `GzCurve::dgz`, `rigging::mainsheet::{rope_path_length,
sheet_output}`, `scenario::load_shipped`, `Simulation::initial_state`. No
equation is re-implemented in the harness; roots and stationary points are
located by a sign-change sweep of `2 × 10⁶` points over `(0, π]` followed by 200
bisection steps, which brackets each one to below `1e-15` rad. The rope-path
extrema come from a `2 × 10⁷`-point sweep of `rope_path_length` over
`(−π, π]`. Anyone at `83fa53d` reproduces them with the same three calls.

**How the corrected numbers are produced.** From the repository, after
section 08:

```
cargo test -p sailgym-physics stability::hydrostatics -- --nocapture
cargo test -p sailgym-physics rigging::mainsheet     -- --nocapture
cargo test -p sailgym-physics --test invariants      -- --nocapture
cargo test -p sailgym-physics --test convergence     -- --nocapture
cargo test -p sailgym-physics --test regression      -- --nocapture
```

---

## 1. Finding 1 — the `GZ` curve

### 1.1 What F6.7 asks for, and what the three-harmonic fit delivers

F6.7 solves `GZ(φ) = c₁ sin φ + c₂ sin 2φ + c₃ sin 3φ` from three constraints:
the slope at the origin, the **value** at `φ_p`, and the zero at `φ_v`. Nothing
in that system says `φ_p` is a stationary point, and nothing bounds the curve
beyond `φ_v`. Both omissions are visible in the shipped parameter set.

Baseline, `GM = 1.00 m`, `φ_p = 0.785 rad`, `GZ_max = 0.30 m`, `φ_v = 1.396 rad`:

| Quantity | Measured | Unit |
|---|---|---|
| `GZ(0)` | `0.000000000000000e0` | m |
| `GZ'(0) − GM` | `−2.22e-16` | m/rad |
| `GZ(φ_p) − GZ_max` | `−6.66e-17` | m |
| **`GZ'(φ_p)`** | **`−4.751793596285286e-1`** | m/rad |
| `GZ(φ_v)` | `1.11e-16` | m |

The three residuals the fit *does* constrain are at the rounding floor. The
fourth line is the defect: the slope at the named peak is **−0.475 m/rad**, so
`φ_p` is not a stationary point at all.

### 1.2 Stationary points and roots on `(0, π]`, baseline

| `φ` (rad) | `φ` (deg) | `GZ` (m) | `GZ'` residual | What it is |
|---|---|---|---|---|
| 0.566719819 | 32.4707 | `+0.355557613` | `1.11e-16` | the **actual** maximum — 12.51° below `φ_p`, and 18.5 % above `GZ_max` |
| 1.396000000 | 79.9849 | `+1.67e-16` | — | root: the intended vanishing angle |
| 1.410003505 | 80.7872 | `−0.000266218` | `−3.89e-16` | a minimum, `−0.27 mm` deep |
| 1.423943682 | 81.5857 | `−1.11e-16` | — | **second root** |
| 2.482060107 | 142.2116 | `+0.784652881` | `−1.39e-15` | a maximum **2.6× `GZ_max`** |

The negative window is `[1.396, 1.4239]` rad — **1.60° wide**, with a worst
value of `−0.27 mm`. Beyond 81.6° of heel the shipped boat has **positive**
righting again, rising to `+0.785 m` at 142.2°: past the angle of vanishing
stability the model pushes the boat back upright instead of over.
`Δ·g·GZ` there is `+1062 N·m`, 2.6× the whole intended righting budget.

`GZ(π) = 2.34e-16`, `GZ'(π) = −1.908 m/rad`: the inverted equilibrium is
**unstable** in the shipped model, so a turtled boat rights itself.

### 1.3 The curve, baseline

| `φ` (deg) | `GZ` (m) | `GZ'` (m/rad) |
|---|---|---|
| 0 | 0.000000 | 1.000000 |
| 10 | 0.167421 | 0.878968 |
| 20 | 0.294654 | 0.550929 |
| 30 | 0.353167 | 0.111211 |
| 40 | 0.334475 | −0.311399 |
| 50 | 0.252860 | −0.591107 |
| 60 | 0.141630 | −0.641167 |
| 70 | 0.043837 | −0.439580 |
| 80 | −0.000010 | −0.037221 |
| 90 | **+0.036084** | +0.454093 |
| 100 | +0.155299 | +0.890637 |
| 110 | +0.335723 | +1.135291 |
| 120 | +0.534886 | +1.095260 |
| 130 | +0.700055 | +0.748812 |
| 140 | **+0.781669** | +0.153694 |
| 150 | +0.746424 | −0.565304 |
| 160 | +0.586539 | −1.246640 |
| 170 | +0.322730 | −1.732384 |
| 180 | 0.000000 | −1.908186 |

### 1.4 The proposed curve

**Representation.** The smallest odd harmonic series that can carry four
independent linear constraints is the four-term one:

```
GZ(φ)  = Σ_{n=1..4} c_n · sin(nφ)
GZ'(φ) = Σ_{n=1..4} n · c_n · cos(nφ)
∫₀^φ GZ = Σ_{n=1..4} c_n · (1 − cos(nφ)) / n
```

Odd, `2π`-periodic, `C^∞`, and — because `sin(nφ)` is built from one `sin_cos`
by the Chebyshev recurrences `sin nφ = 2 cos φ · sin(n−1)φ − sin(n−2)φ` and
`cos nφ = 2 cos φ · cos(n−1)φ − cos(n−2)φ` — every term carries exactly one
factor of `sin φ`, so `gz(−φ)` is the **bit-exact** negation of `gz(φ)`. That
property is what the port/starboard mirror invariants compare with `to_bits`,
and it is preserved verbatim from the three-harmonic form.

The derivative and the integral are the analytic derivative and integral of the
**same** coefficients. There is one curve, not a force approximation beside an
energy approximation.

**Constraints.** Four, solved once at parameter-build time:

```
Σ n·c_n                      = GM        slope at the origin
Σ c_n sin(n φ_p)             = GZ_max    the peak's value
Σ n·c_n cos(n φ_p)           = 0         the peak is a stationary point   ← new
Σ c_n sin(n φ_v)             = 0         the vanishing angle
```

`GZ(0) = 0` and oddness are structural and cost no constraint.

**Supported heel domain.** `φ ∈ [−π, π]`, extended to every real `φ` by the
curve's own `2π` periodicity — `state.phi` is deliberately unwrapped (F3), and
`sin`/`cos` supply the periodicity without a wrap. On that domain a valid
configuration has **exactly three equilibria per half-turn**:

| `φ` | `GZ` | Nature |
|---|---|---|
| `0` | `0`, `GZ' = GM > 0` | stable — upright |
| `±φ_v` | `0`, `GZ' < 0` | **unstable** — the angle of vanishing stability |
| `±π` | `0`, `GZ' ≥ 0` | stable — inverted (turtled) |

**Acceptance rules.** `GzCurve::fit` rejects, with the offending field named:

1. `GM`, `GZ_max` not finite and positive, or `0 < φ_p < φ_v < π` violated;
2. a singular system;
3. `GZ ≤ 0` anywhere on `(0, φ_v)`;
4. `GZ` not unimodal on `(0, φ_v)`, or its maximum not at `φ_p`;
5. `GZ ≥ 0` anywhere on `(φ_v, π)` — **no positive stability between the
   vanishing angle and inversion**;
6. `|GZ(φ)| > beam/2` anywhere. A righting arm is the *horizontal* separation
   of the centre of gravity and the centre of buoyancy; both are points inside
   a hull `beam = 1.37 m` wide, so half the beam is a generous geometric
   envelope and a curve outside it is describing a boat that does not exist.

Rule 4 replaces the three-harmonic era's unimodality compromise. The v1 fit
could not pin the peak's location, so `docs/v1/progress/07-handoff.md` recorded
a contradiction between F6.7's "reject a non-monotonic `GZ` on `[0, φ_p]`" and
its "the peak is pinned in value but not exactly in location". The fourth
harmonic removes the contradiction rather than choosing a side: the peak is now
pinned in **both** value and location, so monotonicity on `[0, φ_p]` and
unimodality on `(0, φ_v)` say the same thing.

### 1.5 Why `GM` must move, and to what

Rules 3–6 are shape rules, so they carve an admissible region out of the four
human tunables. Holding `φ_p = 0.785`, `GZ_max = 0.30`, `φ_v = 1.396` at their
F7 values and sweeping `GM` in steps of `0.001 m`:

| Rule set | Admissible `GM` (m) |
|---|---|
| shape rules 3–5 only | `[0.535, 0.840]` |
| plus `|GZ| ≤ beam` (1.37 m) | `[0.535, 0.599]` |
| plus `|GZ| ≤ beam/2` (0.685 m) — **the rule adopted** | `[0.535, 0.561]` |

`GM = 1.00 m` is **outside every one of them**, and it fails the shape rules,
not the algebra: the four-constraint solve succeeds and puts a stationary point
at `φ_p` with `GZ(φ_p) = 0.300000` and `GZ'(φ_p) = −1.9e-16`, but that
stationary point is a **local minimum**. The curve has maxima at 31.80°
(`0.308956 m`) and 61.20° (`0.320513 m`) on either side of it, so `GZ_max` is
not the maximum and rule 4 rejects it. It is not a value that can be made to
work by a different solver or a longer series; a slope of 1.00 m/rad at the
origin simply cannot reach only 0.30 m by 45°. The failure modes at the two
edges of the admissible interval are named:

* below `GM ≈ 0.5347`, `GZ'(π) < 0` and the curve regains positive stability
  before inversion — the baseline defect in a milder form;
* above `GM ≈ 0.561`, the negative extremum leaves the geometric envelope. It
  deepens steeply with `GM` — `−0.37 m` at 0.54, `−0.50 m` at 0.55, `−0.66 m`
  at 0.56, `−1.38 m` at 0.60, `−5.87 m` at 0.84 — and crosses
  `−beam/2 = −0.685 m` just above 0.561. A 5.9 m righting arm on a boat 1.37 m
  wide is not a curve anyone should be allowed to ship, which is what rule 6 is
  for.

**`GM = 0.55 m` is adopted.** It is not a new number. Three v1 artefacts already
name it: `docs/v1/progress/07-handoff.md`; v1's own
`stability::hydrostatics::tests::consistent_curve` fixture, which called `0.55 m`
"the value at which the F7 `GZ_max` and `φ_v` are self-consistent"; and
`docs/v1/parameters.md`, which listed this exact defect under "What is most
wrong, in the authors' own estimation" and wrote "`gm ≈ 0.55 m` makes the group
self-consistent … **Not changed** — it is a physical coefficient with a scenario
effect … so it is the human's call". It is the `GM` at which the
*three-harmonic* fit very nearly lands its peak on `φ_p`:

| `GM` (m) | three-harmonic peak (deg) | offset from `φ_p` |
|---|---|---|
| 0.50 | 47.88 | `+2.90°` |
| 0.55 | 45.80 | `+0.82°` |
| 0.569041 | 44.98 | `0.00°` |
| 0.60 | 43.64 | `−1.34°` |
| **1.00** | **32.47** | **`−12.51°`** |

So v1's own evidence had already located the self-consistent `GM` between 0.55
and 0.57, recorded it in a test fixture, and shipped 1.00 anyway. Section 08
promotes the recorded value to the default. It sits inside the adopted
admissible interval `[0.535, 0.561]` at 58 % of its width — away from both
edges, which is the margin rule 5 and rule 6 exist to protect.

This is a parameter change under brief §43 and it is recorded as one: the
reason is *infeasibility of the previous value against a newly stated
constraint*, the source is the sweep above plus `docs/v1/progress/07-handoff.md`,
and the assumption is unchanged — `stability.gm` remains **ASSUMED**. No
scenario, tutorial or demonstration was consulted in choosing it.

### 1.6 The proposed curve, measured

`GM = 0.55`, `φ_p = 0.785`, `GZ_max = 0.30`, `φ_v = 1.396`:

```
c₁ = -1.44101697778091981e-1
c₂ =  3.90784411556917066e-1
c₃ =  1.57129767386624750e-2
c₄ = -3.36515138879323814e-2
```

| Constraint | Residual | Unit |
|---|---|---|
| `GZ(0)` | `0.000e0` | m |
| `GZ'(0) − GM` | `0.000e0` | m/rad |
| `GZ(φ_p) − GZ_max` | `5.551e-17` | m |
| **`GZ'(φ_p)`** | **`5.551e-17`** | m/rad |
| `GZ(φ_v)` | `−2.776e-17` | m |

Stationary points and roots on `(0, π]` — **one root and two stationary
points**, against the baseline's two roots and three stationary points:

| `φ` (rad) | `φ` (deg) | `GZ` (m) | What it is |
|---|---|---|---|
| 0.785000000 | 44.9772 | `+0.300000000` | the maximum, **at `φ_p`** |
| 1.396000000 | 79.9849 | `−2.78e-17` | the only root below `π`: `φ_v` |
| 2.200761211 | 126.0943 | `−0.503165611` | the negative extremum |
| `π` | 180 | `−9.11e-17` | inverted; `GZ'(π) = +0.743926` ⇒ **stable** |

`|GZ|_max = 0.503166 m`, against the `beam/2 = 0.685 m` envelope.
`Δ·g·|GZ_min| = 681 N·m` — the moment that has to be overcome to right a
turtled boat, against `Δ·g·GZ_max = 406 N·m` to capsize an upright one.

Agreement of the shared derivative and integral with numerical
differentiation and quadrature of the **same** `gz`:

| Check | Step | Worst error | Unit |
|---|---|---|---|
| `dgz` vs central difference, 20 001 points over `[−π, π]` | `h = 1e-6` | `1.76e-10` | m/rad |
| `gz_integral` vs trapezoid, `φ ∈ {−2, −0.5, 0.3, 1.4, 2.9, π}` | `4 × 10⁵` panels | `1.94e-12` | m·rad |

The central-difference error is the expected `O(h²·|GZ'''|) ≈ 1e-12` truncation
plus the `O(ε/h) ≈ 2e-10` cancellation floor; halving `h` does not improve it,
which is why the bound is stated at `1.8e-10` and not tighter.

### 1.7 Before and after

| `φ` (deg) | `GZ` v1 (m) | `GZ` v2 (m) | `GZ'` v2 (m/rad) |
|---|---|---|---|
| 0 | 0.000000 | 0.000000 | 0.550000 |
| 10 | 0.167421 | 0.094859 | 0.530231 |
| 20 | 0.294654 | 0.182373 | 0.463501 |
| 30 | 0.353167 | 0.252948 | 0.333292 |
| 40 | 0.334475 | 0.294319 | 0.128249 |
| 45 | 0.299811 | **0.300000** | −0.000621 |
| 50 | 0.252860 | 0.293825 | −0.142680 |
| 60 | 0.141630 | 0.242777 | −0.442671 |
| 70 | 0.043837 | 0.141064 | −0.712200 |
| 80 | −0.000010 | −0.000233 | −0.886141 |
| 90 | **+0.036084** | −0.159815 | −0.916175 |
| 100 | +0.155299 | −0.310807 | −0.788956 |
| 120 | +0.534886 | −0.492368 | −0.204292 |
| 140 | **+0.781669** | −0.452357 | +0.396164 |
| 160 | +0.586539 | −0.253729 | +0.687184 |
| 180 | 0.000000 | −0.000000 | +0.743926 |

Two things change, and they are the two defects:

1. **Below `φ_v` the boat is stiffer, not softer, from 45° on.** The v1 curve
   had already passed its peak by 33° and was falling; the v2 curve is still at
   `GZ_max` at 45° and holds `0.24 m` at 60° where v1 held `0.14 m`. At small
   heel it is softer: `Δ·g·GM` falls from `1353` to `744 N·m/rad`, and the
   undamped roll frequency `√(Δ·g·GM / I_x)` from `5.61` to `4.16 rad/s`.
2. **Past `φ_v` the sign is right.** `−0.16 m` at 90° and `−0.45 m` at 140°,
   where v1 read `+0.04 m` and `+0.78 m`.

`Δ·g·GZ_max = 406 N·m` is unchanged, so F7's anchor, R2's tenderness argument
and every shipped scenario's wind speed keep their stated basis.

---

## 2. Finding 2 — the sheet's geometric minimum and the initial preload

### 2.1 The rope path

`ℓ(β) = |P_b(β) − P_k|` with `P_b(β) = mast_pos_b + d_sheet·b̂(β) + (0,0,z_boom)`.
Writing `a = mast.x − block.x`, `b = mast.y − block.y`,
`h = mast.z + z_boom − block.z`, `R = √(a² + b²)`:

```
ℓ(β)² = a² + b² + h² + d_sheet² − 2·d_sheet·R·cos(β − atan2(b, a))
```

so for `d_sheet > 0` the path is shortest at `β_min = atan2(b, a)` and

```
ℓ_min = ℓ(β_min) = √( h² + (R − d_sheet)² )
```

With the F7 geometry `a = 3.3`, `b = 0`, `h = 0.6`, `R = 3.3`, `d_sheet = 2.45`:

| Quantity | Value | Unit |
|---|---|---|
| `β_min` | `0.0` (exactly) | rad |
| **`ℓ_min = ℓ(0)`** | **`1.0404326023342405`** | m |
| `ℓ(±π)` | `5.7812195945146385` | m |
| `l_sheet_min` (F7) | `0.90` | m |
| `l_sheet_max` (F7) | `4.50` | m |

A `2 × 10⁷`-point sweep of `rope_path_length` over `(−π, π]` finds **no** value
below `ℓ(0)`; the sweep minimum and the closed form agree to the last bit.

| `β` (deg) | `ℓ` (m) | | `β` (deg) | `ℓ` (m) |
|---|---|---|---|---|
| 0 | 1.040433 | | 90 | 4.153613 |
| 15 | 1.278076 | | 105 | 4.630076 |
| 30 | 1.802462 | | 120 | 5.033637 |
| 45 | 2.412174 | | 135 | 5.355970 |
| 60 | 3.027788 | | 150 | 5.590718 |
| 75 | 3.614885 | | 180 | 5.781220 |

(`ℓ` is even in `β`; the negative half mirrors this one.)

`l_sheet_min = 0.90 m` is **0.1404 m below the shortest path the rig can
take**. `L` is clamped to `[l_sheet_min, l_sheet_max]` inside the derivative
(F4.3), so that 0.1404 m is not reachable slack a sailor could take up: it is a
floor the model itself enforces, and at `k_sheet = 2.0e4 N/m` it is a permanent
**2.81 kN** preload on a rope holding a 6 kg boom.

### 2.2 Initial tension, every shipped scenario

At `t = 0`, `L̇ = 0`, from each scenario's own resolved parameters:

| Scenario | `L₀` (m) | `β₀` (rad) | `ℓ(β₀)` (m) | `e` (m) | `T` (N) | `M_β` (N·m) |
|---|---|---|---|---|---|---|
| `beam_reach_capsize` | 0.9 | 0.000000 | 1.040433 | **+0.140433** | **2808.7** | 0.0 |
| `close_hauled` | 2.0 | 0.000000 | 1.040433 | −0.959567 | 0.0 | 0.0 |
| `free_sail` | 4.5 | 0.000000 | 1.040433 | −3.459567 | 0.0 | 0.0 |
| `gybe` | 4.0 | −1.308997 | 3.614885 | −0.385115 | 0.0 | 0.0 |
| `sheet_release_recovery` | 0.9 | 0.000000 | 1.040433 | **+0.140433** | **2808.7** | 0.0 |
| `tack` | 2.0 | +0.610865 | 2.001702 | +0.001702 | 34.0 | −78.9 |
| `Simulation::initial_state` | 0.9 | 0.000000 | 1.040433 | **+0.140433** | **2808.7** | 0.0 |

`M_β = 0` for the three preloaded cases only because `dℓ/dβ = 0` at the
minimum: the 2.81 kN is a pure strut load along the rope with no boom torque
and therefore no visible symptom. `tack`'s 34 N is a genuine trim load — the
boom is at 35° with 2.0 m of sheet — and is not a defect.

The preload is also the one place in the rig with **no damping at all**:
`ė = (dℓ/dβ)·β̇ − L̇` and `dℓ/dβ(0) = 0`, so the `c_sheet` term vanishes exactly
where the stiffness is highest. `docs/v1/progress/06-handoff.md` already records
the consequence — an undamped ≈43 rad/s mode that RK2 cannot integrate to the
energy-invariant tolerance at any `dt` the project uses — and worked around it
by drawing test sheet lengths above `ℓ(0)` rather than by fixing the geometry.

### 2.3 The proposed contract

* `l_sheet_min` becomes **geometry-consistent**: `BoatParameters::validate`
  rejects any catalogue with `l_sheet_min < ℓ_min`, naming `sheet.l_sheet_min`
  and printing both numbers. `rigging::mainsheet::min_rope_path(&p) -> (ℓ_min, β_min)`
  is the single definition, and it returns `rope_path_length(β_min, &p)` — the
  same function the physics evaluates — so the bound and the path cannot
  disagree by a rounding step.
* The **default** `sheet.l_sheet_min` becomes `1.0404326023342405 m`, which is
  `ℓ_min` for the F7 geometry to the last bit. A unit test asserts the equality
  exactly, so editing `d_sheet`, `z_boom`, `mast_pos_b` or `block_pos_b` without
  revisiting the sheet's stop is a test failure and not a silent preload.
* **Intentional prestretch is not modelled.** F6.8's `L` is "the effective
  available length at the boom"; a rope shorter than the path it must follow is
  a rigging fault, not a trim setting. The contract is therefore
  `ℓ_min ≤ l_sheet_min < l_sheet_max`, with equality at the bottom meaning
  "two-blocked: the shortest sheet holds the boom on the centreline at zero
  tension".
* `Scenario::validate` gains `l_sheet_min ≤ initial_state.sheet_length ≤
  l_sheet_max`, resolved against the scenario's **own** parameters, so an
  initial condition cannot start outside the range F4.3 will immediately clamp
  it into.
* `beam_reach_capsize` and `sheet_release_recovery` move their
  `initial_state.sheet_length` from `0.9` to `1.0404326023342405`. Both scripts
  command `sheet_rate = −1` from `t = 0`, so `L` was driven onto the stop within
  0.1 s in either case; what changes is that the stop is now a sheet hauled
  fully in rather than a rope stretched 14 cm.

Expected consequence: the initial tension column becomes `0.0 N` for every
scenario but `tack`, and `Simulation::initial_state` starts unloaded.

---

## 3. Finding 3 — the slack law

### 3.1 The defect

F6.8 writes `T = max(0, k_sheet·e + c_sheet·ė)`. The `max` guarantees `T ≥ 0`,
which is what `invariants::tension_never_negative` and
`invariants::sheet_unilateral_constraint` assert — and both pass. What it does
**not** guarantee is `T = 0` when the rope is slack, because a large positive
`ė` can carry the bracket above zero while `e < 0`:

| `e` (m) | `L̇` (m/s) | `ė` (m/s) | `T` (N) |
|---|---|---|---|
| −0.05 | −5 | +5 | **500.0** |
| −0.20 | −20 | +20 | **2000.0** |
| −0.50 | −50 | +50 | **5000.0** |

A rope hanging 0.5 m slack pulling with 5 kN. Both terms are reachable inside
the model:

* from the sheet command alone, `L̇ = −sheet_haul_rate = −1.5 m/s` gives
  `c_sheet·|ė| = 450 N`, enough to fake tension across any slack below
  `450/2e4 = 0.0225 m`;
* from the boom, `ė ⊇ (dℓ/dβ)·β̇` with `|dℓ/dβ| ≤ 2.45` and boom rates of
  several rad/s through a gybe — `β̇ = 10 rad/s` gives `ė ≈ 24 m/s` and fakes
  tension across `0.36 m` of slack.

The second is the one that matters: it fires exactly during the gybe, which is
the manoeuvre the sheet element exists to model.

### 3.2 The proposed law

```
T = if e > 0 { max(0, k_sheet·e + c_sheet·ė) } else { 0 }
```

**The boundary is `e > 0`, strictly.** The taut branch is the open half-line;
the slack set `{e ≤ 0}` is closed and carries `T = 0` on all of it, `e = 0`
included, for every `ė` of either sign. The reasoning is constitutive, not
numerical: the rope's stiffness *and* its damping are properties of stretched
rope, so an element at exactly its natural length transmits nothing. Taking the
slack set closed rather than open also makes `T` a function of state alone at
the boundary — under the alternative (`e ≥ 0` taut) the tension at `e = 0`
would depend on the sign of `ė`, which is a worse object to integrate and a
worse one to port.

Two consequences are stated rather than discovered later:

* **`T` is discontinuous at take-up.** As `e → 0⁺` with `ė > 0`, `T → c_sheet·ė`;
  at `e = 0`, `T = 0`. That jump is the contact-impact discontinuity every
  unilateral spring–damper contact model has, and it is *the price of the fix*:
  the previous law was continuous and wrong. Section 08 therefore tests the
  crossing with event-aware checks — error against a reference at `dt`, `dt/2`,
  `dt/4` without asserting a smooth convergence order across the event — rather
  than asserting an order the model does not have.
* **The `max` still does work.** It is what stops a *taut, fast-easing* rope
  from pushing: `e > 0` with `c_sheet·ė < −k_sheet·e`. `damping_can_zero_tension`
  already exercises that branch and is unchanged.

`k_sheet`, `c_sheet` and `dt` are **not** touched. R1's mitigation order stands,
and §2.3 lowers the worst sheet load in the shipped set rather than raising it.

---

## 4. What the corrections did, measured

Everything below was measured **after** the corrections landed, from the
repository, with the commands in §0. Nothing in this section was used to choose
a coefficient.

### 4.1 The `GZ` curve

Reproduced by `cargo test -p sailgym-physics stability::hydrostatics -- --nocapture`:

```
hydrostatics: coefficients [-0.14410169777809198, 0.39078441155691707,
                             0.015712976738662475, -0.03365151388793238]
hydrostatics: root 1.396000000 rad; extrema 0.785000000 rad (GZ 0.300000000)
              and 2.200761211 rad (GZ -0.503165611)
hydrostatics: worst GZ past phi_vanish -0.000003246 m at 179.9997 deg
hydrostatics: dgz vs central difference h=1e-6, worst 1.7610768399123344e-10 m/rad
hydrostatics: gz_integral vs trapezoid, worst 1.9397677908372657e-12 m.rad
hydrostatics: roll potential barrier 0.265810399 m.rad at phi_v,
              inverted -0.277728078 m.rad
hydrostatics: GM = 1.00 rejected — parameter stability.gm: GZ reaches
              0.30007552759058675 m at 0.445111328125 rad, above gz_max = 0.3 m
              at phi_peak = 0.785 rad: the named peak is not the maximum
```

The worst value past `φ_v` is at the far end of the interval, next to the
inverted equilibrium where the curve returns to zero from below — which is the
shape rule 5 describes, read back out of the measurement.

`the_admissible_gm_interval_brackets_the_default` asserts the interval of §1.5
in the code, so the number in this report cannot drift from the rule that
produced it: `0.534` is rejected, `0.535` and `0.561` are accepted, `0.562` is
rejected, and the shipped `0.55` sits strictly inside.

### 4.2 The sheet

| Property | Before | After |
|---|---|---|
| Initial tension, `beam_reach_capsize` | 2808.7 N | **0.0 N** |
| Initial tension, `sheet_release_recovery` | 2808.7 N | **0.0 N** |
| Initial tension, `Simulation::initial_state` | 2808.7 N | **0.0 N** |
| Initial tension, `tack` | 34.0 N | 34.0 N (a genuine trim load) |
| `T` on a rope 0.5 m slack with `ė = 50 m/s` | 5000 N | **0.0 N** |
| Slack steps walked with `T = 0` | — | 195 147, against 44 853 taut |

`invariants::the_three_v1_defects_are_reproducible` rebuilds each old
expression and measures it failing the rule that replaced it: the
three-harmonic curve reads `+0.784653 m at 142.21°` where the corrected one
reads `−0.436104 m`; the v1 stop implies `2808.7 N` and is now rejected by
`BoatParameters::validate`; the old bracket fakes up to `6498 N` on states the
model can reach, where the corrected law reads `0 N`.

### 4.3 Energy around the slack/taut transition

`invariants::sheet_does_no_negative_work`, 10 × 20 s episodes at `dt = 0.005`,
30 transitions:

| | worst per-step energy rise |
|---|---|
| away from a transition | **exactly 0.0 J** |
| at a transition | 0.416 J |

and refined, 4 × 10 s episodes:

| `dt` (s) | worst rise at a take-up step (J) |
|---|---|
| 0.01 | 1.633 |
| 0.005 | 0.000 |
| 0.0025 | 0.0156 |

A wider sweep at 10 × 20 s reads 1.633, 0.416, 0.0418, 0.0240, 0.0081 J for
`dt = 0.01 … 0.000625` — an observed order of ≈ 1.9. The invariant is therefore
split at the event rather than widened: **the energy never rises at all away
from a transition**, and the excursion at one must fall by at least four when
`dt` is quartered. That second clause is the RV50 trigger.

### 4.4 Convergence

`cargo test -p sailgym-physics --test convergence -- --nocapture`:

| Scenario | interior slack/taut crossings | what is asserted | measured |
|---|---|---|---|
| `beam_reach_capsize` | **0** | asymptotic order in `[1.7, 2.3]` | pos **1.814**, psi **2.272**, phi 2.269 |
| `close_hauled` | 5, at `t = 0.61, 0.645, 0.815, 14.01, 14.05 s` | crossing count and times converge; error falls under refinement | worst crossing spread **0.0187 s**; error 5.56e-3 → 2.05e-3 m over an 8× refinement |
| `gybe` | 5, at `t = 0.35, 0.40, 0.455, 14.01, 14.04 s` | as above | worst spread **0.0412 s**; error 5.88e-5 → 3.28e-5 m |

**No order is asserted for a trajectory that crosses the boundary**, because a
discontinuous right-hand side does not have one: the measured least-squares
slope for those two is 0.43, and reporting that as an "order of accuracy" would
be reporting a number that means nothing. What replaces it is stronger about
the model than an order would be — the *sequence of physical events* is the
same at every timestep in the sweep, and each event lands at the same instant to
within `O(dt)`.

`error_at_default_dt` is unchanged and still passes on all three: 0.00242 m
(`close_hauled`), 0.00059 m (`beam_reach_capsize`), 0.00008 m (`gybe`) against
a 0.05 m bound.

The `Rk4` reference check changed method, and it is stricter, not looser. v1
estimated the reference's own error by Richardson extrapolation, `|y(2h) − y(h)|/15`;
the `/15` assumes fourth order, which the take-up discontinuity denies. A direct
fourfold refinement of the reference reads:

| Scenario | Richardson estimate | direct refinement | share of the finest RK2 error |
|---|---|---|---|
| `close_hauled` | 0.61 % | 4.579e-5 m | **2.23 %** |
| `beam_reach_capsize` | 0.00 % | 2.699e-10 m | **0.00 %** |
| `gybe` | 0.59 % | 2.228e-6 m | **6.80 %** |

so the Richardson figure flattered the reference by about four times on `gybe`.
The budget is now 10 % of the finest RK2 error, stated against the direct
measurement.

### 4.5 Rotation and mirror symmetry

`cargo test -p sailgym-physics --test symmetry -- --nocapture`, 96 cases:

```
symmetry: 96 cases, worst |Δ| — body 2.608e-11 (tol 1e-10),
          body over the first 11 s 7.285e-13 (tol 1e-11),
          world 1.057e-13 (tol 1e-9), mirror 2.193e-11 (tol 1e-9)
```

v1 asserted `1e-11` on body-frame quantities over the whole 20 s horizon and
measured `1.34e-12`. After the correction the worst is `2.6e-11`, and the excess
is confined to `beam_reach_capsize` and `sheet_release_recovery` — the two that
start two-blocked, i.e. **on** the take-up boundary — and to the steps after the
script's release and haul cues. Before the release cue, the worst across all 96
cases is `7.3e-13`.

The cause is the discontinuity, not a frame error: a discontinuous right-hand
side is not Lipschitz in the state, so two runs that differ only in the last
bits — which is what rotating the world by 45° does — can land on opposite sides
of `e = 0` at a step. The file therefore asserts **two** bounds: v1's `1e-11`,
unchanged, over the window before any case can cross, and a measured `1e-10`
over the whole horizon. The tight half covers 100 % of cases for 55 % of the
horizon.

### 4.6 The shipped scenarios

The corrected curve holds `GZ_max` out to `φ_p = 45°` where v1's had already
peaked at 32.5° and was falling, so the boat carries **more** righting through
the middle of the curve even though `GM` fell. Measured on the beam-reach setup
(haul and hold, no rudder, uniform northerly):

| Quantity | v1 | v2 |
|---|---|---|
| Wind at which holding the sheet capsizes it at all | 6.93 m/s | **7.98 m/s** |
| Wind at which the capsize flag trips inside 13 s | — | **8.45 m/s** |
| Wind above which releasing at 4 s no longer recovers | — | **9.65 m/s** |

At the v1 scenario wind of 7.0 m/s the corrected boat reaches 47.1° of heel and
stays there: `beam_reach_capsize` no longer capsizes, and brief §46's first
demonstration cannot be performed. **The scenario wind therefore moves to
9.0 m/s**, near the centre of the `[8.45, 9.65]` window in which the
demonstration's two halves both hold — held, the boat goes over at `t = 9.88 s`;
released at 4 s it recovers from 65.5° and settles at 3°.

This is a **scenario** parameter, not an F7 coefficient. brief §43 governs
`parameters.rs`; R2 in F11 names "tune per-scenario wind speed" as its own
mitigation, and v1 chose 7.0 m/s by exactly this method — "just above the
6.93 m/s threshold measured in `docs/v1/progress/07-handoff.md`", as the
scenario's own description says. The threshold moved because the model was
corrected; re-measuring it is what keeps the scenario's declared meaning true.
No physical coefficient was touched to obtain it.

`sheet_release_recovery` moves with it, because
`scenario::shipped::recovery_matches_capsize_setup` requires the two to be
identical in every physical field.

### 4.7 The golden trajectories

Old outputs preserved and compared before regeneration. Summary, 30 s scripts
sampled at 5 Hz:

| Scenario | max `|φ|` before | max `|φ|` after | final surge before | after |
|---|---|---|---|---|
| `beam_reach_capsize` | 87.08° | **144.32°** | 0.294 m/s | 1.694 m/s |
| `close_hauled` | 8.14° | 13.81° | 0.169 m/s | 0.270 m/s |
| `free_sail` | 21.57° | 31.17° | 1.972 m/s | 1.946 m/s |
| `gybe` | 19.62° | 28.78° | 0.575 m/s | 0.556 m/s |
| `sheet_release_recovery` | 86.82° | 72.15° | 1.243 m/s | 1.255 m/s |
| `tack` | 11.11° | 13.27° | −0.007 m/s | −0.107 m/s |

Every trajectory changed from the first sample after `t = 0`, which is what a
changed `GM` does. Three changes are **explained rather than merely recorded**:

1. **The two beam-reach scenarios stop at 87° in v1 and go over in v2.** 87° was
   the spurious quasi-equilibrium `docs/v1/progress/07-handoff.md` §4 and
   `docs/v1/convergence.md` both describe — the point where v1's curve had
   already come back to `GZ ≈ 0` and was about to turn positive. It does not
   exist in the corrected curve, which is negative on the whole of `(φ_v, π)`.
   The same fact is what removed `beam_reach_capsize`'s "superconvergence": its
   observed order was 2.85 in v1, because truncation error injected into a
   strongly contracting direction was squeezed out rather than accumulated.
   It is 1.81 now, inside the bracket, and `convergence.rs` no longer needs the
   `SUPERCONVERGENT` exclusion v1 carried.
2. **`l_sheet` starts 0.1404 m longer** in the two beam-reach scenarios and in
   every fresh simulation. That is the preload, removed.
3. **The `sheet_release_recovery` script releases at 4 s instead of 8 s.** At
   9 m/s the boat is already past 73° at 8 s and goes over; a fixture named
   "recovery" that capsized would be a false label. The window measured in §5.6
   is what the new cue sits in. The regenerated trajectory reads 65.5° at
   release, **3.3° at `t = 16 s`**, and 51° again at `t = 24 s` after the
   script's closing haul — recovery and re-loading in one fixture, peaking at
   72.15° and never tripping the capsize flag.

The regenerated files were written from a working tree git cannot identify —
unavoidable, because the correction and its fixtures land in one commit — so
each records `state: "dirty"`, the commit it does **not** implement, and the
`--declare` text naming every change above. `regression.rs` fails a
non-baseline golden that declares nothing.

---

## 5. What is still not known

* Nothing here is calibrated against a real ILCA. `gm`, `phi_peak`, `gz_max`,
  `phi_vanish`, `d_sheet`, `z_boom`, `block_pos_b`, `k_sheet` and `c_sheet`
  remain ASSUMED or TUNABLE.
* The `beam/2` envelope (rule 6) is a geometric *bound*, not a measurement. It
  rejects impossible curves; it does not certify possible ones.
* The inverted equilibrium is now stable, which is what a turtled dinghy does.
  Righting a capsized boat, sail immersion and flooding stay deferred
  (`discussions/deferred-features.md`), so the model will sit at `φ = ±π`
  indefinitely. That is a known and accepted end state, not a recovery model.
* The four-harmonic curve is chosen for being the smallest series that carries
  the four constraints. It is not claimed to be the shape of any real `GZ`
  curve beyond those four points and the sign rules.
* The corrected sheet law costs an order of accuracy wherever the rope takes up
  or lets go, and one to two decades of absolute error at the shipped timestep
  on the two scenarios that do (§4.4). That is a measured trade, not a hidden
  one, and it is inside every bound the project asserts — but it is the thing to
  reach for first if a future section needs tighter trajectories. The remedy is
  event detection or sub-stepping the rigging DOF, never a change to `c_sheet`.
* The take-up load is now impulsive, so **a peak sampled at anything slower than
  the physics timestep is not a reliable measurement of it**: the same eased
  gybe reads 2524 N at `dt` and between 398 N and 2524 N at animation-frame
  rate, depending only on sampling phase. Anything downstream that wants "the
  load the sheet took" should integrate rather than take a maximum.
* The beam-reach scenarios' wind was re-measured, not re-derived. 9.0 m/s is a
  point inside a measured window (§4.6); nothing says a real ILCA capsizes
  there, and R2's caveat — that this boat's sailor never hikes — is unchanged.
