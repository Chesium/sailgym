//! The mechanical audit of section 07's prohibited-shortcut list.
//!
//! Brief §7, §16, §17 and §46 all forbid the same thing from different angles:
//! the boat must heel, capsize and recover because of the forces acting on it,
//! and **not** because some rule noticed what the sailor did. Reviewers forget;
//! a grep does not. Each test below is one line of that list, asserted against
//! the physics crate's own source.
//!
//! ## Test code is excluded, deliberately
//!
//! [`code_lines`] drops every `#[cfg(test)]` item before scanning. A test named
//! `release_depowers` is a *proof* that the sail depowers when the sheet is
//! released — precisely the emergent behaviour these audits exist to protect —
//! and it would otherwise trip `no_release_recovery_rule`. The audits are about
//! what the shipped physics does; test names are evidence, not rules.
//!
//! ## Each of these has been proven able to fail
//!
//! Section 07 introduced each forbidden pattern into the source in turn,
//! confirmed the matching test failed, and reverted it. The runs are recorded
//! in `docs/progress/07-handoff.md`. An audit that cannot fail is not an audit.

use std::path::{Path, PathBuf};

/// Every `.rs` file under `crates/sailgym-physics/src`, sorted, so the scan is
/// deterministic (F9.3) and reports the same first offender every run.
fn sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
            .map(|e| e.expect("directory entry").path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    walk(&root, &mut out);
    assert!(out.len() > 20, "the source scan found almost nothing");
    out
}

/// The path as the repository spells it, e.g. `hydro/hull.rs`.
fn relative(path: &Path) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    path.strip_prefix(&root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// `(line number, text)` for every line of a file that is **not** inside a
/// `#[cfg(test)]` item.
///
/// `rustfmt` closes a module or a free function at column 0, which is what the
/// scan keys on; step 1 of the gate is what keeps that true.
fn code_lines(source: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut skipping = false;
    for (i, line) in source.lines().enumerate() {
        if skipping {
            if line == "}" {
                skipping = false;
            }
            continue;
        }
        if line.trim() == "#[cfg(test)]" {
            skipping = true;
            continue;
        }
        out.push((i + 1, line));
    }
    out
}

/// Non-test lines of every source file, as `(path, line number, text)`.
fn code() -> Vec<(String, usize, String)> {
    let mut out = Vec::new();
    for path in sources() {
        let text = std::fs::read_to_string(&path).expect("source file");
        let name = relative(&path);
        for (n, line) in code_lines(&text) {
            out.push((name.clone(), n, line.to_string()));
        }
    }
    out
}

/// True when `needle` occurs in `haystack` as a whole identifier.
fn has_identifier(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    let mut from = 0;
    while let Some(at) = haystack[from..].find(needle) {
        let start = from + at;
        let end = start + needle.len();
        let before = start == 0 || !is_ident(bytes[start - 1]);
        let after = end == bytes.len() || !is_ident(bytes[end]);
        if before && after {
            return true;
        }
        from = start + 1;
    }
    false
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[test]
fn no_capsize_branch_in_physics() {
    // `capsized` is an output (F6.10). Nothing that produces a force or a
    // moment may read it, so nothing can branch on it.
    let flag: String = ["cap", "sized"].concat();
    for (file, n, line) in code() {
        let inside = file.starts_with("forces/")
            || file.starts_with("aero/")
            || file.starts_with("hydro/")
            || file.starts_with("rigging/")
            || file == "dynamics.rs";
        if !inside {
            continue;
        }
        assert!(
            !line.contains(flag.as_str()),
            "{file}:{n} mentions the capsize flag inside the force path: {line}"
        );
    }
}

#[test]
fn no_heel_clamp() {
    // brief §17: the boat must pass dynamically through `|φ| > 90°`. A clamp,
    // a "max heel", or a saturation of any kind would stop it.
    for (file, n, line) in code() {
        for needle in ["phi.clamp", "phi.min(", "phi.max(", "MAX_HEEL"] {
            assert!(
                !line.contains(needle),
                "{file}:{n} clamps the heel angle (`{needle}`): {line}"
            );
        }
    }
}

#[test]
fn no_release_recovery_rule() {
    // brief §46: "No explicit capsize-prevention or 'release causes recovery'
    // rule may be used." The sail depowers because the boom moves and the
    // angle of attack changes, never because a branch noticed the sheet.
    let hits: Vec<String> = code()
        .into_iter()
        .filter(|(_, _, line)| {
            let l = line.to_ascii_lowercase();
            l.contains("depower") || (l.contains("released") && l.contains("heel"))
        })
        .map(|(file, n, line)| format!("{file}:{n}: {line}"))
        .collect();
    assert!(
        hits.is_empty(),
        "a release-causes-recovery rule may have been added:\n{}",
        hits.join("\n")
    );
}

#[test]
fn no_linear_righting_spring() {
    // brief §16: "Do not use only a globally linear restoring spring." The
    // stability model reaches `φ` only through `sin`/`cos`; `φ` is never an
    // operand of a multiplication there, which is exactly what `−k·φ` needs.
    for (file, n, line) in code() {
        if !file.starts_with("stability/") {
            continue;
        }
        let code_part = line.split("//").next().unwrap_or("");
        assert!(
            !multiplies_phi(code_part),
            "{file}:{n} multiplies the heel angle directly — a linear righting \
             spring is forbidden (brief 16): {line}"
        );
    }
}

/// True when the heel angle appears as a direct operand of `*`.
///
/// `phi.sin_cos()` and `curve.gz(phi)` are fine — `φ` reaches the model
/// through a trigonometric function. `k * phi`, `phi * k`, `-k*phi` and
/// `-k * st.phi` are not: the field-access chain in front of the name is
/// walked back over, so writing the same product through a struct hides
/// nothing.
fn multiplies_phi(code: &str) -> bool {
    let bytes = code.as_bytes();
    let mut from = 0;
    while let Some(at) = code[from..].find("phi") {
        let start = from + at;
        let end = start + 3;
        from = start + 1;
        if start > 0 && is_ident(bytes[start - 1]) {
            continue;
        }
        // A bare `phi`, not `phi_v`, `phi.sin()` or `phi!`.
        if end < bytes.len() && (is_ident(bytes[end]) || bytes[end] == b'.' || bytes[end] == b'!') {
            continue;
        }
        // Walk back over `st.`, `self.state.` and the like, so the operand
        // boundary is the one an expression really has.
        let mut head = code[..start].trim_end();
        while let Some(rest) = head.strip_suffix('.') {
            let cut = rest.trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_');
            if cut.len() == rest.len() {
                head = rest;
                break;
            }
            head = cut.trim_end();
        }
        let before = head.chars().next_back();
        let after = code[end..].trim_start().chars().next();
        if before == Some('*') || after == Some('*') {
            return true;
        }
    }
    false
}

#[test]
fn no_maneuver_state() {
    // brief §9: tacking and gybing must emerge from the sail and boom
    // dynamics. There is no state that knows which tack the boat is on, so
    // there is nothing for a manoeuvre rule to hang off.
    for (file, n, line) in code() {
        let code_part = line.split("//").next().unwrap_or("");
        for needle in [
            "tacking",
            "gybing",
            "maneuver",
            "port_tack",
            "starboard_tack",
        ] {
            assert!(
                !has_identifier(code_part, needle),
                "{file}:{n} carries a manoeuvre state (`{needle}`): {line}"
            );
        }
    }
}
