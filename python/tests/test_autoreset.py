"""The returns identity, through the binding.

v2 section 07 task 7.4; F16.5, F17.4, F19.1, RV33, RV44.

Section 06 pinned the autoreset convention in Rust and asserted the returns
identity there, at a bound of **zero ULP**. This file asserts the *same*
identity through the binding, over the same two families of episodes — one
that always terminates and one that always truncates — on a fixed action
sequence from a fixed seed. *"Two implementations of a convention is how a
convention stops being one."* It additionally asserts that the single-env and
vector paths agree about episode boundaries step for step.

**Why a Python-side reward is the right instrument.** ``sailgym-env`` ships
``ZeroReward`` and nothing else, on purpose: a reward is an experiment
parameter. A stream of zeros would make the identity vacuous, so this test
supplies its own — which is exactly where section 06 put the reward its own
``tests/returns.rs`` uses. It is a function of the observation the decision
period **ended** on, which is the one quantity the two conventions agree about
by construction: under ``NextStep`` the returned observation already is it,
and under ``SameStep`` it arrives separately because the returned one is the
reset.

**Why ``gybe``.** It is the only shipped scenario whose wind field is drawn
from the seed (``mode: gust``), so the episodes in a chain are genuinely
different episodes. On a uniform-wind scenario every episode of a chain is the
same episode and the identity would hold over four copies of one number.
"""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest

import sailgym
import sailgym_core

SCENARIO = "gybe"
SEED = 20260922
CADENCE = 10
GAMMAS = (0.9, 0.99)

# Two families, as section 06 used: one that always terminates and one that
# always truncates. Both are experiment configuration -- `sailgym-env`'s own
# defaults are `Bounds::Unbounded` and `max_steps: None`, so it invents
# neither the distance nor the budget.
TERMINATING = {"bounds": (-15.0, -15.0, 15.0, 15.0)}
TRUNCATING = {"max_steps": 2000}
CALLS = {"terminating": 1400, "truncating": 900}


def script(decision: int) -> np.ndarray:
    """The same script ``test_env.py`` uses, keyed off the episode's own
    decision counter (section 06 §3.1)."""
    return np.array([0.3 * np.sin(decision * 0.07), -1.0, -1.0], dtype=np.float64)


def reward_of(end_obs: np.ndarray) -> float:
    """The experiment's reward: a fixed difference of two observation
    columns, evaluated on the observation the period ended on."""
    return float(end_obs[0]) - float(end_obs[1])


def batch_reward(end_obs: np.ndarray) -> np.ndarray:
    """The same reward, over a batch's rows."""
    return end_obs[:, 0].astype(np.float64) - end_obs[:, 1].astype(np.float64)


def spec(mode: str, **family):
    return sailgym_core.Spec(SCENARIO, cadence=CADENCE, autoreset=mode, **family)


def run_single(mode: str, calls: int, **family):
    """One env, ``calls`` decisions, recording what a learner would record."""
    env = sailgym.SailgymEnv(spec(mode, **family), seed=SEED, reward_fn=reward_of)
    obs, _ = env.reset(seed=SEED)
    records = []
    for _ in range(calls):
        obs, reward, terminated, truncated, info = env.step(
            script(env.episode_steps // CADENCE)
        )
        records.append(
            (obs.copy(), reward, terminated, truncated, info["physics_steps"])
        )
    env.close()
    return records


def run_vector(mode: str, seeds, calls: int, *, parallel: bool = False, **family):
    env = sailgym.SailgymVectorEnv(
        spec(mode, **family),
        seeds=list(seeds),
        parallel=parallel,
        reward_fn=batch_reward,
    )
    obs, _ = env.reset(seed=list(seeds))
    period = env.spec_core.cadence_period_steps
    rows = []
    for _ in range(calls):
        actions = np.stack([script(s // period) for s in env.episode_steps])
        obs, rewards, terminations, truncations, _ = env.step(actions)
        rows.append(
            (obs.copy(), rewards.copy(), terminations.copy(), truncations.copy())
        )
    assert env.is_parallel is parallel
    env.close()
    return rows


def split_into_episodes(records):
    """Episode-by-episode reward lists, for either convention.

    One rule, and it reads the *data* rather than the mode: a record that
    executed no physics is the neutral one ``NextStep`` inserts between two
    episodes, and it belongs to neither. Anything else extends the current
    episode and a flag ends it. The assertion inside is RV33's — a neutral
    record that carried a reward or a flag is the off-by-one itself.
    """
    episodes: list[list[float]] = []
    current: list[float] = []
    for _obs, reward, terminated, truncated, steps in records:
        if steps == 0:
            assert reward == 0.0 and not terminated and not truncated, (
                "a reset record carried a reward or a flag; that is RV33's "
                f"off-by-one: {reward!r}, {terminated!r}, {truncated!r}"
            )
            continue
        current.append(reward)
        if terminated or truncated:
            episodes.append(current)
            current = []
    return episodes


def discounted(rewards, gamma: float) -> float:
    total = 0.0
    for i, r in enumerate(rewards):
        total += (gamma**i) * r
    return total


@pytest.mark.parametrize("family", ["terminating", "truncating"])
def test_the_two_autoreset_conventions_give_the_same_discounted_returns(family):
    """Criterion 4, at the bound section 06 established: **zero ULP**.

    Compared with ``float.hex()`` rather than ``pytest.approx``. The two
    conventions differ in exactly one thing — whether the stream carries a
    neutral record between two episodes — and the episodes themselves are the
    same episodes, because the chain shares a root seed, the same
    ``STREAM_SCENARIO`` draws and a script indexed by each episode's own
    decision counter. Anything looser than bit identity would hide which of
    those sentences had stopped being true.
    """
    kwargs = TERMINATING if family == "terminating" else TRUNCATING
    calls = CALLS[family]
    next_step = split_into_episodes(run_single("NextStep", calls, **kwargs))
    same_step = split_into_episodes(run_single("SameStep", calls, **kwargs))
    common = min(len(next_step), len(same_step))
    assert common >= 4, f"only {common} complete episodes; the test is vacuous"
    assert [len(e) for e in next_step[:common]] == [len(e) for e in same_step[:common]]

    for gamma in GAMMAS:
        a = [discounted(e, gamma) for e in next_step[:common]]
        b = [discounted(e, gamma) for e in same_step[:common]]
        print(f"returns [{family}] gamma={gamma}: {a}")
        assert [x.hex() for x in a] == [y.hex() for y in b], (
            f"the two conventions disagree at gamma={gamma}: {a} against {b}"
        )
        assert any(abs(x) > 0.0 for x in a), "every return was zero"
        # The episodes are genuinely different episodes, which is what makes
        # the identity an identity over four values rather than over one.
        assert len(set(a)) == len(a), (
            "every episode of the chain produced the same return; the seed "
            "chain is not reaching the wind field"
        )
    print(f"returns [{family}]: max |delta| between the two conventions = 0.0 (0 ULP)")


def test_a_one_step_shift_would_be_visible():
    """The bound is zero, so it is tight — but tight against *what*?

    This reads a ``NextStep`` stream as though it were a ``SameStep`` one,
    which is exactly RV33's off-by-one, and measures how far the returns move.
    Section 06 made the same measurement on the Rust side; this is the Python
    one, and it is what says the assertion above is not comparing two copies
    of zero.
    """
    records = run_single("NextStep", CALLS["terminating"], **TERMINATING)
    clean = split_into_episodes(records)
    # The wrong reading: keep the neutral records, so every episode after the
    # first acquires one at its front and slides a discount power later.
    shifted: list[list[float]] = []
    current: list[float] = []
    for _obs, reward, terminated, truncated, _steps in records:
        current.append(reward)
        if terminated or truncated:
            shifted.append(current)
            current = []
    common = min(len(clean), len(shifted))
    for gamma in GAMMAS:
        worst = max(
            abs(discounted(clean[i], gamma) - discounted(shifted[i], gamma))
            for i in range(1, common)
        )
        print(f"returns gamma={gamma}: a one-step shift moves a return by {worst:.6f}")
        assert worst > 1e-3, (
            "a one-step shift moved the returns by less than 1e-3; this "
            "reward is too small for the identity above to mean anything"
        )


@pytest.mark.parametrize("mode", ["NextStep", "SameStep"])
def test_one_env_and_a_vector_of_one_agree_step_for_step(mode: str):
    """The agreement the PRD asks for, in all four returned quantities."""
    calls = 400
    single = run_single(mode, calls, **TERMINATING)
    batched = run_vector(mode, [SEED], calls, **TERMINATING)
    assert len(single) == len(batched) == calls
    boundaries = 0
    for i, ((obs, reward, term, trunc, _steps), (o, r, te, tr)) in enumerate(
        zip(single, batched, strict=True)
    ):
        assert obs.tobytes() == o[0].tobytes(), f"observation {i}"
        assert float(reward).hex() == float(r[0]).hex(), f"reward {i}"
        assert bool(term) == bool(te[0]), f"terminated {i}"
        assert bool(trunc) == bool(tr[0]), f"truncated {i}"
        boundaries += int(bool(term) or bool(trunc))
    assert boundaries >= 1, (
        "no episode boundary in the compared window; the agreement about "
        "boundaries was vacuous"
    )


def test_eight_identical_seeds_produce_eight_identical_rows():
    """N = 8, one seed, eight equal boats — equal, and not coupled."""
    calls = 300
    rows = run_vector("NextStep", [SEED] * 8, calls, **TERMINATING)
    for i, (obs, rewards, term, trunc) in enumerate(rows):
        for slot in range(1, 8):
            assert obs[slot].tobytes() == obs[0].tobytes(), f"step {i} slot {slot}"
            assert float(rewards[slot]).hex() == float(rewards[0]).hex()
            assert bool(term[slot]) == bool(term[0])
            assert bool(trunc[slot]) == bool(trunc[0])
    # Not vacuous: different seeds give different rows, so the agreement above
    # is a property of the seeds and not of a buffer that was never written.
    mixed = run_vector("NextStep", [SEED + i for i in range(8)], calls, **TERMINATING)
    obs = mixed[-1][0]
    assert obs[1].tobytes() != obs[0].tobytes()


def test_the_parallel_and_serial_arms_agree_through_the_binding():
    """F16.5, re-asserted where a caller can see it.

    Section 06 asserts this in Rust at N ∈ {1, 8, 64, 512} × six thread
    counts. Here it is asserted once through `numpy` buffers, because a
    binding that transposed a row or shared a buffer between slots would break
    it and no Rust test would notice.
    """
    seeds = [SEED + i for i in range(8)]
    calls = 300
    serial = run_vector("NextStep", seeds, calls, parallel=False, **TERMINATING)[-1]
    parallel = run_vector("NextStep", seeds, calls, parallel=True, **TERMINATING)[-1]
    assert serial[0].tobytes() == parallel[0].tobytes()
    assert serial[1].tobytes() == parallel[1].tobytes()
    assert serial[0][1].tobytes() != serial[0][0].tobytes()


def test_the_pinned_gymnasium_convention_was_read_and_not_assumed():
    """F19.1, and the reason section 06 could not do this.

    ``uv.lock`` now pins Gymnasium, so the convention is read out of the
    release: its enum's spellings must be the three Rust spells, and its
    declared default must be one this build implements.
    """
    import gymnasium
    from gymnasium.vector import AutoresetMode

    from sailgym.vector import gymnasium_autoreset_modes, pinned_autoreset_mode

    assert sorted(gymnasium_autoreset_modes()) == sorted(sailgym_core.autoreset_modes())
    mode = pinned_autoreset_mode()
    assert mode in sailgym_core.autoreset_modes()
    print(
        f"gymnasium {gymnasium.__version__} declares autoreset mode {mode!r}; "
        f"sailgym-env's own default is "
        f"{sailgym_core.default_autoreset_mode()!r}"
    )

    env = sailgym.SailgymVectorEnv(SCENARIO, num_envs=2, seeds=SEED, cadence=CADENCE)
    assert env.metadata["autoreset_mode"] == AutoresetMode(mode)
    assert env.autoreset_mode == mode
    env.close()

    # And it is a *selection*, not an inheritance: a spec that says otherwise
    # is honoured.
    other = "SameStep" if mode != "SameStep" else "NextStep"
    picked = sailgym.SailgymVectorEnv(
        spec(other, **TERMINATING), seeds=[SEED], parallel=False
    )
    assert picked.autoreset_mode == other
    assert picked.metadata["autoreset_mode"] == AutoresetMode(other)
    picked.close()


def test_the_vector_env_exposes_the_same_identity_as_the_single_one():
    """Task 7.2's attribute requirement, on the batched path.

    The two paths share one :class:`sailgym_core.Spec`, so a divergence here
    would mean the binding had built a second layout somewhere — which is
    exactly what the shared `Spec` exists to prevent.
    """
    s = spec("NextStep", **TERMINATING)
    env = sailgym.SailgymEnv(s, seed=SEED)
    vec = sailgym.SailgymVectorEnv(s, seeds=[SEED, SEED + 1])
    assert env.obs_digest == s.obs_digest == vec.obs_digest
    assert env.obs_layout_json == s.obs_layout_json == vec.obs_layout_json
    assert env.observation_contract == vec.observation_contract
    assert vec.single_observation_space == env.observation_space
    assert vec.single_action_space == env.action_space
    assert vec.observation_space.shape == (2, s.obs_len)
    env.close()
    vec.close()


def test_no_time_limit_wrapper_appears_anywhere_under_python():
    """RV44, as a grep rather than as a promise.

    F17.4: the vectorised path cannot use Python wrappers, so a
    wrapper-supplied truncation would make the two paths disagree about
    episode boundaries. The needles are the three ways the wrapper is actually
    *used* rather than the bare word, and they are assembled at run time so
    that this file is scanned like every other and exempts nothing.
    """
    root = Path(__file__).resolve().parents[1]
    # Assembled rather than written out, so that this file is subject to its
    # own grep. A needle tuple spelled literally would make the test the one
    # place the rule does not apply, which is how an exemption starts.
    wrapper = "Time" + "Limit"
    needles = (f"{wrapper}(", f"import {wrapper}", f"wrappers.{wrapper}")
    offenders = []
    scanned = 0
    for path in sorted(root.rglob("*.py")):
        scanned += 1
        for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            for needle in needles:
                if needle in line:
                    offenders.append(f"{path.relative_to(root)}:{n}: {line.strip()}")
    assert scanned > 5, f"only {scanned} files scanned; this grep is vacuous"
    assert not offenders, (
        "a TimeLimit wrapper under python/ would make the single-env and "
        "vector paths disagree about episode boundaries (F17.4, RV44):\n  "
        + "\n  ".join(offenders)
    )
