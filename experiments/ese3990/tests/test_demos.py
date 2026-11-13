"""M4U expert-data arms: demonstration store/loader, SAC + demonstrations,
PPO + BC and the U settings (plan §6, M4U)."""

from __future__ import annotations

import json

import demos
import jax
import numpy as np
import ppo
import pytest
import sac
import train
from evaluate import run_episode
from make_conditions import TrainingSampler
from sailing import HERE, NOMINAL, Task, TaskEnv, TransitionReward
from viz.episode_store import EpisodeStore, state_checksum


def rows(path):
    return [json.loads(x) for x in path.read_text().splitlines()]


# ---------------------------------------------------------------------------
# Demonstration store and loader
# ---------------------------------------------------------------------------


@pytest.fixture(scope="module")
def u_demos(tmp_path_factory):
    store = tmp_path_factory.mktemp("demos") / "U"
    task = Task.load("U")
    sha = demos.collect(store, task, episodes=2)
    return store, sha, task, demos.load(store, sha, task)


def test_demo_store_is_rule_sailor_on_training_conditions(u_demos):
    store, sha, _, _ = u_demos
    s = EpisodeStore(store)
    sampler = TrainingSampler(1000)
    assert [e["condition"]["id"] for e in s.index] == [sampler.next().id for _ in "ab"]
    assert [(e["kind"], e["seed"], e["contract_version"]) for e in s.index] == [
        ("demo", 0, 2),
        ("demo", 1, 2),
    ]
    assert {e["controller"] for e in s.index} == {"rule_sailor"}
    assert demos.store_sha256(store) == sha


def test_demo_transitions_match_a_fresh_replay(u_demos):
    store, _, task, d = u_demos
    s = EpisodeStore(store)
    i = 0
    for e in s.index:
        env = TaskEnv(task, s.condition_of(e), e["seed"])
        env.reset()
        recs = [env.step(a) for a in s.load(e).actions]
        part = slice(i, i + len(recs))
        i += len(recs)
        for k in ("rs", "reward", "next_rs", "bootstrap", "cont"):
            np.testing.assert_array_equal(d[k][part], [getattr(r, k) for r in recs])
        np.testing.assert_array_equal(d["action"][part], [r.action[:2] for r in recs])
        assert state_checksum(d["next_rs"][part][-1]) == e["final_checksum"]
        assert np.cumsum(d["reward"][part])[-1] == e["return"]
    assert i == len(d["reward"]) == sum(e["decisions"] for e in s.index)


def test_returns_to_go_are_discounted_within_each_episode(u_demos):
    *_, task, d = u_demos
    gamma = TransitionReward(task).gamma
    assert gamma == 0.99975  # U contract 2
    start = 0
    for end in np.flatnonzero(d["cont"] == 0):
        r, g = d["reward"][start : end + 1], d["return_to_go"][start : end + 1]
        assert g[-1] == r[-1]  # nothing after the episode's last decision
        np.testing.assert_allclose(g[:-1], r[:-1] + gamma * g[1:], rtol=1e-12)
        np.testing.assert_allclose(
            g[0], np.sum(gamma ** np.arange(len(r)) * r), rtol=1e-9
        )
        start = end + 1
    assert start == len(d["reward"])


def test_load_refuses_another_hash_or_task(u_demos):
    store, sha, task, _ = u_demos
    with pytest.raises(ValueError, match="hash"):
        demos.load(store, "0" * 64, task)
    with pytest.raises(ValueError, match="not a demo of this task"):
        demos.load(store, sha, Task.load("R"))


# ---------------------------------------------------------------------------
# SAC + demonstrations: symmetric sampling
# ---------------------------------------------------------------------------


def replay_with(rewards, capacity):
    r = sac.Replay(capacity, obs_dim=1)
    n = len(rewards)
    r.add(
        obs=np.zeros((n, 1)),
        act=np.zeros((n, 2)),
        reward=np.asarray(rewards, float),
        next_obs=np.zeros((n, 1)),
        bootstrap=np.ones(n),
        cont=np.ones(n),
    )
    return r


def test_demo_rows_are_never_evicted_and_half_of_each_minibatch():
    demo = replay_with([100, 101, 102], capacity=3)
    online = replay_with(range(10), capacity=4)  # wrapped: holds 6..9 only
    rng = np.random.default_rng(0)
    seen = set()
    for _ in range(50):
        b = sac.symmetric_sample(rng, demo, online, 8)
        assert set(b["reward"][:4]) <= {100, 101, 102}
        assert set(b["reward"][4:]) <= {6, 7, 8, 9}
        seen |= set(b["reward"][:4])
    assert seen == {100, 101, 102}
    np.testing.assert_array_equal(demo.data["reward"], [100, 101, 102])


# ---------------------------------------------------------------------------
# PPO + BC
# ---------------------------------------------------------------------------


def test_bc_loss_falls_and_the_bc_actor_reloads_exactly(tmp_path):
    rng = np.random.default_rng(0)
    obs = rng.normal(size=(512, 17)).astype(np.float32)
    w = rng.normal(size=(17, 2)) / 4
    action = np.tanh(obs @ w)
    ret = obs[:, 0] * 2.0 + 5.0
    cfg = {**train.CONFIG, **train.DEMO_ARMS["ppo"], "bc_minibatch": 64}
    params0 = ppo.init(jax.random.PRNGKey(0), 17, 2, cfg["hidden"], cfg["log_std"])
    params, history = ppo.behavior_clone(params0, obs, action, ret, cfg, rng)
    assert len(history) == 20
    for k in ("bc_actor", "bc_value"):
        assert history[-1][k] < history[0][k]
    np.testing.assert_array_equal(params["log_std"], params0["log_std"])

    meta = {"obs_dim": 17, "config": cfg, "normalization": None}
    ppo.save(tmp_path / "bc", params, meta)
    loaded = ppo.PPOPolicy.load(tmp_path / "bc")
    np.testing.assert_array_equal(
        ppo.mean_action(loaded.params, obs), ppo.mean_action(params, obs)
    )


# ---------------------------------------------------------------------------
# Both expert-data arms through train.py, on R for speed
# ---------------------------------------------------------------------------

CAL = 600


@pytest.fixture(scope="module")
def r_demos(tmp_path_factory):
    store = tmp_path_factory.mktemp("rdemos") / "store"
    task = Task.load("R")
    sha = demos.collect(store, task, episodes=2)
    d = demos.load(store, sha, task)
    return {"store": str(store), "sha256": sha, "decisions": len(d["reward"])}, d


def arm_run(tmp_path_factory, r_demos, learner, base, online):
    rec, d = r_demos
    budget = CAL + rec["decisions"] + online
    cfg = {
        **base,
        **train.DEMO_ARMS[learner],
        "demos": rec,
        "calibration_decisions": CAL,
        "decisions": budget,
        "checkpoints": [0, budget],
    }
    fn = train.train if learner == "ppo" else train.train_sac
    out = fn(0, tmp_path_factory.mktemp(learner) / "run", cfg, d)
    return out, rec["decisions"], budget


@pytest.fixture(scope="module")
def ppo_bc(tmp_path_factory, r_demos):
    base = {**train.CONFIG, "rollout": 600, "minibatch": 200}
    return arm_run(tmp_path_factory, r_demos, "ppo", base, 1200)


@pytest.fixture(scope="module")
def sac_demo(tmp_path_factory, r_demos):
    base = {**train.SAC_CONFIG, "minibatch": 64, "log_every": 300}
    return arm_run(tmp_path_factory, r_demos, "sac", base, 600)


@pytest.mark.parametrize("arm", ["ppo_bc", "sac_demo"])
def test_demonstrations_are_charged_to_the_budget(arm, request):
    out, n_demo, budget = request.getfixturevalue(arm)
    initial = CAL + n_demo
    curve = rows(out / "curve.jsonl")
    assert [c["decisions"] for c in curve] == [initial, budget]
    assert json.loads((out / "summary.json").read_text())["decisions"] == budget
    s = EpisodeStore(out / "store")
    assert sum(e["decisions"] for e in s.query(kind="calibration")) == CAL
    assert sum(e["decisions"] for e in s.query(kind="training")) == budget - initial
    config = json.loads((out / "config.json").read_text())["config"]
    meta = json.loads((out / "ckpt" / f"{initial:07d}.json").read_text())
    assert config["arm"] == meta["config"]["arm"] == arm
    assert config["demos"]["decisions"] == meta["config"]["demos"]["decisions"]
    assert config["demos"]["decisions"] == n_demo
    # Every checkpoint reloads through the unchanged loaders and reproduces
    # its recorded nominal evaluation.
    policy_cls = ppo.PPOPolicy if arm == "ppo_bc" else sac.SACPolicy
    for c in curve:
        policy = policy_cls.load(out / "ckpt" / f"{c['decisions']:07d}")
        ep = run_episode(Task.load("R"), NOMINAL, policy)
        assert (ep["return"], ep["outcome"]) == (
            c["nominal_return"],
            c["nominal_outcome"],
        )


def test_ppo_bc_initial_checkpoint_is_the_bc_actor(ppo_bc):
    out, n_demo, _ = ppo_bc
    bc = rows(out / "bc.jsonl")
    assert [r["epoch"] for r in bc] == list(range(1, 21))
    assert bc[-1]["bc_loss"] < bc[0]["bc_loss"]
    params, _ = ppo.load(out / "ckpt" / f"{CAL + n_demo:07d}")
    np.testing.assert_array_equal(params["log_std"], [-0.5, -0.5])  # not fitted
    first = rows(out / "metrics.jsonl")[0]
    assert first["first_ratio_mean"] == pytest.approx(1.0, abs=1e-6)


def test_sac_demo_online_replay_excludes_demonstrations(sac_demo):
    out, n_demo, budget = sac_demo
    metrics = rows(out / "metrics.jsonl")
    assert metrics[-1]["decisions"] == budget
    assert metrics[-1]["replay_size"] == budget - n_demo  # calibration + online
    keys = ("q1_loss", "q2_loss", "actor_loss", "alpha", "q_mean")
    assert all(np.isfinite(m[k]) for m in metrics for k in keys)


def test_cli_refuses_a_wrong_demo_hash_before_creating_the_run(tmp_path, r_demos):
    rec, _ = r_demos
    args = ["--learner", "sac", "--demos", rec["store"], "--out", str(tmp_path / "x")]
    with pytest.raises(ValueError, match="hash"):
        train.main([*args, "--demos-sha256", "0" * 64])
    assert not (tmp_path / "x").exists()


# ---------------------------------------------------------------------------
# U settings (plan §6, M4U table)
# ---------------------------------------------------------------------------


def test_u_settings_amend_only_the_declared_m2_settings():
    m2 = json.loads((HERE / "configs" / "M2_freeze.json").read_text())["configs"]
    schedule = {
        "task": "U",
        "decisions": 4_000_000,
        "checkpoints": [0, 400_000, 1_000_000, 2_000_000, 3_000_000, 4_000_000],
    }
    assert {**m2["ppo"], **schedule, "arm": "ppo"} == train.U_CONFIG
    assert {
        **m2["sac"],
        **schedule,
        "replay": 400_000,
        "arm": "sac",
    } == train.U_SAC_CONFIG
    # R's own configs are untouched: the M2 freeze is them at the 1M schedule.
    r_schedule = {k: m2["ppo"][k] for k in ("decisions", "checkpoints")}
    assert {**train.CONFIG, **r_schedule} == m2["ppo"]
    assert {**train.SAC_CONFIG, **r_schedule} == m2["sac"]
    assert TransitionReward(Task.load("U")).gamma == 0.99975
    assert TransitionReward(Task.load("R")).gamma == 0.999
