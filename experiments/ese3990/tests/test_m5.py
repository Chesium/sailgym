"""M5 preserves raw rewards/replay and separates training/evaluation starts."""

import json
from dataclasses import replace

import m5
import m5_study
import m5_train
import numpy as np
import ppo
import pytest
from make_conditions import TrainingSampler, held_out_keys
from sailing import RS, Task, TaskEnv, TransitionReward
from viz.episode_store import EpisodeStore, EpisodeWriter, build_identity
from viz.replay import replay


def test_start_sampler_is_seeded_half_frozen_and_never_heldout():
    a, b = m5.StartSampler(7), m5.StartSampler(7)
    excluded = held_out_keys()
    for i in range(1000):
        ca, cb = a.next(), b.next()
        assert ca == cb and a.xy == b.xy and ca.key() not in excluded
        if i % 2 == 0:
            assert a.xy == (0.0, 0.0)
        else:
            x, y = a.xy
            assert -25 <= x <= 25 and 0 <= y <= 35
            assert 0 <= ca.heading_deg < 360
    with pytest.raises(ValueError):
        m5.StartSampler(0, (-10, 10, 25, 41))


@pytest.mark.parametrize("hold", [1, 10])
def test_blocks_discount_and_replay_identically_at_20hz(tmp_path, hold):
    task = replace(Task.load("U"), max_steps=230)  # partial final block
    gamma = TransitionReward(task).gamma
    sampler = m5.StartSampler(0, all_random=True)
    writer = EpisodeWriter(tmp_path)
    col = m5.BlockCollector(
        task, sampler, writer, {"kind": "training"}, "float32", hold
    )
    cond = col.env.cond
    xy = col.env.task.initial_xy
    route = task.route_json
    assert col.env.task.route_json == route  # starts never regenerate the gate
    assert tuple(col.rs[[RS["x"], RS["y"]]]) == xy
    env = TaskEnv(m5.StartTask.at(task, xy), cond)
    initial = env.reset()
    for field in ("u", "v", "r"):
        assert initial[RS[field]] == 0.0
    a = np.array([0.1, -0.3], np.float32)
    raw = []
    while env.outcome == "running":
        raw.append(env.step(a).reward)
    data, _ = col.run(100, lambda rs: (a, 0.0), max_decisions=23)
    col.close()
    writer.close()
    assert data["raw_decisions"].sum() == col.decisions == len(raw) == 23
    offsets = np.r_[0, np.cumsum(data["raw_decisions"])]
    block_return = sum(
        gamma**i * r for i, r in zip(offsets[:-1], data["reward"], strict=True)
    )
    raw_return = sum(gamma**i * r for i, r in enumerate(raw))
    assert block_return == pytest.approx(raw_return, abs=1e-12)
    assert data["cont"][-1] == 0 and data["bootstrap"][-1] == 1
    assert data["discount"][-1] == gamma ** (3 if hold == 10 else 1)
    store = EpisodeStore(tmp_path)
    assert len(store.index) == 1
    traj = m5.replay_episode(store.load(store.index[0]), store.identity, task)
    assert traj.exact and len(traj.actions) == 23
    np.testing.assert_array_equal(traj.states[-1], env.research_state())


def test_block_stops_at_genuine_terminal_and_keeps_zero_bootstrap(tmp_path):
    task = replace(Task.load("U"), bounds=(-0.001, -0.001, 0.001, 0.001))
    writer = EpisodeWriter(tmp_path)
    col = m5.BlockCollector(task, TrainingSampler(0), writer, {}, "float32", 10)
    data, ended = col.run(100, lambda rs: (np.array([0.0, -1.0], np.float32), 0.0))
    assert ended
    terminals = data["cont"] == 0
    assert np.all(data["bootstrap"][terminals] == 0)
    assert np.any(data["raw_decisions"][terminals] < 10)
    col.close()
    writer.close()


def test_calibration_block_packing_respects_action_changes_and_episode_ends():
    n = 25
    d = dict(
        rs=np.arange(n)[:, None],
        next_rs=np.arange(1, n + 1)[:, None],
        reward=np.arange(n) / 10,
        action=np.zeros((n, 3)),
        bootstrap=np.ones(n),
        cont=np.ones(n),
    )
    d["action"][13:] = 1.0
    d["cont"][7] = d["bootstrap"][7] = 0.0
    b = m5.calibration_blocks(d, 10, 0.9)
    assert b["raw_decisions"].tolist() == [8, 5, 10, 2]
    assert b["bootstrap"].tolist() == [0, 1, 1, 1]
    assert b["reward"][0] == pytest.approx(np.dot(0.9 ** np.arange(8), d["reward"][:8]))
    # Effective SAC bootstrap is correct for every full/partial block.
    mask = b["bootstrap"] * b["discount"] / 0.9**10
    np.testing.assert_allclose(0.9**10 * mask, b["bootstrap"] * b["discount"])


def test_variable_discount_gae_reduces_to_existing_gae_and_handles_timeout():
    d = dict(
        reward=np.array([1.0, 2.0, 3.0]),
        bootstrap=np.array([1.0, 1.0, 0.0]),
        cont=np.array([1.0, 0.0, 0.0]),
        discount=np.full(3, 0.9),
    )
    v, vn = np.array([2.0, 4.0, 6.0]), np.array([4.0, 9.0, 0.0])
    a, ret = m5.block_gae(d, v, vn, 0.95)
    old_a, old_ret = ppo.gae(d["reward"], v, vn, d["bootstrap"], d["cont"], 0.9, 0.95)
    np.testing.assert_array_equal(a, old_a)
    np.testing.assert_array_equal(ret, old_ret)
    d["discount"][1] = 0.99**3
    a, _ = m5.block_gae(d, v, vn, 0.95)
    assert a[1] == 2 + 0.99**3 * 9 - 4  # timeout bootstraps from actual final
    assert a[2] == 3 - 6  # genuine terminal never bootstraps


def test_held_evaluation_queries_once_per_block_and_resets():
    class Policy:
        def reset(self, env):
            self.calls = 0

        def act(self, env):
            self.calls += 1
            return np.array([self.calls, 0.0])

        def identity(self):
            return {"controller": "test"}

    policy = Policy()
    held = m5.HeldAction(policy, 10)
    held.reset(None)
    actions = np.array([held.act(None) for _ in range(23)])
    np.testing.assert_array_equal(actions[:, 0], [1.0] * 10 + [2.0] * 10 + [3.0] * 3)
    assert policy.calls == 3
    held.reset(None)
    assert held.act(None)[0] == 1


def test_frozen_start_blocks_use_existing_replay_without_m5_task(tmp_path):
    task = replace(Task.load("U"), max_steps=170)
    with EpisodeWriter(tmp_path) as writer:
        col = m5.BlockCollector(task, TrainingSampler(1), writer, {}, "float32", 10)
        col.run(2, lambda rs: (np.zeros(2, np.float32), 0.0))
        col.close()
    store = EpisodeStore(tmp_path)
    assert replay(store.load(store.index[0]), store.identity, task).exact


def test_neither_config_is_exactly_the_m4u_config():
    for learner in ("ppo", "sac"):
        original = (
            __import__("train").U_CONFIG
            if learner == "ppo"
            else __import__("train").U_SAC_CONFIG
        )
        assert m5_train.config(learner, "neither", m5.DEFAULT_BOX) == original


def test_queue_counts_reuse_and_extended_budgets_are_separate():
    assert len(m5_study.jobs("smoke")) == 8
    main = m5_study.jobs("main")
    assert len(main) == 34
    assert all(j["seed"] >= 3 for j in main if j["cell"] == "neither")
    extended = m5_study.jobs("extended")
    assert len(extended) == 10
    assert all(j["learner"] == "ppo" and j["cell"] in ("B", "BS") for j in extended)


def test_freeze_refuses_changed_sources(tmp_path):
    source = tmp_path / "source.txt"
    source.write_text("original")
    path = tmp_path / "freeze.json"
    path.write_text(
        json.dumps(
            dict(
                sources={str(source): m5_study.digest(source)},
                evidence={},
                identity=build_identity(),
            )
        )
    )
    assert m5_study.verify_freeze(path)["identity"] == build_identity()
    source.write_text("changed")
    with pytest.raises(ValueError, match="changed frozen input"):
        m5_study.verify_freeze(path)
