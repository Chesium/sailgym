//! Conformance-bundle identity and its compact key (v2 F16.4, section 02 task 2.1).
//!
//! A conformance bundle is data generated from this crate that a **second
//! implementation** is tested against. Before any comparison is meaningful,
//! one question has to be answered: *does this bundle still describe the
//! physics that is compiled right now?* A bundle that has gone stale is the
//! failure F16.4 exists to prevent, and RV7 is the risk.
//!
//! ## What identifies a bundle
//!
//! F16.4 requires the key to cover "the model/source identity from 08,
//! resolved parameters, integrator/dt, fixture inputs/wind modes,
//! generator/schema and tolerance-contract versions". [`BundleIdentity`] is
//! exactly that record, and it is kept **whole** — the full canonical record
//! travels in `manifest.json` beside the compact key, because F16.4 also says
//! that stable equality of the canonical records is the comparison authority
//! and a digest is only a convenience.
//!
//! Section 10's [`ExperimentIdentity`](crate::recording::ExperimentIdentity)
//! is the same idea for a *recording*; the field shapes here deliberately
//! mirror it — `model`, `parameters`, `integrator`, `dt` — rather than
//! inventing a second vocabulary for the same facts.
//!
//! ## Which part of the record the key covers, and why not all of it
//!
//! The key covers [`BundleIdentity::contract`]: every field above **except**
//! `model.source`. That exclusion is deliberate and is the opposite of a
//! loophole:
//!
//! * The source tree id changes on **every** commit that touches
//!   `crates/sailgym-physics/src`, including the commit that adds the bundle
//!   itself. A directory keyed on it would be stale the instant it was
//!   committed — the chicken-and-egg `gen_golden` already has to live with
//!   (`docs/v2/progress/08-handoff.md` §5).
//! * A changed **equation** is caught by something strictly stronger: each
//!   fixture carries [`FixtureId::data_key`], a digest of the numbers the
//!   fixture actually holds. Recomputing the fixture from changed source
//!   produces different numbers, a different `data_key` and therefore a
//!   different bundle key. A source edit that moves no bit of any sampled
//!   output moves no key — which is the honest answer, not a miss (RV10).
//! * The full `ModelIdentity`, `state` included, is still recorded in the
//!   canonical record and printed by the runner, and
//!   [`BundleIdentity::is_release_baseline`] refuses to certify a bundle
//!   generated from a `dirty` or `unknown` tree.
//!
//! ## No cryptography is implemented here
//!
//! [`sha256_hex`] is a two-line wrapper over the `sha2` crate, which is the
//! `RustCrypto` implementation of FIPS 180-4. F16.4 says in as many words:
//! "use an established SHA-256 implementation, not handwritten cryptography".
//! [`tests::sha256_known_answers`] pins it against the FIPS 180-4 and NIST
//! CAVP vectors so that a dependency swap cannot silently change every
//! directory name in the repository.
//!
//! This module is compiled only under `cfg(test)` or the `testkit` feature,
//! so the `sha2` dependency never reaches `wasm-pack build` and the shipped
//! browser bundle's dependency graph is unchanged.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::identity::ModelIdentity;
use crate::integrator::Integrator;
use crate::parameters::BoatParameters;

/// The on-disk layout of `conformance/<key>/` — the file set, the `.npy`
/// dtype and shape convention, and the manifest's own field names.
///
/// Bumped by hand when a reader written against the previous layout would
/// misread a bundle.
pub const BUNDLE_SCHEMA_VERSION: u32 = 1;

/// The version of the F16.2 tolerance contract the manifest's tolerances were
/// derived under.
///
/// Bumped by hand whenever the *rule* changes — a new tier, a different
/// derivation, a different near-zero treatment. A tolerance number changing
/// because the measurement was redone does **not** bump it; a tolerance number
/// changing because the rule changed does. F16.2: "A tolerance change requires
/// a new derivation, never merely a green port test."
pub const TOLERANCE_CONTRACT_VERSION: u32 = 1;

/// SHA-256 of `bytes`, lowercase hex.
///
/// A thin wrapper over `sha2`, kept in one place so there is exactly one
/// spelling of "the hash we key artefacts by".
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    let mut s = String::with_capacity(2 * out.len());
    for b in out {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// One `.npy` fixture in the bundle, described well enough that a port can
/// read it without reading the generator.
///
/// **Column names are the contract** (F16.4 / task 2.4): a port that reads
/// columns by position is one insertion away from silently comparing the
/// wrong thing, so the names travel with the data and the runner asserts them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureId {
    /// The file stem, e.g. `tier0_foil`. The file is `<name>.npy`.
    pub name: String,
    /// F16.2's tier: 0 pure functions, 1 derivative components,
    /// 2 trajectories.
    pub tier: u32,
    /// The F16.3 sampler that produced the rows, by name, so a failing row
    /// can be traced to the hazard it was written for.
    pub samplers: Vec<String>,
    /// Rows in the `.npy`.
    pub rows: usize,
    /// The leading columns: the inputs a port feeds its own implementation.
    pub input_columns: Vec<String>,
    /// The trailing columns: what this crate produced from those inputs.
    pub output_columns: Vec<String>,
    /// SHA-256 of the file's `f64` payload, little-endian, row-major.
    ///
    /// This is the field that makes an **equation** change invalidate the
    /// bundle even when every parameter is untouched.
    pub data_key: String,
}

/// The wind modes shipped as data for one `(config, seed)` (F16.7).
///
/// No stack other than Rust implements PCG32, so the drawn modes travel in
/// the bundle and a port reads them. The record here is the identity half —
/// the numbers themselves are in `wind_modes.json` and are covered by
/// `modes_key`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindFixtureId {
    /// The bundle's name for this field, e.g. `gust_default`.
    pub name: String,
    /// The `u64` seed the modes were drawn from.
    pub seed: u64,
    /// Number of modes actually drawn. Zero for a uniform field.
    pub mode_count: usize,
    /// SHA-256 of this field's entry in `wind_modes.json`, as written.
    pub modes_key: String,
}

/// Everything a conformance bundle's freshness depends on (F16.4).
///
/// Serialised verbatim into `manifest.json` as `identity`. Kept whole: F16.4
/// says "Keep the full records beside optional digests", and a record nobody
/// can read is a record nobody can audit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BundleIdentity {
    /// [`BUNDLE_SCHEMA_VERSION`].
    pub bundle_schema_version: u32,
    /// The generator's own version, bumped when it changes what it emits.
    pub generator_version: u32,
    /// [`TOLERANCE_CONTRACT_VERSION`].
    pub tolerance_contract_version: u32,
    /// F18.1d: the declared model version and the physics source tree id.
    /// `source` is **not** covered by the key; see the module note.
    pub model: ModelIdentity,
    /// The fully resolved F7 catalogue the fixtures were generated under.
    pub parameters: BoatParameters,
    /// Mirrors `parameters.sim.integrator`, named explicitly so a mismatch
    /// reports the field F16.4 names rather than "parameters".
    pub integrator: Integrator,
    /// s. Mirrors `parameters.sim.dt`, for the same reason.
    pub dt: f64,
    /// The wind fields shipped as data (F16.7), in a fixed order.
    pub wind: Vec<WindFixtureId>,
    /// Every `.npy` in the bundle, in a fixed order.
    pub fixtures: Vec<FixtureId>,
}

/// The subset of [`BundleIdentity`] the compact key is computed over.
///
/// Borrowed rather than cloned, and serialised in this declaration order, so
/// the key is a pure function of the record's content.
#[derive(Serialize)]
struct KeyedContract<'a> {
    bundle_schema_version: u32,
    generator_version: u32,
    tolerance_contract_version: u32,
    model_version: u32,
    parameters: &'a BoatParameters,
    integrator: &'a Integrator,
    dt: f64,
    wind: &'a [WindFixtureId],
    fixtures: &'a [FixtureId],
}

impl BundleIdentity {
    /// The canonical text form of the whole record.
    ///
    /// `serde_json` over a record with one declaration order and no unordered
    /// map anywhere (F9.3), so equal records produce equal text.
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// The text the compact key is taken over: everything except
    /// `model.source`. See the module note for why that field is excluded.
    pub fn contract_json(&self) -> String {
        serde_json::to_string(&KeyedContract {
            bundle_schema_version: self.bundle_schema_version,
            generator_version: self.generator_version,
            tolerance_contract_version: self.tolerance_contract_version,
            model_version: self.model.model_version,
            parameters: &self.parameters,
            integrator: &self.integrator,
            dt: self.dt,
            wind: &self.wind,
            fixtures: &self.fixtures,
        })
        .unwrap_or_default()
    }

    /// The bundle's compact key: SHA-256 of [`BundleIdentity::contract_json`],
    /// lowercase hex. This is the `conformance/<key>/` directory name.
    pub fn key(&self) -> String {
        sha256_hex(self.contract_json().as_bytes())
    }

    /// Whether this identity may certify a release bundle.
    ///
    /// `false` for a `dirty` or `unknown` source tree: such a bundle names no
    /// baseline, so it may be generated and inspected but never presented as
    /// the reference an independent implementation was checked against
    /// (F18.1d, F16.4).
    pub fn is_release_baseline(&self) -> bool {
        self.model.source.is_known()
    }

    /// Every contract field on which this record and `other` disagree, named,
    /// in a fixed order.
    ///
    /// `model.source` is **absent by construction**: it is not part of the
    /// contract, and the runner reports it separately (see the module note).
    /// An empty result is equivalent to `self.key() == other.key()`, but it
    /// says *which* field moved, which a hex string cannot.
    pub fn contract_differences(&self, other: &Self) -> Vec<String> {
        fn note(out: &mut Vec<String>, same: bool, name: &str) {
            if !same {
                out.push(name.to_string());
            }
        }
        let out = &mut Vec::new();
        note(
            out,
            self.bundle_schema_version == other.bundle_schema_version,
            "bundle_schema_version",
        );
        note(
            out,
            self.generator_version == other.generator_version,
            "generator_version",
        );
        note(
            out,
            self.tolerance_contract_version == other.tolerance_contract_version,
            "tolerance_contract_version",
        );
        note(
            out,
            self.model.model_version == other.model.model_version,
            "model.model_version",
        );
        note(out, self.parameters == other.parameters, "parameters");
        note(out, self.integrator == other.integrator, "integrator");
        note(out, self.dt.to_bits() == other.dt.to_bits(), "dt");
        if self.wind != other.wind {
            let mine: Vec<&str> = self.wind.iter().map(|w| w.name.as_str()).collect();
            let theirs: Vec<&str> = other.wind.iter().map(|w| w.name.as_str()).collect();
            if mine == theirs {
                for (a, b) in self.wind.iter().zip(other.wind.iter()) {
                    note(out, a == b, &format!("wind[{}]", a.name));
                }
            } else {
                out.push("wind (the set of fields changed)".to_string());
            }
        }
        if self.fixtures != other.fixtures {
            let mine: Vec<&str> = self.fixtures.iter().map(|f| f.name.as_str()).collect();
            let theirs: Vec<&str> = other.fixtures.iter().map(|f| f.name.as_str()).collect();
            if mine == theirs {
                for (a, b) in self.fixtures.iter().zip(other.fixtures.iter()) {
                    note(out, a == b, &format!("fixtures[{}]", a.name));
                }
            } else {
                out.push("fixtures (the set of files changed)".to_string());
            }
        }
        std::mem::take(out)
    }

    /// One line naming both source identities, for the runner's report.
    ///
    /// The source is not part of the contract, so a difference here is
    /// information rather than a verdict — exactly as `tests/regression.rs`
    /// treats a golden recorded under a different clean identity.
    pub fn source_note(&self, other: &Self) -> String {
        format!(
            "bundle {} | this build {}",
            self.model.describe(),
            other.model.describe()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{SourceId, SourceState};

    fn fixture(name: &str, data_key: &str) -> FixtureId {
        FixtureId {
            name: name.to_string(),
            tier: 0,
            samplers: vec!["halton_background".to_string()],
            rows: 128,
            input_columns: vec!["a".to_string()],
            output_columns: vec!["b".to_string()],
            data_key: data_key.to_string(),
        }
    }

    fn identity() -> BundleIdentity {
        BundleIdentity {
            bundle_schema_version: BUNDLE_SCHEMA_VERSION,
            generator_version: 1,
            tolerance_contract_version: TOLERANCE_CONTRACT_VERSION,
            model: ModelIdentity {
                model_version: 2,
                source: SourceId {
                    tree: "a".repeat(40),
                    state: SourceState::Clean,
                },
            },
            parameters: BoatParameters::ilca7(),
            integrator: BoatParameters::ilca7().sim.integrator,
            dt: BoatParameters::ilca7().sim.dt,
            wind: vec![WindFixtureId {
                name: "gust_default".to_string(),
                seed: 7,
                mode_count: 12,
                modes_key: "c".repeat(64),
            }],
            fixtures: vec![fixture("tier0_foil", &"b".repeat(64))],
        }
    }

    /// FIPS 180-4 appendix B and the NIST CAVP short-message vectors. A
    /// dependency swap that changed these would rename every bundle in the
    /// repository, so they are pinned rather than trusted.
    #[test]
    fn sha256_known_answers() {
        let cases: [(&[u8], &str); 4] = [
            (
                b"",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                b"abc",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
                "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
            ),
            (
                b"\xbd",
                "68325720aabd7c82f30f554b313d0570c95accbb7dc4b5aae11204c08ffe732b",
            ),
        ];
        for (input, expect) in cases {
            assert_eq!(sha256_hex(input), expect, "input {input:?}");
        }
        // A million 'a' — FIPS 180-4's long-message vector, which is what
        // catches a broken multi-block path.
        assert_eq!(
            sha256_hex(&vec![b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn identical_records_round_trip_and_compare_equal() {
        let a = identity();
        let text = a.canonical_json();
        let b: BundleIdentity = serde_json::from_str(&text).expect("the record must round-trip");
        assert_eq!(a, b);
        assert_eq!(b.canonical_json(), text, "serialisation must be stable");
        assert_eq!(a.key(), b.key());
        assert!(a.contract_differences(&b).is_empty());
        // 64 lowercase hex characters, every time.
        assert_eq!(a.key().len(), 64);
        assert!(a
            .key()
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    /// The RV10 case: the equations moved, every parameter stayed put.
    ///
    /// An equation change reaches the identity through the numbers the
    /// fixtures hold, not through the parameter catalogue — which is the
    /// whole reason `data_key` is in the record.
    #[test]
    fn an_equation_change_with_identical_parameters_invalidates_the_bundle() {
        let a = identity();
        let mut b = a.clone();
        b.fixtures[0].data_key = "d".repeat(64);
        assert_eq!(a.parameters, b.parameters, "the premise of this test");
        assert_ne!(a.key(), b.key());
        assert_eq!(a.contract_differences(&b), vec!["fixtures[tier0_foil]"]);
    }

    #[test]
    fn parameters_dt_generator_and_tolerance_all_invalidate_it() {
        let a = identity();
        let mut edits: Vec<(&str, BundleIdentity)> = Vec::new();

        let mut p = a.clone();
        p.parameters.sail.section.area += 0.5;
        edits.push(("parameters", p));

        let mut d = a.clone();
        d.dt = 0.01;
        d.parameters.sim.dt = 0.01;
        edits.push(("dt", d));

        let mut g = a.clone();
        g.generator_version += 1;
        edits.push(("generator_version", g));

        let mut t = a.clone();
        t.tolerance_contract_version += 1;
        edits.push(("tolerance_contract_version", t));

        let mut s = a.clone();
        s.bundle_schema_version += 1;
        edits.push(("bundle_schema_version", s));

        let mut v = a.clone();
        v.model.model_version += 1;
        edits.push(("model.model_version", v));

        let mut w = a.clone();
        w.wind[0].modes_key = "e".repeat(64);
        edits.push(("wind[gust_default]", w));

        let mut i = a.clone();
        i.integrator = Integrator::Rk4;
        i.parameters.sim.integrator = Integrator::Rk4;
        edits.push(("integrator", i));

        for (field, edited) in edits {
            assert_ne!(a.key(), edited.key(), "{field} left the key alone");
            let named = a.contract_differences(&edited);
            assert!(
                named.iter().any(|n| n.contains(field)),
                "{field} changed but the difference report says {named:?}"
            );
        }
    }

    /// A column rename is a contract change, even with identical numbers.
    #[test]
    fn renaming_a_column_invalidates_the_bundle() {
        let a = identity();
        let mut b = a.clone();
        b.fixtures[0].output_columns = vec!["renamed".to_string()];
        assert_ne!(a.key(), b.key());
    }

    /// F18.1d: a dirty or unknown tree names no baseline, and the key is
    /// deliberately blind to the distinction — so the flag has to be read
    /// from the record, which is what the runner does.
    #[test]
    fn a_dirty_or_unknown_source_cannot_certify_a_release_bundle() {
        let clean = identity();
        assert!(clean.is_release_baseline());
        for state in [SourceState::Dirty, SourceState::Unknown] {
            let mut bad = clean.clone();
            bad.model.source.state = state;
            assert!(!bad.is_release_baseline(), "{state:?}");
            // Same contract, same key: the exclusion is explicit, and the
            // refusal comes from the flag rather than from the hash.
            assert_eq!(bad.key(), clean.key());
            assert!(bad.contract_differences(&clean).is_empty());
            assert!(bad.source_note(&clean).contains("model v2"));
        }
    }

    /// A different clean source tree is reported, never keyed: a commit that
    /// touches `src/` without moving a single sampled number must not rename
    /// the directory.
    #[test]
    fn a_different_clean_source_tree_leaves_the_key_alone() {
        let a = identity();
        let mut b = a.clone();
        b.model.source.tree = "f".repeat(40);
        assert_eq!(a.key(), b.key());
        assert!(a.contract_differences(&b).is_empty());
        assert!(a.source_note(&b).contains(&"f".repeat(40)));
    }

    /// A key is only as good as the text under it: if the canonical form ever
    /// stopped being a function of the content, every assertion above would
    /// pass vacuously.
    #[test]
    fn the_key_is_a_function_of_the_contract_text() {
        let a = identity();
        assert_eq!(a.key(), sha256_hex(a.contract_json().as_bytes()));
        assert!(a.contract_json().contains("\"tier0_foil\""));
        assert!(
            !a.contract_json().contains(&"a".repeat(40)),
            "the source tree id must not reach the keyed text"
        );
        assert!(
            a.canonical_json().contains(&"a".repeat(40)),
            "but it must reach the full record"
        );
    }
}
