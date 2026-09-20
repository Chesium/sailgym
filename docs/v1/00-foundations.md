# 00 — Foundations (Normative)

**This document contains no tasks and is not executed by an agent.** It is the
single source of truth for conventions, symbols, equations, parameters, and the
WASM surface. Every section PRD and every task subagent reads this file first.

Rule: **no agent may redefine anything in this document.** If an implementation
contradiction is found, stop, record it in `docs/v1/progress/NN-handoff.md`, and
escalate to the human. Do not silently pick a different convention.

Source brief: `docs/v1/brief.md`. References of the form "brief §10" point there.

---

## F1. Units

| Quantity | Unit | Notes |
|---|---|---|
| length | m | |
| mass | kg | |
| time | s | |
| angle | **radians** everywhere in Rust | degrees only at UI boundaries and in scenario JSON, suffixed `_deg` |
| force | N | |
| moment | N·m | |
| velocity | m/s | |
| angular velocity | rad/s | |
| density | kg/m³ | |

All floating point is `f64` inside the physics core. `f32` appears only in
batched visualization buffers crossing to JS.

Constants (`physics/constants.rs`):

```rust
pub const G: f64 = 9.806_65;          // KNOWN
pub const RHO_AIR: f64 = 1.225;       // KNOWN, ISA sea level 15 °C
pub const RHO_WATER: f64 = 1025.0;    // KNOWN, seawater. Fresh water = 998.0.
```

---

## F2. Frames

Three frames. Conversions live in `physics/frames.rs` and **nowhere else**.

### World frame `W`

Right-handed, `x` east, `y` north, `z` up. The simulation is a 2-D plane in
`z = 0`. Positions in metres from the scenario origin.

### Horizontal body frame `H`

Origin at the boat CG. Obtained from `W` by a yaw rotation `ψ` about `+z`.

- `+x_H` = forward (bow), projected horizontally
- `+y_H` = **to port**
- `+z_H` = up

`ψ` is measured **counter-clockwise from world `+x`** (right-hand rule about
`+z`). Body→world:

```
R_z(ψ) = [ cos ψ  -sin ψ ]
         [ sin ψ   cos ψ ]
```

Surge/sway/yaw dynamics are written in `H`. This is the standard 4-DOF
manoeuvring frame.

### Boat-fixed frame `B`

Obtained from `H` by a roll rotation `φ` about `+x_H` (right-hand rule).

```
R_x(φ) = [ 1    0        0     ]
         [ 0  cos φ   -sin φ   ]
         [ 0  sin φ    cos φ   ]
```

Rig and foil geometry (mast position, CE height, board depth) is defined in `B`
and is **constant**. Heel effects on force and lever arm emerge from `R_x(φ)`;
there is no ad-hoc `cos φ` fudge factor anywhere (see F6.4).

### Sign consequences — memorise these

| Statement | Sign |
|---|---|
| `+y` is **port** | |
| `ψ` increasing = bow swinging **to port** (CCW from above) | `r > 0` ⇒ turning to port |
| `φ > 0` = **starboard side down** (port rail rises) | rotating `+y_H` toward `+z` |
| `p = φ̇ > 0` = rolling toward starboard | |
| `β > 0` = boom to **starboard** | see F2.1 |
| `δr > 0` = bow turns to **starboard** | see F2.2 |

### F2.1 Boom angle `β`

`β` is a right-handed rotation about `+z_B` applied to the boom's reference
direction (dead aft). Boom unit vector in `B`:

```
b̂(β) = ( −cos β , −sin β , 0 )        β ∈ (−π, π]
```

**Gotcha, stated once:** because the boom points *aft*, a positive
(counter-clockwise, seen from above) rotation swings the boom tip to
**starboard**. This is deliberate: every rotation about `+z` in this codebase is
right-handed, so boom dynamics are simply

```
I_b β̈ = Σ M_z,mast
```

with **no sign flip anywhere**. The UI displays a derived
`boom_deg_to_port = −β·180/π` for humans.

`db̂/dβ = ( sin β , −cos β , 0 )`.

### F2.2 Rudder angle `δr`

`δr` is likewise a right-handed rotation about `+z_B` of the rudder chord from
the hull centreline. Rudder chord direction (leading edge at the stock, trailing
edge aft):

```
ĉ_r(δr) = ( −cos δr , −sin δr , 0 )
```

Consequence: `δr > 0` deflects the trailing edge to starboard, generating rudder
side force to **port**, which — the rudder being aft of the CG — yields
`M_z < 0`, so the **bow turns to starboard**. The tiller moves the opposite way;
`A`/`Left` and `D`/`Right` are mapped in section 02 so that "D steers right".

---

## F3. State vector

`physics/state.rs`. Field order is normative — the snapshot buffer layout (F8.3),
the recording schema (section 09) and the golden regression files all depend on it.

```rust
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BoatState {
    // pose, world frame
    pub x: f64,        // m,     world east
    pub y: f64,        // m,     world north
    pub psi: f64,      // rad,   yaw, CCW from world +x, wrapped to (−π, π]
    pub phi: f64,      // rad,   roll, +stbd down, NOT wrapped (capsize may exceed ±π)

    // velocities, horizontal body frame H
    pub u: f64,        // m/s,   surge, +forward
    pub v: f64,        // m/s,   sway,  +to port
    pub r: f64,        // rad/s, yaw rate, +to port
    pub p: f64,        // rad/s, roll rate, +toward starboard

    // rig
    pub beta: f64,     // rad,   boom angle, +to starboard, wrapped to (−π, π]
    pub beta_dot: f64, // rad/s

    // actuators
    pub delta_r: f64,  // rad,   rudder angle, +bow to starboard
    pub l_sheet: f64,  // m,     available mainsheet length at the boom attachment

    // simulation time
    pub t: f64,        // s,     seconds since reset
}
pub const STATE_LEN: usize = 13;
```

`phi` is deliberately **not wrapped**: brief §17 requires the boat to pass
dynamically through `|φ| > 90°`. Wrapping would break roll-rate continuity and
the energy invariant test.

Accelerations are **derived**, never integrated as independent state (brief §5).

### Derivatives

```rust
pub struct StateDot { /* same 13 fields, each the time derivative */ }
```

### Controls

```rust
#[derive(Clone, Copy, Debug, Default)]
pub struct Controls {
    pub rudder_rate_cmd: f64,  // normalised [−1, 1]; +1 = steer bow to starboard
    pub sheet_rate_cmd: f64,   // normalised [−1, 1]; +1 = ease (pay out), −1 = haul
    pub sheet_release: bool,   // Space: ease at the release rate, overrides sheet_rate_cmd
}
```

Controls are **rates**, never absolute angles (brief §12, §13).

---

## F4. Equations of motion

`physics/dynamics.rs`. 5 dynamic DOF (surge, sway, yaw, roll, boom) + 4
kinematic + 2 actuator integrations.

### F4.1 Kinematics

```
ẋ = u cos ψ − v sin ψ
ẏ = u sin ψ + v cos ψ
ψ̇ = r
φ̇ = p
```

### F4.2 Rigid-body dynamics with added mass

```
m_x u̇ = ΣX + m_y · v · r
m_y v̇ = ΣY − m_x · u · r
I_z ṙ = ΣN
I_x ṗ = ΣK
I_b β̈ = ΣM_β
```

where

```
m_x = m + a_x        total mass + surge added mass
m_y = m + a_y        total mass + sway added mass
I_z = I_zz + a_psi   yaw inertia + added
I_x = I_xx + a_phi   roll inertia about the CG, + added
I_b                  boom+sail inertia about the mast axis
m   = m_hull + m_sailor
```

`ΣX, ΣY` are total force components in `H`; `ΣN` is the yaw moment about
`+z_H`; `ΣK` is the roll moment about `+x_H ≡ +x_B`; `ΣM_β` is the boom moment
about `+z_B` at the mast.

**Verification note (do not remove):** with `ΣY = 0`, `u > 0`, `r > 0` (turning
to port), `v̇ = −u r < 0`. The body-frame sway velocity drifts to starboard,
which is correct: holding a circle to port requires a sustained force to port
(`ΣY = m_x u r > 0`) supplied by the foils.

### F4.3 Actuator integration

```
δ̇r  = clamp_rate(rudder command, δ̇r_max),  δr clamped to ±δr_max
L̇   = sheet command rate,                   L clamped to [L_min, L_max]
```

Both clamps are applied **inside** the derivative function so RK2 stages stay
consistent. Clamping only the integrated result breaks time-step convergence.

### F4.4 Generalised force assembly

Every force-producing module returns the same struct. Nothing mutates state.

```rust
/// A force and its application point, expressed in the boat-fixed frame B.
#[derive(Clone, Copy, Debug, Default)]
pub struct Load {
    pub f: Vec3,   // N,   force in B
    pub r: Vec3,   // m,   application point relative to CG, in B
}

/// Accumulated generalised forces in the horizontal frame H.
#[derive(Clone, Copy, Debug, Default)]
pub struct Generalized { pub x: f64, pub y: f64, pub n: f64, pub k: f64 }

impl Generalized {
    /// Rotate a boat-fixed Load into H and accumulate.
    /// K (roll moment) is about +x, shared by both frames, so it is not rotated.
    pub fn add(&mut self, l: Load, phi: f64) { /* see F6.4 */ }
}
```

Pipeline, exactly as brief §7:

```
environment ─► local fluid velocities ─► component Loads ─► Generalized ─► accelerations ─► integrate
```

No module may read or write another module's state. No branch of the form
`if sail_released { reduce_heel() }` is permitted anywhere (brief §7, §46).

---

## F5. Shared foil model

`physics/foil.rs`. Used **unchanged** by sail, centreboard and rudder. There is
exactly one lift/drag implementation in this codebase.

### F5.1 Angle of attack

Given a chord unit vector `ĉ` (leading edge → trailing edge) and an inflow
velocity `v` (velocity of the fluid **relative to the foil**), both 2-D in the
foil's working plane:

```rust
/// Signed angle from the flow direction to the chord. α = 0 ⇒ flow runs LE→TE.
pub fn angle_of_attack(v: Vec2, chord: Vec2) -> f64 {
    let f = v.normalize();            // returns Vec2::ZERO if |v| < EPS_FLOW
    let c = chord.normalize();
    (f.cross(c)).atan2(f.dot(c))      // cross(a,b) = a.x*b.y − a.y*b.x
}
pub const EPS_FLOW: f64 = 1e-9;       // m/s
```

For a rudder in straight-ahead flow this yields `α = δr` exactly. Asserted by a
test in section 04.

### F5.2 Coefficients

Continuous and defined on the whole of `[−π, π]` (brief §10). Attached flow
blended into a flat-plate post-stall model:

```
C_Lα   = 2π / (1 + 2/AR)                       lift-curve slope, per radian
s(α)   = smoothstep(α_s, α_s + Δ_s, |α|)       0 = attached, 1 = fully stalled
α_e    = α − α_0 · tanh(α / α_b)               camber hook, odd in α (α_0 = 0 in v1)

C_L,att(α)  = C_Lα · sin α_e
C_L,stall(α)= C_N,max · sin α · cos α
C_D,ind(α)  = C_L,att² / (π · e · AR)

C_L(α) = (1 − s)·C_L,att + s·C_L,stall
C_D(α) = C_D0 + (1 − s)·C_D,ind + s·C_N,max·sin²α
```

Properties, all asserted by tests in section 04:

- `C_L` is **odd**, `C_D` is **even** ⇒ port/starboard mirror symmetry holds exactly.
- `C_L(0) = 0`, `C_D(0) = C_D0`.
- `C_L(±π/2) = 0`, `C_D(±π/2) = C_D0 + C_N,max` — broad reaching / running is pure drag.
- `C_L(±π) = 0`, `C_D(±π) = C_D0` — flow edge-on from astern; reversed loading handled naturally.
- No singularity, no division by `sin α`, no branch on the sign of `α`.

`α_0 = 0` for v1. Non-zero camber stays odd in `α` because of the `tanh`
factor, so the sail can flip sides without breaking mirror symmetry. Do not
replace it with `α_0 · sign(α)` — that is discontinuous.

### F5.3 Force

```rust
pub struct FoilParams {
    pub area: f64, pub ar: f64,
    pub alpha_stall: f64, pub stall_blend: f64,
    pub cn_max: f64, pub cd0: f64, pub oswald: f64,
    pub alpha_camber: f64, pub camber_blend: f64,
}

/// Lift ⟂ flow, drag ∥ flow. Returns force in the same 2-D frame as `v`/`chord`.
pub fn foil_force(v: Vec2, chord: Vec2, rho: f64, p: &FoilParams) -> Vec2 {
    let vsq = v.length_squared();
    if vsq < EPS_FLOW * EPS_FLOW { return Vec2::ZERO; }   // zero-flow ⇒ zero force
    let q = 0.5 * rho * vsq;
    let a = angle_of_attack(v, chord);
    let f_hat = v.normalize();
    let l_hat = Vec2::new(f_hat.y, -f_hat.x);             // rotate flow dir by −90°
    l_hat * (q * p.area * cl(a, p)) + f_hat * (q * p.area * cd(a, p))
}
```

**The lift direction is `(f̂.y, −f̂.x)`, a −90° rotation of the flow direction.**
Combined with `C_L > 0` for small `α > 0`, this is the unique choice that makes
a rudder at `δr > 0` push its blade to port. Sail and board inherit it. A sign
error here is the single most likely defect in the whole project; section 04
asserts it directly.

---

## F6. Subsystem specifications

### F6.1 Wind field — `physics/environment/wind.rs`

```rust
pub trait WindField {
    fn sample(&self, x: f64, y: f64, t: f64) -> Vec2;
    fn sample_grid(&self, x0: f64, y0: f64, dx: f64, dy: f64,
                   nx: usize, ny: usize, t: f64, out: &mut [f32]);
}
```

The returned vector is the **velocity of the air in the world frame**, i.e. the
direction the wind is blowing **toward**. Scenario JSON and the UI both use the
meteorological **"from" bearing in degrees, clockwise from north**; conversion
lives in exactly one function:

```rust
/// Meteorological bearing (deg, from-direction, CW from north) → world velocity vector.
pub fn wind_from_bearing(speed: f64, bearing_deg: f64) -> Vec2 {
    let b = bearing_deg.to_radians();
    Vec2::new(-speed * b.sin(), -speed * b.cos())
}
```

Modes (brief §18):

- **Uniform** — constant `W0`.
- **Spatial** — `W0` plus a divergence-free perturbation from a stream function,
  so the field has no sources or sinks:

```
Ψ(p, t) = Σ_{k=1..K} A_k · sin( κ_k · p + ω_k t + ϕ_k )
w_pert  = ( ∂Ψ/∂y , −∂Ψ/∂x )
A_k     = A0 · |κ_k|^(−pow) / norm
```

- **Gust** — the temporal component; `ω_k = 0` disables it, giving a frozen field.

All of `κ_k` (direction and magnitude), `ϕ_k`, `ω_k` are drawn once at
construction from the seeded RNG. `K` is fixed (default 12), so `sample` is
`O(K)` and allocation-free. No dense grid is ever stored (brief §18).

`sample_grid` must produce **bit-identical** values to `sample` at the same
points — the visualization and the physics are the same field (brief §19, §47).

### F6.2 Apparent wind — `physics/aero/apparent.rs`

```rust
/// Apparent wind at a point, expressed in the boat-fixed frame B.
/// `r_b` is the point's position relative to the CG, in B.
pub fn apparent_wind_at(st: &BoatState, wind_world: Vec2, r_b: Vec3) -> Vec3
```

Steps, in order:

1. True wind `W` sampled in world at the boat position.
2. Rotate into `H`: `W_H = R_z(−ψ) · W`.
3. Boat CG velocity in `H` is `(u, v, 0)`.
4. Rotation contribution: `ω_H = (p, 0, r)`; point velocity
   `V_pt = (u, v, 0) + ω_H × (R_x(φ)·r_b)`.
5. Apparent wind in `H`: `A_H = W_H − V_pt`.
6. Rotate into `B`: `A_B = R_x(−φ) · A_H`.

This covers every case in brief §8: stationary, accelerating, turning, tacking,
gybing. The `ω × r` term is what makes the sail unload correctly during a fast
tack; omitting it is a silent physics bug, so section 05 tests it explicitly.

### F6.3 Sail — `physics/aero/sail.rs`

```rust
pub struct SailOutput { pub load: Load, pub alpha: f64, pub cl: f64, pub cd: f64,
                        pub m_beta: f64, pub aw_b: Vec3 }
pub fn sail_load(st: &BoatState, wind_world: Vec2, p: &BoatParameters) -> SailOutput
```

1. CE position in `B`: `r_CE = mast_pos_b + d_ce · b̂(β) + (0, 0, z_ce)`.
2. `A_B = apparent_wind_at(st, wind, r_CE)`.
3. **Drop the spanwise component** `A_B.z` (independence principle — flow along
   the mast produces no lift). Work with `A_2 = (A_B.x, A_B.y)`.
4. Chord `ĉ = (b̂.x, b̂.y)` — the sail chord runs from the mast (LE) to the clew (TE).
5. `F_2 = foil_force(A_2, ĉ, RHO_AIR, sail_params)`; `Load { f: (F_2.x, F_2.y, 0), r: r_CE }`.
6. Boom moment about the mast: `M_β = (r_CE − mast_pos_b) × F |_z`.

### F6.4 Heel handling — the only correct treatment

Because forces are computed in `B` and accumulated through `Generalized::add`,
the heel correction of brief §10 is **geometric, not empirical**:

```rust
pub fn add(&mut self, l: Load, phi: f64) {
    let (s, c) = phi.sin_cos();
    // roll moment about +x, computed in B; +x is shared between B and H
    self.k += l.r.y * l.f.z - l.r.z * l.f.y;
    // in-plane force rotated B → H, then projected onto the horizontal plane
    self.x += l.f.x;
    self.y += l.f.y * c - l.f.z * s;
    // yaw moment about +z_B rotated into H
    let n_b = l.r.x * l.f.y - l.r.y * l.f.x;
    self.n += n_b * c;
}
```

Step 6 of F6.2 already reduces the lateral apparent wind by `cos φ`
(`R_x(−φ)` applied to a horizontal vector gives `(a_x, a_y cos φ, −a_y sin φ)`).
That **is** the classical heel correction, derived rather than fudged. Do not
add a second `cos φ` factor anywhere.

### F6.5 Centreboard and rudder — `physics/hydro/`

```rust
pub fn foil_hydro_load(st: &BoatState, r_b: Vec3, chord_b: Vec2,
                       p: &FoilParams) -> (Load, f64 /*alpha*/, f64 /*v_local*/)
```

Local water velocity relative to the surface (water is still — brief §18 defers
currents):

```
v_local_H = −[ (u, v, 0) + ω_H × (R_x(φ)·r_b) ]
```

then rotated into `B` and the spanwise (`z_B`) component dropped, exactly as for
the sail. Chord for the board is `(−1, 0)` (fixed, along the centreline); for
the rudder it is `ĉ_r(δr)` from F2.2.

This produces leeway resistance, board side force, yaw damping, speed-dependent
rudder authority, rudder stall and near-zero authority at rest (brief §14)
**without any of those being coded as special cases**.

### F6.6 Hull — `physics/hydro/hull.rs`

Reduced empirical model (brief §15), linear + quadratic:

```
X_hull = −( X_u·u   + X_uu·u·|u| )
Y_hull = −( Y_v·v   + Y_vv·v·|v| )
N_hull = −( N_r·r   + N_rr·r·|r| )
K_hull = −( K_p·p   + K_pp·p·|p| )
```

applied at the CG (`r = 0`) except `K_hull`, which is added directly to `ΣK`.
Every coefficient is a named parameter, replaceable by towing-tank data later.

### F6.7 Hydrostatic righting — `physics/stability/hydrostatics.rs`

Righting moment (brief §16):

```
K_restore = −Δ · g · GZ(φ)          Δ = m_hull + m_sailor
```

`GZ` is a three-term **odd** harmonic series — smooth, 2π-periodic, mirror-symmetric,
defined for every `φ` including past inversion:

```
GZ(φ) = c1·sin φ + c2·sin 2φ + c3·sin 3φ
```

The human-facing tunables are `GM`, `φ_p`, `GZ_max`, `φ_v`. Coefficients are
solved once at parameter-build time from the 3×3 linear system:

```
c1 + 2c2 + 3c3                                   = GM        (slope at φ = 0)
c1 sin φ_p  + c2 sin 2φ_p  + c3 sin 3φ_p         = GZ_max    (value at the peak)
c1 sin φ_v  + c2 sin 2φ_v  + c3 sin 3φ_v         = 0         (vanishing angle)
```

```rust
pub struct GzCurve { c1: f64, c2: f64, c3: f64 }
impl GzCurve {
    pub fn fit(gm: f64, phi_p: f64, gz_max: f64, phi_v: f64) -> Result<Self, ParamError>;
    pub fn gz(&self, phi: f64) -> f64;
    pub fn dgz(&self, phi: f64) -> f64;
}
```

The peak is pinned in value but not exactly in location; `fit` must reject
parameter sets that produce a non-monotonic `GZ` on `[0, φ_p]` or a sign change
before `φ_v`. This reproduces all the qualitative features brief §16 requires:
rising restoring moment, a peak, decay, vanishing stability, and negative
restoring moment beyond `φ_v`.

### F6.8 Mainsheet — `physics/rigging/mainsheet.rs`

Unilateral tension element (brief §11). `T ≥ 0` is structural, not clamped
after the fact.

```
P_b(β) = mast_pos_b + d_sheet · b̂(β) + (0, 0, z_boom)     boom attachment, in B
P_k                                                        block on the hull, in B, constant
ℓ(β)   = |P_b(β) − P_k|
e      = ℓ − L                                             extension, L = state.l_sheet
dℓ/dβ  = ( (P_b − P_k) · dP_b/dβ ) / ℓ,   dP_b/dβ = d_sheet · (sin β, −cos β, 0)
ė      = (dℓ/dβ)·β̇ − L̇
T      = max( 0 , k_sheet·e + c_sheet·ė )
F_b    = T · (P_k − P_b)/ℓ                                 pulls the boom toward the block
M_β    = (P_b − mast_pos_b) × F_b |_z
```

`T = 0` whenever `e < 0` (slack rope) and whenever damping would make the
bracket negative. Sheet tension produces boom torque **through geometry**; the
boom angle is never assigned (brief §11).

The reaction `−F_b` on the hull at `P_k` is included in `Generalized` so the
sheet load contributes to heel and yaw. It is not dropped.

### F6.9 Boom — `physics/rigging/boom.rs`

```
I_b β̈ = M_aero + M_sheet + M_damp + M_limit
M_damp  = −c_beta · β̇
M_limit = soft one-sided spring outside ±β_max:
          −k_lim·(|β| − β_max)·sign β − c_lim·β̇   when |β| > β_max, else 0
```

No `portTack`/`starboardTack` state exists anywhere in the codebase (brief §9).

### F6.10 Capsize — informational only

```rust
pub struct CapsizeState { pub capsized: bool, pub since: f64 }
```

`capsized` is set when `|φ| > φ_capsize` continuously for `t_capsize` seconds.
It is **reported, never acted on**. The simulation continues integrating; nothing
branches on it (brief §17). Deferred: sail immersion, flooding, righting.

---

## F7. Parameter catalogue

`physics/parameters.rs`. Tags follow brief §48:

- **KNOWN** — from published ILCA/class data or physical constants.
- **ASSUMED** — physically motivated estimate; plausible, unvalidated.
- **TUNABLE** — expected to be adjusted by playtesting or fitting.
- **DEFERRED** — hook exists, not used in v1.

**No numeric literal from this table may appear anywhere outside
`parameters.rs`.** A grep in section 10 enforces this.

### Hull and inertia

| Field | Default | Tag | Note |
|---|---|---|---|
| `loa` | 4.23 m | KNOWN | brief §3 |
| `lwl` | 3.81 m | KNOWN | brief §3 |
| `beam` | 1.37 m | KNOWN | brief §3 |
| `m_hull` | 58.0 kg | KNOWN | brief §3; class minimum varies 56.7–59.0 by era/source |
| `m_sailor` | 80.0 kg | KNOWN | brief §4, by definition |
| `sailor_pos_b` | (0, 0, 0.35) m | ASSUMED | amidships, on centreline (brief §4) |
| `i_zz` | 155 kg·m² | ASSUMED | `m·(0.25·LOA)²` |
| `i_xx` | 28 kg·m² | ASSUMED | roll radius of gyration 0.45 m about the CG |
| `a_x` | 7 kg | ASSUMED | surge added mass ≈ 5 % of Δ |
| `a_y` | 100 kg | ASSUMED | sway added mass, slender-body estimate |
| `a_psi` | 60 kg·m² | ASSUMED | yaw added inertia |
| `a_phi` | 15 kg·m² | ASSUMED | roll added inertia |

### Hull resistance

| Field | Default | Tag |
|---|---|---|
| `x_u` | 5.0 N·s/m | TUNABLE |
| `x_uu` | 9.0 N·s²/m² | TUNABLE |
| `y_v` | 40.0 N·s/m | TUNABLE |
| `y_vv` | 512.0 N·s²/m² | TUNABLE |
| `n_r` | 250.0 N·m·s | TUNABLE |
| `n_rr` | 180.0 N·m·s² | TUNABLE |
| `k_p` | 60.0 N·m·s | TUNABLE |
| `k_pp` | 40.0 N·m·s² | TUNABLE |

Sanity anchor: at `u = 2.06 m/s` (4 kn) total surge resistance ≈ 48 N. The model
has no planing regime and over-predicts resistance above ≈ 5 m/s; documented
limitation (R6), replaceable per brief §15.

### Sail

| Field | Default | Tag | Note |
|---|---|---|---|
| `sail.area` | 7.06 m² | KNOWN | brief §3 |
| `sail.ar` | 3.7 | ASSUMED | luff² / area, luff ≈ 5.1 m |
| `sail.alpha_stall` | 0.262 rad (15°) | TUNABLE | |
| `sail.stall_blend` | 0.105 rad (6°) | TUNABLE | |
| `sail.cn_max` | 1.8 | ASSUMED | flat-plate normal force |
| `sail.cd0` | 0.06 | ASSUMED | |
| `sail.oswald` | 0.85 | ASSUMED | |
| `sail.alpha_camber` | 0.0 | DEFERRED | hook per F5.2 |
| `boom_length` | 2.72 m | KNOWN | |
| `d_ce` | 1.05 m | ASSUMED | ≈ 0.38 × boom length from the mast |
| `z_ce` | 2.40 m | ASSUMED | CE height above the CG |
| `mast_pos_b` | (1.20, 0, 0) m | ASSUMED | mast foot relative to the CG |
| `i_boom` | 12.0 kg·m² | ASSUMED | boom 3 kg + sail 3 kg about the mast |
| `c_beta` | 2.0 N·m·s/rad | TUNABLE | gooseneck friction |
| `beta_max` | 1.745 rad (100°) | ASSUMED | |
| `k_lim` | 400 N·m/rad | TUNABLE | |
| `c_lim` | 40 N·m·s/rad | TUNABLE | |

### Centreboard

| Field | Default | Tag |
|---|---|---|
| `board.area` | 0.20 m² | ASSUMED |
| `board.ar` | 4.9 | ASSUMED (geometric 2.46, doubled for the free-surface mirror) |
| `board.alpha_stall` | 0.209 rad (12°) | TUNABLE |
| `board.stall_blend` | 0.087 rad (5°) | TUNABLE |
| `board.cn_max` | 1.9 | ASSUMED |
| `board.cd0` | 0.012 | ASSUMED |
| `board.oswald` | 0.90 | ASSUMED |
| `board_pos_b` | (0.45, 0, −0.45) m | ASSUMED |

### Rudder

| Field | Default | Tag |
|---|---|---|
| `rudder.area` | 0.105 m² | ASSUMED |
| `rudder.ar` | 3.9 | ASSUMED |
| `rudder.alpha_stall` | 0.209 rad (12°) | TUNABLE |
| `rudder.stall_blend` | 0.087 rad (5°) | TUNABLE |
| `rudder.cn_max` | 1.9 | ASSUMED |
| `rudder.cd0` | 0.012 | ASSUMED |
| `rudder.oswald` | 0.90 | ASSUMED |
| `rudder_pos_b` | (−2.00, 0, −0.28) m | ASSUMED |
| `delta_r_max` | 0.698 rad (40°) | ASSUMED |
| `delta_r_rate_max` | 2.09 rad/s (120°/s) | TUNABLE |
| `delta_r_return_rate` | 1.57 rad/s (90°/s) | TUNABLE |
| `delta_r_self_centre` | `true` | TUNABLE |

### Stability

| Field | Default | Tag |
|---|---|---|
| `gm` | 1.00 m | ASSUMED |
| `phi_peak` | 0.785 rad (45°) | ASSUMED |
| `gz_max` | 0.30 m | ASSUMED |
| `phi_vanish` | 1.396 rad (80°) | ASSUMED |
| `phi_capsize` | 1.396 rad (80°) | TUNABLE |
| `t_capsize` | 1.0 s | TUNABLE |

Anchor: `Δ·g·GZ_max = 138 × 9.81 × 0.30 ≈ 406 N·m`. With `z_ce = 2.4 m` that is
balanced by ≈ 169 N of sail side force, reached at roughly 6 m/s apparent wind
with the sail sheeted in. **This is correct and intended**: brief §4 fixes the
sailor amidships with no hiking, so an ILCA is genuinely capsize-prone above
≈ 12 kn. Scenario wind speeds are chosen accordingly (see R2).

### Mainsheet

| Field | Default | Tag |
|---|---|---|
| `k_sheet` | 2.0e4 N/m | TUNABLE |
| `c_sheet` | 300 N·s/m | TUNABLE |
| `d_sheet` | 2.45 m | ASSUMED (near the clew) |
| `z_boom` | 0.70 m | ASSUMED (above the CG) |
| `block_pos_b` | (−2.10, 0, 0.10) m | ASSUMED (transom block) |
| `l_sheet_min` | 0.90 m | ASSUMED |
| `l_sheet_max` | 4.50 m | ASSUMED |
| `sheet_haul_rate` | 1.5 m/s | TUNABLE |
| `sheet_ease_rate` | 3.0 m/s | TUNABLE |
| `sheet_release_rate` | 6.0 m/s | TUNABLE |

Mechanical advantage, block friction, ratchets, cleats and hand force are
DEFERRED (brief §11); `L` is the effective available length at the boom.

### Integration

| Field | Default | Tag |
|---|---|---|
| `dt` | 0.005 s | TUNABLE (brief §21 range 0.005–0.01) |
| `integrator` | `Rk2Midpoint` | KNOWN (brief §21 preferred default) |

---

## F8. Rust ⇄ WASM boundary

Ownership exactly as brief §23. **No physical equation is implemented in
TypeScript.** Section 10 greps the TS sources for physics constants.

### F8.1 Crates

```
crates/
├── sailgym-physics/     pure Rust, no wasm_bindgen, no JS types. All tests live here.
├── sailgym-wasm/        thin wasm_bindgen wrapper over sailgym-physics
└── sailgym-bench/       native headless benchmark + golden-trajectory generator
```

`sailgym-physics` must build and test with plain `cargo test` on the host. That
is what makes the native headless simulator and later RL work possible
(brief §45).

### F8.2 API

```rust
#[wasm_bindgen]
pub struct Sim { /* … */ }

#[wasm_bindgen]
impl Sim {
    #[wasm_bindgen(constructor)]
    pub fn new(config_json: &str) -> Result<Sim, JsValue>;

    pub fn reset(&mut self, scenario_json: &str) -> Result<(), JsValue>;
    pub fn set_controls(&mut self, rudder_rate: f64, sheet_rate: f64, release: bool);

    /// Advance exactly `n` fixed steps of `dt`. Returns steps actually taken.
    pub fn advance(&mut self, n: u32) -> u32;

    /// Flat f64 view of BoatState, layout = F3 field order.
    pub fn snapshot(&self) -> Box<[f64]>;

    /// Full diagnostics, serde-serialised. Called at UI rate, not physics rate.
    pub fn diagnostics(&self) -> Result<JsValue, JsValue>;

    /// Batched wind sampling. `out.len()` must be 2*nx*ny; writes [wx, wy] pairs
    /// in row-major order. Single call per animation frame (brief §19).
    pub fn sample_wind_grid(&self, x0: f64, y0: f64, dx: f64, dy: f64,
                            nx: u32, ny: u32, t: f64, out: &mut [f32]);

    /// Live parameter editing (brief §31). Returns whether a reset is required.
    pub fn set_parameter(&mut self, path: &str, value: f64) -> Result<bool, JsValue>;
    pub fn parameters_json(&self) -> Result<JsValue, JsValue>;

    /// Recording (brief §33).
    pub fn start_recording(&mut self, hz: f64);
    pub fn stop_recording(&mut self) -> Result<JsValue, JsValue>;
}
```

Coarse-grained by construction (brief §24). Per-force-component and
per-entity calls are forbidden.

### F8.3 Snapshot layout

Index order is the F3 field order:

```
0:x 1:y 2:psi 3:phi 4:u 5:v 6:r 7:p 8:beta 9:beta_dot 10:delta_r 11:l_sheet 12:t
```

Mirrored in `web/src/sim/snapshot.ts` as a typed accessor generated from a
single shared constant list. A test asserts the two agree in length and order.

---

## F9. Determinism rules (brief §34 — hard requirement)

1. Fixed `dt`. Physics never reads a wall clock.
2. All randomness flows from an explicit `u64` seed through `physics/rng.rs`
   (PCG32, implemented in-crate — no dependency on an RNG whose algorithm may
   change across versions).
3. No `HashMap`/`HashSet` iteration inside physics. Use `Vec` or `BTreeMap`.
4. Force summation order is fixed and explicit in `forces.rs`.
5. No `f32` intermediate in physics; no fast-math-style flags.
6. No parallelism inside a single simulation step.
7. `advance(n)` must produce the same result as `n` calls to `advance(1)`.

Guarantee scope: bit-identical for the same build on the same platform, per
brief §34. Cross-platform bit-identity is **not** claimed.

---

## F10. Repository layout

```
sailgym/
├── Cargo.toml                  workspace
├── crates/
│   ├── sailgym-physics/src/
│   │   ├── lib.rs      constants.rs  vec.rs  rng.rs
│   │   ├── state.rs    parameters.rs frames.rs  foil.rs
│   │   ├── dynamics.rs forces.rs     integrator.rs  diagnostics.rs
│   │   ├── scenario.rs recording.rs  simulation.rs
│   │   ├── environment/wind.rs
│   │   ├── aero/{apparent.rs, sail.rs}
│   │   ├── hydro/{hull.rs, centerboard.rs, rudder.rs}
│   │   ├── rigging/{boom.rs, mainsheet.rs}
│   │   └── stability/{hydrostatics.rs, roll.rs}
│   │   tests/          invariants.rs  convergence.rs  regression.rs
│   ├── sailgym-wasm/src/lib.rs
│   └── sailgym-bench/src/main.rs
├── web/
│   ├── src/
│   │   ├── sim/        useSimulation.ts  snapshot.ts  clock.ts  controls.ts
│   │   ├── render/     BoatSvg.tsx  HeelIndicator.tsx  ForceOverlay.tsx  Camera.ts
│   │   ├── wind/       WindLayer.tsx  particles.ts
│   │   ├── ui/         Hud.tsx  DebugPanel.tsx  ParameterPanel.tsx  Timeline.tsx
│   │   └── App.tsx
│   ├── tests/e2e/      *.spec.ts
│   └── package.json  vite.config.ts  playwright.config.ts
├── scenarios/          *.json
├── docs/v1/               brief.md  00-foundations.md … 10-hardening.md  progress/
└── scripts/            build-wasm.(sh|ps1)  check.(sh|ps1)
```

---

## F11. Risk register

Section PRDs reference these by ID. If a risk fires, record it in the section
handoff note.

**R1 — Mainsheet stiffness vs. timestep.** `k_sheet = 2e4 N/m` with
`I_b = 12 kg·m²` and `dℓ/dβ ≈ 1 m/rad` gives `ω ≈ 41 rad/s`, about 30 steps per
period at `dt = 0.005`. Adequate for RK2 with the specified damping, but raising
`k_sheet` is the most likely cause of a blow-up. Mitigation, in order of
preference: keep `k_sheet ≤ 3e4`; raise `c_sheet`; sub-step the rigging DOF.
Do **not** silently reduce `dt` globally — record it.

**R2 — The boat may be too tender to sail.** brief §4 fixes the sailor amidships
with no hiking, so the maximum righting moment is ≈ 406 N·m. Close-hauled
sailing above ≈ 5 m/s true wind may be impossible without capsizing. This is
physically correct, not a bug. Mitigation: tune per-scenario wind speed
(`close_hauled` uses 3.5 m/s). If sailing proves impossible even at low wind, the
proposed escalation is a **static** `sailor_pos_b.y` scenario parameter,
defaulting to 0 — it adds no control input and no dynamics, so it stays inside
brief §4. **It requires human sign-off before implementation.** Do not add it
unilaterally.

**R3 — Sign-convention drift.** The highest-probability defect class. Mitigation:
F2 is normative; `frames.rs` is the only place rotations are written; the mirror
symmetry and rudder-direction tests run from section 04 onward.

**R4 — M1 scaffold survives to v1.** Section 02 ships a deliberately fake force
model. It is deleted by **task 4.5** and by nothing else. Task 4.5's acceptance
criteria include the grep that proves it is gone.

**R5 — deck.gl bundle size / WebGL context loss.** Mitigation: one `Deck`
instance, lazily created; Playwright asserts the canvas is present rather than
comparing pixels.

**R6 — Hull model has no planing regime.** Over-predicts resistance above
≈ 5 m/s. Accepted for v1 (brief §15), surfaced in the diagnostics panel.

**R7 — Golden regression files are build-sensitive.** Per F9 they are only valid
for the same platform and build. Store them with a recorded toolchain version;
a mismatch must produce a skip with a clear message, not a confusing failure.

---

## F12. Toolchain and gate commands

Defined once in `scripts/` and in `CLAUDE.md`; every task's acceptance criteria
call these, never ad-hoc command lines.

```
scripts/check.ps1  (and check.sh)   runs, in order and failing fast:
  1. cargo fmt --check
  2. cargo clippy --all-targets -- -D warnings
  3. cargo test -p sailgym-physics
  4. cargo test -p sailgym-physics --test invariants
  5. cargo test -p sailgym-physics --test regression
  6. wasm-pack build crates/sailgym-wasm --target web --out-dir ../../web/src/wasm
  7. pnpm --dir web typecheck
  8. pnpm --dir web test:unit
  9. pnpm --dir web test:e2e
```

Step 8 added 2026-09-20 by human approval; see `docs/v2/prds/01-boat-3d-svg.md`
D1. The Playwright run moved from step 8 to step 9; steps 1-7 are unchanged.

Stack, pinned by decision (brief §41 leaves it open):

- Rust stable, `wasm-pack`, `wasm-bindgen`, `serde` + `serde_json`
- Vite + React + TypeScript, `pnpm`
- `@deck.gl/core`, `@deck.gl/react`, `@deck.gl/layers`
- `zustand` for UI state (never for physics state)
- Playwright for E2E
- `vite-plugin-wasm` + `vite-plugin-top-level-await`

---

## F13. Agent working agreement

Read this before dispatching or executing any task.

1. **Read `00-foundations.md` and your section PRD in full before editing.**
2. **File ownership is exclusive.** Each task lists `Owns:`. A task may *read*
   any file; it may *write* only the files it owns. If you need a change in a
   file you do not own, stop and report it — do not edit it.
3. **Parallel groups.** Each task carries `P-group: <letter>` or `P-group: S`.
   Tasks sharing a letter within a section may run in parallel. Groups execute in
   alphabetical order. `S` means **solo, executed by the section agent itself** —
   these are the contract and integration tasks and must never be delegated or
   parallelised.
4. **Acceptance criteria are commands or numeric assertions.** A task is done
   when its named tests exist and pass. "Looks right" is never sufficient.
5. **Never tune a coefficient to make a scenario look better** (brief §43). If a
   coefficient must change, record the reason, source and assumption in the
   section handoff note and in the `parameters.rs` doc comment.
6. **Handoff note.** On completing a section, write
   `docs/v1/progress/NN-handoff.md` with: what landed, what deviated from the PRD
   and why, any risk from F11 that fired, any parameter changed and why, and
   anything the next section must know. Section N+1 reads N's handoff.
7. **Every section leaves the app runnable and `scripts/check` green.** No
   exceptions, no "will fix next section".
