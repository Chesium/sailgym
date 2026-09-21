//! sailgym course geometry: routes, marks, guidance and ordered mark passage
//! (v2 `docs/v2/00-foundations.md` F15, section 04).
//!
//! Pure Rust with one dependency, `sailgym-physics`, taken for `Vec2` and
//! `BoatState` and nothing else. The arrow runs **`course → physics` and
//! never the other way** (F14.1): a reverse dependency would end the "builds
//! and tests on the host with plain `cargo test`" property that F8.1 exists
//! to protect, and `route::tests::physics_does_not_depend_on_the_course_crate`
//! asserts it by running `cargo tree`.
//!
//! ## What is here
//!
//! | module | what it owns | task |
//! |---|---|---|
//! | [`route`] | `Route`, `Mark`, `Rounding`, `Leg`, and validation | 4.1 |
//! | [`passage`] | the four-clause passage rule, and gates | 4.2 |
//! | [`guidance`] | `Guidance`, and the **one** definition of cross-track error | 4.3 |
//! | [`progress`] | `Progress`, and the tracker that walks a route | 4.4 |
//!
//! ## What is deliberately not here
//!
//! * **No physics.** No equation in this crate is a physical equation and no
//!   number in it is a physical coefficient (v1 brief §43, v2 F14.9). The
//!   crate never reads `parameters.rs`.
//! * **No forces, ever** (F15.4). If contact is ever scored it is a
//!   termination, computed by section 06, never a force.
//! * **No agent, no controller, no helm, no observation** — section 05. **No
//!   episode, no termination, no `Outcome`** — section 06. This crate says
//!   where the boat has got to, not what to do about it and not what it
//!   scores.
//! * **No obstacles and no ray casting.** `docs/v2/brief.md` S6 is deferred
//!   and `docs/v2/README.md` V-F excludes tasks 4.5–4.6 from default
//!   delivery, so they are **absent** rather than present behind a disabled
//!   flag (RV23, section acceptance 6).
//!
//! ## The passage rule, stated once
//!
//! A mark is passed at the first step at which **all four** hold: the
//! previous mark is already passed; the boat has crossed the plane through
//! the mark perpendicular to the incoming leg; it crossed on the side
//! `rounding` requires; and it crossed in the direction the leg runs.
//!
//! **A radius check is not sufficient and is forbidden** (F15.3). It permits
//! cutting the corner, and a policy will learn to; [`passage`] carries the
//! named regression that says so.

// One `pub mod` line per module, added by the task that owns the file: this
// file is task 4.1's and Rust has no way for a later task to declare its own
// module without touching it. Recorded in `docs/v2/progress/04-handoff.md`
// rather than quietly absorbed (F13.2).
pub mod guidance;
pub mod passage;
pub mod progress;
pub mod route;

pub use guidance::{CourseParams, Guidance};
pub use passage::passed;
pub use progress::{Progress, Tracker};
pub use route::{Leg, Mark, Rounding, Route, RouteError};

/// Re-exported so a caller holding a course needs no physics import for the
/// one type the two share.
pub use sailgym_physics::vec::Vec2;
