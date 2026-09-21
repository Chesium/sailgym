//! Write the conformance bundle and the document that describes it
//! (section 02 task 2.4; v2 F16).
//!
//! ```text
//! cargo run --release -p sailgym-bench --bin gen_conformance
//! cargo run --release -p sailgym-bench --bin gen_conformance -- --write docs/v2/conformance.md
//! ```
//!
//! **Run deliberately, never automatically.** The bundle is committed, and
//! `crates/sailgym-physics/tests/conformance.rs` — a gate step 4 test —
//! compares the committed copy against what current source produces. That is
//! the whole arrangement: a bundle no trusted implementation checks is a
//! bundle that silently goes stale, and a stale bundle is the failure that
//! sends a porting team hunting a phantom bug for a day.
//!
//! ## It refuses to run on a dirty physics tree
//!
//! Copied from `gen_golden.rs`, for the same reason and with the same
//! narrowing: `--allow-dirty` requires `--declare "<what is changing>"`, the
//! declaration and the dirty file list are written into `manifest.json`, and
//! the runner refuses a non-baseline bundle that declares nothing. Refusing
//! equally when git cannot be consulted at all is deliberate — "I could not
//! check" is not "it is clean".
//!
//! ## What it checks before it is finished
//!
//! * every `.npy` it wrote re-reads through the task's own reader bit for bit;
//! * the whole directory is at or under the 2 MB budget (RV9), printed either
//!   way;
//! * no fixture is empty and no fixture's outputs are all zero, because a
//!   comparison against a zero matrix proves nothing;
//! * the single-catalogue premise holds: no shipped scenario overrides a
//!   parameter, so `parameters.json` really is the catalogue every fixture
//!   was generated under.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use sailgym_physics::identity::{ModelIdentity, SourceState};
use sailgym_physics::parameters::BoatParameters;
use sailgym_physics::scenario::{load_shipped, shipped_names};
use sailgym_physics::testkit::npy;

#[path = "../conformance/mod.rs"]
mod conformance;

use conformance::{Bundle, Context, MAX_BUNDLE_BYTES};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Uncommitted changes under `crates/sailgym-physics/src`, as git reports
/// them. An empty string means the tree is clean there.
fn dirty_physics_tree() -> Result<String, String> {
    let src = repo_root().join("crates/sailgym-physics/src");
    let output = Command::new("git")
        .arg("status")
        .arg("--porcelain")
        .arg("--")
        .arg(&src)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git status failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn flag(name: &str) -> bool {
    std::env::args().any(|a| a == name)
}

fn value(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
        .filter(|t| !t.trim().is_empty())
}

fn main() {
    if cfg!(debug_assertions) {
        eprintln!(
            "gen_conformance: NOTE — debug build. The bundle is bit-identical either way \
             (the arithmetic does not change), but the tier-2 reference study runs every \
             case three times and takes minutes. Use --release."
        );
    }
    let identity = ModelIdentity::current();
    println!("gen_conformance: identity {}", identity.describe());

    let allow_dirty = flag("--allow-dirty");
    let declare = value("--declare");
    if allow_dirty && declare.is_none() {
        eprintln!(
            "gen_conformance: refusing to run — --allow-dirty requires \
             --declare \"<what is changing>\".\n\
             A bundle written from an unidentified tree has to say what it describes, \
             or it is an unreviewable claim (v2 F18.1d, RV51)."
        );
        std::process::exit(2);
    }

    let mut declared_changes = String::new();
    match dirty_physics_tree() {
        Err(why) if !allow_dirty => {
            eprintln!("gen_conformance: refusing to run — {why}");
            std::process::exit(2);
        }
        Ok(dirty) if !dirty.is_empty() && !allow_dirty => {
            eprintln!(
                "gen_conformance: refusing to run — crates/sailgym-physics/src has \
                 uncommitted changes:\n{dirty}\n\
                 A conformance bundle says \"this is what the physics produces\". \
                 Generating one against an uncommitted edit would bless it silently. \
                 Commit or stash first, or pass --allow-dirty --declare \"<what is \
                 changing>\" if the source and the bundle have to land together."
            );
            std::process::exit(2);
        }
        Ok(dirty) if !dirty.is_empty() => {
            eprintln!(
                "gen_conformance: WARNING — --allow-dirty given and \
                 crates/sailgym-physics/src is dirty:\n{dirty}\n\
                 The bundle below describes the working tree, not a commit. The manifest \
                 records that, and what you declared."
            );
            declared_changes = format!(
                "{}\n\nGenerated from an uncommitted working tree. \
                 git status --porcelain -- crates/sailgym-physics/src:\n{dirty}",
                declare.clone().unwrap_or_default()
            );
        }
        Err(why) => {
            eprintln!("gen_conformance: WARNING — --allow-dirty given and {why}");
            declared_changes = format!(
                "{}\n\nThe source identity could not be established: {why}",
                declare.clone().unwrap_or_default()
            );
        }
        Ok(_) => {}
    }

    // The two halves have to agree, or the bundle would claim a baseline the
    // build does not have (RV52, copied from gen_golden).
    match identity.source.state {
        SourceState::Clean if !declared_changes.is_empty() => {
            eprintln!(
                "gen_conformance: refusing to run — the tree is dirty but the compiled-in \
                 identity says clean. build.rs did not re-run; touch a source file or \
                 `cargo clean -p sailgym-physics` and try again (RV52)."
            );
            std::process::exit(2);
        }
        SourceState::Dirty | SourceState::Unknown if declared_changes.is_empty() => {
            eprintln!(
                "gen_conformance: refusing to run — the compiled-in identity is not a \
                 baseline ({}) but the working tree looks clean. The binary is stale; \
                 rebuild before generating (RV52).",
                identity.describe()
            );
            std::process::exit(2);
        }
        _ => {}
    }

    // The bundle covers **one** parameter catalogue (the task 2.4 debt).
    // That is only true because no shipped scenario overrides a parameter,
    // and it is the kind of premise that silently stops holding.
    for name in shipped_names() {
        let sc = load_shipped(name).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            sc.parameter_overrides.is_empty(),
            "scenario `{name}` overrides {:?}; the bundle ships one catalogue \
             (parameters.json) and every fixture assumes it. Either drop the override or \
             extend the bundle to carry a catalogue per case.",
            sc.parameter_overrides.keys().collect::<Vec<_>>()
        );
        assert_eq!(
            sc.to_parameters().unwrap_or_else(|e| panic!("{name}: {e}")),
            BoatParameters::ilca7(),
            "scenario `{name}` resolves to a catalogue that is not ilca7"
        );
    }

    let ctx = Context::new();
    let bundle = conformance::build(&ctx, &declared_changes);
    let key = bundle.manifest.digest.clone();
    let dir = repo_root().join("conformance").join(&key);

    check(&ctx, &bundle);

    std::fs::create_dir_all(&dir).expect("the bundle directory must be creatable");
    let mut total: u64 = 0;
    for (name, bytes) in &bundle.files {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap_or_else(|e| panic!("cannot write {name}: {e}"));
        total += bytes.len() as u64;
        println!("  {name:<26} {:>9} bytes", bytes.len());
    }
    println!("  {:<26} {total:>9} bytes total", "");

    // RV9, asserted by the generator and not by review.
    assert!(
        total <= MAX_BUNDLE_BYTES,
        "the bundle is {total} bytes, over the {MAX_BUNDLE_BYTES}-byte budget of task 2.4. \
         Shrink a fixture rather than raising the budget: the budget is what keeps \
         committing this data defensible."
    );
    println!(
        "gen_conformance: {} of the {MAX_BUNDLE_BYTES}-byte budget used ({:.1} %)",
        total,
        100.0 * (total as f64) / (MAX_BUNDLE_BYTES as f64)
    );
    println!("gen_conformance: wrote conformance/{key}/");

    // A bundle under a different key is a bundle nothing reads. Say so
    // loudly rather than deleting anything.
    let root = repo_root().join("conformance");
    let stale: BTreeSet<String> = std::fs::read_dir(&root)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| *n != key)
                .collect()
        })
        .unwrap_or_default();
    if !stale.is_empty() {
        eprintln!(
            "gen_conformance: WARNING — conformance/ also holds {stale:?}, which nothing \
             reads. Delete them by hand; this command never removes a committed artifact."
        );
    }

    if let Some(path) = value("--write") {
        let doc = document(&bundle, total);
        std::fs::write(&path, &doc).unwrap_or_else(|e| panic!("{path}: {e}"));
        eprintln!("gen_conformance: wrote {path}");
    } else {
        eprintln!(
            "gen_conformance: pass `--write docs/v2/conformance.md` to regenerate the \
             document as well."
        );
    }
}

/// Everything the generator checks before it writes.
fn check(ctx: &Context, bundle: &Bundle) {
    for (name, bytes) in &bundle.files {
        if !name.ends_with(".npy") {
            continue;
        }
        let back =
            npy::decode_f64_2d(bytes).unwrap_or_else(|e| panic!("{name} does not read back: {e}"));
        let re = npy::encode_f64_2d(back.rows, back.cols, &back.data);
        assert_eq!(&re, bytes, "{name} does not round-trip byte for byte");
    }

    for fx in conformance::fixtures(ctx) {
        let rows = fx.rows();
        assert!(rows > 0, "{} has no rows", fx.name);
        let outputs = fx.outputs(ctx);
        assert!(
            outputs.iter().any(|v| *v != 0.0),
            "{} recomputed to all zeros; a match against it would prove nothing",
            fx.name
        );
        assert!(
            outputs.iter().all(|v| v.is_finite()),
            "{} produced a non-finite value; a conformance fixture may not carry one",
            fx.name
        );
        assert!(
            !fx.samplers.is_empty(),
            "{} names no sampler, so a failing row cannot be traced to a hazard (F16.3)",
            fx.name
        );
        // The column names are the contract, so they have to be a key. Two
        // columns called `t` would leave a name-keyed reader guessing, which
        // is exactly what naming them was for.
        let mut seen = BTreeSet::new();
        for name in fx.input_columns.iter().chain(fx.output_columns.iter()) {
            assert!(
                seen.insert(name.clone()),
                "{} has two columns called `{name}`; a port reading by name could not \
                 tell them apart",
                fx.name
            );
        }
    }
}

/// `docs/v2/conformance.md`, in the house style of `docs/v1/convergence.md`
/// and `docs/v1/performance.md`: every figure produced by a command in the
/// repository, never typed by hand.
fn document(bundle: &Bundle, total: u64) -> String {
    let m = &bundle.manifest;
    let mut d = String::new();
    let _ = writeln!(d, "# The conformance bundle\n");
    let _ = writeln!(
        d,
        "Generated by `{} -- --write docs/v2/conformance.md`. **Do not edit by hand**: \
         every number below is measured on the build that wrote it, and a tolerance \
         retyped into this file is a tolerance that has stopped describing anything \
         (v2 F16.2, task 2.4).\n",
        conformance::GENERATOR_COMMAND
    );
    let _ = writeln!(
        d,
        "Conformance is **implementation agreement, not physical validation**. Two \
         implementations that agree to the last bit may share a modelling mistake. \
         Nothing here says the boat is right; it says a port is the same boat.\n"
    );
    let _ = writeln!(
        d,
        "**No conformance test may assert equality across stacks** (F16.1). XLA \
         reassociates, a fused multiply-add is not a multiply then an add, GPU reductions \
         are not order-deterministic unless forced, and JAX defaults to f32. What is \
         asserted is a tolerance, tiered by how much error has had a chance to \
         accumulate. The one bit-for-bit comparison in the repository — \
         `cargo test -p sailgym-physics --test conformance` — is Rust against Rust on the \
         same build, and it is a **staleness** check on this artifact, not a cross-stack \
         claim.\n"
    );
    let _ = writeln!(d, "---\n");

    let _ = writeln!(d, "## Identity\n");
    let _ = writeln!(d, "| | |");
    let _ = writeln!(d, "|---|---|");
    let _ = writeln!(d, "| digest | `{}` |", m.digest);
    let _ = writeln!(d, "| directory | `conformance/{}/` |", m.digest);
    let _ = writeln!(d, "| bundle schema | {} |", m.bundle_schema_version);
    let _ = writeln!(d, "| generator | {} |", m.generator_version);
    let _ = writeln!(
        d,
        "| tolerance contract | {} |",
        m.tolerances.contract_version
    );
    let _ = writeln!(d, "| model | {} |", m.identity.model.describe());
    let _ = writeln!(d, "| toolchain | {} |", m.toolchain.describe());
    let _ = writeln!(
        d,
        "| integrator / `dt` | {:?} / {} s |",
        m.identity.integrator, m.identity.dt
    );
    let _ = writeln!(
        d,
        "| size | {total} bytes of the {MAX_BUNDLE_BYTES}-byte budget ({:.1} %) |",
        100.0 * (total as f64) / (MAX_BUNDLE_BYTES as f64)
    );
    let _ = writeln!(d);
    if !m.declared_changes.trim().is_empty() {
        let _ = writeln!(
            d,
            "> **This bundle was generated from a tree git could not certify.** It names \
             no baseline and must not be presented as one. Declared:\n>\n> ```\n> {}\n> ```\n",
            m.declared_changes.replace('\n', "\n> ")
        );
    }
    let _ = writeln!(
        d,
        "The `digest` is SHA-256 over the bundle's **contract**: the schema, generator and \
         tolerance-contract versions, the declared model version, the resolved F7 \
         catalogue, the integrator and `dt`, the wind-mode tables and every fixture's \
         column names and data digest. `model.source` is recorded but deliberately **not** \
         keyed — a commit that touches `crates/sailgym-physics/src` without moving a \
         single sampled number must not rename the directory, and a change that does move \
         a number is caught by that fixture's data digest, which is strictly stronger than \
         a source id (RV10). The full canonical record is in `manifest.json`; equality of \
         records, not of hex strings, is the comparison authority (F16.4).\n"
    );
    let _ = writeln!(d, "---\n");

    let _ = writeln!(d, "## Files\n");
    let _ = writeln!(
        d,
        "| File | Tier | Rows | Inputs | Outputs | Samplers | Bytes |"
    );
    let _ = writeln!(d, "|---|---|---|---|---|---|---|");
    for f in &m.identity.fixtures {
        let bytes = m
            .file_bytes
            .get(&format!("{}.npy", f.name))
            .copied()
            .unwrap_or(0);
        let _ = writeln!(
            d,
            "| `{}.npy` | {} | {} | {} | {} | {} | {bytes} |",
            f.name,
            f.tier,
            f.rows,
            f.input_columns.len(),
            f.output_columns.len(),
            f.samplers
                .iter()
                .map(|s| format!("`{s}`"))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    for name in ["parameters.json", "wind_modes.json"] {
        let _ = writeln!(
            d,
            "| `{name}` | — | — | — | — | — | {} |",
            m.file_bytes.get(name).copied().unwrap_or(0)
        );
    }
    let _ = writeln!(d);
    let _ = writeln!(
        d,
        "Every `.npy` is v1.0, 2-D, `<f8`, row-major: the leading columns are the inputs a \
         port feeds its own implementation and the trailing ones are what this crate \
         produced from them. **The column names are the contract** and are listed below; \
         a port that reads columns by position is one insertion away from silently \
         comparing the wrong thing. `.npy` rather than `.npz` because `.npz` is a zip \
         container and this workspace's dependency graph is `serde`, `serde_json` and a \
         test-only `sha2`.\n"
    );

    let _ = writeln!(d, "### Columns\n");
    for f in &m.identity.fixtures {
        let _ = writeln!(d, "**`{}.npy`** — {} rows\n", f.name, f.rows);
        let _ = writeln!(
            d,
            "- inputs: {}",
            f.input_columns
                .iter()
                .map(|c| format!("`{c}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let _ = writeln!(
            d,
            "- outputs: {}\n",
            f.output_columns
                .iter()
                .map(|c| format!("`{c}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let _ = writeln!(d, "---\n");

    let _ = writeln!(d, "## The wind modes are data (F16.7)\n");
    let _ = writeln!(
        d,
        "`ProceduralWind::new` draws `κ_k`, `ϕ_k` and `ω_k` once at construction from the \
         seeded PCG32 stream, and `sample` is a pure, branch-free sum over the result. The \
         drawn table is in `wind_modes.json`, so **no stack other than Rust ever \
         implements PCG32**: a port reads the numbers. `ProceduralWind::from_modes` \
         rebuilds the field from exactly that data and reproduces `sample` bit for bit.\n"
    );
    let _ = writeln!(d, "| Field | Seed | Modes |");
    let _ = writeln!(d, "|---|---|---|");
    for w in &m.identity.wind {
        let _ = writeln!(d, "| `{}` | {} | {} |", w.name, w.seed, w.mode_count);
    }
    let _ = writeln!(d);
    let _ = writeln!(d, "---\n");

    let _ = writeln!(d, "## The righting-arm curve is data (task 2.3)\n");
    let _ = writeln!(
        d,
        "`manifest.gz` carries the solved curve as `{{version, form, coefficients}}`. The \
         **coefficient count is data**: v1 had three harmonics and v2 F18.1a has four, and \
         a port that hard-codes either is wrong the next time the representation is \
         corrected.\n"
    );
    let _ = writeln!(d, "```json");
    let _ = writeln!(
        d,
        "{}",
        serde_json::to_string_pretty(&m.gz).unwrap_or_default()
    );
    let _ = writeln!(d, "```\n");
    let _ = writeln!(
        d,
        "`form = \"{}\"` means `GZ(φ) = Σ c_n·sin(nφ)`, with the analytic derivative \
         `Σ n·c_n·cos(nφ)` and the analytic integral `Σ c_n·(1 − cos nφ)/n`. One set of \
         coefficients, not three approximations.\n",
        m.gz.form
    );
    let _ = writeln!(d, "---\n");

    let _ = writeln!(d, "## The measured wind-kernel gap (F16.6)\n");
    let k = &m.tolerances.wind_kernel;
    let _ = writeln!(d, "| | |");
    let _ = writeln!(d, "|---|---|");
    let _ = writeln!(d, "| domain | {} |", k.domain);
    let _ = writeln!(
        d,
        "| max \\|`wave(θ)` − `θ.cos()`\\| | **{:e}** at θ = {} |",
        k.kernel_vs_libm_absolute, k.kernel_vs_libm_at
    );
    let _ = writeln!(
        d,
        "| max \\|fixed-order sum − compensated sum\\| | **{:e}** m/s ({:e} of base speed) |",
        k.summation_vs_compensated_absolute, k.summation_vs_compensated_relative
    );
    let _ = writeln!(d);
    let _ = writeln!(d, "{}\n", k.derivation);
    let _ = writeln!(d, "---\n");

    let _ = writeln!(d, "## Tier 2 — the trajectory cases\n");
    let _ = writeln!(d, "| Case | Scenario | Seconds | Wind | Cues at (s) |");
    let _ = writeln!(d, "|---|---|---|---|---|");
    for c in &m.trajectory_cases {
        let _ = writeln!(
            d,
            "| `{}` | `{}` | {} | `{}` | {} |",
            c.name,
            c.scenario,
            c.seconds,
            c.wind,
            c.cues
                .iter()
                .map(|q| format!("{}", q.t))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let _ = writeln!(d);
    let _ = writeln!(
        d,
        "`sheet_transient` is the case F11's **R1** names: `k_sheet = 2e4 N/m` with \
         `I_b = 12 kg·m²` gives `ω ≈ 41 rad/s`, about 30 steps per period at \
         `dt = 0.005`, and v2 F18.1b made the tension law discontinuous at take-up. It is \
         the stiffest mode in the model and the first place a lower-precision port \
         degrades.\n"
    );
    let _ = writeln!(d, "---\n");

    let _ = writeln!(d, "## The tolerance contract\n");
    for note in &m.tolerances.notes {
        let _ = writeln!(d, "- {note}");
    }
    let _ = writeln!(d);

    for tier in &m.tolerances.tiers {
        let _ = writeln!(d, "### Tier {}\n", tier.tier);
        let _ = writeln!(d, "{}\n", tier.rule);
        let _ = writeln!(d, "**Domains.**\n");
        for (name, domain) in &tier.domains {
            let _ = writeln!(d, "- `{name}`: {domain}");
        }
        let _ = writeln!(d);
        if tier.tier == 2 {
            let _ = writeln!(
                d,
                "| Case / quantity | Unit | Scale | `|x(dt) − x(dt/4)|` | at `dt/2` | Floor | **Tolerance** |"
            );
            let _ = writeln!(d, "|---|---|---|---|---|---|---|");
            for q in &tier.quantities {
                let mr = q.measured.as_ref().expect("tier 2 carries its measurement");
                let _ = writeln!(
                    d,
                    "| `{}` | {} | {:e} | {:e} | {:e} | {:e} | **{:e}** |",
                    q.quantity,
                    q.unit,
                    q.measured_scale,
                    mr.reference_error_dt,
                    mr.reference_error_half_dt,
                    mr.floor,
                    q.absolute,
                );
            }
        } else {
            let _ = writeln!(
                d,
                "| Fixture | Quantity | Unit | Measured scale | **Absolute** | Relative | ULP |"
            );
            let _ = writeln!(d, "|---|---|---|---|---|---|---|");
            for q in &tier.quantities {
                let _ = writeln!(
                    d,
                    "| `{}` | `{}` | {} | {:e} | **{:e}** | {:e} | {} |",
                    q.fixture,
                    q.quantity,
                    q.unit,
                    q.measured_scale,
                    q.absolute,
                    q.relative,
                    q.ulp.map(|u| u.to_string()).unwrap_or_else(|| "—".into()),
                );
            }
        }
        let _ = writeln!(d);
    }

    let _ = writeln!(d, "---\n");
    let _ = writeln!(d, "## What this bundle does not cover\n");
    let _ = writeln!(
        d,
        "- **No tier 3.** The brief §35 invariants exist for Rust in \
         `crates/sailgym-physics/tests/invariants.rs`; porting them is section 03's \
         problem for the wind slice and a later section's for the rest.\n\
         - **One catalogue, one wind config per mode.** The bundle covers `ilca7` and the \
         shipped scenarios' own fields; a second catalogue would need a second bundle.\n\
         - **f64 only.** F16.8: if a stack trains in f32, the f32↔f64 divergence is \
         measured on this same bundle and reported as a number. If it exceeds the \
         cross-stack tolerance, that f32 environment is not the environment this bundle \
         describes. The escalation is **not** to raise `c_sheet` or lower `k_sheet` \
         (brief §43, R1); it is to sub-step the rigging DOF in the port, or run that DOF \
         in f64.\n\
         - **No RNG comparison.** F16.7: no stack other than Rust implements PCG32, and no \
         conformance test compares RNG streams across stacks.\n"
    );
    d
}
