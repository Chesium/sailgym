//! Captures the toolchain that compiled this crate, for R7.
//!
//! Golden regression trajectories are only valid for the build that produced
//! them (F9, "bit-identical for the same build on the same platform"). The
//! only honest way to say *which* build is to ask the compiler at build time,
//! so `recording::ToolchainInfo` reads the three values emitted here rather
//! than guessing from `cfg!`.
//!
//! Nothing here reaches the simulation: these are string constants baked into
//! the binary, read only by the recorder's header and by the regression
//! harness. The physics never sees them (F9.1).

use std::process::Command;

fn main() {
    // The build script's own fingerprint already includes the compiler
    // version, so a toolchain change re-runs it without watching anything
    // else.
    println!("cargo:rerun-if-changed=build.rs");

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
}
