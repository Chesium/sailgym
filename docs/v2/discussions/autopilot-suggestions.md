**Updated 2026-09-20:** deliver sections 08–11's human learning loop first. Then build a recorded ghost, one short course and a rule sailor; follow with a polar racer and only later a planner if profiling and evaluation justify it. The original baseline design below remains research context.

Compare completion, time, capsizes, missed marks, failed manoeuvres and control effort on held-out seeds/courses with matching model/task/action contracts. Existing 04–07 are infrastructure drafts, not the browser course or rule-sailor integration. Recorded ghosts require no fleet or collisions; [Vibesail's collision change](https://vibesail.com/blog/changelog-073-race-collisions-removed/) is a useful precedent.

Use shared Rust control semantics. Do not cancel self-centering with tiny epsilon commands or per-frame parameter toggles. Position tracking needs an explicit engaged/released contract.

The project already has a good foundation: deterministic Rust stepping, seeded wind, native execution, and episode recording. What’s missing is the race layer: courses, mark rounding, finish conditions, multiple boats, and controller interfaces.

A few implementation details strongly affect the design:

- [Controls](../../../crates/sailgym-physics/src/state.rs) command **rudder rate, sheet rate, and sheet release**. They do not set heading, rudder angle, or boom angle.
- [Rudder dynamics](../../../crates/sailgym-physics/src/dynamics.rs) automatically center the rudder when its command is zero. A conventional angle controller needs an actuator adapter that accounts for this.
- The boom, heel, and leeway are dynamic. Good steering alone will not make a competent sailor.
- Existing golden control scripts are useful regression fixtures, but they are open-loop sequences rather than autopilots.

**The three baselines I suggest:**

| Baseline | Behavior | Game role | RL comparison |
|---|---|---|---|
| **Rule sailor** | Wind-aware course following, fixed trim table, explicit tack/gybe controller, heel protection | Reliable beginner/intermediate NPC | Interpretable minimum standard |
| **Polar racer** | Measured speed/trim tables, layline planning, maneuver costs, leeway compensation | Main competitive NPC | Strong inexpensive benchmark |
| **Rollout planner** | Simulates candidate maneuver/trim plans and executes the best first action | Optional expert NPC, after profiling | Model-based reference |

**1. Make the rule sailor complete enough to finish races.**

Use a small hierarchy:

`course target → sailing maneuver → heading/trim targets → Controls`

For navigation, follow a line or a look-ahead point on the current leg. When the desired direction enters the upwind no-go zone, select a close-hauled course on one tack. Retain that tack until a corridor boundary or layline warrants switching; add hysteresis and a minimum commitment time so gusts do not trigger repeated tacks.

Jaulin and Le Bars’ [line-following controller](https://webperso.ensta.fr/jaulin/paper_jaulin_irsc12.pdf) (see docs/v1/autopilots/paper_jaulin_irsc12.pdf) is a useful starting reference: it combines cross-track correction, wind feasibility, and tack memory. Its actuator outputs require adaptation to your rate-controlled simulator.

Below navigation, implement:

- **Steering:** wrapped heading error plus yaw-rate damping, with limited rudder demand. Compensate for leeway when tracking a course; course-over-ground becomes unreliable near zero speed.
- **Trim:** an initial apparent-wind-angle-to-sheet-length table, tracked through sheet-rate commands.
- **Heel protection:** ease based on both heel and outward roll rate, with hysteresis before trimming back in.
- **Maneuvers:** prepare → turn → settle states for tacking and gybing, with a timeout and stalled-boat recovery.

Keep maneuver states in the **autopilot**, outside the physics crate. They represent the sailor’s intent; the actual boom crossing must still emerge from the physics.

**2. Build the polar racer from measurements of this simulator.**

A polar is a table of attainable boat speed versus wind speed and sailing angle. Generate yours headlessly, sweeping heading and sheet settings while measuring settled speed, track direction, heel, and stability on both tacks.

There is already useful evidence in [the roll milestone measurements](../../../docs/v1/progress/07-handoff.md): in its close-hauled experiment, a 2.0 m sheet setting produced approximately 1.78 m/s, versus 0.40 m/s fully hauled in. “Pull the sheet tight upwind” would be a particularly weak heuristic here.

Use these tables to:

- Choose upwind and downwind angles that maximize progress along the leg.
- Estimate arrival time for direct and two-tack routes.
- Include measured time/speed losses from tacks and gybes.
- Plan mark approaches and exits, rather than steering directly at the mark center.

This is my preferred production NPC. Difficulty can come from reaction delay, trim accuracy, planning frequency, and conservative maneuver choices. Use seeded, slowly varying errors so weaker NPCs remain believable.

**3. Add rollout planning only after those controllers work.**

Evaluate a small set of meaningful candidates—continue, tack, bear away, change trim—using short forward simulations. Score legal course progress, heel risk, maneuver cost, and control effort.

A horizon that cannot see the benefit after a tack will discourage necessary tacks, so include an estimated remaining-course cost. Treat access to exact physics or future wind as a separately labeled **privileged benchmark**.

**For implementation, I’d add two Rust crates:** `sailgym-autopilot` for controllers and `sailgym-race` for course progression, fleet stepping, and scoring. Both can serve native evaluation and WASM gameplay.

Start with independent boats sharing the same wind configuration, seed, and simulation clock. That supports ghost races immediately. Physical collisions, avoidance, and racing right-of-way need additional work before close fleet racing.

Define mark passage carefully: ordered marks, required rounding side, and directed gate crossings. A simple “within radius” check allows corner-cutting.

**For future RL, freeze the comparison contract early:**

- **Timing:** try 20 Hz policy decisions over the existing 200 Hz physics; schedule by simulation steps, independent of rendering.
- **Actions:** use the same rate commands and release action. If you expose higher-level heading/trim targets, compare every policy through the same low-level controller.
- **Observations:** local wind, motion, heel/roll, boom state, actuator state, and course-relative geometry. Restrict hidden forces and future wind to explicitly privileged experiments.
- **Evaluation:** held-out wind seeds and courses; report finish rate, completion time, capsizes, missed marks, and maneuver failures. Reward alone is insufficient.
- **Episodes:** log every policy action and its step index alongside controller version, course, seed, and physics parameters. The current sampled recorder is useful for inspection but may miss intervening actions.
- **Environment API:** follow Gymnasium’s distinction between task termination and time-limit truncation. [API reference](https://gymnasium.farama.org/api/env/)

The documented stability-curve and mainsheet-preload issues in [parameter provenance](../../../docs/v1/parameters.md) also matter: freeze and version the physics used for comparisons, because stronger policies may exploit those behaviors.

**My first milestone would be:** one rule sailor reliably completing a windward–leeward course from varied starting headings, followed by polar generation and the stronger racer. That delivers a usable NPC while establishing the evaluation machinery RL will need.

This was a source/documentation review; I haven’t changed files or experimentally validated controller performance.
