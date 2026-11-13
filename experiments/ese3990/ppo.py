"""PPO: tanh-squashed Gaussian policy, GAE, clipped surrogate (plan §4.1).

Slide references are PDF page numbers:

* ``[F p.N]`` — ``docs/ese3990/ESE3990_slides_L1-L13_foundations.pdf``
* ``[A p.N]`` — ``docs/ese3990/ESE3990_slides_L14-L19_advanced_topics.pdf``

The algorithm is the one summarised on [A p.57]:

1. Roll out the current policy for a batch of decisions (``train.Collector``).
2. Estimate advantages with GAE (:func:`gae`).
3. Run several epochs of minibatch gradient ascent on ``L_CLIP + c·H(π)``
   (:func:`make_update`), as an actor–critic: the critic supplies the
   baseline/bootstrap values [F p.507–516].

This module holds only the algorithm. Nothing in it knows about boats: the
observation is ``sailing.features`` normalised by :func:`normalize`, and the
action is the two rates ``sailing.ActionAdapter`` expects.
"""

from __future__ import annotations

import json
import math
from pathlib import Path
from typing import Any

import jax
import jax.numpy as jnp
import numpy as np
import optax
from sailing import features

PPO_VERSION = 1

# ---------------------------------------------------------------------------
# Networks: 2 × 64 tanh MLPs for the actor mean and the critic (plan §4.5)
# ---------------------------------------------------------------------------


def mlp_init(key: jax.Array, sizes: list[int]) -> list[dict[str, jax.Array]]:
    keys = jax.random.split(key, len(sizes) - 1)
    return [
        {"w": jax.random.normal(k, (m, n)) / math.sqrt(m), "b": jnp.zeros(n)}
        for k, m, n in zip(keys, sizes[:-1], sizes[1:], strict=True)
    ]


def mlp(params: list[dict[str, jax.Array]], x: jax.Array) -> jax.Array:
    for layer in params[:-1]:
        x = jnp.tanh(x @ layer["w"] + layer["b"])
    return x @ params[-1]["w"] + params[-1]["b"]


def init(
    key: jax.Array,
    obs_dim: int,
    act_dim: int = 2,
    hidden: int = 64,
    log_std: float = -0.5,
) -> dict[str, Any]:
    """Actor μ_θ(s), a state-independent log σ, and critic V_w(s).

    The policy is the Gaussian of [F p.456], π(u|s) = N(μ_θ(s), σ²), with σ
    learned rather than fixed. Actor and critic share no layers.
    """
    ka, kc = jax.random.split(key)
    actor = mlp_init(ka, [obs_dim, hidden, hidden, act_dim])
    # Small last layer: the initial mean is ~0, so the first rollouts explore
    # around "hold the rudder and sheet" rather than a random saturated rate.
    actor[-1]["w"] = actor[-1]["w"] * 0.01
    return {
        "actor": actor,
        "log_std": jnp.full(act_dim, log_std),
        "critic": mlp_init(kc, [obs_dim, hidden, hidden, 1]),
    }


def value(params: dict[str, Any], obs: jax.Array) -> jax.Array:
    """V_w(s), the learned baseline [F p.510–512]."""
    return mlp(params["critic"], obs)[..., 0]


def mean_action(params: dict[str, Any], obs: jax.Array) -> jax.Array:
    """The deterministic action tanh(μ_θ(s)), used for evaluation."""
    return jnp.tanh(mlp(params["actor"], obs))


# ---------------------------------------------------------------------------
# The tanh-transformed action likelihood
# ---------------------------------------------------------------------------
#
# The boat needs rates in [-1, 1], but a Gaussian has unbounded support. So we
# sample u ~ N(μ, σ²) and execute a = tanh(u). By the change-of-variables rule
#
#     log π(a|s) = log N(u; μ, σ²) − Σ_i log(1 − tanh²(u_i)).
#
# (The slides use the plain Gaussian [F p.456] and introduce the same
# reparameterised Gaussian for SAC [A p.153]; the squashing correction is the
# one in Haarnoja et al. 2018, App. C.) We store u, never a, so the likelihood
# is evaluated at exactly the sample that was drawn: inverting a float32 tanh
# near ±1 would not recover it.


def gaussian_log_prob(u: jax.Array, mu: jax.Array, log_std: jax.Array) -> jax.Array:
    z = (u - mu) * jnp.exp(-log_std)
    return jnp.sum(-0.5 * z**2 - log_std - 0.5 * jnp.log(2.0 * jnp.pi), axis=-1)


def tanh_log_det(u: jax.Array) -> jax.Array:
    """Σ log(1 − tanh²(u)), in the overflow-free form 2·(log 2 − u − softplus(−2u))."""
    return jnp.sum(2.0 * (jnp.log(2.0) - u - jax.nn.softplus(-2.0 * u)), axis=-1)


def log_prob(params: dict[str, Any], obs: jax.Array, u: jax.Array) -> jax.Array:
    """log π_θ(a|s) of the executed action a = tanh(u)."""
    mu = mlp(params["actor"], obs)
    return gaussian_log_prob(u, mu, params["log_std"]) - tanh_log_det(u)


def entropy(params: dict[str, Any]) -> jax.Array:
    """Entropy of the pre-squash Gaussian, Σ(log σ + ½ log 2πe).

    shortcut: the tanh-Gaussian's own entropy has no closed form; this is the
    usual surrogate for the entropy bonus [A p.56]. Upgrade only if the bonus
    is ever made large enough for the difference to matter.
    """
    return jnp.sum(params["log_std"] + 0.5 * jnp.log(2.0 * jnp.pi * jnp.e))


# ---------------------------------------------------------------------------
# Observation normalisation [A p.59, tricks 4–5]
# ---------------------------------------------------------------------------


def normalization_from(feats: np.ndarray, std_floor: float) -> dict[str, list[float]]:
    """Frozen mean/std from the calibration features (plan §3.3, §4.5).

    ``std_floor`` stops a feature that never moved during calibration (e.g.
    ``capsize_pending``) from being divided by ~0.
    """
    return {
        "mean": feats.mean(axis=0).tolist(),
        "std": np.maximum(feats.std(axis=0), std_floor).tolist(),
    }


def normalize(
    rs: np.ndarray, target: tuple[float, float], norm: dict[str, Any], clip: float
) -> np.ndarray:
    """Research state → the actor/critic input: standardised, then clipped."""
    z = (features(rs, target) - np.asarray(norm["mean"])) / np.asarray(norm["std"])
    return np.clip(z, -clip, clip).astype(np.float32)


# ---------------------------------------------------------------------------
# Generalized Advantage Estimation [F p.515–516], [A p.7, p.57]
# ---------------------------------------------------------------------------


def gae(
    reward: np.ndarray,
    v: np.ndarray,
    v_next: np.ndarray,
    bootstrap: np.ndarray,
    cont: np.ndarray,
    gamma: float,
    lam: float,
) -> tuple[np.ndarray, np.ndarray]:
    """Advantages Â_t = Σ_{t'≥t} (γλ)^{t'−t} δ_{t'} and critic targets Â_t + V(s_t).

    The TD error is δ_t = r_t + γ·V(s_{t+1}) − V(s_t) [F p.515], with the two
    masks of plan §3.4 deciding what "s_{t+1}" means at an episode boundary:

    * ``bootstrap_t`` = 0 at a genuine terminal (finished/failed): there is no
      future, so δ_t = r_t − V(s_t).
    * ``bootstrap_t`` = 1 at a time-limit truncation: the boat did not stop, so
      we bootstrap from ``v_next`` = V of the **actual final state**, never the
      next episode's reset state.
    * ``cont_t`` = 0 at any episode end: the backward sum must not leak the
      next episode's advantages into this one.

    The sum starts from 0 after the last row, so a rollout cut mid-episode is
    handled like a truncation: δ bootstraps from V(s_T), and nothing beyond.
    """
    adv = np.zeros(len(reward), dtype=np.float64)
    a = 0.0
    for t in reversed(range(len(reward))):
        delta = reward[t] + gamma * bootstrap[t] * v_next[t] - v[t]
        a = delta + gamma * lam * cont[t] * a
        adv[t] = a
    return adv, adv + v


# ---------------------------------------------------------------------------
# The PPO-Clip objective [A p.48–55]
# ---------------------------------------------------------------------------


def clipped_surrogate(ratio: jax.Array, adv: jax.Array, eps: float) -> jax.Array:
    """L_CLIP = E[min(r_t Â_t, clip(r_t, 1−ε, 1+ε) Â_t)] [A p.51].

    r_t = π_θ'(a_t|s_t) / π_θ(a_t|s_t) is the per-decision importance weight
    that lets us take several gradient steps on data from the old policy θ
    [A p.15–21], ignoring the state-distribution ratio because θ' stays close
    to θ [A p.31, p.40]. The min/clip removes the incentive to push r_t past
    1+ε when Â>0 [A p.52] or below 1−ε when Â<0 [A p.53–54]: in those regions
    the gradient is exactly zero.
    """
    return jnp.mean(
        jnp.minimum(ratio * adv, jnp.clip(ratio, 1.0 - eps, 1.0 + eps) * adv)
    )


def loss(
    params: dict[str, Any], batch: dict[str, jax.Array], cfg: dict[str, Any]
) -> tuple[jax.Array, dict[str, jax.Array]]:
    """−L_CLIP + c_v·(V − target)² − c_H·H(π), to be *minimised*."""
    logp = log_prob(params, batch["obs"], batch["u"])
    log_ratio = logp - batch["logp"]
    ratio = jnp.exp(log_ratio)
    # Advantage normalisation per minibatch: keeps the policy step at a
    # consistent scale whatever the reward magnitude [A p.59, trick 2].
    adv = (batch["adv"] - batch["adv"].mean()) / (batch["adv"].std() + 1e-8)
    l_clip = clipped_surrogate(ratio, adv, cfg["clip"])
    # Critic: regress V_w(s) onto the GAE return Â + V_old [F p.512].
    l_v = jnp.mean((value(params, batch["obs"]) - batch["ret"]) ** 2)
    h = entropy(params)
    total = -l_clip + cfg["vf_coef"] * l_v - cfg["ent_coef"] * h
    stats = {
        "loss": total,
        "l_clip": l_clip,
        "l_value": l_v,
        "entropy": h,
        "ratio_mean": ratio.mean(),
        # KL(π_θ‖π_θ') estimated from our own samples as E[(r−1) − log r],
        # which is ≥ 0 per sample [A p.62]; a large value means the "θ' close
        # to θ" assumption behind the clip is failing [A p.56, p.60].
        "approx_kl": jnp.mean((ratio - 1.0) - log_ratio),
        "clip_frac": jnp.mean(jnp.abs(ratio - 1.0) > cfg["clip"]),
    }
    return total, stats


def make_optimizer(cfg: dict[str, Any]) -> optax.GradientTransformation:
    # Gradient-norm clipping before Adam [A p.59, trick 6].
    return optax.chain(
        optax.clip_by_global_norm(cfg["max_grad_norm"]), optax.adam(cfg["lr"])
    )


def make_update(cfg: dict[str, Any]):
    """The minibatch-epoch loop of [A p.55, p.57] around one jitted step."""
    opt = make_optimizer(cfg)
    loss_cfg = {k: cfg[k] for k in ("clip", "vf_coef", "ent_coef")}

    @jax.jit
    def step(params, opt_state, batch):
        (_, stats), grads = jax.value_and_grad(loss, has_aux=True)(
            params, batch, loss_cfg
        )
        stats["grad_norm"] = optax.tree.norm(grads)  # before clipping
        updates, opt_state = opt.update(grads, opt_state, params)
        return optax.apply_updates(params, updates), opt_state, stats

    def update(
        params, opt_state, data: dict[str, np.ndarray], rng: np.random.Generator
    ):
        n = len(data["adv"])
        mb = cfg["minibatch"]
        history = []
        for _ in range(cfg["epochs"]):
            perm = rng.permutation(n)
            for i in range(0, n - mb + 1, mb):
                batch = {k: v[perm[i : i + mb]] for k, v in data.items()}
                params, opt_state, stats = step(params, opt_state, batch)
                history.append(stats)
        mean = {k: float(np.mean([s[k] for s in history])) for k in history[0]}
        first = {
            f"first_{k}": float(history[0][k]) for k in ("ratio_mean", "clip_frac")
        }
        return params, opt_state, {**mean, **first}

    return opt, update


# ---------------------------------------------------------------------------
# Checkpoints and the evaluation controller
# ---------------------------------------------------------------------------


def save(path: Path, params: dict[str, Any], meta: dict[str, Any]) -> None:
    """``<path>.npz`` holds the parameter leaves in tree order; ``<path>.json``
    holds everything needed to rebuild and evaluate the policy."""
    path.parent.mkdir(parents=True, exist_ok=True)
    leaves = jax.tree.leaves(params)
    np.savez(path.with_suffix(".npz"), *[np.asarray(x) for x in leaves])
    path.with_suffix(".json").write_text(json.dumps(meta, indent=1))


def load(path: Path) -> tuple[dict[str, Any], dict[str, Any]]:
    meta = json.loads(path.with_suffix(".json").read_text())
    cfg = meta["config"]
    template = init(
        jax.random.PRNGKey(0), meta["obs_dim"], 2, cfg["hidden"], cfg["log_std"]
    )
    treedef = jax.tree.structure(template)
    with np.load(path.with_suffix(".npz")) as z:
        leaves = [jnp.asarray(z[f"arr_{i}"]) for i in range(len(z.files))]
    for a, b in zip(leaves, jax.tree.leaves(template), strict=True):
        assert a.shape == b.shape, (a.shape, b.shape)
    return jax.tree.unflatten(treedef, leaves), meta


_mean_action = jax.jit(mean_action)


class PPOPolicy:
    """A saved PPO actor as an ``evaluate.run_episode`` controller.

    Deterministic: it executes tanh(μ_θ(s)). Its float32 actions are logged
    as float32, which the episode store checks is lossless.
    """

    action_dtype = "float32"
    _act_fn = staticmethod(_mean_action)  # SACPolicy swaps in its own

    def __init__(
        self, params: dict[str, Any], meta: dict[str, Any], name: str = ""
    ) -> None:
        self.params = params
        self.meta = meta
        self.name = name

    @staticmethod
    def load(path: Path | str) -> PPOPolicy:
        path = Path(path)
        params, meta = load(path)
        return PPOPolicy(params, meta, str(path))

    def reset(self, env) -> None:
        pass

    def act(self, env) -> np.ndarray:
        obs = normalize(
            env.research_state(),
            env.task.target,
            self.meta["normalization"],
            self.meta["config"]["norm_clip"],
        )
        return np.asarray(self._act_fn(self.params, obs))

    def identity(self) -> dict[str, Any]:
        return {
            "controller": "ppo",
            "version": PPO_VERSION,
            "checkpoint": self.name,
            "decisions": self.meta.get("decisions"),
            "training_seed": self.meta.get("training_seed"),
        }
