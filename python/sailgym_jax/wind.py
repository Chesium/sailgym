"""``ProceduralWind::sample`` in JAX — the two arms of F16.6.

The Rust source this file is transcribed from is
``crates/sailgym-physics/src/environment/wind.rs``. That function was chosen
as the first port because it is the easiest possible target: pure,
branch-free, a twelve-term sum, and — because section 02 exported the mode
table as data — carrying no dependence on the generator of the mode table at
all. What is actually being built here is the toolchain. When the hard port
lands, the only new thing being tested will be the physics.

## The two arms

=================  =====================================  ==========================
Arm                Kernel                                 Held to
=================  =====================================  ==========================
:func:`wave_exact` the Cody-Waite reduction and the       tier 0's recorded
                   polynomial of ``wind.rs``, transcribed  per-column bound
:func:`wave_cos`   ``jnp.cos``                            the **measured** bound
                                                          in the manifest
=================  =====================================  ==========================

Both arms read the same mode table and the same parameters. **Neither is "the
port"**: this module exports both and the caller chooses. ``wave_cos`` alone
could never catch a defect smaller than the kernel gap — about two ULP per
mode — so "the port agrees to 1e-13" would be an untestable claim;
``wave_exact`` alone is a conformance witness whose performance under fusion
nobody has measured. Together they give a measured number for what the
convenient choice costs. That divergence is **reported**, never asserted
against a threshold (F16.6).

## What is not here, and must never be

The mode table comes from ``wind_modes.json`` through the loader and is never
regenerated. F16.7: the generator of that table is Rust's alone, and this
file holds no part of it — no draw, no shuffle, no stratification, no
generator state and no argument naming one.
``python/tests/test_no_stray_constants.py`` asserts the absence by scanning
for the four words, as a fourth pass, which is why those words are spelled
nowhere in this file. If a reviewer finds one here, the port is wrong in a
way no tolerance will catch.

## Why the transcription is structural

A mathematically equivalent regrouping is **not** equivalent in floating
point. The Cody-Waite constants, the fold onto ``[0, pi/2]``, the clamp, the
polynomial's Estrin grouping and the two-slot pairwise accumulation of
``sample_inner`` (including the odd-``K`` tail and the empty-table early
return that avoids ``base + 0.0`` negative-zero drift) are reproduced
structurally, not re-derived. Preserving the operation structure is what
isolates a kernel or reduction difference from a modelling difference.

XLA may still fuse arithmetic, so **cross-stack bit identity is not promised
and is not claimed** (F16.1). F6.1's bit identity between ``sample`` and
``sample_grid`` remains the Rust contract; here the same property is asserted
to the manifest tolerance, and :func:`sample_grid` says so in its doc string.
"""

from __future__ import annotations

import math
from typing import Protocol

import jax.numpy as jnp

__all__ = [
    "ModeTable",
    "base_from_bearing",
    "sample",
    "sample_grid",
    "wave_cos",
    "wave_exact",
]

# ---------------------------------------------------------------------------
# Transcribed constants. Every one cites the line it came from (F17.1).
# ---------------------------------------------------------------------------

TAU = math.tau
"""``2*pi``. ``crates/sailgym-physics/src/environment/wind.rs:76`` (``TAU``),
which is ``std::f64::consts::TAU``; ``math.tau`` is the same ``f64``."""

INV_TAU = 1.0 / TAU
"""``1/TAU``, computed rather than typed, exactly as
``crates/sailgym-physics/src/environment/wind.rs:77`` computes it. One
division of two exactly representable constants gives the same ``f64`` in
both languages, and typing the decimal expansion would be one more place to
make a typing mistake."""

PI = math.pi
"""``crates/sailgym-physics/src/environment/wind.rs:175``
(``std::f64::consts::PI``, in the fold ``cos(pi - x) = -cos x``)."""

FRAC_PI_2 = PI / 2.0
"""``crates/sailgym-physics/src/environment/wind.rs:174``
(``std::f64::consts::FRAC_PI_2``). Halving ``pi`` is exact in binary, so this
is bit-identical to Rust's constant."""

TAU_HI = 6.283_185_243_606_567
"""Cody-Waite high word of ``2*pi``, 26 significant bits so that ``n*TAU_HI``
is exact for every ``|n| < 2**26``.
``crates/sailgym-physics/src/environment/wind.rs:87`` (``TAU_HI``)."""

TAU_LO = 6.357_301_909_411_278e-8
"""Cody-Waite middle word of ``2*pi``.
``crates/sailgym-physics/src/environment/wind.rs:88`` (``TAU_LO``)."""

TAU_LO2 = 2.547_326_865_404_38e-24
"""Cody-Waite low word of ``2*pi``; the three sum to ``2*pi`` to 40 digits.
``crates/sailgym-physics/src/environment/wind.rs:89`` (``TAU_LO2``)."""

ROUND_MAGIC = 6_755_399_441_055_744.0
"""``3 * 2**51``, the round-to-nearest-integer magic number for ``f64``.
``crates/sailgym-physics/src/environment/wind.rs:92`` (``ROUND_MAGIC``)."""

# The Taylor coefficients of cos, 1/4! through 1/20!, in the Estrin grouping
# of the Rust kernel. `1/2!` is the structural `0.5` in `_polynomial` below.
# All nine: crates/sailgym-physics/src/environment/wind.rs:191-195.

C4 = 4.166_666_666_666_666_4e-2
"""``1/4!``. ``crates/sailgym-physics/src/environment/wind.rs:191``."""

C6 = 1.388_888_888_888_889e-3
"""``1/6!``. ``crates/sailgym-physics/src/environment/wind.rs:191``."""

C8 = 2.480_158_730_158_73e-5
"""``1/8!``. ``crates/sailgym-physics/src/environment/wind.rs:192``."""

C10 = 2.755_731_922_398_589e-7
"""``1/10!``. ``crates/sailgym-physics/src/environment/wind.rs:192``."""

C12 = 2.087_675_698_786_81e-9
"""``1/12!``. ``crates/sailgym-physics/src/environment/wind.rs:193``."""

C14 = 1.147_074_559_772_972_5e-11
"""``1/14!``. ``crates/sailgym-physics/src/environment/wind.rs:193``."""

C16 = 4.779_477_332_387_385e-14
"""``1/16!``. ``crates/sailgym-physics/src/environment/wind.rs:194``."""

C18 = 1.561_920_696_858_622_5e-16
"""``1/18!``. ``crates/sailgym-physics/src/environment/wind.rs:194``."""

C20 = 4.110_317_623_312_165e-19
"""``1/20!``. ``crates/sailgym-physics/src/environment/wind.rs:195``."""

DEG_TO_RAD = PI / 180.0
"""Rust's ``f64::to_radians`` is ``self * (PI / 180.0)``, and
``crates/sailgym-physics/src/environment/mod.rs:55`` calls it. The division is
written the same way here so the multiplier is the same ``f64``."""


# ---------------------------------------------------------------------------
# The two arms
# ---------------------------------------------------------------------------


def wave_exact(theta):
    """``cos theta`` by the transcribed kernel of ``wind.rs::wave_kernel``.

    ``crates/sailgym-physics/src/environment/wind.rs:163-202``, line for line:
    the three-way Cody-Waite reduction, the fold onto ``[0, pi/2]`` by
    ``cos(pi - x) = -cos x``, the clamp that makes the function total for an
    absurd argument, and the Estrin-grouped Taylor polynomial.

    The reduction is written as three separate subtractions on purpose. One
    step of ``theta - n*TAU`` loses a digit for every power of ten in
    ``theta``; at the edge of the domain the bundle sweeps (``1e5``) that is
    an error of order ``1e-12`` rad, which is three decades outside the
    tier-0 bound. The split keeps the reduction accurate to the last bit of
    the argument.

    The magic-number rounding — add ``3*2**51``, subtract it again — is
    transcribed rather than replaced by ``jnp.round``. Rust adopted it to
    avoid a library call on a target with no ``roundsd``; here the reason is
    different and it is the section's whole method. The trick rounds by
    whatever the *current rounding mode* does to the discarded mantissa bits,
    while a rounding intrinsic rounds by its own documented rule, and
    substituting one for the other is a bet that the two agree on every
    argument including the ties. A conformance witness exists to avoid
    exactly that class of bet. It is also the line most at risk from a
    compiler that reassociates — ``(a + K) - K`` is algebraically ``a`` — so
    it is the first thing to look at if this arm ever stops matching.
    """
    theta = jnp.asarray(theta)
    n = (theta * INV_TAU + ROUND_MAGIC) - ROUND_MAGIC
    r = ((theta - n * TAU_HI) - n * TAU_LO) - n * TAU_LO2

    a = jnp.abs(r)
    folded = a > FRAC_PI_2
    folded_x = PI - a
    x = jnp.clip(jnp.where(folded, folded_x, a), 0.0, FRAC_PI_2)

    p = _polynomial(x)
    return jnp.where(folded, -p, p)


def _polynomial(x):
    """The Estrin tree of ``wind.rs:187-196``, grouped exactly as written.

    Estrin rather than Horner is Rust's latency argument, and the grouping is
    not an optimisation detail here: ``(a0 + u4*a1) + u8*a2`` and a Horner
    chain over the same coefficients are different floating-point numbers.
    """
    u = x * x
    u2 = u * u
    u4 = u2 * u2
    u8 = u4 * u4
    a0 = (1.0 - 0.5 * u) + u2 * (C4 - C6 * u)
    a1 = (C8 - C10 * u) + u2 * (C12 - C14 * u)
    a2 = (C16 - C18 * u) + u2 * C20
    return (a0 + u4 * a1) + u8 * a2


def wave_cos(theta):
    """``cos theta`` by the host library — the second arm of F16.6.

    This is the arm anyone would write. It cannot meet the tier-0 ULP bound,
    and it is not asked to: the manifest carries a **measured** bound for it
    (``tolerances.wind_kernel.kernel_vs_libm_absolute``), measured by section
    02 on the generating build over ``theta`` in ``[-1e5, 1e5]``.
    """
    return jnp.cos(jnp.asarray(theta))


# ---------------------------------------------------------------------------
# The mode table
# ---------------------------------------------------------------------------


class ModeTable(Protocol):
    """What :func:`sample` needs of a mode table.

    ``sailgym_conformance.WindModes`` satisfies it. The protocol is stated
    structurally rather than imported so that this package stays an
    independent implementation: a verification arm that shares a data class
    with the thing it verifies has shared one more decision than it should.

    Every field is a ``float64`` array of length ``K``, in the bundle's order.
    ``amp`` already carries the stream-function amplitude folded in — it is
    ``a_k * n_hat_k`` with ``n_hat_k`` perpendicular to ``k_k``, which is what
    makes the field divergence-free as a property of the stored data rather
    than of the sampling loop (``wind.rs:300-307``).
    """

    k_x: object
    k_y: object
    amp_x: object
    amp_y: object
    omega: object
    phase: object


def base_from_bearing(speed: float, bearing_deg: float):
    """``W0``, the base wind velocity in the world frame.

    F6.1's single conversion, ``wind_from_bearing`` at
    ``crates/sailgym-physics/src/environment/mod.rs:54-57``: the scenario and
    the UI both speak the meteorological **from** bearing in degrees clockwise
    from north, and the field speaks a velocity vector pointing where the air
    is going.

    ``jnp.sin``/``jnp.cos`` are the host library here, so this is
    ``wave_cos``-grade agreement, not kernel-grade. ``sample`` takes ``base``
    as an argument precisely so a caller may supply a bit-exact one instead.
    """
    b = jnp.asarray(bearing_deg) * DEG_TO_RAD
    return (-jnp.asarray(speed) * jnp.sin(b), -jnp.asarray(speed) * jnp.cos(b))


# ---------------------------------------------------------------------------
# sample
# ---------------------------------------------------------------------------


def sample(modes: ModeTable, base, x, y, t, *, wave=wave_exact):
    """The world-frame air velocity at ``(x, y, t)``.

    ``crates/sailgym-physics/src/environment/wind.rs:495-525``
    (``sample_inner``), transcribed including its accumulation order.

    ``x``, ``y`` and ``t`` broadcast against one another; the result is a pair
    of arrays of the broadcast shape. ``base`` is the pair ``(W0x, W0y)``.

    Three details are load-bearing and none is an optimisation:

    * **The empty-table early return.** A field with no modes returns ``base``
      itself, not ``base + 0.0`` — those differ for a negative zero, and the
      shipped uniform fields have one (``wind.rs:496-498``).
    * **Two accumulator pairs, not one.** Even-indexed modes accumulate into
      slots 0 and 1, odd-indexed into 2 and 3, and the two halves are added
      once at the end. Rust adopted the split for throughput; F9.4 then made
      the resulting order part of the answer (``wind.rs:499-514``).
    * **The odd-``K`` tail goes into slots 0 and 1**, not into a third pair
      (``wind.rs:516-519``).

    The per-mode phase is ``((k.x*x + k.y*y) + omega*t) + phase``, left
    associated as Rust associates it.
    """
    base_x, base_y = base
    x = jnp.asarray(x)
    y = jnp.asarray(y)
    t = jnp.asarray(t)

    k_x = jnp.asarray(modes.k_x)
    k_y = jnp.asarray(modes.k_y)
    amp_x = jnp.asarray(modes.amp_x)
    amp_y = jnp.asarray(modes.amp_y)
    omega = jnp.asarray(modes.omega)
    phase = jnp.asarray(modes.phase)

    count = int(k_x.shape[0])
    shape = jnp.broadcast_shapes(jnp.shape(x), jnp.shape(y), jnp.shape(t))
    if count == 0:
        return (
            jnp.broadcast_to(jnp.asarray(base_x), shape),
            jnp.broadcast_to(jnp.asarray(base_y), shape),
        )

    zero = jnp.zeros(shape, dtype=k_x.dtype)
    acc = [zero, zero, zero, zero]

    # `modes.as_chunks::<2>()`: the pairs, then the tail.
    pairs = count - (count % 2)
    for i in range(0, pairs, 2):
        c0 = wave(k_x[i] * x + k_y[i] * y + omega[i] * t + phase[i])
        c1 = wave(k_x[i + 1] * x + k_y[i + 1] * y + omega[i + 1] * t + phase[i + 1])
        acc[0] = acc[0] + amp_x[i] * c0
        acc[1] = acc[1] + amp_y[i] * c0
        acc[2] = acc[2] + amp_x[i + 1] * c1
        acc[3] = acc[3] + amp_y[i + 1] * c1
    for i in range(pairs, count):
        c = wave(k_x[i] * x + k_y[i] * y + omega[i] * t + phase[i])
        acc[0] = acc[0] + amp_x[i] * c
        acc[1] = acc[1] + amp_y[i] * c

    return (base_x + (acc[0] + acc[2]), base_y + (acc[1] + acc[3]))


def sample_grid(modes: ModeTable, base, x0, y0, dx, dy, nx, ny, t, *, wave=wave_exact):
    """The field on a regular grid, row-major, as ``[wx, wy]`` pairs.

    ``crates/sailgym-physics/src/environment/wind.rs:535-561``. Node
    ``(i, j)`` is at ``(x0 + i*dx, y0 + j*dy)`` and lands at index
    ``2*(j*nx + i)``; the node coordinates are formed the same way, as
    ``origin + index*step`` rather than by repeated addition, because
    accumulating a step drifts.

    **This does not promise Rust's bit identity.** F6.1 requires
    ``sample_grid`` to produce bit-identical values to ``sample`` *in Rust*,
    and ``tests/wind.rs::grid_bitwise_matches_point_sweep`` asserts exact
    ``f32`` equality there. Across stacks that is unavailable and is not
    claimed (F16.1): XLA may fuse or reassociate differently between a
    vectorised grid evaluation and a point evaluation, so the property
    asserted here is agreement **to the manifest tolerance**. Do not later
    "fix" the Python test into an equality; it would be asserting something
    this stack does not provide.

    Returns ``(wx, wy)``, each of shape ``(ny, nx)``.
    """
    i = jnp.arange(nx, dtype=jnp.float64)
    j = jnp.arange(ny, dtype=jnp.float64)
    x = jnp.asarray(x0) + i * jnp.asarray(dx)
    y = jnp.asarray(y0) + j * jnp.asarray(dy)
    return sample(
        modes, base, x[jnp.newaxis, :], y[:, jnp.newaxis], jnp.asarray(t), wave=wave
    )
