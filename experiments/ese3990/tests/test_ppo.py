"""PPO numerical checks (plan §4.1, E0) and end-to-end reproducibility (M1)."""

from __future__ import annotations

import json

import jax
import jax.numpy as jnp
import numpy as np
import ppo
import pytest
import train
from evaluate import run_episode
from sailing import NOMINAL, Task
from viz.episode_store import EpisodeStore
from viz.replay import replay


def test_tanh_gaussian_is_a_density_on_the_action_interval():
    """exp(log π) integrates to 1 over a ∈ (−1, 1) and equals N(atanh a)/(1−a²)."""
    mu, log_std = jnp.array([0.3]), jnp.array([np.log(0.6)])
    a = np.linspace(-1 + 1e-6, 1 - 1e-6, 200_001)
    u = jnp.asarray(np.arctanh(a))[:, None]
    logp = ppo.gaussian_log_prob(u, mu, log_std) - ppo.tanh_log_det(u)
    p = np.exp(np.asarray(logp, dtype=np.float64))
    assert np.trapezoid(p, a) == pytest.approx(1.0, abs=1e-3)
    sigma = 0.6
    expect = np.exp(-0.5 * ((np.arctanh(a) - 0.3) / sigma) ** 2) / (
        sigma * np.sqrt(2 * np.pi) * (1 - a**2)
    )
    mid = np.abs(a) < 0.99
    np.testing.assert_allclose(p[mid], expect[mid], rtol=1e-4)


def test_tanh_log_det_is_finite_where_tanh_saturates():
    u = jnp.array([[-30.0], [0.0], [30.0]])
    got = np.asarray(ppo.tanh_log_det(u))
    assert np.all(np.isfinite(got))
    assert got[1] == pytest.approx(0.0, abs=1e-6)
    assert got[0] == pytest.approx(got[2])  # symmetric


def test_gae_hand_example_with_truncation_and_termination():
    """Rows: running, truncated (bootstrap from the real final state, stop the
    sum), running, terminal (no bootstrap). γ = 0.9, λ = 0.5."""
    r = np.array([1.0, 2.0, 3.0, 4.0])
    v = np.array([0.5, 0.6, 0.7, 0.8])
    v_next = np.array([0.6, 1.0, 0.8, 9.9])  # 9.9 must be ignored (terminal)
    boot = np.array([1.0, 1.0, 1.0, 0.0])
    cont = np.array([1.0, 0.0, 1.0, 0.0])
    adv, ret = ppo.gae(r, v, v_next, boot, cont, 0.9, 0.5)
    # δ3 = 4 − .8 = 3.2;            A3 = 3.2
    # δ2 = 3 + .9·.8 − .7 = 3.02;   A2 = 3.02 + .45·3.2 = 4.46
    # δ1 = 2 + .9·1.0 − .6 = 2.3;   A1 = 2.3   (cont = 0: A2 does not leak in)
    # δ0 = 1 + .9·.6 − .5 = 1.04;   A0 = 1.04 + .45·2.3 = 2.075
    np.testing.assert_allclose(adv, [2.075, 2.3, 4.46, 3.2])
    np.testing.assert_allclose(ret, adv + v)


def test_gae_with_lambda_one_is_the_monte_carlo_return():
    """λ = 1 telescopes to G_t − V(s_t) [F p.515], whatever V is."""
    rng = np.random.default_rng(0)
    r, v = rng.normal(size=6), rng.normal(size=6)
    v_next = np.append(v[1:], 123.0)
    boot = np.array([1, 1, 1, 1, 1, 0.0])
    _, ret = ppo.gae(r, v, v_next, boot, boot, 0.9, 1.0)
    g = [sum(0.9**k * r[t + k] for k in range(6 - t)) for t in range(6)]
    np.testing.assert_allclose(ret, g)


@pytest.mark.parametrize(
    ("ratio", "adv", "grad"),
    [
        (1.1, 2.0, 2.0),  # inside the clip: ordinary IS gradient
        (1.5, 2.0, 0.0),  # Â > 0, r > 1+ε: no incentive to go further [A p.52]
        (0.5, 2.0, 2.0),  # Â > 0, r < 1−ε: still pushed back up
        (0.5, -2.0, 0.0),  # Â < 0, r < 1−ε: no incentive to go further [A p.53]
        (1.5, -2.0, -2.0),  # Â < 0, r > 1+ε: unclipped, pulled back [A p.54]
    ],
)
def test_clipped_surrogate_gradient_cases(ratio, adv, grad):
    g = jax.grad(lambda x: ppo.clipped_surrogate(x, jnp.array([adv]), 0.2))(
        jnp.array([ratio])
    )
    assert float(g[0]) == pytest.approx(grad)


def test_ratio_is_one_before_the_first_update():
    params = ppo.init(jax.random.PRNGKey(0), 17)
    rng = np.random.default_rng(0)
    obs = rng.normal(size=(64, 17)).astype(np.float32)
    u = rng.normal(size=(64, 2)).astype(np.float32)
    batch = {
        "obs": obs,
        "u": u,
        "logp": np.asarray(jax.jit(ppo.log_prob)(params, obs, u)),
        "adv": rng.normal(size=64).astype(np.float32),
        "ret": rng.normal(size=64).astype(np.float32),
    }
    cfg = {"clip": 0.2, "vf_coef": 0.5, "ent_coef": 0.001}
    _, stats = ppo.loss(params, batch, cfg)
    assert float(stats["ratio_mean"]) == pytest.approx(1.0, abs=1e-6)
    assert float(stats["approx_kl"]) == pytest.approx(0.0, abs=1e-6)
    assert float(stats["clip_frac"]) == 0.0


TINY = {
    **train.CONFIG,
    "decisions": 3000,
    "calibration_decisions": 600,
    "rollout": 600,
    "minibatch": 200,
    "checkpoints": [600, 3000],
}


@pytest.fixture(scope="module")
def two_runs(tmp_path_factory):
    d = tmp_path_factory.mktemp("ppo")
    return train.train(0, d / "a", TINY), train.train(0, d / "b", TINY)


def test_training_is_finite_and_reproducible(two_runs):
    a, b = two_runs
    rows_a = [json.loads(x) for x in (a / "metrics.jsonl").read_text().splitlines()]
    rows_b = [json.loads(x) for x in (b / "metrics.jsonl").read_text().splitlines()]
    assert len(rows_a) == 4
    keys = ("loss", "approx_kl", "grad_norm", "train_mean_return")
    for ra, rb in zip(rows_a, rows_b, strict=True):
        assert ra["first_ratio_mean"] == pytest.approx(1.0, abs=1e-6)
        assert all(np.isfinite(ra[k]) for k in ("loss", "approx_kl", "grad_norm"))
        assert [ra[k] for k in keys] == [rb[k] for k in keys]
    pa, _ = ppo.load(a / "ckpt" / "0003000")
    pb, _ = ppo.load(b / "ckpt" / "0003000")
    for x, y in zip(jax.tree.leaves(pa), jax.tree.leaves(pb), strict=True):
        np.testing.assert_array_equal(x, y)


def test_reloaded_checkpoint_reproduces_its_evaluation(two_runs):
    a, _ = two_runs
    curve = [json.loads(x) for x in (a / "curve.jsonl").read_text().splitlines()]
    assert [c["decisions"] for c in curve] == [600, 3000]
    policy = ppo.PPOPolicy.load(a / "ckpt" / "0003000")
    ep = run_episode(Task.load("R"), NOMINAL, policy)
    assert ep["return"] == curve[-1]["nominal_return"]


def test_every_stored_run_episode_replays_exactly(two_runs):
    a, _ = two_runs
    s = EpisodeStore(a / "store")
    kinds = {e["kind"] for e in s.index}
    assert kinds == {"calibration", "training", "probe", "validation"}
    assert len(s.query(kind="probe")) == 2 * len(train.PROBE_IDS)
    assert sum(e["decisions"] for e in s.query(kind="calibration")) == 600
    assert sum(e["decisions"] for e in s.query(kind="training")) == 2400
    for e in s.index:
        replay(s.load(e), s.identity)  # raises on any mismatch


def test_select_checkpoint_prefers_finished_then_return_then_earliest():
    from evaluate import select_checkpoint

    def row(d, f, r):
        return {"decisions": d, "val_finished": f, "val_mean_return": r}

    curve = [row(1, 0, 5.0), row(2, 20, 10.0), row(3, 20, 10.5), row(4, 20, 10.5)]
    assert select_checkpoint(curve)["decisions"] == 3
    assert select_checkpoint(curve[:2])["decisions"] == 2


def test_training_refuses_a_last_rollout_shorter_than_a_minibatch(tmp_path):
    cfg = {**TINY, "decisions": 600 + 600 + 199}  # last rollout: 199 < 200
    with pytest.raises(ValueError, match="shorter than one minibatch"):
        train.train(0, tmp_path / "run", cfg)
    assert not (tmp_path / "run").exists()
