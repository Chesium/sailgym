"""The Python constants audit — F17.1, in the shape of ``provenance.rs``.

F7's rule with teeth, restated for Python: **no number belongs loose in the
source.** The Rust side enforces it with
``crates/sailgym-physics/tests/provenance.rs::no_stray_constants``; this is
the same audit, in the same three tiers, over the two scopes F17.1 names —
and it audits them **separately**, because they are held to different rules.

=========================  ==========================================
Scope                      Rule
=========================  ==========================================
``sailgym_conformance``    wrapper scope: no equation, no independently
                           defined parameter or layout value at all
``sailgym_jax``            the explicit F17.1 exception: an independent
                           verification implementation, so it holds the
                           equations it tests — and every constant in
                           it cites the Rust line it was copied from
=========================  ==========================================

A literal is acceptable in exactly three ways, exactly as the Rust audit
allows: it is structural (``0.0``, ``1.0``, ``0.5``, ``2.0``); or it is on
the right-hand side of a **module-level named constant that carries a doc
string**; or it is in :data:`EXEMPT` with a stated reason.

Four passes, and the last two are the ones a Python audit specifically needs:

1. no stray literal;
2. every named constant that introduces a number cites its source line;
3. anti-vacuity — the scan visited files and found numbers;
4. **no generator of the wind mode table** anywhere under
   ``python/sailgym_jax/`` (F16.7, RV15, section acceptance 6).

## Proven able to fail

Task 3.6 requires the demonstration. ``1.225`` — ``RHO_AIR``, the most
physical number in the project — was pasted into ``sailgym_jax/wind.py``,
the audit named the file and the line, and it was reverted. The transcript
is in ``docs/v2/progress/03-handoff.md`` §5.
"""

from __future__ import annotations

import ast
import re
from pathlib import Path

SCOPES = ("python/sailgym_conformance", "python/sailgym_jax")
"""The two scopes F17.1 audits separately."""

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

MIN_FILES = 3
MIN_LITERALS = 12
"""Anti-vacuity floors, the two ``provenance.rs`` uses.

An audit that silently scans nothing is worse than no audit: it is a green
tick that means the opposite of what a reader takes it to mean. The counts
are the current ones (4 files, 19 literals at the time of writing), set just
below so that adding a file does not break the build and deleting the whole
package does.
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

    for scope in SCOPES:
        base = root / scope
        assert base.is_dir(), f"{scope} is missing; this audit would scan nothing"
        for path in sources(base):
            files += 1
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

    for scope in SCOPES:
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
