//! Deterministic scenario regression against golden trajectories
//! (brief §34, §43, F9, task 9.6).
//!
//! For each of the six shipped scenarios: a fixed 30 s control script —
//! defined in `golden/script.rs`, **not** in the scenario, because brief §32
//! forbids a scenario from scripting anything — producing a trajectory
//! sampled at 5 Hz and compared against a committed file.
//!
//! ## R7, and why this file skips rather than fails
//!
//! F9's determinism guarantee is *same build, same platform*. A golden file
//! is therefore only evidence about the toolchain that produced it, and that
//! toolchain is recorded inside the file. On a mismatch these tests print a
//! skip naming **both** toolchains and pass: a red suite on a different
//! compiler would say "the physics changed", which is not what happened, and
//! people learn to ignore it. What it must never do is pass *silently* — the
//! message is the deliverable.
//!
//! Regenerate deliberately, never automatically:
//!
//! ```text
//! cargo run -p sailgym-bench --bin gen_golden
//! ```
//!
//! `gen_golden` refuses to run while `crates/sailgym-physics/src` has
//! uncommitted changes, so goldens cannot be regenerated to paper over a
//! physics change in progress.
//!
//! ## The identity record (v2 F18.1d)
//!
//! A toolchain says which compiler; it does not say which equations. Each
//! golden now also carries `ModelIdentity` — the declared model version and
//! git's content id for `crates/sailgym-physics/src` — and, when that identity
//! is not a baseline, the `--declare` text `gen_golden` required before it
//! would write the file.
//!
//! [`compare`] prints both identities every run and **fails** a file whose
//! identity is `dirty` or `unknown` and declares nothing: that is RV51, golden
//! laundering, and it is the one shape of this file that is not reviewable. It
//! does not fail on a *different* clean identity, because the trajectory
//! comparison below is the authority on whether the physics moved and an
//! identity mismatch with matching trajectories means only that some source
//! file changed without changing any of these six trajectories.

use std::path::{Path, PathBuf};

use sailgym_physics::identity::ModelIdentity;
use sailgym_physics::recording::ToolchainInfo;
use sailgym_physics::scenario::load_shipped;
use sailgym_physics::state::{STATE_FIELDS, STATE_LEN};

#[path = "golden/script.rs"]
mod script;

use script::{
    run_golden, Golden, GOLDEN_DURATION_S, GOLDEN_SAMPLE_HZ, GOLDEN_SCHEMA_VERSION, TOL_POSITION,
    TOL_VELOCITY,
};

fn golden_path(scenario: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{scenario}.json"))
}

/// Per-field absolute tolerance, by F8.3 index.
///
/// Positions and angles at `1e-9`; the body-frame velocities and rates —
/// `u`, `v`, `r`, `p` and `β̇` — at `1e-10` (task 9.6).
fn tolerance(index: usize) -> f64 {
    match STATE_FIELDS[index] {
        "u" | "v" | "r" | "p" | "beta_dot" => TOL_VELOCITY,
        _ => TOL_POSITION,
    }
}

/// Run one scenario's script and compare it with its committed golden.
fn compare(scenario: &str) {
    let path = golden_path(scenario);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}\n\
             golden trajectories are committed; regenerate with \
             `cargo run -p sailgym-bench --bin gen_golden`",
            path.display()
        )
    });
    let golden: Golden = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{} is not a golden file: {e}", path.display()));

    assert_eq!(
        golden.schema_version,
        GOLDEN_SCHEMA_VERSION,
        "{} was written by another golden schema",
        path.display()
    );
    assert_eq!(golden.scenario, scenario);

    // v2 F18.1d / RV51. A file generated from a tree git could not identify is
    // allowed — the commit that corrects the physics has to carry its own
    // fixtures — but it has to say so, and say what it was correcting.
    let identity = ModelIdentity::current();
    assert_eq!(
        golden.identity.model_version,
        identity.model_version,
        "{} was recorded under model v{}, this build is model v{}. A golden from a \
         different declared model is not a regression fixture for this one; regenerate \
         deliberately and record the migration.",
        path.display(),
        golden.identity.model_version,
        identity.model_version,
    );
    if !golden.identity.source.is_known() {
        assert!(
            !golden.declared_changes.trim().is_empty(),
            "{} records a non-baseline source identity ({}) and declares no changes. \
             That is a fixture nobody can review (RV51): regenerate with \
             `--allow-dirty --declare \"<what is changing>\"`, or from a clean tree.",
            path.display(),
            golden.identity.source.describe(),
        );
        eprintln!(
            "{scenario}: golden recorded from a NON-BASELINE tree — {}\n  declared: {}",
            golden.identity.describe(),
            golden.declared_changes.lines().next().unwrap_or("").trim(),
        );
    }
    eprintln!(
        "{scenario}: golden identity {} | this build {}",
        golden.identity.describe(),
        identity.describe(),
    );

    // R7. The one branch in this file, and it prints rather than passing
    // quietly.
    let current = ToolchainInfo::current();
    if golden.toolchain != current {
        eprintln!(
            "skip: {scenario} golden was recorded on a different toolchain.\n  \
             expected: {}\n  \
             actual:   {}\n  \
             Golden trajectories are only valid for the build that produced them \
             (F9, F11 R7). Regenerate with `cargo run -p sailgym-bench --bin gen_golden` \
             if this toolchain is the one you mean to hold.",
            golden.toolchain.describe(),
            current.describe(),
        );
        return;
    }

    let sc = load_shipped(scenario).expect("a shipped scenario");
    assert_eq!(golden.dt, sc.to_parameters().unwrap().sim.dt);
    assert_eq!(golden.duration_s, GOLDEN_DURATION_S);
    assert_eq!(golden.sample_hz, GOLDEN_SAMPLE_HZ);
    assert_eq!(
        golden.fields,
        STATE_FIELDS
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        "the golden file's field order is not the F8.3 layout"
    );

    let produced = run_golden(&sc);
    assert_eq!(
        produced.len(),
        golden.samples.len(),
        "{scenario}: {} samples produced against {} recorded",
        produced.len(),
        golden.samples.len()
    );
    // 30 s at 5 Hz, plus the sample at t = 0.
    let expected = (GOLDEN_DURATION_S * GOLDEN_SAMPLE_HZ) as usize + 1;
    assert_eq!(
        produced.len(),
        expected,
        "{scenario}: unexpected sample count"
    );

    let mut worst = (0.0_f64, 0usize, 0usize);
    for (k, (now, then)) in produced.iter().zip(golden.samples.iter()).enumerate() {
        assert_eq!(then.len(), STATE_LEN, "{scenario}: sample {k} is malformed");
        for i in 0..STATE_LEN {
            let delta = (now[i] - then[i]).abs();
            if delta > worst.0 {
                worst = (delta, k, i);
            }
            assert!(
                delta <= tolerance(i),
                "{scenario}: sample {k} (t = {}), field {}: {} vs recorded {} \
                 — |Δ| = {delta:e} exceeds {:e}",
                now[STATE_LEN - 1],
                STATE_FIELDS[i],
                now[i],
                then[i],
                tolerance(i)
            );
        }
    }

    // The trajectory has to have gone somewhere, or a match proves nothing.
    let last = produced.last().expect("at least one sample");
    let moved = last[0].abs() + last[1].abs() + last[2].abs() + last[3].abs();
    assert!(
        moved > 1e-3,
        "{scenario}: the boat never moved, so this regression asserts nothing"
    );
    eprintln!(
        "{scenario}: {} samples, worst |Δ| {:e} at sample {} field {}",
        produced.len(),
        worst.0,
        worst.1,
        STATE_FIELDS[worst.2]
    );
}

#[test]
fn beam_reach_capsize() {
    compare("beam_reach_capsize");
}

#[test]
fn close_hauled() {
    compare("close_hauled");
}

#[test]
fn free_sail() {
    compare("free_sail");
}

#[test]
fn gybe() {
    compare("gybe");
}

#[test]
fn sheet_release_recovery() {
    compare("sheet_release_recovery");
}

#[test]
fn tack() {
    compare("tack");
}
