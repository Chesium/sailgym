"""SAC numerical checks (plan §4.2, E0) and end-to-end reproducibility (M2)."""

from __future__ import annotations

import json

import jax
import jax.numpy as jnp
import numpy as np
import pytest
import sac
import train
from evaluate import run_episode
from sailing import NOMINAL, Task
from viz.episode_store import EpisodeStore
from viz.replay import replay

OBS = 5


def batch(n=8, seed=0):
    rng = np.random.default_rng(seed)
    return {
        "obs": rng.normal(size=(n, OBS)).astype(np.float32),
        "act": rng.uniform(-1, 1, size=(n, 2)).astype(np.float32),
        "reward": rng.normal(size=n).astype(np.float32),
        "next_obs": rng.normal(size=(n, OBS)).astype(np.float32),
        "bootstrap": np.array([1, 1, 0, 1, 0, 1, 1, 0], np.float32)[:n],
        "cont": np.array([1, 0, 0, 1, 0, 1, 0, 0], np.float32)[:n],
    }


def constant_q(q, values):
    """Critics whose output is the constant ``values[k]`` whatever (s, a)."""
    q = jax.tree.map(jnp.zeros_like, q)
    for qk, c in zip(q, values, strict=True):
        qk[-1]["b"] = jnp.array([c])
    return q


def test_soft_bellman_target_by_hand():
    """Q̄_1 ≡ 3, Q̄_2 ≡ 5 → y = r + γ·b·(3 − α log π(a'|s')); b = 0 gives y = r."""
    params = sac.init(jax.random.PRNGKey(0), OBS, alpha0=0.5)
    target_q = constant_q(params["q"], [3.0, 5.0])
    b, key, gamma = batch(), jax.random.PRNGKey(7), 0.9
    y = np.asarray(sac.soft_target(params, target_q, b, key, gamma))
    _, logp2 = sac.sample_action(params["actor"], b["next_obs"], key)  # same a'
    expect = b["reward"] + gamma * b["bootstrap"] * (3.0 - 0.5 * np.asarray(logp2))
    np.testing.assert_allclose(y, expect, rtol=1e-6)
    term = b["bootstrap"] == 0  # genuine terminals: no future at all
    np.testing.assert_allclose(y[term], b["reward"][term], rtol=1e-6)


def test_truncated_rows_bootstrap_from_their_own_final_state():
    """A truncated row (b = 1) uses V(s'_final) from *its* next_obs: changing
    another row's next_obs leaves it alone; changing its own changes y."""
    params = sac.init(jax.random.PRNGKey(1), OBS)
    target_q = params["q"]
    b, key = batch(), jax.random.PRNGKey(3)
    y0 = np.asarray(sac.soft_target(params, target_q, b, key, 0.99))
    b2 = {**b, "next_obs": b["next_obs"].copy()}
    b2["next_obs"][0] += 1.0  # row 0: bootstrap 1
    y1 = np.asarray(sac.soft_target(params, target_q, b2, key, 0.99))
    assert y1[0] != y0[0]
    b3 = {**b, "next_obs": b["next_obs"].copy()}
    b3["next_obs"][2] += 1.0  # row 2: terminal, s' ignored
    np.testing.assert_array_equal(
        np.asarray(sac.soft_target(params, target_q, b3, key, 0.99)), y0
    )


def test_polyak_update_by_hand():
    t = {"w": jnp.array([0.0, 10.0])}
    o = {"w": jnp.array([1.0, 0.0])}
    got = sac.polyak(t, o, 0.1)["w"]
    np.testing.assert_allclose(got, [0.1, 9.0], rtol=1e-6)


def test_replay_owns_its_data_and_wraps_oldest_first():
    r = sac.Replay(4, OBS)
    b = batch(3)
    r.add(**b)
    np.testing.assert_array_equal(r.data["bootstrap"][:3], [1, 1, 0])
    np.testing.assert_array_equal(r.data["cont"][:3], [1, 0, 0])
    b["obs"][:] = 99.0  # the caller reuses its buffer
    assert not np.any(r.data["obs"] == 99.0)
    assert r.size == 3
    r.add(**batch(3, seed=1))  # 6 rows into capacity 4
    assert r.size == 4
    np.testing.assert_array_equal(r.data["reward"][:2], batch(3, 1)["reward"][1:])
    s = r.sample(np.random.default_rng(0), 16)
    assert s["obs"].shape == (16, OBS) and s["act"].shape == (16, 2)


def nonzero(tree) -> bool:
    return any(float(jnp.abs(x).max()) > 0 for x in jax.tree.leaves(tree))


@pytest.mark.parametrize(
    ("name", "own"),
    [("critic", "q"), ("actor", "actor"), ("alpha", "log_alpha")],
)
def test_gradient_destinations(name, own):
    """Each loss changes only its own parameters (plan §4.2)."""
    params = sac.init(jax.random.PRNGKey(2), OBS)
    # A non-trivial actor last layer, so every path carries gradient.
    params["actor"][-1]["w"] = params["actor"][-1]["w"] * 100
    b, key = batch(), jax.random.PRNGKey(4)
    fn = {
        "critic": lambda p: sac.critic_loss(p, p["q"], b, key, 0.99),
        "actor": lambda p: sac.actor_loss(p, b, key),
        "alpha": lambda p: sac.alpha_loss(p, b, key, -2.0),
    }[name]
    grads = jax.grad(lambda p: fn(p)[0])(params)
    for part in ("q", "actor", "log_alpha"):
        assert nonzero(grads[part]) == (part == own), part


def test_actor_gradient_flows_through_q_of_the_action():
    """With α ≈ 0 the actor's only signal is ∂Q/∂ã: it must be non-zero, and
    following it must raise min Q."""
    params = sac.init(jax.random.PRNGKey(5), OBS)
    params["log_alpha"] = jnp.array(-50.0)
    b, key = batch(), jax.random.PRNGKey(6)
    loss = lambda p: sac.actor_loss(p, b, key)[0]  # noqa: E731
    g = jax.grad(loss)(params)
    assert nonzero(g["actor"])
    stepped = {
        **params,
        "actor": jax.tree.map(lambda w, d: w - 1e-2 * d, params["actor"], g["actor"]),
    }
    assert float(loss(stepped)) < float(loss(params))


@pytest.mark.parametrize(("target_entropy", "alpha_up"), [(10.0, True), (-10.0, False)])
def test_temperature_update_sign(target_entropy, alpha_up):
    """Entropy below H_min → α grows; above → α shrinks [A p.157]."""
    params = sac.init(jax.random.PRNGKey(8), OBS)
    cfg = {**train.SAC_CONFIG, "target_entropy": target_entropy}
    init_opt, update = sac.make_update(cfg, 0.99)
    state = {
        "params": params,
        "target_q": params["q"],
        "opt": init_opt(params),
        "key": jax.random.PRNGKey(9),
    }
    new, stats = update(state, batch())
    assert (float(stats["alpha"]) > 0.2) == alpha_up
    assert float(new["params"]["log_alpha"]) == pytest.approx(
        np.log(float(stats["alpha"]))
    )


def test_target_networks_move_by_tau_only():
    params = sac.init(jax.random.PRNGKey(10), OBS)
    init_opt, update = sac.make_update(train.SAC_CONFIG, 0.99)
    target0 = jax.tree.map(jnp.zeros_like, params["q"])
    state = {
        "params": params,
        "target_q": target0,
        "opt": init_opt(params),
        "key": jax.random.PRNGKey(0),
    }
    new, _ = update(state, batch())
    tau = train.SAC_CONFIG["tau"]
    for t, q in zip(
        jax.tree.leaves(new["target_q"]),
        jax.tree.leaves(new["params"]["q"]),
        strict=True,
    ):
        np.testing.assert_allclose(t, tau * q, rtol=1e-5, atol=1e-7)


TINY = {
    **train.SAC_CONFIG,
    "decisions": 1500,
    "calibration_decisions": 600,
    "minibatch": 64,
    "log_every": 300,
    "checkpoints": [600, 1500],
}


@pytest.mark.parametrize(
    ("learner", "cfg"), [(train.train, train.CONFIG), (train.train_sac, TINY)]
)
def test_training_refuses_to_overwrite_an_existing_run(tmp_path, learner, cfg):
    marker = tmp_path / "config.json"
    marker.write_text("existing run")
    with pytest.raises(FileExistsError):
        learner(0, tmp_path, cfg)
    assert marker.read_text() == "existing run"


@pytest.fixture(scope="module")
def two_runs(tmp_path_factory):
    d = tmp_path_factory.mktemp("sac")
    return train.train_sac(0, d / "a", TINY), train.train_sac(0, d / "b", TINY)


def test_training_is_finite_and_reproducible(two_runs):
    a, b = two_runs
    rows_a = [json.loads(x) for x in (a / "metrics.jsonl").read_text().splitlines()]
    rows_b = [json.loads(x) for x in (b / "metrics.jsonl").read_text().splitlines()]
    assert [r["decisions"] for r in rows_a] == [900, 1200, 1500]
    assert rows_a[-1]["replay_size"] == 1500  # calibration is in replay
    keys = ("q1_loss", "q2_loss", "actor_loss", "alpha", "q_mean", "saturation")
    for ra, rb in zip(rows_a, rows_b, strict=True):
        assert all(np.isfinite(ra[k]) for k in keys)
        assert [ra[k] for k in keys] == [rb[k] for k in keys]
    pa, _ = sac.load(a / "ckpt" / "0001500")
    pb, _ = sac.load(b / "ckpt" / "0001500")
    for x, y in zip(jax.tree.leaves(pa), jax.tree.leaves(pb), strict=True):
        np.testing.assert_array_equal(x, y)


def test_calibration_is_identical_to_ppo(two_runs):
    """Same training seed → the same calibration episodes and normalisation."""
    a, _ = two_runs
    _, meta = sac.load(a / "ckpt" / "0001500")
    task = Task.load("R")
    from make_conditions import TrainingSampler
    from viz.episode_store import EpisodeWriter

    w = EpisodeWriter(a.parent / "ppo_calib", {"learner": "ppo"})
    norm, _ = train.calibrate(task, 0, TINY, TrainingSampler(0), w)
    w.close()
    assert norm == meta["normalization"]


def test_reloaded_checkpoint_reproduces_its_evaluation(two_runs):
    a, _ = two_runs
    curve = [json.loads(x) for x in (a / "curve.jsonl").read_text().splitlines()]
    assert [c["decisions"] for c in curve] == [600, 1500]
    policy = sac.SACPolicy.load(a / "ckpt" / "0001500")
    assert policy.identity()["controller"] == "sac"
    ep = run_episode(Task.load("R"), NOMINAL, policy)
    assert ep["return"] == curve[-1]["nominal_return"]


def test_every_stored_run_episode_replays_exactly(two_runs):
    a, _ = two_runs
    s = EpisodeStore(a / "store")
    assert {e["kind"] for e in s.index} == {
        "calibration",
        "training",
        "probe",
        "validation",
    }
    assert len(s.query(kind="probe")) == 2 * len(train.PROBE_IDS)
    assert sum(e["decisions"] for e in s.query(kind="training")) == 900
    for e in s.index:
        replay(s.load(e), s.identity)  # raises on any mismatch


# ---------------------------------------------------------------------------
# M4X X1: critic LayerNorm, default off
# ---------------------------------------------------------------------------


def test_critic_layernorm_is_off_by_default_and_adds_parameters_only():
    """Off: the M2/M4U parameter tree, and critic == ppo.mlp. On: the same
    random draws plus a unit scale and zero offset per critic hidden layer."""
    off = sac.init(jax.random.PRNGKey(11), OBS)
    on = sac.init(jax.random.PRNGKey(11), OBS, layernorm=True)
    assert all(set(layer) == {"w", "b"} for qk in off["q"] for layer in qk)
    for qk_off, qk_on in zip(off["q"], on["q"], strict=True):
        for a, b in zip(qk_off, qk_on, strict=True):
            np.testing.assert_array_equal(a["w"], b["w"])
        assert "ln_scale" not in qk_on[-1]  # no LayerNorm on the output layer
    x = np.random.default_rng(0).normal(size=(4, OBS + 2)).astype(np.float32)
    np.testing.assert_array_equal(sac.critic(off["q"][0], x), sac.mlp(off["q"][0], x))


def test_critic_layernorm_normalizes_each_hidden_preactivation():
    """LayerNorm makes a hidden layer invariant to a positive rescaling and a
    uniform shift of its pre-activations; the plain critic is not."""
    x = np.random.default_rng(1).normal(size=(4, OBS + 2)).astype(np.float32)
    for layernorm, invariant in ((True, True), (False, False)):
        qk = sac.init(jax.random.PRNGKey(12), OBS, layernorm=layernorm)["q"][0]
        moved = [dict(layer) for layer in qk]
        moved[0]["w"], moved[0]["b"] = 3.0 * qk[0]["w"], 3.0 * qk[0]["b"] + 0.7
        same = np.allclose(sac.critic(qk, x), sac.critic(moved, x), atol=1e-5)
        assert same == invariant


def test_layernorm_gradients_polyak_and_checkpoint_reload(tmp_path):
    params = sac.init(jax.random.PRNGKey(13), OBS, layernorm=True)
    params["actor"][-1]["w"] = params["actor"][-1]["w"] * 100
    b, key = batch(), jax.random.PRNGKey(4)
    g = jax.grad(lambda p: sac.critic_loss(p, p["q"], b, key, 0.99)[0])(params)
    assert nonzero(g["q"][0][0]["ln_scale"]) and not nonzero(g["actor"])
    g = jax.grad(lambda p: sac.actor_loss(p, b, key)[0])(params)
    assert not nonzero(g["q"])  # critics, LayerNorm included, frozen for the actor
    target = sac.polyak(jax.tree.map(jnp.zeros_like, params["q"]), params["q"], 0.1)
    np.testing.assert_allclose(target[1][0]["ln_scale"], 0.1)  # targets carry it too
    cfg = {**train.U_SAC_CONFIG, "critic_layernorm": True}
    sac.save(tmp_path / "c", params, {"obs_dim": OBS, "config": cfg})
    loaded, _ = sac.load(tmp_path / "c")
    for a, c in zip(jax.tree.leaves(loaded), jax.tree.leaves(params), strict=True):
        np.testing.assert_array_equal(a, c)
    assert jax.tree.structure(loaded) == jax.tree.structure(params)


def test_layernorm_flag_is_the_m4x_sac_on_u_arm_only(tmp_path):
    with pytest.raises(SystemExit):
        train.main(["--learner", "ppo", "--task", "U", "--critic-layernorm",
                    "--out", str(tmp_path / "x")])  # fmt: skip
    assert not (tmp_path / "x").exists()
