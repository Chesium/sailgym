"""The Python constants audit — F17.1, in the shape of ``provenance.rs``.

F7's rule with teeth, restated for Python: **no number belongs loose in the
source.** The Rust side enforces it with
``crates/sailgym-physics/tests/provenance.rs::no_stray_constants``; this is
the same audit, in the same three tiers, over the three scopes F17.1 names
— and it audits them **separately**, because they are held to different rules.

=========================  ==========================================
Scope                      Rule
=========================  ==========================================
``sailgym_conformance``    wrapper scope: no equation, no independently
                           defined parameter or layout value at all
``sailgym``                wrapper scope, added by section 07. The
                           Gymnasium binding is where this discipline
                           usually collapses, which is the reason
                           ``cross-stack.md`` §1.2 gives for stating it
``sailgym_jax``            the explicit F17.1 exception: an independent
                           verification implementation, so it holds the
                           equations it tests — and every constant in
                           it cites the Rust line it was copied from
=========================  ==========================================

A literal is acceptable in exactly three ways, exactly as the Rust audit
allows: it is structural (``0.0``, ``1.0``, ``0.5``, ``2.0``); or it is on
the right-hand side of a **module-level named constant that carries a doc
string**; or it is in :data:`EXEMPT` with a stated reason.

Six passes, and the last four are the ones this repository specifically
needs:

1. no stray literal, in any scope;
2. every named constant that introduces a number **in the exception scope**
   cites its source line;
3. **wrapper scope defines no number at all** — not even a documented,
   cited one. Section 07 added this pass: F17.1 permits a citation only to
   the independent implementation, and a binding that cited ``dt`` would
   still have a second copy of ``dt``;
4. anti-vacuity — the scan visited files and found numbers;
5. **the binding binds the env and not the WASM surface** — F17.2 and RV47,
   a grep over the crate manifests. It is here because this file is where
   the Python side's boundary rules are greps, and a boundary is a boundary;
6. **no generator of the wind mode table** anywhere under
   ``python/sailgym_jax/`` (F16.7, RV15, section 03 acceptance 6).

## Proven able to fail

Task 3.6 requires the demonstration. ``1.225`` — ``RHO_AIR``, the most
physical number in the project — was pasted into ``sailgym_jax/wind.py``,
the audit named the file and the line, and it was reverted. The transcript
is in ``docs/v2/progress/03-handoff.md`` §5. Section 07 acceptance 2
requires the same demonstration in ``python/sailgym/``; its transcript is
in ``docs/v2/progress/07-handoff.md``.
"""

from __future__ import annotations

import ast
import re
from pathlib import Path

WRAPPER_SCOPES = ("python/sailgym", "python/sailgym_conformance")
"""Binding and loader scope: **no** independently defined number at all.

``python/sailgym`` joined in section 07. F17.1's exception is named and it
names one package; everything else that imports from Rust is wrapper scope
and holds no equation, no parameter and no layout.
"""

EXCEPTION_SCOPES = ("python/sailgym_jax",)
"""The one package F17.1 excepts: an independent verification implementation."""

SCOPES = WRAPPER_SCOPES + EXCEPTION_SCOPES
"""The three scopes F17.1 audits separately."""

UNIVERSAL = (0.0, 1.0, 2.0, 0.5)
"""Literals that carry no physical content and may appear anywhere.

The same four ``provenance.rs`` allows. An integer is not audited at all, for
the same reason it gives: ``2`` is a count, ``2.0`` might be a coefficient.
"""

# Literals that are neither UNIVERSAL nor part of a named constant, each with
# the reason it is not a physical or contract number.
#
# **The list is meant to stay short.** A new entry is a decision that a number
# belongs in the code rather than in the bundle, and it has to be argued here.
# It is currently empty, and that is the intended steady state: every number
# either comes from the identified bundle or is a named constant citing the
# Rust line it was transcribed from.
EXEMPT: tuple[tuple[str, tuple[float, ...], str], ...] = ()

CITATION = re.compile(r"[\w./-]+\.rs:\d+")
"""A doc string that names a source line. ``wind.rs:87`` and the like."""

FORBIDDEN = ("pcg", "seed", "stratum", "rng")
"""F16.7 / RV15: the four words a port of the wind field must not contain.

The mode table is data. A port that reproduces the draw instead of reading
the table has spent days reproducing a kilobyte, and is wrong in a way no
tolerance catches — so the absence is asserted rather than trusted.
"""

MIN_FILES = 6
MIN_LITERALS = 12
"""Anti-vacuity floors, the two ``provenance.rs`` uses.

An audit that silently scans nothing is worse than no audit: it is a green
tick that means the opposite of what a reader takes it to mean. The counts
are the current ones, set just below so that adding a file does not break
the build and deleting a whole package does. Section 07 raised ``MIN_FILES``
from 3 to 6 when ``python/sailgym`` brought the count from 4 to 8: a floor
left at the old value would not have noticed the new package disappearing.
Each scope additionally has to contribute at least one file, which is the
check that catches a deletion the totals would hide.
"""


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def sources(root: Path) -> list[Path]:
    """Every ``.py`` under a scope, **sorted**.

    F9.3's reason, on the other side of the boundary: a deterministic walk
    reports the same first offender every run, and a first offender that
    moves between runs is a first offender nobody fixes.
    """
    return sorted(root.rglob("*.py"))


def documented_constants(tree: ast.Module) -> dict[str, tuple[ast.expr, str | None]]:
    """Module-level ``NAME = expr`` with the doc string that follows it.

    PEP 257's attribute doc string: a bare string expression immediately
    after the assignment. It is the only convention Python has for what Rust
    writes as ``/// …`` above a ``const``.
    """
    out: dict[str, tuple[ast.expr, str | None]] = {}
    body = tree.body
    for i, node in enumerate(body):
        if not isinstance(node, ast.Assign) or len(node.targets) != 1:
            continue
        target = node.targets[0]
        if not isinstance(target, ast.Name):
            continue
        doc = None
        if i + 1 < len(body):
            nxt = body[i + 1]
            if (
                isinstance(nxt, ast.Expr)
                and isinstance(nxt.value, ast.Constant)
                and isinstance(nxt.value.value, str)
            ):
                doc = nxt.value.value
        out[target.id] = (node.value, doc)
    return out


def float_literals(node: ast.AST) -> list[tuple[int, float]]:
    """``(line, value)`` for every float constant in a subtree.

    A doc string is a ``str`` constant and a count is an ``int`` constant, so
    neither appears here — which is the Python equivalent of ``provenance.rs``
    stripping ``//`` comments and skipping integers.
    """
    out = []
    for n in ast.walk(node):
        if isinstance(n, ast.Constant) and type(n.value) is float:
            out.append((n.lineno, n.value))
    return out


def test_no_stray_constants():
    root = repo_root()
    offenders: list[str] = []
    scanned = 0
    inside_named = 0
    files = 0
    per_scope: dict[str, int] = {}

    for scope in SCOPES:
        base = root / scope
        assert base.is_dir(), f"{scope} is missing; this audit would scan nothing"
        per_scope[scope] = 0
        for path in sources(base):
            files += 1
            per_scope[scope] += 1
            rel = path.relative_to(root).as_posix()
            text = path.read_text(encoding="utf-8")
            tree = ast.parse(text, filename=rel)
            constants = documented_constants(tree)

            permitted_lines = set()
            for _name, (value, doc) in constants.items():
                if doc is None:
                    continue
                for line, _v in float_literals(value):
                    permitted_lines.add(line)

            exempt = [v for f, vs, _ in EXEMPT if rel.endswith(f) for v in vs]

            for line, value in float_literals(tree):
                scanned += 1
                if value in UNIVERSAL:
                    continue
                if line in permitted_lines:
                    inside_named += 1
                    continue
                if value in exempt:
                    continue
                source_line = text.splitlines()[line - 1].strip()
                offenders.append(f"{rel}:{line}: {value!r}   |{source_line}")

    assert files > MIN_FILES, (
        f"the audit visited only {files} files; it is not scanning"
    )
    for scope, seen in per_scope.items():
        assert seen > 0, f"{scope} contributed no file; the audit lost a scope"
    assert scanned > MIN_LITERALS, (
        f"the literal scan found only {scanned} numbers; it is not scanning"
    )
    assert not offenders, (
        "physical or contract literals loose in Python — move each one into the "
        "bundle, give it a module-level named constant with a doc string citing "
        "its Rust source line, or argue for it in EXEMPT:\n  " + "\n  ".join(offenders)
    )
    print(
        f"[audit] {files} files, {scanned} float literals, "
        f"{inside_named} inside documented named constants"
    )


def test_every_named_constant_cites_its_source_line():
    """F17.1's second half: a name is not a provenance.

    ``provenance.rs`` makes the same point — "a named constant is the second
    escape route above, so it has to be a discipline rather than a hole".
    Here the discipline is stronger than Rust's, because F17.1 asks for it:
    a constant that introduces a number must not merely be documented, it
    must name the Rust file **and line** it was transcribed from, and that
    file must exist.
    """
    root = repo_root()
    undocumented: list[str] = []
    uncited: list[str] = []
    missing: list[str] = []
    cited = 0

    for scope in EXCEPTION_SCOPES:
        for path in sources(root / scope):
            rel = path.relative_to(root).as_posix()
            tree = ast.parse(path.read_text(encoding="utf-8"), filename=rel)
            for name, (value, doc) in documented_constants(tree).items():
                introduces = [v for _, v in float_literals(value) if v not in UNIVERSAL]
                if not introduces:
                    # `INV_TAU = 1.0 / TAU` introduces nothing: it is a name
                    # for arithmetic over constants that already exist, which
                    # `provenance.rs` also exempts.
                    continue
                if doc is None:
                    undocumented.append(f"{rel}: {name}")
                    continue
                found = CITATION.findall(doc)
                if not found:
                    uncited.append(f"{rel}: {name} = {introduces}")
                    continue
                for citation in found:
                    source = citation.rsplit(":", 1)[0]
                    if not (root / source).is_file():
                        missing.append(
                            f"{rel}: {name} cites {citation}, which is not a file"
                        )
                cited += 1

    assert not undocumented, (
        "named constants that introduce a number and carry no doc string:\n  "
        + "\n  ".join(undocumented)
    )
    assert not uncited, (
        "named constants whose doc string does not name a Rust source line — "
        "F17.1: an independent implementation's kernel constants cite the Rust "
        "source:\n  " + "\n  ".join(uncited)
    )
    assert not missing, "citations that point at no file:\n  " + "\n  ".join(missing)
    assert cited >= 12, f"only {cited} cited constants; the second pass is vacuous"
    print(f"[audit] {cited} named constants cite a Rust source line")


def test_wrapper_scope_defines_no_number_at_all():
    """F17.1's first half, with teeth — section 07, RV41.

    The literal scan permits a number that sits on the right-hand side of a
    documented, cited module-level constant. That escape route is F17.1's
    **exception** and the exception names one package. In wrapper scope there
    is nothing to except: a binding's job is to read the contract from Rust,
    so a ``DT = 0.005`` in ``python/sailgym/`` would be a second copy of
    ``dt`` no matter how well it cited the first.

    This is the pass that makes RV41 a gate failure rather than a review
    comment: *"any integer other than a shape derived from Rust appears in
    ``spaces.py``"*. Integers are not audited — ``provenance.rs`` gives the
    reason and it holds here — so what this pass forbids is the *named
    constant*, which is the only way a number could have been made to look
    respectable.
    """
    root = repo_root()
    offenders: list[str] = []
    files = 0
    names = 0
    for scope in WRAPPER_SCOPES:
        base = root / scope
        assert base.is_dir(), f"{scope} is missing; this pass would scan nothing"
        seen = 0
        for path in sources(base):
            files += 1
            seen += 1
            rel = path.relative_to(root).as_posix()
            tree = ast.parse(path.read_text(encoding="utf-8"), filename=rel)
            for name, (value, _doc) in documented_constants(tree).items():
                names += 1
                introduces = [v for _, v in float_literals(value) if v not in UNIVERSAL]
                if introduces:
                    offenders.append(f"{rel}: {name} = {introduces}")
        assert seen > 0, f"{scope} contributed no file"
    assert files >= 4, f"only {files} files in wrapper scope; this pass is vacuous"
    assert names > 0, "wrapper scope declares no module-level name at all"
    assert not offenders, (
        "a wrapper package defined a number of its own. F17.1 excepts "
        f"{EXCEPTION_SCOPES} and nothing else: read it from Rust, or from the "
        "identified bundle:\n  " + "\n  ".join(offenders)
    )
    print(
        f"[audit] {files} files in wrapper scope declare {names} module-level "
        "names and not one number"
    )


def test_the_binding_binds_the_env_and_not_the_wasm_surface():
    """F17.2 and RV47, as a grep over the manifests.

    *"The binding grows a shortcut through ``sailgym-wasm``, collapsing the
    two boundaries."* The two have different performance shapes — one call
    per animation frame against 20 Hz x N envs — and different lifetimes, and
    F8.2's coarseness is right for one and wrong for the other. So the
    dependency simply is not there, and its absence is asserted rather than
    remembered.

    The second half is the arrow's direction: **nothing depends on the
    binding**. `sailgym-py` is a leaf, as `sailgym-wasm` is, and a crate that
    acquired it would drag `pyo3` into a build that has no interpreter.

    It lives in this file because this file is where the Python side's
    boundary rules are greps. It is task 7.2's file and this assertion is
    task 7.1's subject; ``docs/v2/progress/07-handoff.md`` records the seam.
    """
    root = repo_root()
    binding = root / "crates/sailgym-py/Cargo.toml"
    assert binding.is_file(), "the binding's manifest is missing"
    text = binding.read_text(encoding="utf-8")
    dependencies = text.split("[dependencies]", 1)[1]
    assert "sailgym-wasm" not in dependencies, (
        "sailgym-py declares a dependency on sailgym-wasm. F17.2 keeps the "
        "two boundaries apart deliberately (RV47)"
    )
    assert "sailgym-env" in dependencies, (
        "sailgym-py does not depend on sailgym-env; it binds the episode "
        "runner and nothing else (F17.2)"
    )

    manifests = sorted((root / "crates").glob("*/Cargo.toml"))
    assert len(manifests) >= 7, f"only {len(manifests)} manifests; not scanning"
    dependents = [
        m.relative_to(root).as_posix()
        for m in manifests
        if m != binding and "sailgym-py" in m.read_text(encoding="utf-8")
    ]
    assert not dependents, (
        "a crate depends on the Python binding, which would drag pyo3 into a "
        "build that needs no interpreter: " + ", ".join(dependents)
    )
    print(
        f"[audit] {len(manifests)} manifests: sailgym-py binds sailgym-env, "
        "names no sailgym-wasm, and nothing names it"
    )


def test_the_verification_implementation_has_no_generator_of_the_mode_table():
    """Section acceptance 6, RV15, F16.7.

    ``ProceduralWind::new`` draws the mode table once at construction and
    ``sample`` is a pure sum over the result. The table is exported in the
    bundle, and **no stack other than Rust implements PCG32**. This pass is
    the cheapest possible guard against someone spending a week reproducing
    a kilobyte — and against the subtler version, a port that takes an
    innocent-looking extra argument and quietly re-derives a table that ought
    to have been read.
    """
    root = repo_root()
    scope = root / "python/sailgym_jax"
    offenders = []
    files = 0
    for path in sources(scope):
        files += 1
        rel = path.relative_to(root).as_posix()
        for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            lowered = line.lower()
            for word in FORBIDDEN:
                if word in lowered:
                    offenders.append(f"{rel}:{n}: {word!r} in |{line.strip()}")
    assert files >= 2, f"only {files} files under {scope}; this pass is vacuous"
    assert not offenders, (
        "the wind mode table is data and its generator is Rust's alone (F16.7). "
        "A port that draws instead of reading is wrong in a way no tolerance "
        "catches:\n  " + "\n  ".join(offenders)
    )
    print(f"[audit] {files} files under python/sailgym_jax/ carry none of {FORBIDDEN}")
