"""Task, action, feature, reward and mask contract (plan §3.2–3.4, E0)."""

from __future__ import annotations

import math

import numpy as np
import pytest
from sailing import (
    FEATURE_NAMES,
    FINISHED,
    NOMINAL,
    OUTCOME_CODE,
    RS,
    RS_NAMES,
    RS_UNITS,
    RUNNING,
    TERMINATED,
    TRUNCATED,
    ActionAdapter,
    Task,
    TaskEnv,
    TransitionReward,
    features,
    masks,
    scenario_json,
)

PARAMS = {
    "time_scale_s": 30.0,
    "distance_scale_m": 30.0,
    "finish_bonus": 10.0,
    "failure_penalty": 10.0,
    "gamma": 0.999,
}


@pytest.fixture(scope="module")
def task_r() -> Task:
    return Task.load("R")


def rs_at(x: float, y: float) -> np.ndarray:
    s = np.zeros(len(RS_NAMES))
    s[RS["x"]], s[RS["y"]] = x, y
    return s


def test_budgets_are_whole_decisions(task_r: Task) -> None:
    u = Task.load("U")
    assert task_r.max_steps * task_r.dt == pytest.approx(45.0)
    assert u.max_steps * u.dt == pytest.approx(180.0)
    for t in (task_r, u):
        assert t.max_steps % t.cadence == 0
        assert t.decision_dt == pytest.approx(0.05)


def test_research_state_schema() -> None:
    state = [
        "x",
        "y",
        "psi",
        "phi",
        "u",
        "v",
        "r",
        "p",
        "beta",
        "beta_dot",
        "delta_r",
        "l_sheet",
        "t",
    ]
    assert RS_NAMES[:13] == state
    assert len(RS_UNITS) == len(RS_NAMES)
    assert "capsize_pending" in RS_NAMES


def test_adapter_forces_release_off_for_every_shape() -> None:
    ad = ActionAdapter(3)
    assert ad(np.array([0.2, -0.3])).tolist() == [0.2, -0.3, -1.0]
    assert ad(np.array([0.2, -0.3, 1.0])).tolist() == [0.2, -0.3, -1.0]
    assert ad(np.array([3.0, -3.0])).tolist() == [1.0, -1.0, -1.0]
    with pytest.raises(ValueError):
        ad(np.zeros(4))


def test_features_wrap_angles_and_exclude_time(task_r: Task) -> None:
    s = rs_at(10.0, -2.0)
    s[RS["psi"]] = math.pi
    s[RS["phi"]] = 2.5  # unwrapped heel survives
    s[RS["t"]] = 123.0
    f = features(s, task_r.target)
    assert len(f) == len(FEATURE_NAMES)
    named = dict(zip(FEATURE_NAMES, f, strict=True))
    assert named["dx_target"] == pytest.approx(20.0)
    assert named["dy_target"] == pytest.approx(2.0)
    assert named["cos_psi"] == pytest.approx(-1.0)
    assert named["phi"] == 2.5
    assert 123.0 not in f
    batch = features(np.stack([s, s]), task_r.target)
    assert batch.shape == (2, len(FEATURE_NAMES))


# -- reward: hand-calculated ---------------------------------------------------


def test_reward_running_hand_calculated(task_r: Task) -> None:
    rf = TransitionReward(task_r, PARAMS)
    prev, nxt = rs_at(0.0, 0.0), rs_at(3.0, 4.0)  # d: 30 -> hypot(27, 4)
    d1 = math.hypot(27.0, 4.0)
    expected = -0.05 / 30.0 + 0.999 * (-d1 / 30.0) - (-30.0 / 30.0)
    assert rf(prev, nxt, RUNNING, 10) == pytest.approx(expected, abs=1e-12)


def test_reward_finished_has_zero_terminal_potential(task_r: Task) -> None:
    rf = TransitionReward(task_r, PARAMS)
    prev, nxt = rs_at(29.0, 0.0), rs_at(30.2, 0.0)
    # 4 steps executed in the shortened terminal decision.
    expected = -4 * task_r.dt / 30.0 + 10.0 + 0.0 - (-1.0 / 30.0)
    assert rf(prev, nxt, FINISHED, 4) == pytest.approx(expected, abs=1e-12)


def test_reward_failure_has_zero_terminal_potential(task_r: Task) -> None:
    rf = TransitionReward(task_r, PARAMS)
    prev, nxt = rs_at(0.0, 0.0), rs_at(0.0, 1.0)
    expected = -7 * task_r.dt / 30.0 - 10.0 + 0.0 - (-30.0 / 30.0)
    assert rf(prev, nxt, TERMINATED, 7) == pytest.approx(expected, abs=1e-12)


def test_reward_truncation_keeps_potential_and_no_bonus(task_r: Task) -> None:
    rf = TransitionReward(task_r, PARAMS)
    prev, nxt = rs_at(0.0, 0.0), rs_at(6.0, 0.0)
    expected = -10 * task_r.dt / 30.0 + 0.999 * (-24.0 / 30.0) + 1.0
    assert rf(prev, nxt, TRUNCATED, 10) == pytest.approx(expected, abs=1e-12)


def test_reward_batch_equals_scalar(task_r: Task) -> None:
    rf = TransitionReward(task_r)
    rng = np.random.default_rng(0)
    prev = np.zeros((40, len(RS_NAMES)))
    nxt = np.zeros((40, len(RS_NAMES)))
    prev[:, :2] = rng.uniform(-50, 50, (40, 2))
    nxt[:, :2] = rng.uniform(-50, 50, (40, 2))
    names = [RUNNING, FINISHED, TERMINATED, TRUNCATED] * 10
    codes = np.array([OUTCOME_CODE[n] for n in names])
    steps = rng.integers(1, 11, 40)
    batch = rf.batch(prev, nxt, codes, steps)
    scalar = [rf(prev[i], nxt[i], names[i], int(steps[i])) for i in range(40)]
    np.testing.assert_array_equal(batch, np.array(scalar))


def test_masks() -> None:
    assert masks(RUNNING) == (1.0, 1.0)
    assert masks(TRUNCATED) == (1.0, 0.0)
    assert masks(FINISHED) == (0.0, 0.0)
    assert masks(TERMINATED) == (0.0, 0.0)
    with pytest.raises(ValueError):
        masks("capsized")


# -- the real boundary ---------------------------------------------------------


def test_scenario_document_changes_only_the_condition() -> None:
    import json

    from sailing import Condition, template_scenario

    base = template_scenario()
    doc = json.loads(scenario_json(Condition("x", 4.75, -3.0, 100.0)))
    assert doc["wind"]["speed"] == 4.75
    assert doc["wind"]["bearing_deg"] == -3.0
    assert doc["initial_state"]["heading_deg"] == 100.0
    for k in ("parameter_overrides", "seed"):
        assert doc[k] == base[k]
    assert {
        k: v for k, v in doc["wind"].items() if k not in ("speed", "bearing_deg")
    } == {k: v for k, v in base["wind"].items() if k not in ("speed", "bearing_deg")}


def test_truncation_final_state_is_the_real_final_state() -> None:
    """Budget exhaustion reports the state the period ended on, not a reset."""
    t = Task.load("R")
    t.max_steps = 20 * t.cadence  # 1 s, for the test only
    env = TaskEnv(t, NOMINAL)
    env.reset()
    recs = [env.step(np.array([0.0, -1.0])) for _ in range(20)]
    last = recs[-1]
    assert last.outcome == TRUNCATED and last.bootstrap == 1.0 and last.cont == 0.0
    assert last.next_rs[RS["t"]] == pytest.approx(1.0)
    np.testing.assert_array_equal(last.next_rs, env.research_state())
    for a, b in zip(recs, recs[1:], strict=False):
        np.testing.assert_array_equal(a.next_rs, b.rs)
    with pytest.raises(ValueError):
        env.step(np.zeros(2))  # Disabled: no implicit reset


def test_termination_is_normalised_with_reason() -> None:
    """A capsize reports outcome `terminated` and reason `capsized`."""
    from sailing import Condition

    t = Task.load("R")
    env = TaskEnv(t, Condition("gale", 12.0, 0.0, 90.0))
    env.reset()
    rec = None
    while env.outcome == RUNNING:
        rec = env.step(np.array([0.0, -1.0]))
    assert rec is not None
    assert rec.outcome == TERMINATED and env.outcome == TERMINATED
    assert rec.reason == "capsized"
    assert (rec.bootstrap, rec.cont) == (0.0, 0.0)
    # The final record is the capsized state itself, with the observer's
    # pending duration populated.
    assert rec.next_rs[RS["capsized"]] == 1.0
    assert rec.next_rs[RS["capsize_pending"]] > 0.0
    assert rec.reward < -9.0  # the failure penalty dominates


# -- M4U contract version ------------------------------------------------------

# The committed M0 file, as recorded in every M0–M4 store's run.json.
TASKS_JSON_SHA256_V1 = (
    "3115e1bf8d5f7b035ae03aadc384b23ee1d1f79a6236b9990bbcc56185f1fac0"
)


def test_contract_1_is_unchanged_and_r_stays_on_it(task_r: Task) -> None:
    """tasks.json is replay identity for every existing store (M4U)."""
    from sailing import TASKS_PATH, contract_identity, run_metadata, sha256_text

    assert sha256_text(TASKS_PATH.read_text()) == TASKS_JSON_SHA256_V1
    assert task_r.contract_version == 1
    assert TransitionReward(task_r).gamma == 0.999
    assert contract_identity(task_r) == {}
    assert "contract_version" not in run_metadata(task_r)


def test_u_contract_2_reward_hand_calculated() -> None:
    """One U transition at γ = 0.99975; contract 1 U keeps 0.999."""
    u = Task.load("U")
    assert u.contract_version == 2
    rf = TransitionReward(u)
    assert rf.gamma == 0.99975
    prev, nxt = rs_at(0.0, 0.0), rs_at(0.0, 1.0)  # d: 40 -> 39 from (0, 40)
    expected = -0.05 / 30.0 + 0.99975 * (-39.0 / 30.0) + 40.0 / 30.0
    assert rf(prev, nxt, RUNNING, 10) == pytest.approx(expected, abs=1e-12)
    u1 = Task.load("U", 1)
    assert TransitionReward(u1).gamma == 0.999
    # Only the reward constant differs between the two U contracts.
    assert {k: v for k, v in u1.reward.items() if k != "gamma"} == {
        k: v for k, v in u.reward.items() if k != "gamma"
    }
    assert (u1.target, u1.max_steps, u1.bounds) == (u.target, u.max_steps, u.bounds)


def test_u_episodes_record_and_replay_their_contract(tmp_path) -> None:
    from baselines import SmoothRandom
    from evaluate import run_episode
    from viz.episode_store import EpisodeStore, EpisodeWriter
    from viz.replay import replay

    short = [Task.load("U", 1), Task.load("U", 2)]
    with EpisodeWriter(tmp_path) as w:
        for t in short:
            run_episode(t, NOMINAL, SmoothRandom(3), writer=w)
    store = EpisodeStore(tmp_path)
    e1, e2 = store.index
    assert "contract_version" not in e1 and e2["contract_version"] == 2
    assert e1["return"] != e2["return"]  # same actions, different γ
    for e, t in zip((e1, e2), short, strict=True):
        # A task of the other contract is ignored: the episode's own one is used.
        other = short[1] if t is short[0] else short[0]
        assert replay(store.load(e), store.identity, other).exact
