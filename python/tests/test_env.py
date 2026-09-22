"""One episode, through Gymnasium's own checker and F17.3's seeding rule.

v2 section 07 task 7.3; F17.3, F17.4, F19.2, RV43, RV44.

Four things are asserted, and the third is the one that matters:

1. ``gymnasium.utils.env_checker.check_env`` passes.
2. ``reset(seed=k)`` twice gives identical observations.
3. **Two different ``np_random`` draws with the same ``k`` give identical
   trajectories.** This is the only test in the repository that would notice
   a second generator leaking into the simulation (RV43). If it is ever
   removed or weakened, F17.3 stops being enforced by anything.
4. ``terminated`` and ``truncated`` are never both true.

The scenarios and the box are chosen so that each assertion is **not
vacuous**, and each one says so: ``gybe`` is the only shipped scenario whose
wind field is drawn from the seed (``mode: gust``), so it is the one that can
show a seed changing a trajectory at all; ``beam_reach_capsize`` with the
sheet hauled capsizes in about ten seconds, which is the fastest honest way
to reach a ``Terminated(Capsized)``.
"""

from __future__ import annotations

import numpy as np
import pytest
from gymnasium.utils.env_checker import check_env

import sailgym
import sailgym_core

SEEDED_SCENARIO = "gybe"
"""The one shipped scenario with a ``gust`` wind field, and therefore the one
whose trajectory depends on the episode's ``u64``."""

CAPSIZING_SCENARIO = "beam_reach_capsize"
CADENCE = 10
SEED = 20260922

BOX = (-15.0, -15.0, 15.0, 15.0)
"""A sailing area, in metres, supplied by this experiment.

``sailgym-env``'s default is ``Bounds::Unbounded`` — it invents no distance —
so a test that wants a termination states one.
"""

BUDGET = 2000
"""A step budget, in physics steps. The only thing that produces
``Truncated`` (F19.2)."""


def make(scenario: str = SEEDED_SCENARIO, **kwargs):
    kwargs.setdefault("cadence", CADENCE)
    return sailgym.SailgymEnv(scenario, **kwargs)


def script(decision: int) -> np.ndarray:
    """A deterministic action, keyed off the **episode's own** decision index.

    Never off a global call counter: under ``NextStep`` one call per boundary
    is spent on a reset, so a globally indexed script would feed two
    conventions different actions and 7.4's identity would fail for a reason
    that has nothing to do with the convention (section 06 §3.1).
    """
    return np.array([0.3 * np.sin(decision * 0.07), -1.0, -1.0], dtype=np.float64)


def roll(env, calls: int, *, draws: int = 0):
    """Run ``calls`` decisions, optionally burning ``draws`` from np_random."""
    obs, _ = env.reset(seed=SEED)
    if draws:
        env.np_random.random(draws)
    out = [obs.copy()]
    for _ in range(calls):
        obs, _reward, terminated, truncated, _info = env.step(
            script(env.episode_steps // CADENCE)
        )
        out.append(obs.copy())
        if terminated or truncated:
            break
    return out


def test_check_env_passes():
    """Section acceptance 3, with nothing skipped.

    ``skip_render_check`` is **not** passed: this environment declares
    ``render_modes: []`` and ``render_mode = None`` honestly, so the render
    check has nothing to complain about and there is no reason to turn it
    off. The two warnings it does emit — that some observation bounds are
    infinite — are correct and are the point: ``imu.roll_rate`` has no
    declared bound, and inventing one to quiet a warning would be inventing
    a number (F17.1).
    """
    env = make()
    check_env(env.unwrapped)
    env.close()


def test_the_env_is_not_wrapped_in_a_time_limit():
    """RV44. The budget is Rust's, and there is no Python wrapper."""
    env = make(max_steps=BUDGET)
    assert env.unwrapped is env
    assert env.spec_core.max_steps == BUDGET
    env.close()


def test_reset_with_the_same_seed_twice_gives_identical_observations():
    env = make()
    first, _ = env.reset(seed=SEED)
    second, _ = env.reset(seed=SEED)
    np.testing.assert_array_equal(first, second)
    assert first.dtype == np.float32
    env.close()


def test_a_different_seed_is_a_different_trajectory():
    """Not vacuity-proofing for its own sake: without this, the test above
    would pass on an environment that ignored its seed entirely."""
    a = roll(make(max_steps=BUDGET), 200)
    env = make(max_steps=BUDGET)
    obs, _ = env.reset(seed=SEED + 1)
    b = [obs.copy()]
    for _ in range(200):
        obs, _r, terminated, truncated, _i = env.step(
            script(env.episode_steps // CADENCE)
        )
        b.append(obs.copy())
        if terminated or truncated:
            break
    assert a[-1].tobytes() != b[-1].tobytes()
    env.close()


def test_np_random_draws_do_not_move_the_trajectory():
    """RV43, and the assertion F17.3 exists to be checked by.

    Two runs from the same ``k``, one of which draws 1 000 numbers from
    Gymnasium's ``np_random`` before stepping. If any part of the simulation
    ever read that generator the two trajectories would diverge; they are
    compared **bit for bit**, so a divergence of one ULP is a failure. The
    scenario is the seeded one, so there is something for a leak to disturb.
    """
    quiet = roll(make(max_steps=BUDGET, bounds=BOX), 400)
    noisy = roll(make(max_steps=BUDGET, bounds=BOX), 400, draws=1000)
    assert len(quiet) == len(noisy) > 1
    for i, (a, b) in enumerate(zip(quiet, noisy, strict=True)):
        assert a.tobytes() == b.tobytes(), f"observation {i} moved with np_random"
    # And the draws really happened: the generator's state advanced.
    env = make()
    env.reset(seed=SEED)
    before = env.np_random.bit_generator.state
    env.np_random.random(1000)
    assert env.np_random.bit_generator.state != before
    env.close()


def run_to_the_end(env, calls: int = 4000):
    """Step until the episode ends, asserting the two flags all the way."""
    env.reset(seed=SEED)
    for _ in range(calls):
        _obs, _r, terminated, truncated, info = env.step(
            script(env.episode_steps // CADENCE)
        )
        assert not (terminated and truncated), "F19.2's Outcome is an enum"
        if terminated or truncated:
            return terminated, truncated, info
    raise AssertionError("the episode never ended; this assertion was vacuous")


def test_terminated_and_truncated_are_never_both_true():
    """Both flags observed, in the two arms that produce them."""
    bounded = make(bounds=BOX)
    terminated, truncated, info = run_to_the_end(bounded)
    assert terminated and not truncated
    # `Outcome::as_str` names the *reason* for a termination, so the two
    # fields agree rather than repeating "terminated" twice.
    assert info["outcome"] == info["termination_reason"]
    assert info["outcome"] in sailgym_core.termination_reasons()
    bounded.close()

    budgeted = make(max_steps=BUDGET)
    terminated, truncated, info = run_to_the_end(budgeted)
    assert truncated and not terminated
    assert info["outcome"] == "truncated"
    assert info["termination_reason"] is None
    assert info["episode_steps"] == BUDGET
    budgeted.close()


def test_each_terminal_reason_arrives_from_rust():
    """The reason is Rust's, and Python neither invents nor renames one."""
    bounded = make(bounds=BOX)
    _terminated, _truncated, info = run_to_the_end(bounded)
    assert info["termination_reason"] == "out_of_bounds"
    assert info["termination_reason"] in sailgym_core.termination_reasons()
    bounded.close()

    capsizing = make(CAPSIZING_SCENARIO)
    _terminated, _truncated, info = run_to_the_end(capsizing)
    assert info["termination_reason"] == "capsized"
    assert info["termination_reason"] in sailgym_core.termination_reasons()
    capsizing.close()


def test_a_seed_that_is_not_a_u64_is_refused():
    """F17.3 maps the seed straight onto the Rust ``u64``; it does not mask.

    The check runs **before** Gymnasium's own, so that the message names the
    contract that was broken rather than the narrower one that noticed.
    """
    env = make()
    with pytest.raises(ValueError, match="not a u64"):
        env.reset(seed=-1)
    with pytest.raises(ValueError, match="not a u64"):
        env.reset(seed=2**64)
    env.reset(seed=2**64 - 1)
    assert env.root_seed == 2**64 - 1
    env.close()


def test_the_reward_is_zero_unless_an_experiment_supplies_one():
    """No reward function ships (section 06 §8), and this is where yours
    goes."""
    bare = make()
    bare.reset(seed=SEED)
    _obs, reward, _te, _tr, _info = bare.step(np.zeros(3))
    assert reward == 0.0
    bare.close()

    mine = make(reward_fn=lambda obs: float(obs[0]) - float(obs[1]))
    mine.reset(seed=SEED)
    for _ in range(20):
        _obs, reward, _te, _tr, _info = mine.step(script(mine.episode_steps // CADENCE))
    assert isinstance(reward, float)
    assert reward != 0.0, "the supplied reward never produced a non-zero value"
    mine.close()


def test_the_observation_is_always_inside_the_declared_space():
    """The bounds in the space are the sensors' own declared bounds (F16.4).

    If a sensor ever emitted outside them the space would be a lie, and
    ``check_env`` only samples one step.
    """
    env = make(bounds=BOX, max_steps=BUDGET)
    obs, _ = env.reset(seed=SEED)
    assert env.observation_space.contains(obs)
    steps = 0
    for _ in range(600):
        obs, _r, terminated, truncated, _i = env.step(
            script(env.episode_steps // CADENCE)
        )
        steps += 1
        assert env.observation_space.contains(obs), f"outside the space at {steps}"
        if terminated or truncated:
            break
    assert steps > 100, "too few steps for this to have checked anything"
    env.close()


def test_the_action_is_refused_when_it_leaves_the_funnels_box():
    """F14.5's bound is checked in Rust, in the one place `apply` is called.

    The message comes back verbatim, so a caller sees which scalar and which
    adapter — not a Python restatement of the rule.
    """
    env = make()
    env.reset(seed=SEED)
    with pytest.raises(ValueError, match=r"outside \[-1, 1\]"):
        env.step(np.array([2.0, 0.0, 0.0]))
    with pytest.raises(ValueError):
        env.step(np.array([0.0, 0.0]))
    env.close()
