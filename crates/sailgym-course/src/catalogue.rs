//! The shipped waypoint courses, embedded from `courses/` (v2 section 12,
//! task 12.1).
//!
//! Embedded with `include_str!` exactly as `sailgym_physics::scenario`
//! embeds `scenarios/`: the browser and the native headless runner get the
//! same documents without a file system, and there is one copy of each in the
//! repository.
//!
//! # A course is geometry and a scenario name
//!
//! A [`Course`] says where the start and the waypoints are, how wide each
//! gate is, and which shipped scenario it is sailed in. It holds **no**
//! physical coefficient and no task threshold: the conditions are the
//! scenario's, and a time limit belongs to the practice task that scores the
//! course (`sailgym-task`, v2 F18.4). The scenario is named, not loaded — this
//! crate takes `sailgym-physics` for `Vec2` and `BoatState` only — and
//! `tests/waypoints.rs` checks that every name resolves and that every
//! course starts where its scenario puts the boat.
//!
//! # `CourseId` is `Copy`
//!
//! So a `TaskSpec` can carry one and stay `Copy`, as section 11's are.

use serde::{Deserialize, Serialize};

use crate::route::{vec2_serde, Route, RouteError};
use sailgym_physics::vec::Vec2;

/// The only course schema this build understands.
pub const COURSE_SCHEMA_VERSION: u32 = 1;

/// A shipped course.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CourseId {
    Reach,
    Triangle,
    WindwardLeeward,
}

impl CourseId {
    /// Every shipped course, in the order a page offers them.
    pub const ALL: [CourseId; 3] = [Self::Reach, Self::Triangle, Self::WindwardLeeward];

    /// The id used in course files, task ids and logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reach => "reach",
            Self::Triangle => "triangle",
            Self::WindwardLeeward => "windward_leeward",
        }
    }

    /// The inverse of [`CourseId::as_str`].
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.as_str() == s)
    }

    /// The embedded course document.
    pub fn source(self) -> &'static str {
        match self {
            Self::Reach => include_str!("../../../courses/reach.json"),
            Self::Triangle => include_str!("../../../courses/triangle.json"),
            Self::WindwardLeeward => include_str!("../../../courses/windward_leeward.json"),
        }
    }

    /// Load the shipped course, checking that the document is the course it
    /// is filed as.
    pub fn load(self) -> Result<Course, CourseError> {
        Course::load_as(self, self.source())
    }
}

/// One course document, as written in `courses/*.json`.
///
/// `deny_unknown_fields`, because a course file is hand-written data, and a
/// misspelt key that silently fell back to nothing would move a waypoint
/// without anyone noticing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Course {
    pub schema_version: u32,
    pub id: CourseId,
    /// For a page; never read by any rule.
    pub title: String,
    /// For a page; never read by any rule.
    pub description: String,
    /// The shipped scenario the course is sailed in, by name.
    pub scenario: String,
    /// Where leg 0 runs from: the scenario's initial position.
    #[serde(with = "vec2_serde")]
    pub start: Vec2,
    /// Half the width of every gate, metres. Course geometry, not a physical
    /// coefficient (F14.9).
    pub half_width: f64,
    /// The waypoints, in the order they are sailed, numbered from 1 on a
    /// page and indexed from 0 here.
    #[serde(with = "vec2_serde::seq")]
    pub waypoints: Vec<Vec2>,
}

impl Course {
    /// Parse a course document, refusing an unknown schema version and a
    /// course whose route cannot be built.
    pub fn load(json: &str) -> Result<Self, CourseError> {
        let course: Course =
            serde_json::from_str(json).map_err(|e| CourseError::Parse(e.to_string()))?;
        if course.schema_version != COURSE_SCHEMA_VERSION {
            return Err(CourseError::UnsupportedSchema(course.schema_version));
        }
        course.route()?;
        Ok(course)
    }

    /// [`Course::load`], refusing a document that is not course `id`.
    pub fn load_as(id: CourseId, json: &str) -> Result<Self, CourseError> {
        let course = Self::load(json)?;
        if course.id != id {
            return Err(CourseError::WrongId {
                expected: id,
                found: course.id,
            });
        }
        Ok(course)
    }

    /// The course as a [`Route`]: one gate per waypoint, square to its leg
    /// and centred on it ([`Route::waypoints`]).
    pub fn route(&self) -> Result<Route, RouteError> {
        Route::waypoints(self.start, &self.waypoints, self.half_width)
    }
}

/// Why a course document was refused.
#[derive(Clone, Debug, PartialEq)]
pub enum CourseError {
    Parse(String),
    UnsupportedSchema(u32),
    WrongId { expected: CourseId, found: CourseId },
    Route(RouteError),
}

impl From<RouteError> for CourseError {
    fn from(e: RouteError) -> Self {
        Self::Route(e)
    }
}

impl std::fmt::Display for CourseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "course document: {e}"),
            Self::UnsupportedSchema(v) => write!(
                f,
                "course schema version {v} is not supported (this build reads {COURSE_SCHEMA_VERSION})"
            ),
            Self::WrongId { expected, found } => write!(
                f,
                "course file for `{}` says it is `{}`",
                expected.as_str(),
                found.as_str()
            ),
            Self::Route(e) => write!(f, "course route: {e}"),
        }
    }
}

impl std::error::Error for CourseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_through_their_text() {
        for id in CourseId::ALL {
            assert_eq!(CourseId::parse(id.as_str()), Some(id));
            let json = serde_json::to_string(&id).expect("serialise");
            assert_eq!(json, format!("\"{}\"", id.as_str()));
        }
        assert_eq!(CourseId::parse("nope"), None);
    }

    #[test]
    fn every_shipped_course_loads_as_itself() {
        for id in CourseId::ALL {
            let course = id.load().unwrap_or_else(|e| panic!("{}: {e}", id.as_str()));
            assert_eq!(course.id, id);
            assert!(!course.waypoints.is_empty());
        }
    }

    #[test]
    fn the_loader_refuses_each_problem_in_isolation() {
        let good = CourseId::Reach.source();
        let edit = |from: &str, to: &str| {
            assert!(good.contains(from), "the fixture must contain `{from}`");
            good.replacen(from, to, 1)
        };

        let v2 = edit("\"schema_version\": 1", "\"schema_version\": 2");
        assert_eq!(Course::load(&v2), Err(CourseError::UnsupportedSchema(2)));

        let typo = edit("\"half_width\"", "\"halfwidth\"");
        assert!(matches!(Course::load(&typo), Err(CourseError::Parse(_))));

        let flat = edit("\"half_width\": 5.0", "\"half_width\": 0.0");
        assert_eq!(
            Course::load(&flat),
            Err(CourseError::Route(RouteError::GatePostsCoincide {
                mark: 0
            }))
        );

        // A valid document filed under the wrong name.
        let misfiled = edit("\"id\": \"reach\"", "\"id\": \"triangle\"");
        assert_eq!(
            Course::load(&misfiled).map(|c| c.id),
            Ok(CourseId::Triangle)
        );
        assert_eq!(
            Course::load_as(CourseId::Reach, &misfiled),
            Err(CourseError::WrongId {
                expected: CourseId::Reach,
                found: CourseId::Triangle
            })
        );
        Course::load_as(CourseId::Reach, good).expect("the real file is the reach course");
    }
}
