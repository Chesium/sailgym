//! Deterministic scenario regression against golden trajectories
//! (brief section 34, F9).
//!
//! R7: golden files are only valid for the build that produced them. Each
//! golden file records its toolchain; a mismatch must skip with a clear
//! message rather than fail confusingly. Golden files themselves arrive in
//! section 09; section 01 lands the target and the skip convention.

/// Toolchain fingerprint golden files are recorded against.
/// Compare against the `toolchain` field of a golden file before asserting.
fn toolchain_fingerprint() -> String {
    format!(
        "rustc {} / {} / {}",
        option_env!("CARGO_PKG_RUST_VERSION").unwrap_or("stable"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

#[test]
fn no_golden_files_yet() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    if !dir.exists() {
        eprintln!(
            "skip: no golden trajectories recorded yet (section 09); \
             current toolchain fingerprint = {}",
            toolchain_fingerprint()
        );
        return;
    }
    panic!("tests/golden exists but no regression comparison is implemented yet");
}
