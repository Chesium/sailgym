//! The committed conformance bundle, checked against current source
//! (section 02 task 2.6; v2 F16, gate step 4).
//!
//! **This file is the section's reason for existing.** A bundle that no
//! trusted implementation checks is a bundle that silently goes stale, and a
//! stale bundle is the failure that sends a porting team hunting a phantom
//! bug for a day. The runner is written *before* any port exists, so that
//! when the port lands the only new thing being tested is the port.
//!
//! It is the `provenance.rs::docs_match_source` pattern: a generated artifact
//! is committed, and a test asserts the committed copy still matches what the
//! source produces.
//!
//! ## Bit-for-bit, and why that is not a cross-stack claim
//!
//! F16.1 is explicit that **no conformance test may assert equality across
//! stacks**. This one is not across stacks: it is Rust against Rust on the
//! same build, which is F9's territory, and what it is testing is whether the
//! committed data still describes the compiled physics. The tolerances in
//! `manifest.json` are for *other* implementations and are never applied
//! here — applying them would turn a staleness check into a check that the
//! physics has not moved *much*, which is not a thing anyone wants to know.
//!
//! ## R7, and why this file skips rather than fails
//!
//! A generated artifact is only evidence about the toolchain that produced
//! it. On a `rustc` or target mismatch these tests **skip with a message
//! naming both** and pass: a red suite on a different compiler would say "the
//! physics changed", which is not what happened, and people learn to ignore
//! it. What it must never do is pass *silently* — the message is the
//! deliverable.
//!
//! **The build profile is recorded and compared, but a difference in it does
//! not skip**, which is where this file departs from `tests/regression.rs`.
//! The bundle is generated with `cargo run --release` (the tier-2 reference
//! study runs every case three times) and this suite runs under `cargo test`,
//! which is a debug build, so comparing the profile would make the runner
//! skip on every single run — and a gate step that always skips is RV7 with
//! extra steps. The argument that it is safe is that Rust enables no
//! fast-math at any optimisation level and never reassociates floating point,
//! so `-O` cannot move an `f64` bit; the argument was **measured** rather
//! than asserted, and `docs/v2/progress/02-handoff.md` records the debug and
//! release payload digests side by side.
//!
//! ## Proven able to fail
//!
//! Task 2.6 requires the three failure modes to be demonstrated and
//! reverted. Recorded here the way `no_shortcuts.rs`'s module doc records its
//! audits, and in full in `docs/v2/progress/02-handoff.md` §6:
//!
//! | Injected defect | What went red |
//! |---|---|
//! | one sign flipped in `foil.rs::foil_force` | `every_fixture_matches_current_source` named `tier0_foil`, column `fx`, row 0 |
//! | one F7 default changed (`sail.area`) | `the_manifest_identity_is_the_current_contract` named `parameters` |
//! | one byte of `tier0_gz.npy` hand-edited | `every_fixture_matches_current_source` named `tier0_gz` |
//!
//! Each was made on a temporary copy of the source or the fixture, measured,
//! and reverted; nothing unrelated was touched.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sailgym_physics::digest::{sha256_hex, BundleIdentity};
use sailgym_physics::environment::wind::{ProceduralWind, WindConfig, WindMode3};
use sailgym_physics::environment::WindField;
use sailgym_physics::identity::ModelIdentity;
use sailgym_physics::parameters::BoatParameters;
use sailgym_physics::recording::ToolchainInfo;
use sailgym_physics::stability::hydrostatics::{GzCurve, GzRepresentation};
use sailgym_physics::testkit::npy;
use sailgym_physics::vec::Vec2;

#[path = "../../sailgym-bench/src/conformance/mod.rs"]
mod conformance;

use conformance::{Context, Manifest};

/// The smallest row count each fixture is allowed to have.
///
/// An anti-vacuity floor in the style of `regression.rs`: a fixture that
/// shrank to a handful of rows would still pass every comparison below and
/// would prove nothing. The numbers are a little under what the generator
/// currently produces, so a deliberate reduction is visible and a drift is
/// caught.
const MIN_ROWS: [(&str, usize); 11] = [
    ("tier0_wrap_pi", 200),
    ("tier0_wave", 1200),
    ("tier0_wind_sample", 300),
    ("tier0_apparent", 300),
    ("tier0_foil", 600),
    ("tier0_sail", 300),
    ("tier0_sheet", 600),
    ("tier0_gz", 300),
    ("tier0_hydro", 200),
    ("tier1_derivative", 400),
    ("tier2_trajectories", 3000),
];

/// Every F16.3 hazard has a named sampler, and every named sampler has to
/// appear somewhere in the bundle.
///
/// This is the other half of the anti-vacuity guard: a bundle with the right
/// row counts and none of the branch-point rows would agree with itself
/// perfectly and would miss exactly the places a port differs.
const REQUIRED_SAMPLERS: [&str; 8] = [
    "wrap_pi_edge",
    "stall_blend_edge",
    "zero_flow_eps",
    "phi_unwrapped",
    "sheet_slack_boundary",
    "limit_rate_boundary",
    "rudder_self_centre_zero",
    "sheet_release_precedence",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The one committed bundle directory.
fn bundle_dir() -> PathBuf {
    let root = repo_root().join("conformance");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| {
            panic!(
                "cannot read {}: {e}\n\
                 The conformance bundle is committed; regenerate it with\n  \
                 cargo run --release -p sailgym-bench --bin gen_conformance",
                root.display()
            )
        })
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    assert_eq!(
        dirs.len(),
        1,
        "conformance/ holds {} bundles ({:?}); exactly one is committed. A leftover \
         directory from an earlier key is a bundle nothing reads — delete it.",
        dirs.len(),
        dirs.iter()
            .map(|p| p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned())
            .collect::<Vec<_>>()
    );
    dirs.remove(0)
}

fn manifest() -> (PathBuf, Manifest) {
    let dir = bundle_dir();
    let path = dir.join("manifest.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let m: Manifest = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{} is not a bundle manifest: {e}", path.display()));
    (dir, m)
}

/// R7. Returns `false` — with a printed message naming both toolchains — when
/// the committed bundle was produced by a different compiler or for a
/// different target. The build profile is printed but is not a mismatch; see
/// the module note.
fn toolchain_agrees(m: &Manifest) -> bool {
    let current = ToolchainInfo::current();
    if m.toolchain.rustc == current.rustc && m.toolchain.target == current.target {
        if m.toolchain.profile != current.profile {
            eprintln!(
                "conformance: bundle profile `{}`, this run `{}`. Not a mismatch — Rust \
                 enables no fast-math at any optimisation level and never reassociates \
                 floating point, so -O cannot move an f64 bit. Measured; see \
                 docs/v2/progress/02-handoff.md.",
                m.toolchain.profile, current.profile
            );
        }
        return true;
    }
    eprintln!(
        "skip: the committed conformance bundle was produced by a different toolchain.\n  \
         expected: {}\n  \
         actual:   {}\n  \
         A generated artifact is only valid for the build that produced it (F9, F11 R7). \
         Regenerate with `cargo run --release -p sailgym-bench --bin gen_conformance` if \
         this toolchain is the one you mean to hold.",
        m.toolchain.describe(),
        current.describe(),
    );
    false
}

/// The identity of the bundle as current source would generate it.
///
/// Built by running the generator's own `build`, which is the same code path
/// `gen_conformance` uses — there is one definition of what a bundle is.
fn rebuild(ctx: &Context) -> conformance::Bundle {
    conformance::build(ctx, "")
}

// ---------------------------------------------------------------------------
// The comparison
// ---------------------------------------------------------------------------

/// Read every committed fixture's **inputs**, recompute the outputs from
/// current source, and compare bit for bit.
///
/// Bit-for-bit, not to a tolerance: see the module note.
#[test]
fn every_fixture_matches_current_source() {
    let (dir, m) = manifest();
    if !toolchain_agrees(&m) {
        return;
    }
    let ctx = Context::new();
    let fixtures = conformance::fixtures(&ctx);

    let recorded: BTreeSet<&str> = m
        .identity
        .fixtures
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    let produced: BTreeSet<&str> = fixtures.iter().map(|f| f.name).collect();
    assert_eq!(
        recorded, produced,
        "the committed bundle and this build disagree about which fixtures exist"
    );

    let mut compared = 0usize;
    for fx in &fixtures {
        let id = m
            .identity
            .fixtures
            .iter()
            .find(|f| f.name == fx.name)
            .expect("the sets agree");
        let path = dir.join(format!("{}.npy", fx.name));
        let bytes =
            std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let file = npy::decode_f64_2d(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        // The column names are the contract. A port reading by position is
        // one insertion away from comparing the wrong thing, and so is this
        // runner.
        assert_eq!(
            id.input_columns, fx.input_columns,
            "{}: the committed input columns are not this build's",
            fx.name
        );
        assert_eq!(
            id.output_columns, fx.output_columns,
            "{}: the committed output columns are not this build's",
            fx.name
        );
        let n_in = fx.input_columns.len();
        let n_out = fx.output_columns.len();
        assert_eq!(file.cols, n_in + n_out, "{}: column count", fx.name);
        assert_eq!(file.rows, id.rows, "{}: row count", fx.name);
        assert_eq!(
            file.rows,
            fx.rows(),
            "{}: this build's sampler produces {} rows against {} committed. The samplers \
             are part of the artifact (F16.3); regenerate the bundle.",
            fx.name,
            fx.rows(),
            file.rows
        );

        // The inputs come from the file, not from this build's sampler: the
        // point is to recompute what was recorded, and a sampler that drifted
        // would otherwise hide behind its own new inputs.
        let mut inputs = Vec::with_capacity(file.rows * n_in);
        for r in 0..file.rows {
            inputs.extend_from_slice(&file.row(r)[..n_in]);
        }
        for (r, (a, b)) in inputs.iter().zip(fx.inputs.iter()).enumerate() {
            assert_eq!(
                a.to_bits(),
                b.to_bits(),
                "{}: committed input row {} differs from this build's sampler ({a} vs {b})",
                fx.name,
                r / n_in
            );
        }

        let recomputed = (fx.compute)(&ctx, &inputs, n_in);
        assert_eq!(recomputed.len(), file.rows * n_out);
        for r in 0..file.rows {
            let row = file.row(r);
            for c in 0..n_out {
                let want = row[n_in + c];
                let got = recomputed[r * n_out + c];
                assert_eq!(
                    got.to_bits(),
                    want.to_bits(),
                    "{}: row {r}, column `{}`: this build produces {got} where the \
                     committed bundle records {want}.\n\
                     Either the physics changed and the bundle is stale — regenerate with \
                     `cargo run --release -p sailgym-bench --bin gen_conformance` and \
                     commit the result — or the committed file has been edited.",
                    fx.name,
                    fx.output_columns[c],
                );
                compared += 1;
            }
        }

        // And the digest the key is built from, over the payload as written.
        let payload: Vec<u8> = file.data.iter().flat_map(|v| v.to_le_bytes()).collect();
        assert_eq!(
            sha256_hex(&payload),
            id.data_key,
            "{}: the file's payload does not match the digest in the manifest",
            fx.name
        );
    }

    // "or a match proves nothing": the comparison has to have compared
    // something.
    // "or a match proves nothing." The floor is a little under what the
    // current bundle produces (98 401 output values across eleven fixtures),
    // so a fixture quietly dropping out is visible.
    assert!(
        compared > 50_000,
        "only {compared} values compared; the runner is not running"
    );
    eprintln!(
        "conformance: {} fixtures, {compared} values, bit-identical | bundle {}",
        fixtures.len(),
        m.key
    );
}

/// The manifest's **full identity** equals the contract current source
/// describes — not merely the directory name.
///
/// `model.source` is the one field compared differently, exactly as
/// `tests/regression.rs` treats a golden's identity: the declared model
/// version must match, a bundle recorded from a tree git could not certify
/// must say what it was recording, and a *different clean* source tree is
/// reported rather than failed, because the bit-for-bit comparison above is
/// the authority on whether the physics moved.
#[test]
fn the_manifest_identity_is_the_current_contract() {
    let (_, m) = manifest();
    if !toolchain_agrees(&m) {
        return;
    }
    let ctx = Context::new();
    let now: BundleIdentity = rebuild(&ctx).manifest.identity;

    let differences = m.identity.contract_differences(&now);
    assert!(
        differences.is_empty(),
        "the committed bundle does not describe this build's contract: {differences:?}.\n\
         Regenerate with `cargo run --release -p sailgym-bench --bin gen_conformance` and \
         commit the result. A stale bundle is the one failure this whole section exists to \
         prevent (RV7)."
    );
    assert_eq!(
        m.key,
        now.key(),
        "the manifest's key is not the key of its own contract"
    );

    // v2 F18.1d / RV51, the `regression.rs` treatment.
    let current = ModelIdentity::current();
    assert_eq!(
        m.identity.model.model_version, current.model_version,
        "the bundle was generated under model v{}, this build is model v{}. A bundle from \
         a different declared model is not a conformance reference for this one.",
        m.identity.model.model_version, current.model_version
    );
    if !m.identity.is_release_baseline() {
        assert!(
            !m.declared_changes.trim().is_empty(),
            "the bundle records a non-baseline source identity ({}) and declares no \
             changes. That is an artifact nobody can review (RV51): regenerate with \
             `--allow-dirty --declare \"<what is changing>\"`, or from a clean tree.",
            m.identity.model.source.describe(),
        );
        eprintln!(
            "conformance: bundle generated from a NON-BASELINE tree — {}\n  declared: {}",
            m.identity.model.describe(),
            m.declared_changes.lines().next().unwrap_or("").trim(),
        );
    }
    eprintln!("conformance: {}", m.identity.source_note(&now));
}

/// The key is the directory name, and the manifest agrees with itself.
#[test]
fn the_key_is_the_directory_name() {
    let (dir, m) = manifest();
    let name = dir
        .file_name()
        .expect("a bundle directory")
        .to_string_lossy()
        .into_owned();
    assert_eq!(m.key, name, "manifest.key is not the directory name");
    assert_eq!(m.key.len(), 64);
    assert!(m.key.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(m.bundle_schema_version, m.identity.bundle_schema_version);
    assert_eq!(m.generator_version, m.identity.generator_version);
    assert_eq!(
        m.tolerances.contract_version,
        m.identity.tolerance_contract_version
    );
    // Every file the identity names is on disk, at the size recorded.
    for f in &m.identity.fixtures {
        let name = format!("{}.npy", f.name);
        let on_disk = std::fs::metadata(dir.join(&name))
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .len();
        assert_eq!(
            Some(&on_disk),
            m.file_bytes.get(&name),
            "{name} is {on_disk} bytes; the manifest says otherwise"
        );
    }
    assert_eq!(
        m.data_bytes,
        m.file_bytes.values().sum::<u64>(),
        "data_bytes is not the sum of file_bytes"
    );
    assert!(
        m.data_bytes + (std::fs::metadata(dir.join("manifest.json")).unwrap().len())
            <= conformance::MAX_BUNDLE_BYTES,
        "the committed bundle is over the {} byte budget (RV9)",
        conformance::MAX_BUNDLE_BYTES
    );
}

/// A bundle that agrees with itself and says nothing.
///
/// `regression.rs:150`'s guard, generalised: row counts above a stated
/// minimum, no all-zero fixture, and — the part that matters most for F16.3 —
/// every named branch-point sampler present somewhere.
#[test]
fn the_bundle_is_not_vacuous() {
    let (dir, m) = manifest();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for f in &m.identity.fixtures {
        let floor = MIN_ROWS
            .iter()
            .find(|(n, _)| *n == f.name)
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("{} has no stated row minimum", f.name));
        assert!(
            f.rows >= floor,
            "{} has {} rows, below the stated minimum of {floor}. A fixture that shrank \
             would still match and would prove nothing.",
            f.name,
            f.rows
        );
        assert!(!f.samplers.is_empty(), "{} names no sampler", f.name);
        seen.extend(f.samplers.iter().cloned());

        // **The column names are the contract**, so they have to be a key:
        // a port reading by name could not tell two columns called `t`
        // apart, and a port reading by position is one insertion away from
        // comparing the wrong thing either way.
        let mut columns = BTreeSet::new();
        for name in f.input_columns.iter().chain(f.output_columns.iter()) {
            assert!(
                columns.insert(name.clone()),
                "{}: two columns called `{name}`",
                f.name
            );
        }

        let bytes = std::fs::read(dir.join(format!("{}.npy", f.name))).expect("a fixture");
        let file = npy::decode_f64_2d(&bytes).expect("a readable fixture");
        let n_in = f.input_columns.len();
        let mut nonzero = 0usize;
        let mut zero_column = vec![true; f.output_columns.len()];
        for r in 0..file.rows {
            for (c, v) in file.row(r)[n_in..].iter().enumerate() {
                assert!(v.is_finite(), "{}: a non-finite output at row {r}", f.name);
                if *v != 0.0 {
                    nonzero += 1;
                    zero_column[c] = false;
                }
            }
        }
        // A zero tolerance is only honest for a column that really is
        // structurally zero. Record which ones those are, so the check below
        // can tell "zero by construction" from "zero because nothing
        // sampled it".
        for (c, name) in f.output_columns.iter().enumerate() {
            let bound = m
                .tolerances
                .tiers
                .iter()
                .flat_map(|t| t.quantities.iter())
                .find(|q| q.fixture == f.name && q.quantity == *name);
            if let Some(q) = bound {
                if q.absolute == 0.0 {
                    assert!(
                        zero_column[c],
                        "{}/{name} carries a zero bound but is not identically zero",
                        f.name
                    );
                    assert_eq!(q.measured_scale, 0.0);
                }
            }
        }
        let total = file.rows * f.output_columns.len();
        assert!(
            nonzero * 4 > total,
            "{}: only {nonzero} of {total} recorded outputs are non-zero; a comparison \
             against a mostly-zero matrix proves little",
            f.name
        );
    }
    for required in REQUIRED_SAMPLERS {
        assert!(
            seen.contains(required),
            "no fixture names the `{required}` sampler. F16.3's whole point is that \
             uniform sampling never hits the places a port differs; a bundle without the \
             branch-point rows agrees with itself and misses them."
        );
    }
    assert!(
        m.tolerances.tiers.len() == 3,
        "the contract must carry all three tiers"
    );
    for tier in &m.tolerances.tiers {
        assert!(
            !tier.rule.trim().is_empty(),
            "tier {} carries no derivation; F16.2 requires the tolerance and its \
             justification, per tier",
            tier.tier
        );
        assert!(
            !tier.quantities.is_empty(),
            "tier {} bounds nothing",
            tier.tier
        );
        for q in &tier.quantities {
            assert!(
                q.absolute >= 0.0 && q.absolute.is_finite(),
                "{}/{} has a negative or non-finite bound",
                q.fixture,
                q.quantity
            );
            assert!(
                !q.unit.is_empty(),
                "{}/{} has no unit; F16.2 requires units per column",
                q.fixture,
                q.quantity
            );
        }
    }
    // The measured F16.6 gaps have to be measurements, not placeholders.
    let k = &m.tolerances.wind_kernel;
    assert!(k.kernel_vs_libm_absolute > 0.0 && k.kernel_vs_libm_absolute < 1e-13);
    assert!(k.summation_vs_compensated_absolute >= 0.0);
    assert!(!k.derivation.trim().is_empty());
    eprintln!(
        "conformance: wave vs libm {:e} at theta {}; fixed-order sum vs compensated {:e} m/s",
        k.kernel_vs_libm_absolute, k.kernel_vs_libm_at, k.summation_vs_compensated_absolute
    );
}

/// `parameters.json` is the shipped catalogue, verbatim (F16.4).
#[test]
fn parameters_json_is_the_shipped_catalogue() {
    let (dir, m) = manifest();
    let text = std::fs::read_to_string(dir.join("parameters.json")).expect("parameters.json");
    let read: BoatParameters = serde_json::from_str(&text).expect("a catalogue");
    assert_eq!(read, BoatParameters::ilca7());
    assert_eq!(read, m.identity.parameters);
    assert_eq!(m.identity.dt.to_bits(), read.sim.dt.to_bits());
    assert_eq!(m.identity.integrator, read.sim.integrator);
}

/// `wind_modes.json` really is the whole of the field's randomness (F16.7).
///
/// A field rebuilt from the shipped table reproduces `sample` bit for bit, at
/// the very points `tier0_wind_sample` records — which is the property that
/// lets a port read twelve numbers instead of implementing PCG32.
#[test]
fn the_shipped_wind_modes_rebuild_the_field() {
    let (dir, m) = manifest();
    if !toolchain_agrees(&m) {
        return;
    }
    #[derive(serde::Deserialize)]
    struct Xy {
        x: f64,
        y: f64,
    }
    #[derive(serde::Deserialize)]
    struct Mode {
        k: Xy,
        amp: Xy,
        omega: f64,
        phase: f64,
    }
    #[derive(serde::Deserialize)]
    struct Entry {
        name: String,
        seed: u64,
        config: WindConfig,
        modes: Vec<Mode>,
    }

    let text = std::fs::read_to_string(dir.join("wind_modes.json")).expect("wind_modes.json");
    let entries: Vec<Entry> = serde_json::from_str(&text).expect("the wind table");
    assert_eq!(entries.len(), m.identity.wind.len());

    let bytes = std::fs::read(dir.join("tier0_wind_sample.npy")).expect("the wind fixture");
    let file = npy::decode_f64_2d(&bytes).expect("a readable fixture");

    let mut checked = 0usize;
    for (i, e) in entries.iter().enumerate() {
        let id = &m.identity.wind[i];
        assert_eq!(id.name, e.name);
        assert_eq!(id.seed, e.seed);
        assert_eq!(id.mode_count, e.modes.len());

        // Drawn from the seed, and read from the file: the same table.
        let drawn = ProceduralWind::new(e.config, e.seed);
        assert_eq!(drawn.mode_count(), e.modes.len(), "{}", e.name);
        for (k, (a, b)) in drawn.modes().iter().zip(e.modes.iter()).enumerate() {
            for (x, y, what) in [
                (a.k().x, b.k.x, "k.x"),
                (a.k().y, b.k.y, "k.y"),
                (a.amp().x, b.amp.x, "amp.x"),
                (a.amp().y, b.amp.y, "amp.y"),
                (a.omega(), b.omega, "omega"),
                (a.phase(), b.phase, "phase"),
            ] {
                assert_eq!(x.to_bits(), y.to_bits(), "{} mode {k} {what}", e.name);
            }
        }

        let rebuilt = ProceduralWind::from_modes(
            e.config,
            e.modes
                .iter()
                .map(|md| {
                    WindMode3::new(
                        Vec2::new(md.k.x, md.k.y),
                        Vec2::new(md.amp.x, md.amp.y),
                        md.omega,
                        md.phase,
                    )
                })
                .collect(),
        );
        for r in 0..file.rows {
            let row = file.row(r);
            if row[0] as usize != i {
                continue;
            }
            let got = rebuilt.sample(row[1], row[2], row[3]);
            assert_eq!(got.x.to_bits(), row[4].to_bits(), "{} row {r} wx", e.name);
            assert_eq!(got.y.to_bits(), row[5].to_bits(), "{} row {r} wy", e.name);
            checked += 1;
        }
    }
    assert_eq!(checked, file.rows, "every recorded sample must be covered");
    eprintln!("conformance: {checked} wind samples reproduced from the shipped modes alone");
}

/// The `GZ` representation in the manifest rebuilds the curve (task 2.3).
#[test]
fn the_shipped_gz_representation_rebuilds_the_curve() {
    let (_, m) = manifest();
    if !toolchain_agrees(&m) {
        return;
    }
    let r: GzRepresentation = m.gz.clone();
    let rebuilt = GzCurve::from_representation(&r).expect("the shipped representation");
    let live = GzCurve::from_params(&BoatParameters::ilca7());
    assert_eq!(rebuilt, live, "the shipped curve is not this build's curve");

    // And the tier-0 fixture's `gz`/`dgz`/`gz_integral` columns are that
    // curve, so the representation and the data cannot drift apart.
    let n = 4001;
    for i in 0..n {
        let phi =
            -std::f64::consts::PI + 2.0 * std::f64::consts::PI * (i as f64) / ((n - 1) as f64);
        assert_eq!(rebuilt.gz(phi).to_bits(), live.gz(phi).to_bits());
        assert_eq!(rebuilt.dgz(phi).to_bits(), live.dgz(phi).to_bits());
        assert_eq!(
            rebuilt.gz_integral(phi).to_bits(),
            live.gz_integral(phi).to_bits()
        );
    }
}
