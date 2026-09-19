//! Regenerate the golden trajectories (task 9.6).
//!
//! **Run deliberately, never automatically.**
//!
//! ```text
//! cargo run -p sailgym-bench --bin gen_golden
//! ```
//!
//! The control scripts and the runner are shared with the regression suite
//! through `#[path]` below, so the file this writes and the trajectory that
//! test recomputes come from one definition. Nothing here is a second
//! implementation of the physics.
//!
//! ## It refuses to run on a dirty physics tree
//!
//! A golden file exists to say "the physics did not change". Regenerating one
//! while `crates/sailgym-physics/src` has uncommitted edits would silently
//! bless whatever is in the working tree, which is exactly the failure the
//! suite is there to prevent. Commit (or stash) first.
//!
//! `--allow-dirty` overrides the refusal, the way `cargo publish` does, and
//! prints a warning naming every dirty file. It exists for exactly one
//! situation — the change that *introduces* the golden files, where the
//! physics they describe is necessarily still uncommitted — and section 09's
//! handoff records the one run that used it. If you find yourself reaching
//! for it to make a failing regression pass, that is the bug.
//!
//! ## R7
//!
//! Each file records the toolchain that produced it. The regression tests
//! skip with an explicit message on a mismatch rather than failing, so a file
//! written here is evidence about *this* build and says so.

use std::path::{Path, PathBuf};
use std::process::Command;

use sailgym_physics::recording::ToolchainInfo;
use sailgym_physics::scenario::{load_shipped, shipped_names};

#[path = "../../../sailgym-physics/tests/golden/script.rs"]
mod script;

use script::golden_for;

/// `crates/sailgym-physics`, from this crate's manifest directory.
fn physics_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../sailgym-physics")
}

/// Uncommitted changes under `crates/sailgym-physics/src`, as git reports
/// them. An empty string means the tree is clean there.
///
/// A repository git cannot answer about at all (no git, no checkout) is
/// reported as such and also refuses: "I could not check" is not "it is
/// clean".
fn dirty_physics_tree() -> Result<String, String> {
    let src = physics_root().join("src");
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

fn main() {
    let toolchain = ToolchainInfo::current();
    println!("gen_golden: toolchain {}", toolchain.describe());
    let allow_dirty = std::env::args().any(|a| a == "--allow-dirty");

    match dirty_physics_tree() {
        Err(why) if !allow_dirty => {
            eprintln!("gen_golden: refusing to run — {why}");
            std::process::exit(2);
        }
        Ok(dirty) if !dirty.is_empty() && !allow_dirty => {
            eprintln!(
                "gen_golden: refusing to run — crates/sailgym-physics/src has \
                 uncommitted changes:\n{dirty}\n\
                 Golden trajectories say \"the physics did not change\". \
                 Regenerating them against an uncommitted edit would bless it \
                 silently. Commit or stash first, or pass --allow-dirty if you \
                 are bootstrapping the files themselves."
            );
            std::process::exit(2);
        }
        Ok(dirty) if !dirty.is_empty() => {
            eprintln!(
                "gen_golden: WARNING — --allow-dirty given and \
                 crates/sailgym-physics/src is dirty:\n{dirty}\n\
                 The files written below describe the working tree, not a \
                 commit. Say so wherever you record them."
            );
        }
        Err(why) => eprintln!("gen_golden: WARNING — --allow-dirty given and {why}"),
        Ok(_) => {}
    }

    let dir = physics_root().join("tests/golden");
    std::fs::create_dir_all(&dir).expect("the golden directory must be creatable");

    for name in shipped_names() {
        let sc = load_shipped(name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let golden = golden_for(&sc);
        let path = dir.join(format!("{name}.json"));
        let mut json = serde_json::to_string_pretty(&golden).expect("a golden file must serialise");
        json.push('\n');
        std::fs::write(&path, &json)
            .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
        println!(
            "  {name:<24} {:>4} samples  {:>8} bytes  -> {}",
            golden.samples.len(),
            json.len(),
            path.display()
        );
    }

    println!(
        "gen_golden: done. `cargo test -p sailgym-physics --test regression` \
         now compares against this build."
    );
}
