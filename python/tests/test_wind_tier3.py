"""Tier 3: the invariants a wind-only port can actually satisfy.

These mirror ``crates/sailgym-physics/tests/wind.rs`` and the brief §35
invariants it enforces. Each test's doc string names the invariant or the
Rust test it mirrors — that is task 3.5's acceptance criterion, and it is
also the only way a reader can tell a real invariant from a property someone
liked the look of.

**What this slice licenses, and what it does not.** Four properties hold for
the wind field alone, and holding them is what makes "the same wind field" a
defensible phrase rather than "a function that tracks a table for a few
rows". The rest of brief §35 — energy, momentum, the finite-number invariant
over a *trajectory* — needs a full port and belongs to a later section. There
is no tier 3 in the bundle (section 02 §F16.9.7); these are computed here,
against criteria taken from the Rust tests and from the field's own
configuration, never from a recorded array.
"""

from __future__ import annotations

import numpy as np

from sailgym_conformance import Bundle
from sailgym_jax import wind as W

ARMS = (("wave_exact", W.wave_exact), ("wave_cos", W.wave_cos))


def halton(i: int, base: int) -> float:
    """Radical inverse. A deterministic point source with no generator state.

    ``tests/wind.rs`` uses a test-local PCG32 for the same job. A port may
    not carry PCG32 (F16.7), and the property under test does not care where
    the points come from, so this is the low-discrepancy equivalent — and it
    covers the domain more evenly than a pseudo-random sweep of the same size.
    """
    f, r = 1.0, 0.0
    while i > 0:
        f /= base
        r += f * (i % base)
        i //= base
    return r


def points(n: int, span: float, t_span: float):
    x = np.array([-span + 2.0 * span * halton(i + 1, 2) for i in range(n)])
    y = np.array([-span + 2.0 * span * halton(i + 1, 3) for i in range(n)])
    t = np.array([t_span * halton(i + 1, 5) for i in range(n)])
    return x, y, t


def base_of(field) -> tuple:
    return W.base_from_bearing(field.config["speed"], field.config["bearing_deg"])


# ---------------------------------------------------------------------------


def test_finite_over_a_wide_domain(bundle: Bundle):
    """brief §35's finite-number invariant; mirrors
    ``tests/wind.rs::finite_over_wide_domain``.

    No NaN and no infinity over a domain far wider than any scenario uses —
    the same corners and times the Rust test sweeps (``|x|`` to ``1e5`` m,
    ``t`` to ``1e4`` s), plus a dense interior sweep. This is the invariant
    the ``clamp`` in ``wave_kernel`` exists for: beyond ``2**51`` turns the
    magic-number rounding stops working, and an unclamped kernel would
    overflow rather than return a number in ``[-1, 1]``.
    """
    corners = np.array([-1e5, -1e3, -1.0, 0.0, 1.0, 1e3, 1e5])
    times = np.array([0.0, 1e-3, 1.0, 60.0, 3600.0, 1e4])
    gx, gy, gt = (
        a.ravel() for a in np.meshgrid(corners, corners, times, indexing="ij")
    )
    dx, dy, dt = points(5000, 1e5, 1e4)
    x = np.concatenate([gx, dx])
    y = np.concatenate([gy, dy])
    t = np.concatenate([gt, dt])

    for field in bundle.modes():
        for name, arm in ARMS:
            wx, wy = W.sample(field, base_of(field), x, y, t, wave=arm)
            wx, wy = np.asarray(wx), np.asarray(wy)
            bad = ~(np.isfinite(wx) & np.isfinite(wy))
            assert not bad.any(), (
                f"{field.name}/{name}: non-finite at "
                f"({x[bad][0]}, {y[bad][0]}, {t[bad][0]})"
            )


def test_the_perturbation_stays_inside_the_bound_variation_implies(bundle: Bundle):
    """Mirrors ``tests/wind.rs::bounded_magnitude``.

    ``variation`` is *defined* by the bound it implies: the field stays inside
    ``speed * (1 + 3*variation)`` (``wind.rs:56-68``, ``RMS_PER_VARIATION``).
    The criterion comes from the field's own configuration in
    ``wind_modes.json``, not from a recorded array — an invariant checked
    against a recording is a recording, not an invariant.

    A regular sweep catches the structured peaks and a low-discrepancy one
    catches what falls between the grid lines, exactly as the Rust test does.
    """
    for field in bundle.modes():
        cfg = field.config
        bound = cfg["speed"] * (1.0 + 3.0 * cfg["variation"])
        span = 4.0 * cfg["length_scale"]
        n = 128
        axis = -span + 2.0 * span * np.arange(n) / n
        gx = np.repeat(axis, n)
        gy = np.tile(axis, n)
        time_scale = min(cfg["time_scale"], 1e3)
        worst = 0.0
        for name, arm in ARMS:
            for k in range(4):
                t = float(k) * 0.37 * time_scale
                wx, wy = W.sample(field, base_of(field), gx, gy, t, wave=arm)
                worst = max(worst, float(np.max(np.hypot(wx, wy))))
            sx, sy, st = points(20000, 1e4, 1e3)
            wx, wy = W.sample(field, base_of(field), sx, sy, st, wave=arm)
            worst = max(worst, float(np.max(np.hypot(wx, wy))))
            assert worst <= bound, (
                f"{field.name}/{name}: |w| reached {worst} > {bound} "
                f"(speed {cfg['speed']}, variation {cfg['variation']})"
            )
        print(f"[tier3] {field.name}: worst |w| {worst:.6f} of bound {bound:.6f}")


def test_the_grid_matches_point_sampling(bundle: Bundle):
    """Mirrors ``tests/wind.rs::grid_bitwise_matches_point_sweep`` — **to a
    tolerance, not to equality, and that difference is deliberate.**

    F6.1 requires ``sample_grid`` to produce bit-identical values to
    ``sample``: the visualization and the physics are the same field (brief
    §19, §47), and the Rust test asserts exact ``f32`` equality. Across
    stacks that guarantee is unavailable and F16.1 forbids claiming it — XLA
    may fuse or reassociate a vectorised grid evaluation differently from a
    point evaluation. So the property asserted here is agreement to the
    manifest's tier-0 bound for ``tier0_wind_sample``.

    **Do not "fix" this into an equality.** If it happens to hold today on
    one backend, asserting it would be asserting something this stack does
    not provide, and the next XLA release would make it a mystery failure.
    """
    nx, ny = 24, 17
    # A deliberately awkward origin and spacing, as the Rust test uses: a
    # grid aligned to the wavelength could hide a phase error.
    x0, y0, dx, dy, t = -317.5, 88.25, 9.75, -6.125, 23.5
    i = np.arange(nx, dtype=np.float64)
    j = np.arange(ny, dtype=np.float64)

    for field in bundle.modes():
        base = base_of(field)
        for name, arm in ARMS:
            gx, gy = W.sample_grid(field, base, x0, y0, dx, dy, nx, ny, t, wave=arm)
            px, py = W.sample(
                field,
                base,
                (x0 + i * dx)[np.newaxis, :],
                (y0 + j * dy)[:, np.newaxis],
                t,
                wave=arm,
            )
            assert np.asarray(gx).shape == (ny, nx)
            for column, g, p in (("wx", gx, px), ("wy", gy, py)):
                tol, _ = bundle.tolerance("tier0_wind_sample", column)
                g, p = np.asarray(g), np.asarray(p)
                err = np.abs(g - p)
                assert np.all(err <= tol.bound(p)), (
                    f"{field.name}/{name}/{column}: grid and point differ by "
                    f"{err.max():.3e}, beyond the tier-0 bound"
                )


def test_a_uniform_field_is_exactly_the_base_vector(bundle: Bundle):
    """Mirrors ``tests/wind.rs::uniform_is_constant``; the empty-modes path.

    ``ProceduralWind::new`` builds an **empty** mode list for a uniform field
    so that ``sample`` returns ``base`` bit for bit rather than ``base + 0.0``
    — and those differ for a negative zero (``wind.rs:375-388``,
    ``wind.rs:496-498``). This is the one place in the port where exact
    equality *is* the right assertion, because no arithmetic happens: the
    early return either fires or it does not.
    """
    uniform = [m for m in bundle.modes() if m.count == 0]
    assert uniform, "the bundle ships no uniform field; this test is vacuous"

    x, y, t = points(64, 5000.0, 1000.0)
    for field in uniform:
        base_x, base_y = base_of(field)
        wx, wy = W.sample(field, (base_x, base_y), x, y, t)
        assert np.all(np.asarray(wx) == np.float64(base_x))
        assert np.all(np.asarray(wy) == np.float64(base_y))

    # …including the sign of zero, which is the whole reason the early return
    # is written the way it is. `base + 0.0` would turn -0.0 into +0.0.
    field = uniform[0]
    wx, wy = W.sample(field, (0.0, -0.0), x, y, t)
    assert np.all(np.signbit(np.asarray(wy))), (
        "the empty-modes path added something: -0.0 came back as +0.0, which is "
        "`base + 0.0` and not `base` (wind.rs:496-498)"
    )
    assert not np.any(np.signbit(np.asarray(wx)))


def test_the_uniform_base_agrees_with_the_recorded_field(bundle: Bundle):
    """``base_from_bearing`` against Rust's ``wind_from_bearing``.

    The conversion of F6.1 lives in exactly one function on each side, and a
    uniform field's recorded sample **is** that function's output. Held to
    the tier-0 bound rather than to equality: both sides call their host's
    ``sin`` and ``cos`` here, and F16.1 does not let a port claim more.
    """
    table = bundle.table("tier0_wind_sample")
    index = table.column("field").astype(int)
    for field in bundle.modes():
        if field.count != 0:
            continue
        rows = index == field.index
        if not rows.any():
            continue
        base_x, base_y = base_of(field)
        for column, value in (("wx", base_x), ("wy", base_y)):
            recorded = table.column(column)[rows]
            tol, _ = bundle.tolerance("tier0_wind_sample", column)
            err = np.abs(np.float64(value) - recorded)
            assert np.all(err <= tol.bound(recorded)), (
                f"{field.name}/{column}: base_from_bearing gives {float(value)!r}, "
                f"the bundle records {recorded[0]!r}"
            )
