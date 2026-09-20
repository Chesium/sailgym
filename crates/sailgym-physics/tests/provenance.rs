//! Parameter provenance, made enforceable (section 10, task 10.7).
//!
//! brief §36 and §48 require every important physical parameter to carry an
//! honest KNOWN / ASSUMED / TUNABLE / DEFERRED tag, and F7 adds a rule with
//! teeth: **no numeric literal from the F7 table may appear anywhere outside
//! `parameters.rs`.** brief §23 and F8 add the other half — no physical
//! equation, and no physical constant, in TypeScript.
//!
//! Four audits, plus one this section added because it is the strongest
//! statement brief §43 can be given:
//!
//! | test | what it refuses |
//! |---|---|
//! | [`every_field_tagged`] | a parameter whose tag is missing, or ambiguous |
//! | [`no_stray_constants`] | a physical literal loose in the source |
//! | [`no_physics_in_typescript`] | a density, a `g`, or `½ρV²` on the JS side |
//! | [`docs_match_source`] | `docs/v1/parameters.md` drifting from `parameters.rs` |
//! | [`shipped_values_match_the_f7_table`] | a coefficient quietly tuned since F7 |
//!
//! Gate step 4 runs this target.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use sailgym_physics::parameters::{catalogue, BoatParameters, ParamMeta};

/// The four F7 tags.
const TAGS: [&str; 4] = ["KNOWN", "ASSUMED", "TUNABLE", "DEFERRED"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// ---------------------------------------------------------------------------
// Source scanning
// ---------------------------------------------------------------------------

/// Every `.rs` file under a directory, sorted so the scan is deterministic
/// (F9.3) and reports the same first offender every run.
fn rust_sources(root: &Path) -> Vec<PathBuf> {
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
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

/// `(line number, text)` for every line **not** inside a `#[cfg(test)]` item,
/// with `//` comments stripped.
///
/// Test code is excluded for the reason `tests/no_shortcuts.rs` gives at
/// length: a fixture that says `u = 4.0` is evidence, not a smuggled
/// coefficient. Comments are stripped because a doc comment quoting a value is
/// documentation, which is what this section is asking for more of.
fn code_lines(source: &str) -> Vec<(usize, String)> {
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
        let code = line.split("//").next().unwrap_or("");
        out.push((i + 1, code.to_string()));
    }
    out
}

/// Every float literal on a line: `4.23`, `1e-9`, `2.0e4`, `6_755_399.0`.
fn float_literals(line: &str) -> Vec<(String, f64)> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if !c.is_ascii_digit() {
            i += 1;
            continue;
        }
        // Not a literal if it is part of an identifier or a field access
        // (`vec.0`, `a1`), or a suffix of a longer token.
        if i > 0 {
            let prev = bytes[i - 1] as char;
            if prev.is_ascii_alphanumeric() || prev == '_' || prev == '.' {
                i += 1;
                while i < bytes.len()
                    && ((bytes[i] as char).is_ascii_alphanumeric() || bytes[i] == b'_')
                {
                    i += 1;
                }
                continue;
            }
        }
        let start = i;
        let mut seen_dot = false;
        let mut seen_exp = false;
        while i < bytes.len() {
            let ch = bytes[i] as char;
            if ch.is_ascii_digit() || ch == '_' {
                i += 1;
            } else if ch == '.'
                && !seen_dot
                && !seen_exp
                && i + 1 < bytes.len()
                && (bytes[i + 1] as char).is_ascii_digit()
            {
                seen_dot = true;
                i += 1;
            } else if (ch == 'e' || ch == 'E') && !seen_exp && i + 1 < bytes.len() {
                let next = bytes[i + 1] as char;
                let signed = (next == '-' || next == '+')
                    && i + 2 < bytes.len()
                    && (bytes[i + 2] as char).is_ascii_digit();
                if next.is_ascii_digit() || signed {
                    seen_exp = true;
                    i += if signed { 2 } else { 1 };
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        let text = &line[start..i];
        // Integers with no decimal point and no exponent are counts, indices
        // and array sizes, not physical quantities. `2` is not a coefficient;
        // `2.0` might be.
        if !seen_dot && !seen_exp {
            continue;
        }
        if let Ok(v) = text.replace('_', "").parse::<f64>() {
            out.push((text.to_string(), v));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// every_field_tagged
// ---------------------------------------------------------------------------

#[test]
fn every_field_tagged() {
    // brief §48: every important physical parameter carries an honest tag.
    // "Exactly one" is the point — a doc comment naming two tags is a
    // parameter nobody has decided about, which is how `board.area` and
    // `rudder.area` came to be displayed as KNOWN when they are estimates
    // (section 08 handoff §5).
    let meta = catalogue();
    assert!(
        meta.len() >= 55,
        "the catalogue resolved only {} leaves; that is not the F7 table",
        meta.len()
    );

    let mut counts: BTreeMap<&str, usize> = TAGS.iter().map(|t| (*t, 0)).collect();
    for m in &meta {
        assert!(
            TAGS.contains(&m.tag.as_str()),
            "{}: tag is {:?}, which is not one of {TAGS:?}",
            m.path,
            m.tag
        );
        *counts.get_mut(m.tag.as_str()).expect("a known tag") += 1;

        // Exactly one tag word in the documentation the panel shows.
        let mentions: Vec<&str> = TAGS
            .iter()
            .filter(|t| m.doc.contains(**t))
            .copied()
            .collect();
        assert_eq!(
            mentions.len(),
            1,
            "{}: documentation names {} tags {mentions:?} — exactly one is required.\n{}",
            m.path,
            mentions.len(),
            m.doc
        );
        assert_eq!(
            mentions[0], m.tag,
            "{}: resolved tag disagrees with the doc",
            m.path
        );
    }

    // Every tag must be in use. A catalogue in which nothing is ASSUMED, or
    // nothing is KNOWN, is a catalogue that is not being honest with itself.
    for (tag, n) in &counts {
        assert!(*n > 0, "no parameter is tagged {tag}");
    }
    eprintln!(
        "provenance: {} leaves — {} KNOWN, {} ASSUMED, {} TUNABLE, {} DEFERRED",
        meta.len(),
        counts["KNOWN"],
        counts["ASSUMED"],
        counts["TUNABLE"],
        counts["DEFERRED"]
    );
}

// ---------------------------------------------------------------------------
// no_stray_constants
// ---------------------------------------------------------------------------

/// Literals that carry no physical content and may appear anywhere.
const UNIVERSAL: [f64; 4] = [0.0, 1.0, 2.0, 0.5];

/// Every float literal in non-test physics source outside `parameters.rs` and
/// `constants.rs` that is neither [`UNIVERSAL`] nor part of a named `const`
/// declaration.
///
/// Each entry states where it is and why it is not a physical coefficient.
/// **The list is meant to stay short.** A new entry is a decision that a number
/// belongs in the code rather than in F7, and it has to be argued here.
const EXEMPT: &[(&str, &[f64], &str)] = &[
    (
        "foil.rs",
        &[3.0],
        "smoothstep's `t²(3 − 2t)`; the cubic's own coefficient, not a foil property",
    ),
    (
        "integrator.rs",
        &[6.0],
        "RK4's `1/6` stage weight (F7 `integrator`); arithmetic, not physics",
    ),
    (
        "stability/hydrostatics.rs",
        &[3.0, 4.0],
        "the multiple-angle identities sin 2φ = 2sc, sin 3φ = s(3 − 4s²), cos 3φ = c(4c² − 3), \
         and the `[1, 2, 3]` slope row of the F6.7 fit",
    ),
    (
        "scenario.rs",
        &[90.0, 360.0],
        "compass ↔ ψ: `ψ = 90° − heading` and its inverse, wrapped to 360° (F1, section 09)",
    ),
    (
        "environment/mod.rs",
        &[360.0],
        "bearing wrap in `wind_to_bearing`",
    ),
    (
        "environment/wind.rs",
        &[
            4.166_666_666_666_666_4e-2,
            1.388_888_888_888_889e-3,
            2.480_158_730_158_73e-5,
            2.755_731_922_398_589e-7,
            2.087_675_698_786_81e-9,
            1.147_074_559_772_972_5e-11,
            4.779_477_332_387_385e-14,
            1.561_920_696_858_622_5e-16,
            4.110_317_623_312_165e-19,
        ],
        "the Taylor coefficients 1/4!, 1/6!, … of the hand-rolled cosine kernel \
         (section 03 §1); mathematics, and 1.7× faster than `f64::cos` in WebAssembly",
    ),
    (
        "environment/wind.rs",
        &[5.0, 270.0, 0.15, 120.0, 25.0, 1.5],
        "`WindConfig::default()`. The wind is scenario configuration, not boat \
         parameters: F7 is the catalogue of the *boat* and section 09 gives every \
         shipped scenario its own field (section 03 handoff §2.3)",
    ),
];

#[test]
fn no_stray_constants() {
    // F7's rule, made enforceable: no physical literal loose in the source.
    //
    // A literal is acceptable in exactly three ways. It is universal (`0.0`,
    // `1.0`, `2.0`, `0.5`); or it is the right-hand side of a **named `const`
    // declaration**, which is how `EPS_FLOW`, `EPS_ROPE`, `EPS_DET`,
    // `MODE_BAND` and `HULL_MODEL_VALID_TO` already live — a name and a doc
    // comment in the module that owns them; or it is in `EXEMPT` above with a
    // stated reason.
    let root = repo_root().join("crates/sailgym-physics/src");
    let mut offenders: Vec<String> = Vec::new();
    let mut scanned = 0usize;
    let mut named_consts = 0usize;

    for path in rust_sources(&root) {
        let file = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if file == "parameters.rs" || file == "constants.rs" {
            continue;
        }
        let exempt: Vec<f64> = EXEMPT
            .iter()
            .filter(|(f, _, _)| *f == file)
            .flat_map(|(_, vs, _)| vs.iter().copied())
            .collect();
        let text = std::fs::read_to_string(&path).expect("source file");
        for (n, line) in code_lines(&text) {
            let declares_const = line.contains("const ") && line.contains(": f64 =");
            for (text, value) in float_literals(&line) {
                scanned += 1;
                if UNIVERSAL.contains(&value) {
                    continue;
                }
                if declares_const {
                    named_consts += 1;
                    continue;
                }
                if exempt.contains(&value) {
                    continue;
                }
                offenders.push(format!("{file}:{n}: {text}   |{}", line.trim()));
            }
        }
    }

    assert!(
        scanned > 200,
        "the literal scan found only {scanned} numbers; it is not scanning"
    );
    assert!(
        offenders.is_empty(),
        "physical literals outside parameters.rs and constants.rs — move each one into \
         the F7 catalogue, give it a named `const` with a doc comment, or argue for it in \
         `EXEMPT`:\n  {}",
        offenders.join("\n  ")
    );

    // A named constant is the second escape route above, so it has to be a
    // discipline rather than a hole: a `const` that *introduces a number* must
    // carry a doc comment saying where the number came from.
    //
    // Two things deliberately do not need one. A constant with no
    // non-universal literal on its right-hand side introduces nothing —
    // `const TWO_PI: f64 = 2.0 * PI;` is a name for something `std` already
    // defines. And a doc comment may cover a **run** of consecutive
    // declarations, which is how the Cody-Waite split of `2π` and the gust
    // frequency band are written; splitting one explanation into three copies
    // would be the duplication this project spends its effort avoiding.
    let mut undocumented = Vec::new();
    for path in rust_sources(&root) {
        let file = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(&path).expect("source file");
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim();
            if !(t.starts_with("const ") || t.starts_with("pub const ")) || !t.contains(": f64") {
                continue;
            }
            let Some((_, rhs)) = t.split_once('=') else {
                continue;
            };
            if float_literals(rhs)
                .iter()
                .all(|(_, v)| UNIVERSAL.contains(v))
            {
                continue;
            }
            let mut j = i;
            let mut documented = false;
            while j > 0 {
                j -= 1;
                let p = lines[j].trim();
                if p.starts_with("///") {
                    documented = true;
                    break;
                }
                // Walk back over attributes and over sibling declarations, so
                // one doc comment can head a group.
                let sibling =
                    (p.starts_with("const ") || p.starts_with("pub const ")) && p.ends_with(';');
                if !(p.starts_with("#[") || sibling) {
                    break;
                }
            }
            let trailing = line.contains("//") && !line.trim_start().starts_with("//");
            if !documented && !trailing {
                undocumented.push(format!("{file}:{}: {t}", i + 1));
            }
        }
    }
    assert!(
        undocumented.is_empty(),
        "named f64 constants that introduce a number and carry no doc comment:\n  {}",
        undocumented.join("\n  ")
    );
    eprintln!(
        "provenance: {scanned} float literals scanned, {named_consts} inside named constants"
    );
}

// ---------------------------------------------------------------------------
// no_physics_in_typescript
// ---------------------------------------------------------------------------

#[test]
fn no_physics_in_typescript() {
    // F8 and brief §23: the physics lives in Rust. A density or a `g` on the
    // TypeScript side is the first step towards a second, divergent model.
    //
    // `web/src/wasm/` is the generated `wasm-pack` package — it is gitignored,
    // it is machine-written, and the F7 path strings really are baked into the
    // `.wasm` binary (section 02 handoff, open items). Hand-written sources
    // only.
    let root = repo_root().join("web/src");
    if !root.exists() {
        eprintln!("skip: {} is not present", root.display());
        return;
    }

    /// The physical constants of F1, spelled every way a careless hand would.
    const FORBIDDEN: [&str; 5] = ["1.225", "1025", "9.81", "9.80665", "9.806_65"];
    /// Identifiers that mean "a fluid density".
    const DENSITY: [&str; 5] = ["rho", "RHO", "Rho", "density", "Density"];

    let mut files = 0usize;
    let mut offenders: Vec<String> = Vec::new();

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("web/src") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "wasm") {
                    continue;
                }
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "ts" || e == "tsx") {
                out.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    walk(&root, &mut paths);
    paths.sort();

    for path in &paths {
        files += 1;
        let name = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();
        let text = std::fs::read_to_string(path).expect("source file");
        for (i, line) in text.lines().enumerate() {
            for needle in FORBIDDEN {
                if line.contains(needle) {
                    offenders.push(format!("{name}:{}: {needle} in `{}`", i + 1, line.trim()));
                }
            }
            // `½ρV²` written out: a `0.5 *` on the same line as anything that
            // means a density.
            if line.contains("0.5 *") && DENSITY.iter().any(|d| line.contains(d)) {
                offenders.push(format!(
                    "{name}:{}: `0.5 *` beside a density identifier in `{}`",
                    i + 1,
                    line.trim()
                ));
            }
        }
    }

    assert!(files > 20, "only {files} TypeScript files were scanned");
    assert!(
        offenders.is_empty(),
        "physics in TypeScript (F8, brief §23):\n  {}",
        offenders.join("\n  ")
    );
    eprintln!("provenance: {files} hand-written TypeScript files scanned, clean");
}

// ---------------------------------------------------------------------------
// shipped_values_match_the_f7_table
// ---------------------------------------------------------------------------

/// Parse the F7 tables of `docs/v1/00-foundations.md` into `name -> value text`.
///
/// Rows look like `| \`loa\` | 4.23 m | KNOWN | brief §3 |`. Only the F7
/// section is read, so the risk registers and the equation tables elsewhere in
/// the document cannot be mistaken for parameters.
fn f7_table() -> BTreeMap<String, String> {
    let path = repo_root().join("docs/v1/00-foundations.md");
    let doc = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let start = doc
        .find("## F7. Parameter catalogue")
        .expect("00-foundations.md must contain the F7 catalogue");
    let end = doc[start..]
        .find("## F8.")
        .map(|i| start + i)
        .unwrap_or(doc.len());

    let mut out = BTreeMap::new();
    for line in doc[start..end].lines() {
        let t = line.trim();
        if !t.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = t.trim_matches('|').split('|').map(|c| c.trim()).collect();
        if cells.len() < 2 {
            continue;
        }
        let name = cells[0].trim_matches('`').trim();
        if name.is_empty() || name == "Field" || name.starts_with('-') {
            continue;
        }
        out.insert(name.to_string(), cells[1].to_string());
    }
    out
}

/// The leading number of an F7 value cell: `4.23 m` → 4.23, `2.0e4 N/m` →
/// 20000, `0.262 rad (15°)` → 0.262, `(0, 0, 0.35) m` → the three components.
fn f7_numbers(cell: &str) -> Option<Vec<f64>> {
    // `00-foundations.md` is prose and spells negatives with U+2212 MINUS SIGN.
    let owned = cell.replace('\u{2212}', "-");
    let text = owned.trim();
    if let Some(inner) = text.strip_prefix('(').and_then(|r| r.split(')').next()) {
        let parts: Vec<f64> = inner
            .split(',')
            .filter_map(|p| p.trim().parse::<f64>().ok())
            .collect();
        if parts.len() == 3 {
            return Some(parts);
        }
    }
    let head: String = text
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == 'e' || *c == '+')
        .collect();
    head.parse::<f64>().ok().map(|v| vec![v])
}

/// The **explicit** v2 overrides of the F7 table, read from the fenced region
/// of `docs/v2/00-foundations.md` (F18.1c).
///
/// A row here says "this F7 default was changed by a recorded v2 delta, to
/// this value, for this reason". Nothing else may differ from F7, and a row
/// whose value equals F7's is a stale row rather than an override — both are
/// asserted below, which is what stops this hatch from making the F7 audit
/// vacuous.
fn v2_overrides() -> BTreeMap<String, (f64, String)> {
    const BEGIN: &str = "<!-- BEGIN F7-OVERRIDES -->";
    const END: &str = "<!-- END F7-OVERRIDES -->";

    let path = repo_root().join("docs/v2/00-foundations.md");
    let doc = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let start = doc
        .find(BEGIN)
        .unwrap_or_else(|| panic!("{} must contain {BEGIN}", path.display()))
        + BEGIN.len();
    let end = doc[start..]
        .find(END)
        .map(|i| start + i)
        .unwrap_or_else(|| panic!("{} must contain {END}", path.display()));

    let mut out = BTreeMap::new();
    for line in doc[start..end].lines() {
        let t = line.trim();
        if !t.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = t.trim_matches('|').split('|').map(|c| c.trim()).collect();
        if cells.len() < 5 {
            continue;
        }
        let path = cells[0].trim_matches('`').trim();
        if path.is_empty() || path == "Path" || path.starts_with('-') {
            continue;
        }
        let value: f64 = cells[2]
            .replace('\u{2212}', "-")
            .trim()
            .parse()
            .unwrap_or_else(|e| panic!("v2 override {path}: `{}` is not a number: {e}", cells[2]));
        assert!(
            cells[4].len() > 60,
            "v2 override {path} has no reason recorded (brief §43): {:?}",
            cells[4]
        );
        assert!(
            TAGS.contains(&cells[3]),
            "v2 override {path} carries tag {:?}, which is not one of {TAGS:?}",
            cells[3]
        );
        out.insert(path.to_string(), (value, cells[1].to_string()));
    }
    out
}

#[test]
fn v2_overrides_are_real_overrides() {
    // The hatch, audited. Every row must name a real parameter, must differ
    // from the F7 value it claims to replace, and the whole table must stay
    // small enough to read: an override list is a decision record, and a long
    // one means the F7 catalogue has silently forked.
    let overrides = v2_overrides();
    let table = f7_table();
    let shipped = BoatParameters::ilca7();

    assert!(
        !overrides.is_empty(),
        "docs/v2/00-foundations.md F18.1c declares no overrides, but section 08 changed two \
         defaults. Either the table or the catalogue is wrong."
    );
    assert!(
        overrides.len() <= 8,
        "{} v2 overrides. That is not a delta list any more; fold them into F7 and have the \
         human sign the change there",
        overrides.len()
    );

    for (path, (value, v1_cell)) in &overrides {
        shipped
            .get_path(path)
            .unwrap_or_else(|e| panic!("v2 override names `{path}`, which does not exist: {e}"));
        let leaf = path.rsplit('.').next().expect("a dotted path");
        let row = table
            .get(leaf)
            .or_else(|| table.get(path.as_str()))
            .unwrap_or_else(|| panic!("v2 override `{path}` overrides no F7 row"));
        let f7 = f7_numbers(row)
            .unwrap_or_else(|| panic!("the F7 row for `{path}` is not numeric: {row}"))[0];
        assert!(
            (f7 - value).abs() > 1e-9,
            "v2 override `{path}` claims {value}, which is the F7 value. A row that changes \
             nothing is stale; delete it."
        );
        // The "v1" column has to quote the value it is replacing, or the table
        // is a claim about a number nobody checked.
        let quoted = f7_numbers(v1_cell).unwrap_or_else(|| {
            panic!("v2 override `{path}`: the v1 column `{v1_cell}` is not a number")
        })[0];
        assert!(
            (quoted - f7).abs() <= (f7.abs() * 5e-3).max(1e-9),
            "v2 override `{path}` says v1 was {quoted}; F7 says {f7}"
        );
    }
    eprintln!(
        "provenance: {} explicit v2 override(s) — {:?}",
        overrides.len(),
        overrides.keys().collect::<Vec<_>>()
    );
}

#[test]
fn shipped_values_match_the_f7_table() {
    // brief §43 and F13.5: **no coefficient may be silently tuned.** Every
    // handoff from section 01 to section 09 says none was; this is the first
    // test that checks it rather than taking their word for it, against F7
    // itself rather than against a copy.
    //
    // A parameter that legitimately moves has to move in `00-foundations.md`
    // too, which is a change the human signs off (F13.1) — which is exactly the
    // control brief §43 asks for.
    let table = f7_table();
    assert!(
        table.len() > 50,
        "the F7 parser found only {} rows in 00-foundations.md",
        table.len()
    );
    // v2 section 08 changed two F7 defaults. Each one is a recorded delta with
    // a reason (F18.1c), read here so the audit compares against the contract
    // in force rather than against a superseded one — and `overridden` counts
    // them, so the escape hatch cannot quietly grow (see
    // `v2_overrides_are_real_overrides`).
    let overrides = v2_overrides();
    let mut overridden = 0usize;

    let shipped = BoatParameters::ilca7();
    let by_leaf: BTreeMap<String, Vec<ParamMeta>> =
        catalogue().into_iter().fold(BTreeMap::new(), |mut acc, m| {
            // The F7 table names fields unqualified (`loa`, `i_zz`) or with
            // their group (`sail.area`, `board.cd0`), and vectors without the
            // component. Index by every suffix so either spelling resolves.
            let path = m.path.clone();
            let trimmed = path
                .trim_end_matches(".x")
                .trim_end_matches(".y")
                .trim_end_matches(".z");
            for key in [path.as_str(), trimmed] {
                let segments: Vec<&str> = key.split('.').collect();
                for i in 0..segments.len() {
                    acc.entry(segments[i..].join("."))
                        .or_default()
                        .push(m.clone());
                }
            }
            acc
        });

    /// F7 names a handful of vectors flat (`board_pos_b`) where the dotted
    /// path puts them inside their group (`board.pos_b`). The scheme is
    /// section 02 §2.8's and is stated once here rather than guessed at.
    const ALIASES: [(&str, &str); 4] = [
        ("board_pos_b", "board.pos_b"),
        ("rudder_pos_b", "rudder.pos_b"),
        ("mast_pos_b", "sail.mast_pos_b"),
        ("block_pos_b", "sheet.block_pos_b"),
    ];

    let mut compared = 0usize;
    let mut skipped: Vec<String> = Vec::new();
    let mut wrong: Vec<String> = Vec::new();

    for (name, cell) in &table {
        let Some(values) = f7_numbers(cell) else {
            // `true`, `Rk2Midpoint` and the like: not numbers.
            skipped.push(format!("{name} = {cell}"));
            continue;
        };
        let key = ALIASES
            .iter()
            .find(|(from, _)| from == name)
            .map_or(name.as_str(), |(_, to)| *to);
        let Some(metas) = by_leaf.get(key) else {
            skipped.push(format!("{name} (no such path)"));
            continue;
        };
        let mut unique: BTreeSet<String> = BTreeSet::new();
        for m in metas {
            unique.insert(m.path.clone());
        }
        let paths: Vec<String> = unique.into_iter().collect();
        if values.len() == 3 {
            for (component, want) in ["x", "y", "z"].iter().zip(values.iter()) {
                let Some(path) = paths.iter().find(|p| p.ends_with(&format!(".{component}")))
                else {
                    skipped.push(format!("{name}.{component}"));
                    continue;
                };
                let got = shipped.get_path(path).expect("a resolvable path");
                compared += 1;
                if (got - want).abs() > 1e-9 {
                    wrong.push(format!("{path}: shipped {got}, F7 says {want} ({cell})"));
                }
            }
            continue;
        }
        // An unqualified name must resolve to exactly one leaf, or the row is
        // ambiguous and is reported rather than guessed at.
        let scalars: Vec<&String> = paths
            .iter()
            .filter(|p| !p.ends_with(".x") && !p.ends_with(".y") && !p.ends_with(".z"))
            .collect();
        if scalars.len() != 1 {
            skipped.push(format!("{name} (resolves to {paths:?})"));
            continue;
        }
        let path = scalars[0];
        let got = shipped.get_path(path).expect("a resolvable path");
        compared += 1;
        let want = match overrides.get(path.as_str()) {
            Some((v2, _)) => {
                overridden += 1;
                *v2
            }
            None => values[0],
        };
        // The F7 table rounds (`155 kg·m²`, `0.262 rad (15°)`), so the
        // comparison is relative and generous enough for the printed precision
        // — but far too tight for a coefficient that has actually been tuned.
        let tolerance = (want.abs() * 5e-3).max(1e-9);
        if (got - want).abs() > tolerance {
            let source = if overrides.contains_key(path.as_str()) {
                "the v2 F18.1c override table"
            } else {
                "F7"
            };
            wrong.push(format!(
                "{path}: shipped {got}, {source} says {want} ({cell})"
            ));
        }
    }

    // Every override has to have been *used*. A row naming a path the F7
    // parser never reaches would silently exempt nothing, and would look like
    // a decision that had been recorded when it had not.
    assert_eq!(
        overridden,
        overrides.len(),
        "{} of the {} v2 overrides were never applied: the F7 table has no comparable row \
         for them, so recording them there proves nothing",
        overrides.len() - overridden,
        overrides.len()
    );

    assert!(
        wrong.is_empty(),
        "shipped parameters disagree with the F7 table in `docs/v1/00-foundations.md`. \
         If one of these is a deliberate change, brief §43 requires the reason, the source \
         and the assumption to be recorded — in the `parameters.rs` doc comment, in the \
         section handoff, and in F7 itself:\n  {}",
        wrong.join("\n  ")
    );
    // The two rows that are not numbers are checked by hand rather than left
    // in the skipped list as if nobody had looked at them.
    assert!(
        shipped.rudder.delta_r_self_centre,
        "F7 says `delta_r_self_centre` is true"
    );
    assert_eq!(
        shipped.sim.integrator,
        sailgym_physics::integrator::Integrator::Rk2Midpoint,
        "F7 says the default integrator is Rk2Midpoint (brief §21)"
    );

    assert!(
        compared >= 80,
        "only {compared} of the F7 table's {} rows were compared; skipped: {skipped:?}",
        table.len()
    );
    eprintln!(
        "provenance: {compared} F7 values compared against the shipped catalogue, all equal \
         ({overridden} through a recorded v2 override, {} rows not numeric or not \
         addressable: {skipped:?})",
        skipped.len()
    );
}

// ---------------------------------------------------------------------------
// docs_match_source
// ---------------------------------------------------------------------------

/// The generated region of `docs/v1/parameters.md`.
const BEGIN: &str =
    "<!-- BEGIN GENERATED — regenerate with `cargo test -p sailgym-physics --test provenance` -->";
const END: &str = "<!-- END GENERATED -->";

/// Render the parameter table from the catalogue and the shipped values.
fn render_table() -> String {
    let shipped = BoatParameters::ilca7();
    let mut out = String::new();
    out.push_str("| Path | Value | Unit | Tag | Source or rationale |\n");
    out.push_str("|---|---|---|---|---|\n");
    for m in catalogue() {
        let value = shipped
            .get_path(&m.path)
            .map(|v| format!("{v}"))
            .unwrap_or_else(|_| "—".to_string());
        // The doc comment minus the unit it starts with and the tag word: what
        // is left is the rationale, which is the column brief §36 asks for.
        let rationale = m
            .doc
            .split_once(&m.tag)
            .map(|(_, tail)| tail)
            .unwrap_or(&m.doc)
            .trim()
            .trim_start_matches('—')
            .trim()
            .replace('|', "\\|");
        let unit = if m.unit.is_empty() {
            "—".to_string()
        } else {
            m.unit.replace('|', "\\|")
        };
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} |\n",
            m.path,
            value,
            unit,
            m.tag,
            if rationale.is_empty() {
                "—".to_string()
            } else {
                rationale
            }
        ));
    }
    out
}

#[test]
fn docs_match_source() {
    // Task 10.7: "`docs/v1/parameters.md` regenerated matches the committed copy —
    // the doc cannot drift from the code."
    //
    // Only the fenced region is generated. The prose around it — brief §36's
    // disclaimer, the deferred validation routes, the record of what changed
    // since F7 — is written by hand because it is judgement, and judgement
    // does not belong in a code generator.
    //
    // Set `SAILGYM_UPDATE_DOCS=1` to rewrite the region instead of asserting on
    // it. That is the regeneration path, and it is deliberately explicit: a
    // test that silently rewrote the file it checks would prove nothing.
    let path = repo_root().join("docs/v1/parameters.md");
    let doc = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let start = doc
        .find(BEGIN)
        .unwrap_or_else(|| panic!("docs/v1/parameters.md must contain the BEGIN marker"))
        + BEGIN.len();
    let end = doc[start..]
        .find(END)
        .map(|i| start + i)
        .expect("docs/v1/parameters.md must contain the END marker");

    let want = format!("\n\n{}\n", render_table().trim_end());
    let have = &doc[start..end];

    if std::env::var("SAILGYM_UPDATE_DOCS").is_ok() {
        let updated = format!("{}{}{}", &doc[..start], want, &doc[end..]);
        std::fs::write(&path, updated).expect("write docs/v1/parameters.md");
        eprintln!("provenance: rewrote the generated region of docs/v1/parameters.md");
        return;
    }

    assert_eq!(
        have.trim(),
        want.trim(),
        "docs/v1/parameters.md is out of date. Regenerate it with:\n  \
         SAILGYM_UPDATE_DOCS=1 cargo test -p sailgym-physics --test provenance docs_match_source"
    );

    // The prose the criterion also requires, checked for presence rather than
    // wording: a generated table with no disclaimer around it would satisfy the
    // comparison above and none of brief §36.
    for needle in [
        "does not claim",
        "towing-tank",
        "VPP",
        "CFD",
        "IMU",
        "KNOWN",
        "ASSUMED",
        "TUNABLE",
        "DEFERRED",
    ] {
        assert!(
            doc.contains(needle),
            "docs/v1/parameters.md must mention {needle:?} (brief §36)"
        );
    }
}
