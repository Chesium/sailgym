//! sailgym agent interface: sensors, actions, cadence and the contract every
//! controller plugs into (v2 `docs/v2/00-foundations.md` F14, section 05).
//!
//! Pure Rust with two dependencies, `sailgym-physics` and `sailgym-course`,
//! taken for state, parameters, the RNG, the F6.2 apparent wind, the F6.8 rope
//! geometry and `Guidance`. The arrows run **`agent → {course, physics}` and
//! never the other way** (F14.1): a reverse dependency would end the "builds
//! and tests on the host with plain `cargo test`" property that F8.1 exists to
//! protect, and `spec::tests::physics_does_not_depend_on_the_agent_crate`
//! asserts it by running `cargo tree`.
//!
//! ## What is here
//!
//! | module | what it owns | task |
//! |---|---|---|
//! | [`spec`] | `Action`, `ActionSpace`, `AgentSpec`, `Cadence`, the `Agent` trait | 5.1 |
//! | [`worldview`] | `WorldView` — the F14.4 containment boundary | 5.2 |
//! | [`sensor`] | the `Sensor` trait, the tier-0 suite and the registry | 5.2, 5.3 |
//! | [`observation`] | the ordered concatenation, the layout, `obs_digest` | 5.4 |
//! | [`actuation`] | the `[−1, 1]^k` funnel and the `rate` adapter | 5.5 |
//! | [`manual`] | the external action source | 5.7 |
//!
//! ## The one structural fact
//!
//! A [`Sensor`](sensor::Sensor) receives a [`WorldView`](worldview::WorldView)
//! and sees everything in it. An [`Agent`](spec::Agent) receives `&[f64]` — the
//! concatenated observation vector — and nothing else. There is no path from an
//! `Agent` to a `WindField`, a `Route` or another boat's state, and that is a
//! property of the signatures rather than of anyone's discipline (F14.4).
//!
//! ## What is deliberately not here
//!
//! * **No physics.** No equation in this crate is an equation of motion and no
//!   number in it is a physical coefficient (v1 brief §43, v2 F14.9).
//!   `parameters.rs` is read for geometry and limits and is never written.
//! * **No autopilot.** [`manual`] and a test stub exercise the shared
//!   actuation path; the rule sailor, the polar racer and the polar tables are
//!   later sections.
//! * **No episode, no `Outcome`, no `VecEnv`** — section 06. This crate says
//!   what an agent sees and what it emits, not when an episode ends.
//! * **No `Helm` and no `Setpoint` action.** Task 5.6 is deferred until an
//!   engaged/released contract and a concrete consumer are selected, so no
//!   placeholder ships; [`spec::ActionSpace::Setpoint`] is a reserved name in
//!   the logged vocabulary that [`spec::AgentSpec::validate`] refuses.
//! * **No noise, bias, latency or dropout models.** The per-sensor substream
//!   scheme of F14.8 is built ([`sensor::sensor_stream`]) and every sensor
//!   already takes a `&mut Pcg32`; the models themselves are the sensor-quality
//!   ablation's.
//! * **No `true_wind` sensor**, deliberately — see [`sensor::wind`].
//! * **No WASM surface.** F8.2 enumerates the entire WASM API and no task in
//!   this section owns `crates/sailgym-wasm`.
//!
//! ## The three determinism traps, stated once
//!
//! 1. **The observation never reads the force cache.** `Simulation::forces` is
//!    refreshed once per `advance(n)` call, so an observation built on it would
//!    depend on how the caller chunked its calls and F9.7 would break in the
//!    one place nobody tests. [`observation::observe`] is pure in its
//!    arguments (F14.7) and [`sensor::imu`] calls `forces::evaluate` itself at
//!    the decision instant.
//! 2. **Cadence keys off the episode step counter** (F14.6), never off a
//!    per-`advance` counter and never off elapsed time.
//! 3. **Agent randomness goes through `Pcg32::stream(STREAM_AGENT)`**, with
//!    per-sensor substreams below it (F14.8).

// One `pub mod` line per module, added by the task that owns the file: Rust
// has no way for a later task to declare its own module without touching the
// crate root, which is task 5.1's. Recorded in
// `docs/v2/progress/05-handoff.md` rather than quietly absorbed (F13.2), as
// section 04 recorded the same gap.
pub mod actuation;
pub mod manual;
pub mod observation;
pub mod sensor;
pub mod spec;
pub mod worldview;

pub use actuation::{rate::Rate, ActionError, Actuation};
pub use manual::Manual;
pub use observation::{obs_digest, observe, ObsColumn, ObsLayout};
pub use sensor::{Sensor, SensorRegistry};
pub use spec::{Action, ActionSpace, ActionVec, Agent, AgentDebug, AgentSpec, Cadence};
pub use worldview::WorldView;
