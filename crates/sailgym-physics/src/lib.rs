//! sailgym physics core.
//!
//! Pure Rust: this crate must build and test on the host with plain
//! `cargo test`, and must never depend on `wasm-bindgen` or any JS-facing
//! crate (`docs/v1/00-foundations.md` F8.1).
//!
//! Conventions, equations and parameters are normative in
//! `docs/v1/00-foundations.md`; nothing here may redefine them.

pub mod aero;
pub mod constants;
pub mod diagnostics;
pub mod dynamics;
pub mod environment;
pub mod foil;
pub mod forces;
pub mod frames;
pub mod hydro;
/// Model and source identity (v2 F18.1d).
///
/// Sections 10 and 02 need one question answered before they may compare two
/// recordings, two conformance bundles or two experiments: **are these the
/// same equations?** A parameter digest cannot answer it — changing `foil.rs`
/// leaves every F7 value untouched — and an implementation identity cannot
/// answer the complementary question either. Both records travel, separately.
///
/// This module is the implementation half. It is deliberately small, it
/// implements no cryptography of its own, and it is honest about not knowing.
pub mod identity {
    use serde::{Deserialize, Serialize};

    /// The declared contract version of the reduced model.
    ///
    /// Bumped **by hand**, and only when F6 or F7 changes in a way that makes
    /// a previously recorded episode describe a different boat. It is not a
    /// substitute for [`SourceId`]: a version number records an intent, a
    /// source id records a fact.
    ///
    /// | value | what it means |
    /// |---|---|
    /// | 1 | the v1 model: `docs/v1/00-foundations.md` F6/F7 as shipped through section 10 |
    /// | 2 | the v2 section 08 corrections: F18.1a's four-harmonic `GZ`, F18.1b's unilateral sheet and geometric stop, and F18.1c's two parameter deltas |
    pub const MODEL_VERSION: u32 = 2;

    /// Whether the compiled-in source identity can be trusted to name a
    /// baseline.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum SourceState {
        /// The working tree matched the recorded commit at build time.
        Clean,
        /// `crates/sailgym-physics/src` had uncommitted edits at build time,
        /// so the id names a commit the binary does **not** implement.
        Dirty,
        /// The identity could not be established at all — no git, no checkout,
        /// a source tarball. Not the same as "unchanged".
        Unknown,
    }

    /// The content identity of `crates/sailgym-physics/src` at build time.
    ///
    /// `tree` is git's own content-addressed tree id, captured by `build.rs`
    /// from `git rev-parse HEAD:crates/sailgym-physics/src`. Two builds agree
    /// **iff** every byte of every physics source file agrees; an unrelated
    /// commit that leaves `src/` alone leaves this alone.
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SourceId {
        pub tree: String,
        pub state: SourceState,
    }

    /// The identity this binary was built from: the declared model version and
    /// the source content it actually implements.
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ModelIdentity {
        pub model_version: u32,
        pub source: SourceId,
    }

    impl SourceId {
        /// The identity compiled into this binary.
        pub fn current() -> Self {
            Self {
                tree: env!("SAILGYM_SOURCE_TREE").to_string(),
                state: match env!("SAILGYM_SOURCE_STATE") {
                    "clean" => SourceState::Clean,
                    "dirty" => SourceState::Dirty,
                    _ => SourceState::Unknown,
                },
            }
        }

        /// Whether this id names a baseline another artefact may be compared
        /// against. `Dirty` and `Unknown` never do — that is F18.1d's whole
        /// point, and RV52 is the risk it exists to close.
        pub fn is_known(&self) -> bool {
            self.state == SourceState::Clean
        }

        /// One line, for a manifest, a log or an error message.
        pub fn describe(&self) -> String {
            match self.state {
                SourceState::Clean => format!("physics src tree {}", self.tree),
                SourceState::Dirty => format!(
                    "physics src tree {} PLUS UNCOMMITTED EDITS — not a baseline",
                    self.tree
                ),
                SourceState::Unknown => {
                    format!("physics src identity unknown ({})", self.tree)
                }
            }
        }
    }

    impl ModelIdentity {
        /// The identity this binary was built from.
        pub fn current() -> Self {
            Self {
                model_version: MODEL_VERSION,
                source: SourceId::current(),
            }
        }

        /// Whether an artefact recorded under `self` may be compared, quantity
        /// for quantity, with one recorded under `other`.
        ///
        /// Two unknowns are **not** equal, and a dirty tree equals nothing,
        /// including itself: `ModelIdentity::current()` from two different
        /// working trees can carry the same commit id and different source.
        pub fn is_comparable_with(&self, other: &Self) -> bool {
            self.model_version == other.model_version
                && self.source.is_known()
                && other.source.is_known()
                && self.source.tree == other.source.tree
        }

        /// One line, for a manifest, a log or an error message.
        pub fn describe(&self) -> String {
            format!("model v{} ({})", self.model_version, self.source.describe())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_compiled_identity_is_well_formed() {
            let id = ModelIdentity::current();
            assert_eq!(id.model_version, MODEL_VERSION);
            assert!(!id.source.tree.is_empty());
            // A clean identity is a 40-character git object id and nothing
            // else; anything shorter is a message, and a message is not a
            // baseline.
            if id.source.is_known() {
                assert_eq!(id.source.tree.len(), 40, "{}", id.source.tree);
                assert!(id.source.tree.chars().all(|c| c.is_ascii_hexdigit()));
            }
            eprintln!("identity: {}", id.describe());
        }

        #[test]
        fn rebuilding_unchanged_source_keeps_it_stable() {
            // `current()` reads compiled-in constants, so two calls in one
            // binary are trivially equal; what this asserts is the property
            // that makes that meaningful — the constant is the *content* id of
            // `src`, so asking git again now must give the same answer.
            let compiled = SourceId::current();
            let live = std::process::Command::new("git")
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .args(["rev-parse", "HEAD:crates/sailgym-physics/src"])
                .output();
            let Ok(out) = live else {
                eprintln!("skip: git is not available");
                return;
            };
            if !out.status.success() {
                eprintln!("skip: not a git checkout");
                return;
            }
            let tree = String::from_utf8_lossy(&out.stdout).trim().to_string();
            assert!(
                compiled.tree.starts_with(&tree),
                "the compiled source id is {} but git now says {tree}; build.rs did not \
                 re-run when src changed (RV52)",
                compiled.tree
            );
        }

        #[test]
        fn dirty_and_unknown_are_never_comparable() {
            let known = |tree: &str| ModelIdentity {
                model_version: MODEL_VERSION,
                source: SourceId {
                    tree: tree.to_string(),
                    state: SourceState::Clean,
                },
            };
            let a = known("a".repeat(40).as_str());
            assert!(a.is_comparable_with(&a));
            assert!(!a.is_comparable_with(&known("b".repeat(40).as_str())));

            for state in [SourceState::Dirty, SourceState::Unknown] {
                let b = ModelIdentity {
                    model_version: MODEL_VERSION,
                    source: SourceId {
                        tree: "a".repeat(40),
                        state,
                    },
                };
                assert!(!b.is_comparable_with(&b), "{state:?} compared with itself");
                assert!(!a.is_comparable_with(&b));
                assert!(!b.is_comparable_with(&a));
                assert!(!b.source.is_known());
            }

            // A different declared model version is never comparable either,
            // even with identical source — the version is what a human bumps
            // when the *meaning* of a recording changes.
            let mut old = a.clone();
            old.model_version = MODEL_VERSION - 1;
            assert!(!a.is_comparable_with(&old));
        }

        #[test]
        fn describe_says_which_of_the_three_it_is() {
            let id = |state| ModelIdentity {
                model_version: MODEL_VERSION,
                source: SourceId {
                    tree: "0".repeat(40),
                    state,
                },
            };
            assert!(id(SourceState::Clean).describe().contains("model v2"));
            assert!(id(SourceState::Dirty).describe().contains("UNCOMMITTED"));
            assert!(id(SourceState::Unknown).describe().contains("unknown"));
        }
    }
}
pub mod integrator;
pub mod parameters;
pub mod recording;
pub mod rigging;
pub mod rng;
pub mod scenario;
pub mod simulation;
pub mod stability;
pub mod state;
/// Test-only helpers (section 04, task 4.1). Compiled only under `cfg(test)`
/// or the `testkit` feature, so nothing here reaches the wasm build.
#[cfg(any(test, feature = "testkit"))]
pub mod testkit;
pub mod vec;
