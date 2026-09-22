"""The three performance non-negotiables, measured.

v2 section 07 task 7.5; F17.5, RV42, RV46.

*"These are numeric assertions, per F13.4. A comment saying 'we release the
GIL' is not one."* So each of the three is a measurement with a stated bound,
and each is printed so the handoff can quote a number rather than a verdict.

The three are deliberately different kinds of claim, and the file keeps them
apart:

1. **Zero-copy** is a claim about *addresses*. The observation array's data
   pointer does not move across ``step_all`` calls, and what Rust writes is
   visible in the caller's own array.
2. **The GIL** is a claim about *overlap*, and the right way to measure it is
   **how much work a Python thread gets done** while Rust steps — not the
   wall-clock ratio, which cannot tell the two cases apart. See
   :data:`OVERLAP_BOUND` for the transcript that settled that.
3. **Bounded memory** is a claim about *growth*, and it is two claims: what
   is retained across 1 000 steps, and what is allocated temporarily inside
   one. A stable retained figure does not prove the second, which is why they
   are measured separately — and the low-level buffers are reported
   separately from the Gymnasium tuple and ``info`` dictionary above them,
   which are required by the standard and are the right trade.
"""

from __future__ import annotations

import gc
import os
import sys
import threading
import time
import tracemalloc

import numpy as np
import pytest

import sailgym
import sailgym_core

SCENARIO = "free_sail"
SEED = 20260922
CADENCE = 10

# The bounds, stated here rather than discovered. Each is deliberately loose
# enough not to be flaky on a shared machine and tight enough that the defect
# it guards against fails it by a wide margin -- the margins are printed.
PROGRESS_BOUND = 0.5
"""How much of its solo work the Python thread must finish *while Rust steps*.

The sharp signal, and the primary assertion. The worker runs to a wall-clock
deadline, so a GIL held across ``step_all`` blocks it for the whole window and
it finishes **almost nothing**: the measurement is near 1.0 when the GIL is
released and near 0.0 when it is not, so 0.5 is not a threshold between two
close numbers but a line drawn across an empty middle.

The batch is deliberately **few, long calls** rather than many short ones.
With many short calls a held GIL is released between them and the worker
creeps forward on the interpreter's switch interval, which would put the
"held" case somewhere in the middle and give the bound something to be noisy
about. Three calls of ~100 ms each leave a held GIL nowhere to hide.
"""

OVERLAP_BOUND = 0.9
"""The two-tasks-together time, as a fraction of the two-apart sum.

**Reported, and deliberately not asserted.** The obvious formulation of an
overlap test, and it does not work — which is a finding rather than a
guess. Removing ``py.detach`` from ``crates/sailgym-py/src/vector.rs`` and
re-running this file put *progress* at 0.073 and left the ratio at **0.500**,
inside this bound and better than the correct build's 0.540. The reason is
that the denominator contains a solo Python measurement that, in the held-GIL
case, never happens: the worker is blocked, contributes no wall time, and the
pair looks beautifully overlapped. The number is printed because it is
informative about how much a competing thread costs the batch; the assertion
is :data:`PROGRESS_BOUND`'s. ``docs/v2/progress/07-handoff.md`` records the
transcript.
"""

RETAINED_BLOCKS = 64
"""Python blocks the low-level batch loop may retain over 1 000 steps.

Not zero, because `tracemalloc` and the interpreter's own bookkeeping move a
little; small enough that one allocation per step (1 000 of them) fails it by
an order of magnitude. RV46 is the risk.
"""

PEAK_BYTES_PER_STEP = 4096
"""Peak temporary allocation attributable to one low-level batch step."""

BIG_N = 4096
"""Environments in the GIL measurement's batch — section 06's largest N."""

ROUNDS = 3
"""Calls per arm. Few and long, for the reason :data:`PROGRESS_BOUND` gives."""


def big_batch(n: int = 512, *, parallel: bool = True):
    spec = sailgym_core.Spec(SCENARIO, cadence=CADENCE)
    seeds = [SEED + i for i in range(n)]
    raw = sailgym_core.RawVecEnv(spec, seeds, parallel)
    buffers = {
        "actions": np.zeros((n, spec.action_dim), dtype=np.float64),
        "obs": np.zeros((n, spec.obs_len), dtype=np.float32),
        "rewards": np.zeros(n, dtype=np.float64),
        "terminated": np.zeros(n, dtype=np.uint8),
        "truncated": np.zeros(n, dtype=np.uint8),
    }
    return raw, buffers


def step(raw, b) -> None:
    raw.step_all(b["actions"], b["obs"], b["rewards"], b["terminated"], b["truncated"])


# ---------------------------------------------------------------------------
# 1. Zero-copy
# ---------------------------------------------------------------------------


def test_the_observation_buffer_is_the_callers_and_never_moves():
    """F17.5's first non-negotiable, as an address and as a write."""
    raw, b = big_batch(64)
    obs = b["obs"]
    pointer = obs.__array_interface__["data"][0]

    obs[:] = np.nan
    step(raw, b)
    assert obs.__array_interface__["data"][0] == pointer
    # The caller's own array was written, in place: nothing was returned and
    # nothing was copied back.
    assert np.isfinite(obs).all(), "step_all did not write into the caller's buffer"

    first = obs.copy()
    for _ in range(16):
        step(raw, b)
        assert obs.__array_interface__["data"][0] == pointer
    assert not np.array_equal(first, obs), (
        "the buffer never changed; this test would pass on a no-op"
    )
    # The rewards and both masks are the caller's too.
    for name in ("rewards", "terminated", "truncated"):
        assert b[name].__array_interface__["data"][0] == b[name].ctypes.data
    print(
        f"[perf] zero-copy: obs buffer at 0x{pointer:x} unchanged across 17 "
        f"step_all calls, {obs.nbytes} bytes written in place"
    )


def test_the_vector_env_reuses_one_buffer_and_says_so():
    """The public adapter keeps the property the low-level API has."""
    env = sailgym.SailgymVectorEnv(SCENARIO, num_envs=32, seeds=SEED, cadence=CADENCE)
    obs, _ = env.reset(seed=SEED)
    pointer = obs.__array_interface__["data"][0]
    actions = np.zeros((env.num_envs, env.single_action_space.shape[0]))
    for _ in range(8):
        obs, rewards, _, _, _ = env.step(actions)
        assert obs.__array_interface__["data"][0] == pointer
    print(f"[perf] the VectorEnv returns one buffer, at 0x{pointer:x}")
    env.close()


def test_a_buffer_of_the_wrong_shape_is_refused_not_truncated():
    raw, b = big_batch(8)
    bad = np.zeros((7, b["obs"].shape[1]), dtype=np.float32)
    with pytest.raises(ValueError, match="shape"):
        raw.step_all(b["actions"], bad, b["rewards"], b["terminated"], b["truncated"])
    # A non-contiguous view is refused rather than silently copied: a caller
    # who handed in a slice meant to see the answer in the parent array.
    view = np.zeros((8, b["obs"].shape[1] * 2), dtype=np.float32)[:, ::2]
    assert not view.flags["C_CONTIGUOUS"]
    with pytest.raises(ValueError):
        raw.step_all(b["actions"], view, b["rewards"], b["terminated"], b["truncated"])


# ---------------------------------------------------------------------------
# 2. The GIL
# ---------------------------------------------------------------------------


def _busy(deadline: float, counter: list[int]) -> None:
    """Pure-Python work. It cannot run at all while another thread holds the
    GIL inside a C extension, which is the whole measurement."""
    n = 0
    while time.perf_counter() < deadline:
        for _ in range(1000):
            n += 1
    counter.append(n)


def _spin(seconds: float) -> int:
    counter: list[int] = []
    _busy(time.perf_counter() + seconds, counter)
    return counter[0]


def test_the_gil_is_released_for_the_duration_of_step_all():
    """F17.5's second non-negotiable, and RV42.

    Two numbers, and the first is the one that matters:

    * **progress** — how much of its solo work a Python thread finishes while
      Rust steps. The thread runs to a wall-clock deadline, so a held GIL
      leaves it at ~0.0 and a released one at ~1.0.
    * **ratio** — the wall time of the two together over the sum of the two
      apart. Looser, because a competing thread does slow the batch down.

    Skipped **loudly** on a single-core host, where there is nothing to
    overlap with and the measurement would mean nothing.
    """
    cores = os.cpu_count() or 1
    if cores < 2:
        pytest.skip(
            f"the GIL overlap measurement needs at least two cores and this "
            f"host reports {cores}; RV42 is unmeasured here"
        )
    # The serial arm, so the measurement is about the GIL and not about
    # whether rayon has saturated the machine, and a large N so that one call
    # is long.
    raw, b = big_batch(BIG_N, parallel=False)
    for _ in range(2):
        step(raw, b)

    def batch_only(rounds: int) -> float:
        start = time.perf_counter()
        for _ in range(rounds):
            step(raw, b)
        return time.perf_counter() - start

    # Best of three. The quantity is a ratio of work done in two different
    # wall-clock windows on a machine whose other load this test does not
    # control, so a single attempt measures the host as much as the binding.
    # A held GIL cannot produce a good attempt, which is what makes taking
    # the best of several honest rather than convenient.
    attempts = []
    for _ in range(3):
        t_batch = batch_only(ROUNDS)
        n_alone = _spin(t_batch)
        start = time.perf_counter()
        counter: list[int] = []
        worker = threading.Thread(
            target=_busy, args=(start + t_batch, counter), daemon=True
        )
        worker.start()
        batch_only(ROUNDS)
        worker.join()
        t_together = time.perf_counter() - start
        attempts.append(
            (counter[0] / n_alone, t_together / (2 * t_batch), t_batch, t_together)
        )

    for progress, ratio, t_batch, t_together in attempts:
        print(
            f"[perf] GIL, serial arm, N={BIG_N}: batch {t_batch * 1e3:.1f} ms, "
            f"python {t_batch * 1e3:.1f} ms, together {t_together * 1e3:.1f} ms "
            f"-> progress {progress:.3f}, ratio {ratio:.3f}"
        )
    progress, ratio, _t_batch, _t_together = max(attempts, key=lambda a: a[0])
    print(
        f"[perf] GIL, serial arm: best progress {progress:.3f} "
        f"(bound {PROGRESS_BOUND}), its ratio {ratio:.3f} "
        f"(bound {OVERLAP_BOUND})"
    )
    assert progress > PROGRESS_BOUND, (
        f"the Python thread completed only {progress:.3f} of its solo work "
        f"while Rust stepped, in the best of {len(attempts)} attempts; the "
        "GIL is being held across step_all (RV42)"
    )
    assert ratio < OVERLAP_BOUND, (
        f"the two tasks took {ratio:.3f} of their combined solo time; bound "
        f"{OVERLAP_BOUND}. This is the weak half of the measurement -- see "
        "OVERLAP_BOUND -- so a failure here with a healthy `progress` above "
        "means the host is loaded, not that the GIL is held."
    )


def test_the_parallel_arm_also_overlaps():
    """The same measurement with rayon running, reported rather than pinned.

    With every core busy the Python thread competes for CPU, so the numbers
    are a property of the host as much as of the binding. The progress figure
    is still held to a bound — a held GIL would put it at zero whatever the
    host — and the time ratio is printed for the handoff.
    """
    cores = os.cpu_count() or 1
    if cores < 2:
        pytest.skip(f"needs at least two cores; this host reports {cores}")
    raw, b = big_batch(512, parallel=True)
    for _ in range(3):
        step(raw, b)

    def batch_only(rounds: int) -> float:
        start = time.perf_counter()
        for _ in range(rounds):
            step(raw, b)
        return time.perf_counter() - start

    rounds = 20
    t_batch = batch_only(rounds)
    n_alone = _spin(t_batch)
    start = time.perf_counter()
    counter: list[int] = []
    worker = threading.Thread(
        target=_busy, args=(start + t_batch, counter), daemon=True
    )
    worker.start()
    batch_only(rounds)
    worker.join()
    t_together = time.perf_counter() - start
    ratio = t_together / (2 * t_batch)
    progress = counter[0] / n_alone
    print(
        f"[perf] GIL, rayon arm on {cores} cores: progress {progress:.3f}, "
        f"ratio {ratio:.3f} (reported, not pinned)"
    )
    assert progress > 0.25, (
        f"the Python thread completed only {progress:.3f} of its solo work "
        "with rayon running; on a multi-core host that is a held GIL rather "
        "than contention"
    )


# ---------------------------------------------------------------------------
# 3. Bounded memory
# ---------------------------------------------------------------------------


def test_the_low_level_batch_loop_retains_nothing_across_a_thousand_steps():
    """RV46, measured in blocks and in bytes, and separately from peak."""
    raw, b = big_batch(32)
    for _ in range(10):
        step(raw, b)
    gc.collect()

    before = sys.getallocatedblocks()
    tracemalloc.start()
    start_size, _ = tracemalloc.get_traced_memory()
    for _ in range(1000):
        step(raw, b)
    end_size, peak = tracemalloc.get_traced_memory()
    tracemalloc.stop()
    gc.collect()
    after = sys.getallocatedblocks()

    retained_blocks = after - before
    retained_bytes = end_size - start_size
    peak_bytes = peak - start_size
    print(
        f"[perf] 1 000 low-level steps: retained {retained_blocks} blocks / "
        f"{retained_bytes} bytes (bound {RETAINED_BLOCKS} blocks), peak "
        f"temporary {peak_bytes} bytes (bound "
        f"{PEAK_BYTES_PER_STEP} per step)"
    )
    assert retained_blocks < RETAINED_BLOCKS, (
        f"{retained_blocks} Python blocks retained over 1 000 steps; that is "
        "per-step churn, which is RV46"
    )
    assert peak_bytes < PEAK_BYTES_PER_STEP, (
        f"peak temporary allocation {peak_bytes} bytes over the run; the "
        "low-level loop should allocate nothing at all"
    )


def test_the_gymnasium_adapter_costs_are_reported_separately():
    """The tracked debt, as a number rather than as a sentence.

    Gymnasium requires a tuple and an ``info`` dictionary per step. They are
    built above the reusable buffers and they are the right trade; what is
    not acceptable is *not knowing what they cost*, so this measures them and
    holds only the retained figure.
    """
    env = sailgym.SailgymVectorEnv(SCENARIO, num_envs=32, seeds=SEED, cadence=CADENCE)
    env.reset(seed=SEED)
    actions = np.zeros((env.num_envs, env.single_action_space.shape[0]))
    for _ in range(10):
        env.step(actions)
    gc.collect()

    before = sys.getallocatedblocks()
    tracemalloc.start()
    start_size, _ = tracemalloc.get_traced_memory()
    for _ in range(1000):
        env.step(actions)
    end_size, peak = tracemalloc.get_traced_memory()
    tracemalloc.stop()
    gc.collect()
    after = sys.getallocatedblocks()

    retained_blocks = after - before
    per_step_peak = peak - start_size
    print(
        f"[perf] 1 000 Gymnasium VectorEnv steps: retained "
        f"{retained_blocks} blocks / {end_size - start_size} bytes, peak "
        f"temporary {per_step_peak} bytes -- the tuple and the info dict, "
        "above the reusable buffers"
    )
    assert retained_blocks < RETAINED_BLOCKS, (
        f"{retained_blocks} blocks retained over 1 000 adapter steps: the "
        "tuples are being kept, not merely created"
    )
    env.close()
