# Deferred features — v2 disposition of v1 brief §44

Updated 2026-09-20. This is the scope parking lot, not a second committed roadmap. The [v2 index](../README.md) defines delivery order and the [brief](../brief.md) defines proposed scope. “Promoted” means planned, not implemented. All 24 entries from v1 §44 are tracked below.

| v1 §44 item | v2 disposition | Revisit when |
|---|---|---|
| Water currents | Deferred | A named drift/navigation exercise needs them, with water-relative force semantics |
| Waves | Deferred | Flat-water tasks are validated and a specific wave interaction is measurable |
| Heave and pitch | Deferred | A wave/planing study justifies extra state and validation |
| Full 6-DOF | Deferred | Reduced-model limits demonstrably block a chosen task |
| Sail cloth simulation | Deferred | Sail deformation is the subject of a separate validated study |
| Aeroelastic deformation | Deferred | Coupling has data and a measurable learning benefit |
| Mast bend | Deferred | Rig calibration data and trim scope exist |
| Traveler/vang/cunningham | Deferred | Basic sheet/helm learning is complete and extra controls have a lesson |
| Hiking/body movement | Deferred | Crew position, moments and dynamics are explicitly modeled |
| Multiple crew | Deferred | A multi-crew boat/task is selected |
| Detailed block-and-tackle | Deferred | Effective rope path demonstrably fails the intended rigging exercise |
| Hand-force simulation | Deferred | Force limits and actuator dynamics have evidence; not a touch animation |
| Shoreline/terrain | Deferred | A selected navigation task needs geometry; no scenery requirement |
| Obstacle avoidance | Deferred | S6 selected with task geometry and sensing semantics; 4.5–4.6 are not default scope |
| Collisions between boats | Deferred | Explicit interaction model and fair task rules exist; ghosts never collide |
| Multiple boats | Later, non-interacting only proposed | Recorded ghost first; live fleet only with a concrete race/evaluation need |
| RL training | Deferred | 04–07 contracts, baselines and held-out evaluation work; environment plumbing is not training |
| Multiplayer | Deferred | Single-player learning loop is useful and networking cost is justified |
| Mobile controls | **Basic rate controls promoted to 09** | Advanced gestures remain below; no implicit promotion of crew or hand-force physics |
| Full CFD | Deferred | Separate research objective and resources, not product rendering |
| Hydroelasticity | Deferred | A validated flexible-body model is explicitly required |
| Sail immersion after capsize | Deferred | A post-capsize physical model is selected |
| Capsize recovery procedure | Deferred | Crew/righting interactions are modeled; 11 teaches pre-capsize heel recovery only |
| Quantitative certification against real ILCA behavior | Deferred | Independent measurements, calibration protocol and uncertainty bounds exist |

## Additional v2 ideas kept outside M-next

| Feature | Disposition / prerequisite |
|---|---|
| Rudder/sheet position-target tracks | Later shared adapter with explicit engaged/released semantics; zero rudder rate currently self-centres |
| Regripping, flick-to-dump, ratchet/cleat gestures | Later usability evidence; first release has a large explicit hold-to-release button |
| Tension-limited haul and rope slip | Separate physical model proposal, including validation; never approximate silently in TypeScript |
| Haptics | Optional later enhancement after touch usability; vibration patterns cannot command physical force/intensity |
| Tilt input | Later, and never presented as hiking before crew dynamics exist |
| Full 3-D world / photorealism | Outside product direction; retain current SVG rendering |
| Planing / high-speed hull model | Separate model extension if chosen tasks exceed the current model's useful range |
| Recorded ghost | First racing follow-on; visual trajectory, not an interacting fleet |
| Rule sailor → polar racer → planner | Deliver in that order; planner only after profiling and a measurable gain |
| Runtime sensor registry and sensor/action ablations | Research 05, sized to an actual study; one versioned layout first, no competing fixed and dynamic authorities |
| Conformance and JAX wind | Follow-on 02–03 after corrected baseline; useful verification, not prerequisite for learning UI |
| Full JAX/Warp physics and GPU training | After native/environment benchmarks and wind pilot; new PRD for full-step state, tolerances and invariants |
| PettingZoo, live fleet export, wind shadow | Separate scope after single-agent runtime; independent vector episodes do not need these |
| Cloud accounts, leaderboards, curriculum authoring system | No demonstrated need for the local three-challenge milestone |

Promotion requires a concrete task, measurable acceptance, dependencies and an explicit scope decision. Preserve this table when a feature moves; change its status and link the implementing PRD rather than silently deleting the exclusion.
