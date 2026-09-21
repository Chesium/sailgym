"""Tier 0: the two arms against the recorded pure-function fixtures.

``tier0_wave`` is the kernel alone; ``tier0_wind_sample`` is the whole of
``sample`` over the nine wind fields the bundle ships. Between them they are
every number section 03 is allowed to claim agreement on.

Three things are asserted and one is only reported:

* :func:`sailgym_jax.wind.wave_exact` — and ``sample`` driven by it — meet the
  **tier-0** bound the manifest records per column: an absolute floor near
  zero, a relative bound away from it, and a stated ULP budget.
* :func:`sailgym_jax.wind.wave_cos` — and ``sample`` driven by it — meet the
  **measured** bound the manifest records under ``tolerances.wind_kernel``.
  That arm cannot meet the ULP bound and is not asked to (F16.6).
* The divergence between the two arms is computed over the same inputs and
  **reported as a number** — max absolute, max relative, and the ULP
  distribution. It is not asserted against a threshold, because F16.6 says it
  is a measurement and a threshold would quietly turn it into a claim.

## Failures name a hazard, not a row index

A red conformance test whose message is "row 417 differs" tells nobody
anything. F16.3's argument is that ports differ at *branch points*, so every
row here is classified by a **named sampler** and the failure message leads
with the sampler of the worst row. For a wind-only port the hazards are the
argument-reduction joins (the ``pi/2`` and ``pi`` fold seams, and the
large-argument reduction the Cody-Waite split exists for), the mode-count
paths (zero, even, odd) and cancellation, where a relative bound means
nothing and only the absolute floor does. The ``EPS_FLOW`` guard of
``foil.rs`` is a hazard of the foil port and belongs to that section.

## Proven able to fail

Task 3.4 requires the demonstration, in the style of ``no_shortcuts.rs``'s
module doc. One component of ``amp`` was negated inside
``sailgym_jax.wind.sample`` — the smallest possible sign error, and the R3
defect class — and reverted. Both arms went red, on
``test_sample_with_wave_exact_matches_the_recorded_field`` and on
``test_sample_with_wave_cos_is_within_the_measured_bound``, naming the
sampler. The observed magnitude is in ``docs/v2/progress/03-handoff.md`` §5.
"""

from __future__ import annotations

import os
import re
from pathlib import Path

import numpy as np

from sailgym_conformance import Bundle
from sailgym_jax import wind as W

# ---------------------------------------------------------------------------
# ULP arithmetic
# ---------------------------------------------------------------------------


def ulp_distance(a: np.ndarray, b: np.ndarray) -> np.ndarray:
    """Distance in representable ``float64`` steps.

    An ULP distance and a relative error are distinct metrics (F16.2 says so
    twice), so both are reported and neither is derived from the other. The
    sign-magnitude bit pattern is mapped to a monotone integer first, which is
    what makes the count meaningful across zero.
    """
    ai = a.astype(np.float64).view(np.int64)
    bi = b.astype(np.float64).view(np.int64)
    ordered = lambda v: np.where(v < 0, np.int64(np.iinfo(np.int64).min) - v, v)  # noqa: E731
    return np.abs(ordered(ai) - ordered(bi))


def ulp_histogram(d: np.ndarray) -> str:
    """``0 ulp: n`` … in ascending buckets, which is the shape F16.2 wants."""
    edges = [0, 1, 2, 4, 8, 16, 32]
    parts = []
    for lo, hi in zip(edges, edges[1:] + [None], strict=True):
        if hi is None:
            n = int(np.sum(d >= lo))
            parts.append(f">={lo}: {n}")
        elif lo == 0:
            parts.append(f"0: {int(np.sum(d == 0))}")
        else:
            parts.append(f"{lo}-{hi - 1}: {int(np.sum((d >= lo) & (d < hi)))}")
    return ", ".join(parts)


# ---------------------------------------------------------------------------
# Named samplers (F16.3)
# ---------------------------------------------------------------------------

JOIN_WINDOW = 1e-6
"""How close to a fold seam a row has to be to count as sitting on it (rad)."""

LARGE_ARGUMENT = 1.0e3
"""Above this ``|theta|`` a one-step reduction would already have lost digits;
the Cody-Waite split of ``wind.rs:87-89`` is what this band tests."""


def wave_samplers(theta: np.ndarray) -> np.ndarray:
    """Name the argument-reduction hazard each ``theta`` row exercises."""
    n = (theta * W.INV_TAU + W.ROUND_MAGIC) - W.ROUND_MAGIC
    r = ((theta - n * W.TAU_HI) - n * W.TAU_LO) - n * W.TAU_LO2
    a = np.abs(r)
    names = np.full(theta.shape, "reduced_interior", dtype=object)
    names[np.abs(theta) > LARGE_ARGUMENT] = "large_argument_reduction"
    names[np.abs(a - W.FRAC_PI_2) <= JOIN_WINDOW] = "fold_seam_half_pi"
    names[np.abs(a - W.PI) <= JOIN_WINDOW] = "fold_seam_pi"
    names[a <= JOIN_WINDOW] = "reduction_join_zero"
    return names


CANCELLATION_FRACTION = 1e-3
"""A row whose recorded value is this far below its column's measured scale is
a cancellation case: the relative bound says nothing there and only the
absolute floor does (F16.2)."""


def sample_samplers(bundle: Bundle, field_index: np.ndarray, recorded) -> np.ndarray:
    """Name the mode-count and cancellation hazard each sample row exercises."""
    counts = np.array([m.count for m in bundle.modes()])
    k = counts[field_index.astype(int)]
    names = np.where(
        k == 0,
        "zero_modes",
        np.where(k % 2 == 0, "even_mode_count", "odd_mode_count"),
    ).astype(object)
    scale = max(float(np.max(np.abs(r))) for r in recorded)
    small = np.ones(names.shape, dtype=bool)
    for r in recorded:
        small &= np.abs(r) < CANCELLATION_FRACTION * scale
    names[small] = "cancellation"
    return names


# ---------------------------------------------------------------------------
# Comparison
# ---------------------------------------------------------------------------


def check(
    label: str,
    port: np.ndarray,
    recorded: np.ndarray,
    bound: np.ndarray,
    samplers: np.ndarray,
    justification: str,
) -> None:
    """Assert, and on failure lead with the hazard rather than the row."""
    err = np.abs(port - recorded)
    bad = err > bound
    used_floor = int(np.sum(bound > 0.0))
    d = ulp_distance(port, recorded)
    print(
        f"[tier0] {label}: max|err| {np.max(err):.6e}, "
        f"max ulp {int(np.max(d))}, exact rows {int(np.sum(d == 0))}/{len(err)}, "
        f"ulp {{{ulp_histogram(d)}}}"
    )
    if not bad.any():
        return
    worst = int(np.argmax(np.where(bad, err - bound, -np.inf)))
    offenders = sorted(set(samplers[bad]))
    raise AssertionError(
        f"{label}: {int(bad.sum())} of {len(err)} rows exceed the recorded bound.\n"
        f"  hazards hit: {offenders}\n"
        f"  worst row {worst} (sampler {samplers[worst]!r}): "
        f"port {port[worst]!r}, recorded {recorded[worst]!r}, "
        f"|err| {err[worst]:.6e} > bound {bound[worst]:.6e}\n"
        f"  bound derivation, do not widen it without a new one:\n  {justification}\n"
        f"  (rows with a positive bound: {used_floor})"
    )


def field_rows(bundle: Bundle, table, arm):
    """``sample`` over every row of ``tier0_wind_sample``, field by field."""
    index = table.column("field")
    x, y, t = (table.column(c) for c in ("x", "y", "t"))
    out_x = np.empty(table.rows, dtype=np.float64)
    out_y = np.empty(table.rows, dtype=np.float64)
    for m in bundle.modes():
        rows = index.astype(int) == m.index
        if not rows.any():
            continue
        base = W.base_from_bearing(m.config["speed"], m.config["bearing_deg"])
        wx, wy = W.sample(m, base, x[rows], y[rows], t[rows], wave=arm)
        out_x[rows] = np.asarray(wx)
        out_y[rows] = np.asarray(wy)
    return out_x, out_y


# ---------------------------------------------------------------------------
# tier0_wave
# ---------------------------------------------------------------------------


def test_wave_exact_matches_the_recorded_kernel(bundle: Bundle):
    table = bundle.table("tier0_wave")
    theta = table.column("theta")
    recorded = table.column("wave")
    tol, justification = bundle.tolerance("tier0_wave", "wave")
    port = np.asarray(W.wave_exact(theta))
    assert port.dtype == np.float64
    check(
        "tier0_wave/wave_exact",
        port,
        recorded,
        tol.bound(recorded),
        wave_samplers(theta),
        justification,
    )


def test_wave_cos_is_within_the_measured_kernel_bound(bundle: Bundle):
    """The host-cosine arm, held to F16.6's measurement, not to 32 ULP."""
    table = bundle.table("tier0_wave")
    theta = table.column("theta")
    recorded = table.column("wave")
    kernel = bundle.wind_kernel()
    port = np.asarray(W.wave_cos(theta))
    bound = np.full(recorded.shape, kernel.kernel_vs_libm_absolute)
    check(
        "tier0_wave/wave_cos",
        port,
        recorded,
        bound,
        wave_samplers(theta),
        kernel.derivation,
    )


# ---------------------------------------------------------------------------
# tier0_wind_sample
# ---------------------------------------------------------------------------


def test_sample_with_wave_exact_matches_the_recorded_field(bundle: Bundle):
    table = bundle.table("tier0_wind_sample")
    port_x, port_y = field_rows(bundle, table, W.wave_exact)
    samplers = sample_samplers(
        bundle, table.column("field"), (table.column("wx"), table.column("wy"))
    )
    for column, port in (("wx", port_x), ("wy", port_y)):
        recorded = table.column(column)
        tol, justification = bundle.tolerance("tier0_wind_sample", column)
        check(
            f"tier0_wind_sample/{column}/wave_exact",
            port,
            recorded,
            tol.bound(recorded),
            samplers,
            justification,
        )


def test_sample_with_wave_cos_is_within_the_measured_bound(bundle: Bundle):
    """Two manifest measurements compose here, and neither is invented.

    The host-cosine arm substitutes the kernel **and** may have XLA
    reassociate the mode sum, so the bound is the sum of exactly the two
    numbers section 02 measured for those two effects:

    * ``summation_vs_compensated_absolute`` — the fixed two-slot pairwise
      reduction of ``sample_inner`` against a Kahan-compensated sum of the
      same modes, in m/s; and
    * ``kernel_vs_libm_absolute`` propagated through the stored amplitudes:
      each mode's contribution is ``amp_k * cos(theta_k)``, so a per-kernel
      gap of ``g`` moves the field by at most ``sum_k |amp_k| * g``.

    Both come from ``manifest.json``, which this section does not own and
    cannot edit (RV13). The composition is stated here rather than in the
    manifest because it is a property of *this* arm.
    """
    table = bundle.table("tier0_wind_sample")
    kernel = bundle.wind_kernel()
    index = table.column("field").astype(int)
    port_x, port_y = field_rows(bundle, table, W.wave_cos)
    samplers = sample_samplers(
        bundle, table.column("field"), (table.column("wx"), table.column("wy"))
    )

    amp_sum = {"wx": {}, "wy": {}}
    for m in bundle.modes():
        amp_sum["wx"][m.index] = float(np.sum(np.abs(m.amp_x)))
        amp_sum["wy"][m.index] = float(np.sum(np.abs(m.amp_y)))

    for column, port in (("wx", port_x), ("wy", port_y)):
        recorded = table.column(column)
        propagated = np.array([amp_sum[column][i] for i in index])
        bound = (
            kernel.summation_vs_compensated_absolute
            + propagated * kernel.kernel_vs_libm_absolute
        )
        check(
            f"tier0_wind_sample/{column}/wave_cos",
            port,
            recorded,
            bound,
            samplers,
            kernel.derivation,
        )


# ---------------------------------------------------------------------------
# The divergence between the arms — reported, never asserted
# ---------------------------------------------------------------------------

CONFORMANCE_DOC = "docs/v2/conformance.md"
BEGIN = "<!-- BEGIN section-03 two-arm divergence -->"
END = "<!-- END section-03 two-arm divergence -->"


def divergence(a: np.ndarray, b: np.ndarray, floor: float) -> dict[str, float | str]:
    """Max absolute over every row; relative and ULP **away from zero**.

    F16.2: "An ULP distance and a relative error are distinct metrics. Both
    are given; neither is meaningful near cancellation, which is what the
    absolute bound is for." A row where the two arms straddle zero has a
    relative error of order 1 and an ULP distance of order ``2**52``, and
    reporting those as the headline would be reporting the cancellation, not
    the arms. ``floor`` is the manifest's own absolute bound for the column,
    so the split is the manifest's, not this test's.
    """
    delta = np.abs(a - b)
    scale = np.maximum(np.abs(a), np.abs(b))
    away = scale > floor
    d = ulp_distance(a, b)
    d_away = d[away]
    rel_away = delta[away] / scale[away] if away.any() else np.zeros(1)
    return {
        "max_abs": float(np.max(delta)),
        "max_rel": float(np.max(rel_away)) if away.any() else 0.0,
        "max_ulp": int(np.max(d_away)) if away.any() else 0,
        "rows": int(delta.size),
        "near_zero": int(np.sum(~away)),
        "floor": float(floor),
        "identical": int(np.sum(d == 0)),
        "histogram": ulp_histogram(d_away if away.any() else d),
    }


def test_the_two_arms_diverge_by_a_measured_amount(bundle: Bundle, repo_root: Path):
    """F16.6's reported number. **No threshold** — that is the whole point.

    ``wave_cos`` alone can never catch a defect smaller than the kernel gap.
    That is enough for every sign error, unit slip and stratification mistake
    — the R3 class — but it cannot tell a correct port from one that is wrong
    in the sixteenth digit, so "the port agrees to 1e-13" would be an
    untestable claim. Measuring the gap is what makes the cheap arm's silence
    mean something.
    """
    rows = []

    theta = bundle.table("tier0_wave").column("theta")
    wave_tol, _ = bundle.tolerance("tier0_wave", "wave")
    rows.append(
        (
            "`tier0_wave` — `wave`",
            divergence(
                np.asarray(W.wave_exact(theta)),
                np.asarray(W.wave_cos(theta)),
                wave_tol.absolute,
            ),
        )
    )

    table = bundle.table("tier0_wind_sample")
    ex_x, ex_y = field_rows(bundle, table, W.wave_exact)
    co_x, co_y = field_rows(bundle, table, W.wave_cos)
    for column, ex, co in (("wx", ex_x, co_x), ("wy", ex_y, co_y)):
        tol, _ = bundle.tolerance("tier0_wind_sample", column)
        rows.append(
            (f"`tier0_wind_sample` — `{column}`", divergence(ex, co, tol.absolute))
        )

    for name, d in rows:
        assert np.isfinite(d["max_abs"]), name
        print(
            f"[two-arm] {name}: max|Δ| {d['max_abs']:.6e} over all {d['rows']} rows; "
            f"away from a |value| floor of {d['floor']:.3e} "
            f"({d['near_zero']} rows below it) max rel {d['max_rel']:.6e}, "
            f"max ulp {d['max_ulp']}, identical {d['identical']}/{d['rows']}, "
            f"ulp {{{d['histogram']}}}"
        )

    text = _render(bundle, rows)
    path = repo_root / CONFORMANCE_DOC
    if os.environ.get("SAILGYM_WRITE_CONFORMANCE") == "1":
        _write(path, text)
    _assert_recorded(path, bundle)


def _render(bundle: Bundle, rows) -> str:
    kernel = bundle.wind_kernel()
    out = [
        BEGIN,
        "",
        "## The two arms of the JAX wind port (section 03)",
        "",
        "Appended by `SAILGYM_WRITE_CONFORMANCE=1 scripts/py-test.sh`, **not** by",
        "`gen_conformance`. A regeneration of this file by the Rust generator drops",
        "this section; re-run the command above to restore it. The live numbers are",
        "always the ones `python/tests/test_wind_tier0.py` prints.",
        "",
        "F16.6 requires a port to ship two arms and to report the divergence between",
        "them **as a number, not as a pass or a fail**. `wave_exact` transcribes the",
        "Cody-Waite reduction, the Estrin polynomial and the fixed two-slot pairwise",
        "mode reduction of `crates/sailgym-physics/src/environment/wind.rs`;",
        "`wave_cos` calls `jnp.cos` and lets XLA reduce however it likes. Neither is",
        '"the port": the module exports both and the caller chooses.',
        "",
        f"Bundle `{bundle.digest}`.",
        "",
        "`max |Δ|` is over every row. `max relative` and the ULP figures are over",
        "the rows whose larger value exceeds the manifest's own absolute bound for",
        "that column — F16.2: neither metric means anything near cancellation, and",
        "a sign crossing would otherwise be the only thing the table showed.",
        "",
        "| Fixture / column | max \\|Δ\\| | rows | near-zero rows | max relative |"
        " max ULP | identical | ULP distribution |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for name, d in rows:
        out.append(
            f"| {name} | {d['max_abs']:.6e} | {d['rows']} | "
            f"{d['near_zero']} (|v| ≤ {d['floor']:.3e}) | {d['max_rel']:.6e} | "
            f"{d['max_ulp']} | {d['identical']} | {d['histogram']} |"
        )
    out += [
        "",
        "For reference, the two bounds section 02 measured on the generating build "
        "and wrote into the manifest, which this section consumes rather than "
        "choosing its own (F16.6):",
        "",
        "| Measured by section 02 | value |",
        "|---|---|",
        f"| `kernel_vs_libm_absolute` | {kernel.kernel_vs_libm_absolute:.6e} |",
        f"| `summation_vs_compensated_absolute` | "
        f"{kernel.summation_vs_compensated_absolute:.6e} m/s |",
        "",
        END,
        "",
    ]
    return "\n".join(out)


def _write(path: Path, text: str) -> None:
    doc = path.read_text(encoding="utf-8")
    if BEGIN in doc and END in doc:
        doc = re.sub(
            re.escape(BEGIN) + r".*?" + re.escape(END) + r"\n?",
            text,
            doc,
            flags=re.DOTALL,
        )
    else:
        doc = doc.rstrip("\n") + "\n\n---\n\n" + text
    path.write_text(doc, encoding="utf-8")


def _assert_recorded(path: Path, bundle: Bundle) -> None:
    """The section is in the document, and names this bundle.

    The *numbers* are deliberately not asserted: they are a property of the
    JAX build and the backend, and F16.6 makes them a report rather than a
    threshold. What is asserted is that the report exists and describes the
    bundle in front of it — which is what goes wrong when someone regenerates
    `conformance.md` from Rust and silently drops the appended section.
    """
    doc = path.read_text(encoding="utf-8")
    assert BEGIN in doc and END in doc, (
        f"{CONFORMANCE_DOC} has no two-arm divergence section. It is appended by "
        f"`SAILGYM_WRITE_CONFORMANCE=1 scripts/py-test.sh` and is dropped whenever "
        f"`gen_conformance --write` regenerates the file; re-run it."
    )
    block = doc[doc.index(BEGIN) : doc.index(END)]
    assert bundle.digest in block, (
        f"{CONFORMANCE_DOC}'s two-arm section names a different bundle than the one "
        f"loaded ({bundle.digest}); re-run with SAILGYM_WRITE_CONFORMANCE=1."
    )


# ---------------------------------------------------------------------------
# What the bundle does not cover
# ---------------------------------------------------------------------------


def test_the_odd_mode_tail_is_not_covered_by_this_bundle(bundle: Bundle):
    """An honest gap, asserted so it cannot quietly become untrue.

    ``sample_inner`` has a tail loop for an odd ``K`` (``wind.rs:516-519``)
    whose contribution lands in accumulator slots 0 and 1 rather than in a
    third pair. Every field in this bundle has ``K`` of 0 or 12, so **the
    tail path is transcribed but unverified against Rust**. It is recorded as
    a limitation in ``docs/v2/progress/03-handoff.md`` rather than papered
    over with a self-comparison, which would only prove the port agrees with
    itself.

    What can be checked without a reference is that the tail is not silently
    dropped: truncating a twelve-mode field to eleven must move the answer by
    that eleventh mode's contribution. Not *exactly* — the tail lands inside
    slot 0 rather than being added to the finished total, so the two differ
    in their association — which is itself the reason the tail is transcribed
    rather than appended.
    """
    counts = sorted({m.count for m in bundle.modes()})
    assert counts == [0, 12], (
        f"a field with an odd mode count has appeared ({counts}); the odd-K tail is "
        f"now covered by the bundle and this test should become a real comparison"
    )

    full = bundle.modes("synthetic_gust")
    assert full.count == 12

    class _Truncated:
        k_x = full.k_x[:11]
        k_y = full.k_y[:11]
        amp_x = full.amp_x[:11]
        amp_y = full.amp_y[:11]
        omega = full.omega[:11]
        phase = full.phase[:11]

    class _Ten:
        k_x = full.k_x[:10]
        k_y = full.k_y[:10]
        amp_x = full.amp_x[:10]
        amp_y = full.amp_y[:10]
        omega = full.omega[:10]
        phase = full.phase[:10]

    base = (0.0, 0.0)
    x, y, t = 37.5, -12.25, 3.75
    odd_x, odd_y = W.sample(_Truncated, base, x, y, t)
    even_x, even_y = W.sample(_Ten, base, x, y, t)
    theta = full.k_x[10] * x + full.k_y[10] * y + full.omega[10] * t + full.phase[10]
    c = np.asarray(W.wave_exact(theta))
    for got, without, amp, column in (
        (odd_x, even_x, full.amp_x[10], "wx"),
        (odd_y, even_y, full.amp_y[10], "wy"),
    ):
        contribution = float(amp) * float(c)
        assert float(got) != float(without), (
            f"the eleventh mode contributed nothing to {column}; the odd-K tail of "
            f"wind.rs:516-519 has been dropped"
        )
        tol, _ = bundle.tolerance("tier0_wind_sample", column)
        assert abs((float(got) - float(without)) - contribution) <= tol.bound(
            np.abs(np.float64(contribution))
        )
