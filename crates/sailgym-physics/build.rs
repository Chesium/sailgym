//! Captures the toolchain that compiled this crate (R7), and the content
//! identity of the physics source (v2 F18.1d).
//!
//! Golden regression trajectories are only valid for the build that produced
//! them (F9, "bit-identical for the same build on the same platform"). The
//! only honest way to say *which* build is to ask the compiler at build time,
//! so `recording::ToolchainInfo` reads the three toolchain values emitted here
//! rather than guessing from `cfg!`.
//!
//! ## The source identity (v2 F18.1d)
//!
//! A toolchain is not a model. Sections 10 and 02 need to know whether two
//! recordings describe the **same equations**, and a parameter digest cannot
//! answer that: changing `foil.rs` leaves every parameter untouched.
//!
//! The identity emitted here is git's own content-addressed tree id for
//! `crates/sailgym-physics/src`:
//!
//! ```text
//! git rev-parse HEAD:crates/sailgym-physics/src
//! ```
//!
//! It has exactly the properties F18.1d asks for, and it has them because git
//! already had them — **no cryptographic primitive is implemented here**:
//!
//! * it is a function of the source content alone, so an unrelated commit that
//!   leaves `src/` alone leaves it alone, and any change to any file under
//!   `src/` changes it;
//! * an uncommitted edit under `src/` makes it **dirty**, which is not equal to
//!   any known baseline;
//! * a build outside a git checkout makes it **unknown**, likewise.
//!
//! Nothing here reaches the simulation: these are string constants baked into
//! the binary, read only by `identity`, by the recorder's header and by the
//! regression harness. The physics never sees them (F9.1).

use std::process::Command;

fn main() {
    // The build script's own fingerprint already includes the compiler
    // version, so a toolchain change re-runs it without watching anything
    // else. `src` is watched so that an edit to any physics source file
    // refreshes the identity below (F18.1d, RV52).
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");

    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let version = Command::new(&rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown rustc".to_string());

    println!("cargo:rustc-env=SAILGYM_RUSTC={version}");
    println!(
        "cargo:rustc-env=SAILGYM_TARGET={}",
        std::env::var("TARGET").unwrap_or_else(|_| "unknown target".to_string())
    );
    println!(
        "cargo:rustc-env=SAILGYM_PROFILE={}",
        std::env::var("PROFILE").unwrap_or_else(|_| "unknown profile".to_string())
    );

    let (tree, state) = source_identity();
    println!("cargo:rustc-env=SAILGYM_SOURCE_TREE={tree}");
    println!("cargo:rustc-env=SAILGYM_SOURCE_STATE={state}");
}

/// `(tree id, state)` for `crates/sailgym-physics/src`.
///
/// `state` is one of `clean`, `dirty` or `unknown`. A failure at any step is
/// `unknown` with the reason carried in the tree field, never a guess: F18.1d
/// requires an unknown identity to be explicit, because "I could not check" is
/// not "it did not change".
fn source_identity() -> (String, String) {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let git = |args: &[&str]| -> Result<String, String> {
        let out = Command::new("git")
            .current_dir(&manifest)
            .args(args)
            .output()
            .map_err(|e| format!("cannot run git: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        String::from_utf8(out.stdout)
            .map(|s| s.trim().to_string())
            .map_err(|e| format!("git output is not utf-8: {e}"))
    };

    let tree = match git(&["rev-parse", "HEAD:crates/sailgym-physics/src"]) {
        Ok(t) if !t.is_empty() => t,
        Ok(_) => return ("empty tree id".to_string(), "unknown".to_string()),
        Err(why) => return (why, "unknown".to_string()),
    };
    match git(&["status", "--porcelain", "--", "src"]) {
        Ok(status) if status.is_empty() => (tree, "clean".to_string()),
        Ok(_) => (tree, "dirty".to_string()),
        Err(why) => (format!("{tree} ({why})"), "unknown".to_string()),
    }
}
