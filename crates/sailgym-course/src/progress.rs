//! `Progress`, and the tracker that walks a boat around a route (section 04
//! task 4.4).
//!
//! # Scoring stops here
//!
//! Progress and elapsed time, and nothing else. Finish rate, capsize counts
//! and control effort are evaluation concerns and belong to section 06; an
//! outcome — succeeded, failed, timed out — belongs to `sailgym-task`, which
//! already defines it (v2 F18.4a), and this crate must not grow a second set
//! of completion semantics.
//!
//! # The tracker is a fold, not a state machine
//!
//! [`Tracker`] holds exactly three things that change: the leg index, the
//! previous position, and the passages it has seen. Every decision it makes
//! comes from [`passage::advance`], which is the only thing in this crate that
//! moves a leg index and which moves it by at most one. There is no
//! hysteresis, no timer and no "nearly", so a tracker driven twice over the
//! same track produces the same answer both times, and a tracker driven one
//! sample at a time produces the same answer as one driven every other
//! sample — as far as the samples allow, which
//! `replay.rs` measures rather than assumes.

use sailgym_physics::state::BoatState;
use sailgym_physics::vec::Vec2;
use serde::Serialize;

use crate::passage;
use crate::route::{position, Route, RouteError};

/// Where the boat has got to.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Progress {
    /// The leg being sailed, `0 .. Route::legs()`. Equal to `Route::legs()`
    /// when the course is finished.
    pub leg_index: u32,
    /// Completed laps: `leg_index / marks.len()`.
    pub laps_done: u32,
    /// Metres from the boat to the current leg's mark, or `0.0` once the
    /// course is finished — ask [`Progress::finished`] rather than reading a
    /// meaning into the zero.
    pub distance_to_next: f64,
    /// Simulated seconds since the tracker was started.
    pub elapsed: f64,
}

impl Progress {
    /// Whether the course is finished.
    pub fn finished(&self, route: &Route) -> bool {
        route.is_finished(self.leg_index)
    }
}

/// One mark passed, and when.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Passage {
    /// The leg that was completed.
    pub leg_index: u32,
    /// The mark it ended at, as an index into [`Route::marks`]. Distinct from
    /// `leg_index` as soon as `laps > 1` (RV24).
    pub mark_index: usize,
    /// Simulated time of the **step that completed it**, in seconds. The step
    /// is the authority: a passage happens between two states, and this is
    /// the later one's `t`.
    pub t: f64,
}

/// Walks a boat around a route.
#[derive(Clone, Debug)]
pub struct Tracker {
    route: Route,
    leg_index: u32,
    t0: f64,
    prev: Vec2,
    passages: Vec<Passage>,
}

impl Tracker {
    /// Start tracking from an initial state, after validating the route.
    ///
    /// The initial state is the first "previous position", so a mark the boat
    /// is already past at `t = 0` is not passed by starting.
    pub fn start(route: Route, initial: &BoatState) -> Result<Self, RouteError> {
        route.validate()?;
        Ok(Self {
            route,
            leg_index: 0,
            t0: initial.t,
            prev: position(initial),
            passages: Vec::new(),
        })
    }

    /// Feed one step. Returns the passage it completed, if any.
    ///
    /// At most one mark per step: `passage::advance` moves the index by one
    /// or not at all. A step long enough to cross **two** marks' planes
    /// records the first and does not pass the second — not then, and not
    /// afterwards either, because the plane test is a *crossing* and the boat
    /// is already on the far side of it. That is a property of the sampling
    /// rate rather than of the rule, and it is the reason to drive this at
    /// the physics step rate; `replay.rs` states the rate it uses and why
    /// that rate is enough for the tracks it drives.
    pub fn observe(&mut self, st: &BoatState) -> Option<Passage> {
        let cur = position(st);
        let before = self.leg_index;
        // `advance` needs two states; the tracker keeps the previous position
        // rather than the whole state because nothing in the rule reads a
        // velocity, a heading or a heel angle.
        let prev_state = BoatState {
            x: self.prev.x,
            y: self.prev.y,
            ..BoatState::ZERO
        };
        let cur_state = BoatState {
            x: cur.x,
            y: cur.y,
            ..BoatState::ZERO
        };
        self.leg_index = passage::advance(&self.route, before, &prev_state, &cur_state);
        self.prev = cur;
        if self.leg_index == before {
            return None;
        }
        let mark_index = self
            .route
            .leg(before)
            .expect("a leg that was just passed exists")
            .mark_index;
        let p = Passage {
            leg_index: before,
            mark_index,
            t: st.t,
        };
        self.passages.push(p);
        Some(p)
    }

    /// The current progress, for a boat at `st`.
    pub fn progress(&self, st: &BoatState) -> Progress {
        let n = self.route.marks.len().max(1) as u32;
        let distance_to_next = match self.route.leg(self.leg_index) {
            Some(leg) => (leg.to - position(st)).length(),
            None => 0.0,
        };
        Progress {
            leg_index: self.leg_index,
            laps_done: self.leg_index / n,
            distance_to_next,
            elapsed: st.t - self.t0,
        }
    }

    /// The leg being sailed.
    pub fn leg_index(&self) -> u32 {
        self.leg_index
    }

    /// Whether the course is finished.
    pub fn finished(&self) -> bool {
        self.route.is_finished(self.leg_index)
    }

    /// Every passage so far, in order.
    pub fn passages(&self) -> &[Passage] {
        &self.passages
    }

    /// The route being sailed.
    pub fn route(&self) -> &Route {
        &self.route
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::{Mark, Rounding};

    fn at(x: f64, y: f64) -> Vec2 {
        Vec2::new(x, y)
    }

    fn state(p: Vec2, t: f64) -> BoatState {
        BoatState {
            x: p.x,
            y: p.y,
            t,
            ..BoatState::ZERO
        }
    }

    /// A 20 m square, every mark left to port, sailed anticlockwise.
    fn square() -> Route {
        Route::new(
            [at(20.0, 0.0), at(20.0, 20.0), at(0.0, 20.0), at(0.0, 0.0)]
                .into_iter()
                .map(|position| Mark {
                    position,
                    radius: 1.0,
                    rounding: Rounding::Port,
                })
                .collect(),
            1,
        )
    }

    /// The same square with no side required, for the cases that are about
    /// the ordering rather than about the side.
    fn either_square() -> Route {
        let mut r = square();
        for m in r.marks.iter_mut() {
            m.rounding = Rounding::Either;
        }
        r
    }

    /// A lap **outside** the marks, 4 m clear of each — which is what leaving
    /// every mark to port means when the course is sailed anticlockwise:
    /// heading east, port is north, so the mark must be north of the boat.
    fn lap_track() -> Vec<Vec2> {
        let corners = [
            at(-4.0, -4.0),
            at(24.0, -4.0),
            at(24.0, 24.0),
            at(-4.0, 24.0),
        ];
        let mut out = vec![corners[0]];
        for i in 0..4 {
            let a = corners[i];
            let b = corners[(i + 1) % 4];
            let steps = 14;
            for s in 1..=steps {
                out.push(a + (b - a) * (s as f64 / steps as f64));
            }
        }
        out
    }

    fn drive(route: Route, track: &[Vec2]) -> Tracker {
        let mut tr = Tracker::start(route, &state(track[0], 0.0)).expect("a valid route");
        for (i, p) in track.iter().enumerate().skip(1) {
            tr.observe(&state(*p, i as f64 * 0.25));
        }
        tr
    }

    #[test]
    fn a_lap_of_the_square_passes_every_mark_in_order() {
        let tr = drive(square(), &lap_track());
        let legs: Vec<u32> = tr.passages().iter().map(|p| p.leg_index).collect();
        let marks: Vec<usize> = tr.passages().iter().map(|p| p.mark_index).collect();
        assert_eq!(legs, vec![0, 1, 2, 3]);
        assert_eq!(marks, vec![0, 1, 2, 3]);
        assert!(tr.finished());
        assert_eq!(tr.leg_index(), 4);
    }

    /// RV24, driven rather than declared. `route.rs` states the same property
    /// at the representation.
    #[test]
    fn laps_repeat_the_marks_rather_than_duplicating_them() {
        let mut route = square();
        route.laps = 3;
        assert_eq!(route.marks.len(), 4, "laps must never duplicate marks");
        assert_eq!(route.legs(), 12);

        // Three laps of the same track.
        let one = lap_track();
        let mut track = one.clone();
        for _ in 0..2 {
            track.extend(one.iter().skip(1).copied());
        }
        let tr = drive(route, &track);
        assert_eq!(tr.passages().len(), 12);
        // Twelve legs, four marks, each mark passed three times.
        let legs: Vec<u32> = tr.passages().iter().map(|p| p.leg_index).collect();
        assert_eq!(legs, (0..12).collect::<Vec<u32>>());
        let marks: Vec<usize> = tr.passages().iter().map(|p| p.mark_index).collect();
        assert_eq!(marks, vec![0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3]);
        assert_eq!(tr.route().marks.len(), 4);
        assert!(tr.finished());
    }

    #[test]
    fn progress_counts_laps_and_measures_the_distance_to_the_next_mark() {
        let mut route = square();
        route.laps = 2;
        let mut tr = Tracker::start(route, &state(at(-4.0, -4.0), 7.0)).expect("valid");
        let p = tr.progress(&state(at(-4.0, -4.0), 7.0));
        assert_eq!(p.leg_index, 0);
        assert_eq!(p.laps_done, 0);
        assert_eq!(p.elapsed, 0.0, "elapsed is measured from the start");
        // The boat is at (−4, −4) and mark 0 is at (20, 0).
        assert!((p.distance_to_next - (24.0f64 * 24.0 + 16.0).sqrt()).abs() < 1e-12);

        // Sail the first lap: four legs, one lap done.
        for (i, q) in lap_track().iter().enumerate().skip(1) {
            tr.observe(&state(*q, 7.0 + i as f64 * 0.25));
        }
        let last = state(at(-4.0, -4.0), 20.0);
        let p = tr.progress(&last);
        assert_eq!(p.leg_index, 4);
        assert_eq!(p.laps_done, 1);
        assert_eq!(p.elapsed, 13.0);
        assert!(!p.finished(tr.route()));
    }

    #[test]
    fn a_finished_course_reports_zero_distance_and_says_it_is_finished() {
        let tr = drive(square(), &lap_track());
        let p = tr.progress(&state(at(-4.0, -4.0), 12.0));
        assert!(p.finished(tr.route()));
        assert_eq!(p.distance_to_next, 0.0);
        assert_eq!(p.leg_index, 4);
        assert_eq!(p.laps_done, 1);
    }

    #[test]
    fn the_tracker_refuses_an_invalid_route() {
        let mut route = square();
        route.marks[2].position = route.marks[1].position;
        assert_eq!(
            Tracker::start(route, &state(at(0.0, 0.0), 0.0)).unwrap_err(),
            crate::route::RouteError::ZeroLengthLeg { mark: 2 }
        );
    }

    #[test]
    fn a_mark_the_boat_is_already_past_is_not_passed_by_starting() {
        // The tracker's first "previous position" is the initial state, so
        // starting is not a step and cannot complete a leg — and a boat that
        // begins beyond mark 0's plane stays on leg 0, because the plane test
        // is a crossing.
        let mut tr = Tracker::start(square(), &state(at(21.0, -4.0), 0.0)).expect("valid");
        assert_eq!(tr.leg_index(), 0);
        assert!(tr.passages().is_empty());
        assert!(tr.observe(&state(at(25.0, -4.0), 1.0)).is_none());
        assert_eq!(tr.leg_index(), 0);
    }

    #[test]
    fn a_step_that_crosses_two_planes_passes_one_mark_and_misses_the_other() {
        // The sampling-rate property, stated as a test rather than as a
        // comment. One step from south-west of mark 0 to north-east of
        // mark 1 crosses both planes; the tracker takes mark 0 and mark 1 is
        // gone, because the boat is already past its plane on every later
        // step.
        let mut tr = Tracker::start(either_square(), &state(at(18.0, -4.0), 0.0)).expect("valid");
        let p = tr.observe(&state(at(26.0, 24.0), 1.0)).expect("mark 0");
        assert_eq!(p.mark_index, 0);
        assert_eq!(tr.leg_index(), 1);
        for i in 2..8 {
            assert!(tr
                .observe(&state(at(26.0, 24.0 + i as f64), i as f64))
                .is_none());
        }
        assert_eq!(
            tr.leg_index(),
            1,
            "mark 1's plane was crossed inside a step"
        );
    }

    #[test]
    fn the_passage_time_is_the_later_states_own_t() {
        let mut tr = Tracker::start(square(), &state(at(16.0, -4.0), 3.5)).expect("valid");
        let p = tr
            .observe(&state(at(24.0, -4.0), 4.25))
            .expect("passed mark 0");
        assert_eq!(p.t, 4.25);
        assert_eq!(p.leg_index, 0);
        assert_eq!(p.mark_index, 0);
    }
}
