# Web Sailing Simulator — Prototype Specification Brief

## 1. Purpose

Design and implement an interactive web-based 2D sailing simulator focused on physically meaningful sailboat dynamics rather than conventional game-style kinematics.

The prototype should model an ILCA/Laser-class dinghy sailing in a spatially varying wind field. The sailor controls only the rudder/tiller and mainsheet. Sail angle, boat acceleration, heel, yaw, and capsize behavior must emerge from the physical simulation.

The simulator serves three purposes:

1. Interactive sailing experimentation and learning.
2. Physics-model development, debugging, and validation.
3. A foundation for later headless/vectorized reinforcement-learning environments.

The prototype should prioritize a clean physics architecture, determinism, debuggability, and future extensibility over visual polish.

---

# 2. Prototype Scope

The prototype shall include:

- Rust physics core.
- Rust compiled to WebAssembly for browser execution.
- TypeScript/JavaScript web application.
- SVG-based sailboat and force/debug rendering.
- deck.gl/WebGL-based animated wind-field visualization inspired by Earth Nullschool.
- Interactive mouse + keyboard control.
- Spatially varying wind.
- No water-current model in this prototype.
- Surge, sway, yaw, and roll dynamics.
- Dynamic sail/boom motion.
- Physical mainsheet model.
- Rudder and centerboard hydrodynamics.
- Heel and capsize simulation.
- Scenario loading/resetting.
- Deterministic recording and replay.
- Physics/debug instrumentation.
- Automated physics-invariant tests.
- Browser integration/E2E testing with Playwright.

The simulator should run interactively in real time while allowing the physics engine to run independently and substantially faster than real time when rendering is disabled.

---

# 3. Reference Boat

Use an **ILCA 7 / Laser Standard-class dinghy with MkII-style rig** as the primary prototype reference.

Use publicly available ILCA/Laser dimensions and published data where practical.

Initial reference parameters should approximately use:

- Hull length: ~4.23 m.
- Waterline length: ~3.81 m.
- Beam: ~1.37 m.
- Hull/boat nominal mass: ~58 kg.
- Sail area: ~7.06 m².
- Sailor mass: 80 kg default.

All reference parameters must live in configurable data structures rather than being embedded throughout the physics implementation.

The underlying architecture should conceptually support:

```text
ILCA common hull / foils
+
rig configuration
+
sailor configuration
```

so ILCA 4 and ILCA 6 can later be added primarily through parameter changes.

Exact cross-validation against external simulators and experimental datasets is deferred.

---

# 4. Sailor Model

For the prototype, the sailor is modeled as a fixed mass located amidships.

Do not model:

- hiking;
- body motion;
- sailor lean;
- crew movement;
- active righting-moment control.

The only player-controlled sailing inputs are:

1. rudder/tiller;
2. mainsheet.

The sailor mass must nevertheless contribute correctly to:

- total mass;
- center of gravity;
- roll inertia;
- hydrostatic/righting behavior.

Future versions may make sailor position a dynamic/control variable.

---

# 5. Coordinate System and Core State

Use a consistent documented coordinate convention throughout the engine.

Recommended structure:

```text
World frame:
x, y

Boat/body frame:
+x = forward
+y = port or starboard, but choose one and use consistently

Angles:
ψ = yaw / heading
φ = roll / heel
β = boom angle relative to hull centerline
δr = rudder angle
```

Core physical state should include at least:

```text
x, y
ψ                  yaw / heading
φ                  roll / heel

u                  surge velocity
v                  sway velocity
r                  yaw rate
p                  roll rate

β                  boom/sail angle
β_dot              boom angular velocity

sheet-related state as required by rope model
rudder angle
```

Acceleration should generally be a derived quantity, not an independently integrated state.

---

# 6. Degrees of Freedom

Use a reduced **4-DOF marine dynamics model**:

- surge;
- sway;
- yaw;
- roll.

Do not simulate:

- heave;
- pitch;
- waves;
- full six-degree-of-freedom rigid-body motion.

The 4-DOF model should nevertheless permit:

- leeway;
- turning;
- tacking;
- gybing;
- heel;
- capsize.

---

# 7. High-Level Physics Architecture

The physics core should be decomposed into independent force/moment-producing modules.

Recommended conceptual structure:

```text
Simulation
├── BoatState
├── BoatParameters
├── Environment
│   └── WindField
│
├── Aerodynamics
│   └── SailModel
│
├── Hydrodynamics
│   ├── HullModel
│   ├── CenterboardModel
│   └── RudderModel
│
├── Rigging
│   ├── BoomDynamics
│   └── MainsheetModel
│
├── Stability
│   ├── Hydrostatics
│   └── RollDynamics
│
├── Integrator
└── Diagnostics
```

Each subsystem should compute forces/moments from state rather than directly manipulating unrelated state variables.

Conceptually:

```text
environment
    ↓
local fluid velocities
    ↓
component forces and moments
    ↓
sum generalized forces
    ↓
solve accelerations
    ↓
integrate state
```

Avoid game-style rules such as:

```text
if sail released:
    reduce heel
```

Instead, behavior must emerge from forces and moments.

---

# 8. Apparent Wind

Wind force calculations must use **apparent wind**, not true wind directly.

At minimum account for:

- world wind vector;
- boat translational velocity;
- boat yaw orientation;
- local velocity of the sail center of effort due to boat rotation;
- boom orientation.

The sail should therefore respond naturally when:

- stationary;
- accelerating;
- turning;
- tacking;
- gybing.

The exact displayed wind field and the physical wind sampled by the simulation must come from the same environment model.

---

# 9. Sail and Boom Dynamics

The sail angle is **not a player-commanded variable**.

Model boom + sail initially as one rigid rotational degree of freedom around the mast:

```text
β
β_dot
```

The boom's motion should result from:

- aerodynamic torque;
- mainsheet tension;
- damping/friction;
- mechanical angular limits if applicable.

Conceptually:

\[
I_b \ddot{\beta}
=
M_\mathrm{aero}
+
M_\mathrm{sheet}
+
M_\mathrm{damping}
+
M_\mathrm{limits}.
\]

The sail must be able to:

- swing freely when sheet tension disappears;
- cross the boat during tacks;
- accelerate violently during uncontrolled gybes;
- reach equilibrium under aerodynamic and sheet loads.

No explicit `portTack` / `starboardTack` sail state should be necessary for physics.

---

# 10. Sail Aerodynamics

Use an analytic, continuous full-angle aerodynamic model suitable for:

\[
-180^\circ \leq \alpha \leq 180^\circ.
\]

The model must cover:

- attached flow;
- stall;
- deep stall;
- reversed-side loading.

Do not rely only on thin-airfoil theory.

Use the standard structure:

\[
q = \frac12 \rho_\mathrm{air} V_\mathrm{AW}^2
\]

\[
L=qAC_L(\alpha)
\]

\[
D=qAC_D(\alpha).
\]

Include a simple finite-aspect-ratio induced-drag contribution.

Apply aerodynamic force at a configurable center of effort so that:

- yaw moment;
- roll/heeling moment;
- boom torque

are derived physically from force application geometry.

Use a simple heel correction to projected/effective sail behavior.

The initial coefficient model may be approximate but must be centralized and replaceable with empirical data later.

---

# 11. Mainsheet Mechanics

The mainsheet is a physical, **unilateral tension element**.

Fundamental requirement:

\[
T_\mathrm{sheet} \geq 0.
\]

The sheet may pull but may never push.

Use a geometric sheet model with:

- attachment point on boom;
- attachment/block point on boat;
- available sheet length;
- finite elasticity;
- damping.

A simple model may use:

\[
T =
\max\left(
0,
k e+c\dot e
\right)
\]

when the geometric rope path exceeds available sheet length.

Sheet tension should generate boom torque from geometry, rather than assigning boom angle directly.

The architecture should permit later modeling of:

- multiple blocks;
- mechanical advantage;
- block friction;
- ratchet blocks;
- cleats;
- hand force.

These need not be explicitly simulated in v1.

---

# 12. Sheet Player Control

For prototype v1, use **sheet payout/haul rate** as the primary player command.

Recommended semantics:

```text
mouse drag / vertical motion:
    haul in or ease mainsheet

release action:
    let sheet run freely / increase available length rapidly
```

The exact sensitivity may be tuned through playtesting.

The control should command:

\[
\dot L_\mathrm{sheet}
\]

rather than sail angle.

The displayed rope should visibly correspond to sheet state.

Initial UX may use mouse motion rather than requiring the player to physically grab a specific SVG rope segment.

Future versions can implement direct rope grabbing and hand-force simulation.

---

# 13. Rudder Player Control

Use rudder-angle-rate control rather than instantaneous absolute rudder changes.

Recommended keyboard mapping:

```text
A / Left Arrow:
    steer one direction

D / Right Arrow:
    steer opposite direction

released:
    rudder/tiller returns toward neutral at a configurable rate,
    or holds depending on whichever proves more intuitive during testing
```

The detailed feel is intentionally left tunable.

Physics must enforce:

- maximum rudder angle;
- maximum rudder rate.

The rudder must generate hydrodynamic force from local water-relative flow rather than directly changing yaw rate.

---

# 14. Centerboard and Rudder Hydrodynamics

Model centerboard and rudder as finite lifting surfaces in water.

For each:

\[
q_w=\frac12\rho_w V^2
\]

and use continuous:

\[
C_L(\alpha), \quad C_D(\alpha).
\]

Use each surface's local velocity, including the contribution from boat yaw:

\[
v_\mathrm{local}
=
v_\mathrm{CG}
+
\omega \times r.
\]

This should naturally produce:

- leeway resistance;
- centerboard side force;
- yaw damping;
- rudder authority increasing with boat speed;
- rudder stall at high angles;
- weak rudder authority when nearly stationary.

---

# 15. Hull Hydrodynamics

Initial prototype hull behavior may use a reduced empirical model rather than detailed CFD-derived resistance.

Include at minimum:

- nonlinear surge resistance;
- sway damping;
- yaw damping.

The implementation should be parameterized so later Laser-specific towing-tank/CFD data can replace these approximations.

Do not attempt full free-surface hydrodynamics in the prototype.

---

# 16. Roll and Hydrostatic Stability

Roll must be dynamic.

Do not use only a globally linear restoring spring.

Represent restoring moment through an approximate nonlinear righting-arm/righting-moment relation:

\[
M_\mathrm{restore}
=
-\Delta g\,GZ(\phi).
\]

The model should capture qualitatively:

- increasing restoring moment at small/moderate heel;
- peak righting moment;
- reduction at large heel;
- angle of vanishing stability;
- possible negative restoring moment after sufficient capsize.

Include:

- roll inertia;
- roll damping;
- sail-generated heeling moment;
- hydrodynamic contributions where appropriate.

The initial \(GZ\) curve may be approximate using public ILCA geometry and simplified transverse hull assumptions.

Exact hydrostatic validation is deferred.

---

# 17. Capsize Behavior

Capsize must arise from physical roll dynamics.

Do not immediately terminate when heel exceeds a threshold.

The simulation should support:

\[
|\phi| > 90^\circ
\]

so the boat can dynamically pass through a capsize.

Expose an informational state:

```text
capsized: bool
```

based on configurable heel magnitude and/or duration.

For v1, once significantly capsized:

- the simulation may continue using approximate roll physics;
- reset should remain available.

Do not model:

- sail immersion hydrodynamics;
- mast/sail water drag in detail;
- flooding;
- sailor falling out;
- righting procedure;
- full inverted-boat fluid dynamics.

These are deferred.

---

# 18. Wind Field

Ignore water current entirely in the prototype.

Implement a deterministic wind-field interface:

```text
wind(x, y, t, seed) -> Vec2
```

Support at least:

### Uniform mode

Constant wind vector.

### Spatial mode

Smooth deterministic spatially varying vector field.

### Optional temporal/gust component

Low-frequency evolving perturbations.

A suitable procedural representation may use a limited sum of Fourier-like modes or a compact coarse vector grid.

Requirements:

- deterministic from seed;
- inexpensive to sample;
- differentiable/smooth enough to avoid numerical artifacts;
- suitable for both physics and visualization.

The wind implementation should avoid allocating a huge dense field unless needed.

---

# 19. Wind Visualization

Use **deck.gl / WebGL** for the dense wind visualization.

Target visual inspiration:

- Earth Nullschool;
- animated vector-field particles/streamlines;
- optional arrow/grid overlay.

The visualization is not authoritative physics.

It must sample/display the same wind field used by the Rust simulation.

Rust should expose a batched wind-field sampling interface suitable for generating a visualization grid efficiently.

Do not perform thousands of individual JS→WASM wind queries per animation frame.

---

# 20. World

Use an effectively unbounded 2D plane.

No prototype support for:

- shorelines;
- rocks;
- navigation obstacles;
- collision geometry;
- wrapping world boundaries.

Scenario configuration may specify visualization bounds and initial placement.

World positions should remain numerically stable over prototype-scale runs.

---

# 21. Numerical Integration

Use a fixed physics timestep independent of rendering.

Recommended initial range:

\[
\Delta t = 0.005\text{–}0.01\ \mathrm{s}.
\]

Start with either:

- midpoint/RK2; or
- semi-implicit Euler if stable and demonstrably adequate.

RK2 is the preferred default for prototype implementation.

Do not couple physics to:

```text
requestAnimationFrame()
```

Rendering should interpolate or display the most recent physics state.

Provide higher-accuracy/reference integration capability only if useful for convergence tests.

---

# 22. Simulation Clock

Provide:

- pause;
- resume;
- reset;
- single physics-step;
- 0.25×;
- 1×;
- 2×;
- 4× simulation speed.

Rendering frequency must remain independent of physics timestep.

The headless engine should also support running as quickly as possible without wall-clock synchronization.

---

# 23. Rust / WASM Ownership Boundary

Rust owns:

- complete physical state;
- boat parameters;
- wind model and sampling;
- force calculations;
- mainsheet state;
- numerical integration;
- deterministic RNG where applicable;
- scenario initialization;
- diagnostic physical quantities;
- invariant/unit-testable physics functions.

TypeScript/JavaScript owns:

- UI;
- keyboard/mouse input;
- camera;
- SVG rendering;
- deck.gl visualization;
- graphs/debug panels;
- recording/export orchestration where practical;
- browser application state unrelated to physics.

Avoid duplicating physical equations in JavaScript.

---

# 24. WASM API Design

Design the boundary around coarse-grained calls.

Conceptually:

```text
create_sim(config)

reset(seed, scenario)

set_controls(control)

advance(num_steps)

get_snapshot()

sample_wind_grid(...)

get_diagnostics()
```

Avoid APIs requiring many calls per force component or entity.

For one interactive boat, prioritize clean API semantics over premature shared-memory optimization.

Batched arrays should be used for wind-field visualization and future expansion.

---

# 25. Boat Rendering

Render the boat primarily using SVG.

Top-down world view should show:

- hull outline;
- mast;
- boom;
- sail;
- centerboard indication if useful;
- rudder;
- mainsheet;
- trajectory;
- optional force vectors.

Geometry should be approximately physically scaled to the ILCA reference.

Do not attempt full 3D rendering.

---

# 26. Heel Visualization

Because top-down geometry alone cannot communicate roll well, provide a separate compact heel indicator.

Recommended:

```text
small stern/transverse-section view
+
numeric heel angle
```

The indicator should clearly show:

- upright;
- moderate heel;
- severe heel;
- inversion/capsize.

No full 3D scene is required.

---

# 27. Camera

Support:

### Follow mode

Boat remains near screen center while world moves.

### World / north-up mode

World orientation remains fixed.

Also support:

- pan;
- zoom;
- reset camera.

Camera controls must not interfere unnecessarily with mainsheet controls.

---

# 28. Interaction Design

Prototype inputs are mouse + keyboard.

Exact tuning is intentionally deferred to iterative playtesting.

Recommended starting mapping:

```text
A / D
or Left / Right:
    rudder rate

mouse vertical drag:
    sheet haul / ease

Space:
    rapidly ease / release mainsheet

R:
    reset scenario

P:
    pause

.:
    single-step while paused
```

Control rate, gain, direction, and dead zones must be configurable rather than hardcoded throughout the codebase.

---

# 29. User Modes

Provide two conceptual UI modes.

## Sail Mode

Minimal instrumentation:

- wind direction/speed;
- boat speed;
- heading;
- heel;
- rudder indication;
- sheet indication;
- capsize state.

Focus on interaction.

## Engineering / Debug Mode

Expose detailed physics diagnostics and visualization.

---

# 30. Debug Instrumentation

Debug mode should support toggling visualizations for:

- true wind;
- apparent wind;
- boat velocity;
- acceleration;
- sail aerodynamic force;
- centerboard force;
- rudder force;
- hull force;
- total force;
- yaw moment;
- heeling moment;
- righting moment;
- sail center of effort;
- centerboard/rudder centers;
- boom angular velocity;
- sheet tension;
- sail angle of attack;
- rudder angle of attack;
- relevant \(C_L/C_D\);
- yaw rate;
- roll rate.

Values should be inspectable numerically where practical.

This tooling is a core prototype feature, not optional polish.

---

# 31. Live Parameter Editing

Provide a collapsible engineering panel allowing selected physical/environment parameters to be adjusted.

Examples:

- wind magnitude/direction;
- wind-field variation amplitude;
- boat mass;
- sailor mass;
- sail area;
- drag/damping coefficients;
- sheet stiffness;
- sheet damping;
- maximum rudder angle;
- righting-moment parameters.

Provide:

```text
Reset to ILCA defaults
```

Changes that invalidate simulation continuity may require reset.

---

# 32. Scenario System

Represent scenarios as serializable structured configuration, ideally JSON-compatible.

A scenario defines:

```text
boat parameters / parameter overrides
initial boat state
wind configuration
random seed
camera suggestion
optional scripted initial control state
```

Ship with at least:

### `beam_reach_capsize`

Beam-on or near-beam wind, demonstrating oversheeting and capsize risk.

### `sheet_release_recovery`

Same setup, demonstrating reduced heeling after easing/releasing.

### `close_hauled`

Stable close-hauled sailing configuration.

### `tack`

Initial condition suitable for manually performing a tack.

### `gybe`

Initial condition suitable for controlled/uncontrolled gybe experiments.

### `free_sail`

Neutral general sandbox.

These scenarios should not script outcomes.

---

# 33. Recording and Replay

Build episode recording into the prototype architecture.

Record at a configurable logging rate rather than necessarily every physics substep.

Include enough information to reproduce/inspect:

```text
time
state
controls
wind at boat
forces/moments
sheet tension
reward placeholder if useful later
diagnostic values
seed
scenario/config version
```

Support:

- start/stop recording;
- replay;
- timeline scrubbing;
- reset to episode start.

A replay should primarily consume stored trajectory/state data rather than recomputing the physics.

Use a simple format initially, such as structured JSON or typed-array-backed binary data.

Design the format so it can later become the RL episode-inspection interface.

---

# 34. Determinism

Determinism is a hard requirement.

Given:

```text
same simulator build
same configuration
same initial state
same seed
same sequence of controls
same timestep
```

the simulator should reproduce the same trajectory to floating-point reproducibility expected within the same platform/build.

All procedural wind randomness must derive from an explicit seed.

Avoid hidden wall-clock dependence inside physics.

---

# 35. Physics Invariant Tests

Automated tests must include at least:

## Rest equilibrium

With:

```text
zero wind
zero initial velocity
neutral controls
```

the boat remains at rest.

## Port/starboard mirror symmetry

Mirroring initial conditions and inputs should produce an appropriately mirrored trajectory.

## Sheet unilateral constraint

\[
T_\mathrm{sheet} \geq 0
\]

always.

## Force sign sanity

Drag must oppose appropriate relative motion.

## Velocity scaling

Where expected:

\[
F \propto V^2
\]

approximately.

## Zero-flow foil behavior

Sail/centerboard/rudder lift should approach zero as relevant flow velocity approaches zero.

## Dissipative behavior

Without external energy input, drag/damping must not spontaneously increase total mechanical energy.

## Coordinate-frame consistency

Rotating the complete physical setup in world coordinates should rotate the resulting trajectory without changing intrinsic dynamics.

## Time-step convergence

Running the same scenario at:

```text
dt
dt/2
dt/4
```

should demonstrate numerical convergence over a defined test horizon.

## Deterministic replay

Identical seed/control sequences reproduce identical states.

## Finite-number invariant

No scenario should generate:

```text
NaN
Infinity
```

under valid parameter ranges.

---

# 36. Prototype Validation Philosophy

Exact cross-simulator and experimental validation is deferred.

For v1:

1. Use public ILCA/Laser specifications for basic parameters.
2. Use physically motivated coefficients and simplified models.
3. Enforce analytical and symmetry invariants.
4. Use convergence tests.
5. Inspect behavior interactively.
6. Keep every approximate coefficient isolated/configurable.

Do not claim quantitative ILCA performance accuracy yet.

Later validation may incorporate:

- towing-tank resistance data;
- published Laser VPPs;
- CFD;
- IMU/GNSS tack datasets;
- independent sailing simulators;
- instrumented real-boat experiments.

The prototype architecture must make those replacements/comparisons straightforward.

---

# 37. Performance Targets

For interactive browser use:

- target 60 fps rendering on a modern desktop browser;
- physics must remain independent of render rate;
- no visible input lag from WASM/UI architecture.

For headless browser physics:

- target at least approximately 100× real time for a single environment as an aspirational prototype benchmark;
- exact number is secondary to correctness and architecture.

The design must not preclude later:

- batched native Rust execution;
- JAX/Warp reimplementation;
- thousands of parallel RL environments.

Do not prematurely optimize single-boat rendering code for RL batching.

---

# 38. Browser Targets

Prototype target:

- current Chrome;
- current Edge;
- current Firefox;
- desktop;
- mouse + keyboard.

Mobile/touch support is out of scope unless trivial.

Safari compatibility is desirable but not a primary v1 acceptance criterion.

---

# 39. Renderer Technology

Use:

```text
SVG:
    boat
    boom/sail
    rudder
    rope
    trajectory/debug overlays where convenient

deck.gl / WebGL:
    dense wind-field visualization
    particle/streamline layers
    potentially long trajectory layers later
```

Do not represent the dense wind field with thousands of SVG DOM elements.

---

# 40. Code Quality and Module Boundaries

Rust physics should use:

- explicit strong types where useful;
- small modules;
- pure/testable force functions;
- centralized units/conventions;
- documented coordinate system;
- documented sign conventions;
- minimal mutable global state.

Avoid a monolithic simulation object containing all equations inline.

Suggested Rust layout:

```text
physics/
├── state.rs
├── parameters.rs
├── environment/
│   └── wind.rs
├── aero/
│   └── sail.rs
├── hydro/
│   ├── hull.rs
│   ├── centerboard.rs
│   └── rudder.rs
├── rigging/
│   ├── boom.rs
│   └── mainsheet.rs
├── stability/
│   └── roll.rs
├── integrator.rs
├── diagnostics.rs
├── scenario.rs
└── simulation.rs
```

Web layer may use a conventional modern TypeScript build system.

---

# 41. Dependency Philosophy

The physics is intentionally implemented from scratch.

Allowed third-party dependencies include:

- math/vector libraries;
- serialization;
- deterministic RNG;
- WASM binding tooling;
- deck.gl/WebGL;
- UI frameworks if useful;
- charting;
- test libraries;
- Playwright;
- development/build tooling.

Do not use a generic game/rigid-body/boat simulator as the actual sailing-physics engine.

External implementations may later be used as references.

---

# 42. Web Testing

Use **Playwright** for browser-level testing.

E2E tests should cover at least:

- application loads;
- WASM initializes;
- default scenario appears;
- keyboard rudder controls affect state;
- mouse sheet interaction affects sheet state;
- pause/resume works;
- reset restores deterministic initial state;
- scenario switching works;
- debug mode toggles;
- recording/replay basic workflow works;
- no obvious JS/WASM errors;
- simulator remains functional during accelerated simulation.

Where practical, deterministic browser tests should check numeric state rather than only pixels.

---

# 43. Developer / Agent Workflow

The project should be easy for coding agents to modify safely.

Provide:

- clear module responsibilities;
- explicit physical assumptions;
- parameter documentation;
- invariant tests;
- scenario regression tests;
- commands for formatting/linting/testing;
- browser E2E tests;
- small focused commits/changes.

When an agent changes physics, it should be possible to run:

```text
unit tests
→ invariant tests
→ deterministic scenario regression
→ WASM build
→ Playwright integration tests
```

before accepting the change.

Physics coefficients must not be silently tuned merely to make a visual scenario look better.

Document the reason/source/assumption behind meaningful coefficient changes.

---

# 44. Explicitly Deferred Features

Do not expand the initial PRD to require:

- water currents;
- waves;
- heave/pitch;
- full 6-DOF motion;
- sail cloth simulation;
- aeroelastic deformation;
- detailed mast bend;
- traveler/vang/cunningham controls;
- hiking/body movement;
- multiple crew;
- detailed block-and-tackle mechanics;
- hand-force simulation;
- shoreline/terrain;
- obstacle avoidance;
- collisions between boats;
- multiple boats;
- RL training;
- multiplayer;
- mobile controls;
- full CFD;
- full hydroelasticity;
- sail immersion after capsize;
- capsize recovery procedure;
- quantitative certification against real ILCA performance.

These should not complicate v1 architecture unnecessarily, although the design should avoid obvious dead ends for later additions.

---

# 45. Future Architecture Direction

The prototype should leave a clean path toward:

```text
Rust/WASM interactive simulator
        │
        ├── episode inspector
        │
        ├── native Rust headless simulator
        │
        ├── vectorized alternate physics backend
        │      └── JAX / Warp experiments
        │
        ├── Gymnasium/PettingZoo-style RL environment
        │
        ├── currents
        │
        ├── obstacles
        │
        └── multi-agent sailing
```

The physics API should therefore conceptually resemble:

```text
state_next = step(
    state,
    controls,
    environment,
    parameters,
    dt
)
```

rather than intertwining simulation with rendering/UI.

---

# 46. Primary Prototype Demonstration

A successful prototype should make the following demonstration possible:

1. Load the `beam_reach_capsize` scenario.
2. Wind field is visibly moving across the map.
3. Boat starts approximately beam-to-wind.
4. User hauls and holds the mainsheet.
5. Sheet tension restrains the boom.
6. Sail remains powered.
7. Aerodynamic side force creates increasing heel.
8. Boat approaches or enters capsize dynamically.
9. Reset.
10. Repeat scenario.
11. User eases/releases the sheet.
12. Sheet tension drops.
13. Aerodynamic torque lets the boom move outward.
14. Sail depowers/luffs.
15. Heeling moment falls.
16. Hydrostatic restoring moment brings the boat back toward upright.

No explicit capsize-prevention or "release causes recovery" rule may be used.

The behavior must emerge from the physical model.

A second primary demonstration should show:

1. Manual tack through the wind.
2. Sail unloads near head-to-wind.
3. Boom crosses naturally.
4. Sail fills on opposite side.

A third should show:

1. Downwind sailing.
2. User initiates gybe.
3. Aerodynamic torque reverses.
4. Boom accelerates across centerline.
5. Sheet tension shows a transient load increase.
6. Result differs visibly between controlled and uncontrolled sheet handling.

---

# 47. Prototype Success Criteria

The prototype is successful when:

- it is interactive and enjoyable enough to manually experiment with;
- sail angle emerges from physics rather than direct command;
- sheet release physically depowers the sail;
- tacking and gybing emerge without hard-coded maneuver states;
- roll responds plausibly to sail loading;
- capsize can occur dynamically;
- the same wind field powers visualization and physics;
- the simulator is deterministic;
- debug tooling makes force/moment behavior inspectable;
- invariant tests pass;
- rendering is cleanly separated from physics;
- the Rust core can run without rendering;
- architecture is suitable for later RL/vectorization work;
- the application and key interactions are covered by Playwright tests.

Quantitative real-world ILCA accuracy is explicitly **not** required for prototype completion.

---

# 48. Guidance to the PRD-Writing Agent

Expand this brief into a detailed implementation PRD.

The detailed PRD should add:

- user stories;
- functional requirements;
- subsystem interfaces;
- Rust/WASM API definitions;
- proposed TypeScript architecture;
- data models;
- state definitions;
- mathematical equations and sign conventions;
- scenario schemas;
- logging/replay schema;
- UI layout;
- interaction state machines;
- test plans;
- invariant definitions with tolerances;
- milestone breakdown;
- acceptance criteria;
- likely implementation risks;
- performance instrumentation;
- development commands/toolchain recommendations.

Do **not** substantially change the physical scope or architecture defined here unless an implementation contradiction is discovered.

Where numerical constants remain unspecified, the PRD should identify them as tunable parameters and recommend initial values rather than presenting poorly supported values as validated ILCA measurements.

The PRD should clearly distinguish:

```text
KNOWN / reference-derived
ASSUMED / physically motivated
TUNABLE
DEFERRED
```

for important physical parameters.

Prefer a vertically sliced implementation plan in which each milestone produces a runnable simulator rather than implementing all physics modules before any interactive integration.

A desirable progression is approximately:

```text
M0  repository/toolchain/WASM/Playwright skeleton
M1  deterministic boat kinematics + SVG + controls
M2  wind field + deck.gl visualization
M3  hull/centerboard/rudder dynamics
M4  dynamic sail/boom + apparent wind
M5  physical mainsheet
M6  roll/righting/capsize
M7  debug instrumentation + parameter panel
M8  scenarios + recording/replay
M9  invariant/convergence/performance hardening
```

At every milestone, retain a runnable browser application and passing automated test suite.