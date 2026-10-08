//! The shipped controllers (v2 section 12, D6).
//!
//! One so far: [`rule_sailor::RuleSailor`], an easy-to-read baseline that
//! sails a waypoint course. It lives here rather than in a crate of its own
//! so that gate step 3's crate list does not change (F12′ keeps five entries
//! and adds none).
//!
//! # What a controller in this module may read
//!
//! The concatenated observation vector, by column **name**, resolved once at
//! [`Agent::reset`](crate::spec::Agent::reset) — and nothing else. There is no
//! path from here to a `WindField`, a `Route` or another boat's state, because
//! `decide` takes `&[f64]` (F14.4). A controller here also may not read a
//! **privileged** column: `guidance.leg_bearing_vs_wind` is derived from the
//! true wind, and a baseline that read it would be measuring something no
//! sailor can see (RV67).
//!
//! # And what its numbers are
//!
//! Gains, thresholds and tables, every one an **F14.9 tunable**: brief §43
//! governs `parameters.rs` and does not govern a controller gain. The crate
//! boundary is the distinction, which is why the boundary is worth having. No
//! number in this module is an F7 coefficient and
//! `tests/rule_sailor.rs::no_f7_literal_appears_in_the_pilot_module` says so
//! on every gate run.
//!
//! Durations are counted in **decisions**, never in seconds. An agent sees no
//! clock and no time column, and `dt` is an F7 value it may not carry; the
//! decision counter is the one monotone quantity it owns (F9.1, F14.6).

pub mod rule_sailor;

pub use rule_sailor::{Mode, RuleSailor, Tunables};
