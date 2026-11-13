"""Small numerical CEM checks, then real-model and stored-probe proofs (E0)."""

import os
import subprocess
import sys
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace

import numpy as np
import pytest
from evaluate import run_episode
from mpc import CEMMPC, CEMConfig, fit_elites
from mpc_study import cost_summary
from sailing import NOMINAL, RS, Task, TaskEnv
from viz.episode_store import EpisodeStore, EpisodeWriter
from viz.replay import replay


def test_elite_ranking_mean_population_std_and_floor():
    samples = np.array([[[0.0, 0.0]], [[1.0, 2.0]], [[3.0, 2.0]], [[9.0, 9.0]]])
    mean, std = fit_elites(samples, np.array([-1.0, 4.0, 3.0, -2.0]), 2, 0.05)
    np.testing.assert_array_equal(mean, [[2.0, 2.0]])
    np.testing.assert_array_equal(std, [[1.0, 0.05]])
    # Ties keep candidate order, independent of worker completion order.
    mean, _ = fit_elites(samples, np.ones(4), 2, 0.05)
    np.testing.assert_array_equal(mean, [[0.5, 1.0]])


def test_discounted_ranking_and_actual_work():
    env = SimpleNamespace(
        task=SimpleNamespace(decision_dt=0.05),
        outcome="running",
        reward_fn=SimpleNamespace(gamma=0.25),
        research_state=lambda: np.zeros(20),
    )

    # Undiscounted ranking would pick candidate 0 (return 2); discounted
    # ranking must pick candidate 1 (1 > 0.25*2). No critic/tail is added.
    def rollout(rates):
        np.testing.assert_array_equal(rates[0], 0.0)
        return (
            np.zeros((2, 2, 20)),
            np.zeros((2, 2)),
            np.array([[10, 10], [10, 3]]),
            np.array([[0.0, 2.0], [1.0, 0.0]]),
        )

    env.rollout = rollout
    ctrl = CEMMPC(0, CEMConfig(horizon=2, candidates=2, iterations=1, block_s=0.05))
    ctrl.reset(env)
    ctrl.rng = SimpleNamespace(normal=lambda *args: np.ones((2, 2, 2)))
    np.testing.assert_array_equal(ctrl.act(env), [1.0, 1.0])
    assert ctrl.physics_steps == 33
    assert ctrl.rollout_calls == 1 and ctrl.candidate_rollouts == 2


def test_warm_start_shifts_one_decision_and_reprojects_blocks():
    env = TaskEnv(Task.load("R"), NOMINAL)
    env.reset()
    cfg = CEMConfig(horizon=20, candidates=2, iterations=1)
    ctrl = CEMMPC(1, cfg)
    ctrl.reset(env)
    old = np.linspace(-1, 1, 40).reshape(20, 2)
    ctrl.plan = old.copy()
    original = env.rollout
    captured = []

    def rollout(rates):
        captured.append(rates.copy())
        return original(rates)

    env.rollout = rollout
    before = env.research_state()
    ctrl.act(env)
    shifted = np.concatenate([old[1:], old[-1:]])
    expected = np.repeat(shifted.reshape(2, 10, 2).mean(axis=1), 10, axis=0)
    np.testing.assert_array_equal(captured[0][0], expected)
    np.testing.assert_array_equal(env.research_state(), before)
    ctrl.reset(env)
    assert ctrl.plan is None and ctrl.physics_steps == ctrl.rollout_calls == 0
    ctrl.act(env)
    np.testing.assert_array_equal(captured[1][0], 0.0)


def test_reproducible_closed_loop_probe_storage_and_short_terminal(tmp_path):
    # The Rust budget cuts the last decision to 3 physics steps.
    task = replace(Task.load("R"), max_steps=23)
    cfg = CEMConfig(horizon=10, candidates=4, iterations=2)
    with EpisodeWriter(tmp_path, shard_size=1) as writer:
        reports = [
            run_episode(
                task, NOMINAL, CEMMPC(3, cfg), writer=writer, log_meta={"kind": "probe"}
            )
            for _ in range(2)
        ]
    store = EpisodeStore(tmp_path)
    logs = [store.load(e) for e in store.index]
    np.testing.assert_array_equal(logs[0].actions, logs[1].actions)
    assert logs[0].meta["final_checksum"] == logs[1].meta["final_checksum"]
    for log, report in zip(logs, reports, strict=True):
        tr = replay(log, store.identity, task)
        assert report["physics_steps"] == 23 and report["outcome"] == "truncated"
        np.testing.assert_array_equal(
            log.plans[:, 1], tr.states[1:, [RS["x"], RS["y"]]]
        )
        assert np.isnan(log.plans[-1, 2:]).all()  # never simulates past truncation
        assert report["planning"]["hypothetical_physics_steps"] == 4 * 2 * (23 + 13 + 3)
        assert (np.abs(log.actions) <= 1).all()
        np.testing.assert_array_equal(tr.col("sheet_release"), 0.0)


def test_parallel_rollout_is_bit_identical_across_thread_counts():
    script = """
import hashlib, sys
import numpy as np
sys.path.insert(0, 'experiments/ese3990')
from sailing import Task, TaskEnv, NOMINAL
env = TaskEnv(Task.load('R'), NOMINAL); env.reset()
rates = np.random.default_rng(7).uniform(-1, 1, (8, 20, 2))
print(hashlib.sha256(b''.join(x.tobytes() for x in env.rollout(rates))).hexdigest())
"""
    results = [
        subprocess.check_output(
            [sys.executable, "-c", script],
            cwd=Path(__file__).resolve().parents[3],
            env={**os.environ, "RAYON_NUM_THREADS": str(n)},
            text=True,
        )
        for n in (1, 4)
    ]
    assert results[0] == results[1]


def test_cost_report_counts_short_decisions_and_strict_deadline_misses():
    task = replace(Task.load("R"), max_steps=23)
    episode = run_episode(
        task,
        NOMINAL,
        CEMMPC(
            0,
            CEMConfig(
                horizon=10,
                candidates=2,
                iterations=1,
            ),
        ),
    )
    episode["decision_latency_s"] = [0.01, 0.05, 0.09]
    stats = cost_summary([episode])
    assert stats["real_physics_steps"] == 23 and stats["real_decisions"] == 3
    assert stats["hypothetical_physics_steps"] == 2 * (23 + 13 + 3)
    assert stats["rollout_calls"] == 3 and stats["candidate_rollouts"] == 6
    assert stats["latency_ms"] == {"p50": 50.0, "p95": 86.0, "max": 90.0}
    assert stats["deadline_misses_50ms"] == 1
    assert stats["deadline_miss_rate"] == 1 / 3


@pytest.mark.parametrize(
    "config",
    [
        {"horizon": 0},
        {"candidates": 1},
        {"iterations": -1},
        {"min_std": 0},
        {"initial_std": float("nan")},
        {"horizon": 15},
        {"block_s": 0.07},
        {"block_s": float("inf")},
    ],
)
def test_inconsistent_settings_are_rejected(config):
    with pytest.raises(ValueError):
        ctrl = CEMMPC(config=CEMConfig(**config))
        env = TaskEnv(Task.load("R"), NOMINAL)
        ctrl.reset(env)


@pytest.mark.parametrize(
    "rates",
    [
        np.zeros((0, 10, 2)),
        np.zeros((2, 0, 2)),
        np.zeros((2, 10, 3)),
        np.full((2, 10, 2), np.nan),
    ],
)
def test_malformed_candidates_are_rejected(rates):
    env = TaskEnv(Task.load("R"), NOMINAL)
    env.reset()
    with pytest.raises(ValueError):
        env.rollout(rates)
