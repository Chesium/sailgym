"""SAC: replay, twin Q + target Q, tanh-Gaussian actor, learned temperature (plan §4.2).

Slide references are PDF page numbers:

* ``[F p.N]`` — ``docs/ese3990/ESE3990_slides_L1-L13_foundations.pdf``
* ``[A p.N]`` — ``docs/ese3990/ESE3990_slides_L14-L19_advanced_topics.pdf``

Reading entry point: [SAC algorithm][slides].
[slides]: ../../docs/ese3990/ESE3990_slides_L14-L19_advanced_topics.pdf#page=158

One gradient step of :func:`make_update` is one pass of the inner loop of the
"SAC (Revised)" algorithm on [A p.158]:

1. Sample a minibatch (s, a, r, s', bootstrap) from the replay buffer D.
2. Soft target  y = r + γ·b·(min_k Q̄_k(s', a') − α log π(a'|s')),  a' ~ π(·|s').
3. Critic step on  L_Q(w_k) = mean (y − Q_k(s, a))²  for k = 1, 2.
4. Actor step on   L_π(θ) = mean (α log π(ã|s) − min_k Q_k(s, ã)),  ã = tanh(μ + σε).
5. Temperature step on  L(α) = mean −α (log π(ã|s) + H_min).
6. Polyak:  w̄ ← m·w̄ + (1 − m)·w,  m = 1 − τ.

Nothing here knows about boats: observations come from ``ppo.normalize`` (the
same frozen feature transform PPO uses) and actions are the two rates
``sailing.ActionAdapter`` expects.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import jax
import jax.numpy as jnp
import numpy as np
import optax
from jax.lax import stop_gradient
from ppo import (
    PPOPolicy,
    gaussian_log_prob,
    make_optimizer,
    mlp,
    mlp_init,
    save,
    tanh_log_det,
)

SAC_VERSION = 1
LOG_STD_MIN, LOG_STD_MAX = -5.0, 2.0  # keeps σ in [0.007, 7.4]: no 0 or ∞ log π

# ---------------------------------------------------------------------------
# Replay buffer: off-policy data reuse [F p.431, p.437–438], [A p.86]
# ---------------------------------------------------------------------------


class Replay:
    """A fixed-capacity ring buffer of transitions (s, a, r, s', bootstrap, cont).

    Unlike PPO's rollout batch, transitions stay here for ``capacity``
    decisions, are sampled uniformly and reused by many updates, whatever
    (older) policy collected them — which is why SAC can learn off-policy.

    ``bootstrap`` is the plan §3.4 mask: 0 after a genuine terminal (finished
    or failed), 1 otherwise — including a time-limit truncation, where s' is
    the episode's **actual final state**, never the next reset state. That one
    mask is all the target needs. ``cont`` also preserves episode boundaries:
    (bootstrap, cont) is (1, 1) running, (1, 0) truncated, (0, 0) terminal.

    :meth:`add` copies into preallocated arrays, so the buffer owns its data.
    """

    def __init__(self, capacity: int, obs_dim: int, act_dim: int = 2) -> None:
        self.capacity, self.size, self._i = capacity, 0, 0
        self.data = {
            "obs": np.zeros((capacity, obs_dim), np.float32),
            "act": np.zeros((capacity, act_dim), np.float32),
            "reward": np.zeros(capacity, np.float32),
            "next_obs": np.zeros((capacity, obs_dim), np.float32),
            "bootstrap": np.zeros(capacity, np.float32),
            "cont": np.zeros(capacity, np.float32),
        }

    def add(self, **rows: np.ndarray) -> None:
        """Append a batch of rows; the oldest are overwritten once full."""
        n = len(rows["reward"])
        idx = (self._i + np.arange(n)) % self.capacity
        for k, v in self.data.items():
            v[idx] = rows[k]
        self._i = int((self._i + n) % self.capacity)
        self.size = min(self.size + n, self.capacity)

    def sample(self, rng: np.random.Generator, n: int) -> dict[str, np.ndarray]:
        idx = rng.integers(0, self.size, n)
        return {k: v[idx] for k, v in self.data.items()}


def symmetric_sample(
    rng: np.random.Generator, demo: Replay, online: Replay, n: int
) -> dict[str, np.ndarray]:
    """M4U's SAC + demonstrations: n/2 demonstration rows, then n/2 online rows.

    RLPD's symmetric sampling (Ball et al. 2023) only. ``demo`` is filled once
    to exactly its capacity and never added to, so nothing in it is evicted.
    """
    d, o = demo.sample(rng, n // 2), online.sample(rng, n - n // 2)
    return {k: np.concatenate([d[k], o[k]]) for k in d}


# ---------------------------------------------------------------------------
# Networks (plan §4.5: 2 × 64 tanh, the same hidden widths as PPO)
# ---------------------------------------------------------------------------


def init(
    key: jax.Array,
    obs_dim: int,
    act_dim: int = 2,
    hidden: int = 64,
    alpha0=0.2,
    layernorm: bool = False,
) -> dict[str, Any]:
    """Actor π_θ, critics Q_{w1}, Q_{w2}, and log α — "Init w1, w2, θ, α" [A p.158].

    The actor outputs μ_θ(s) and log σ_θ(s): unlike PPO, σ depends on the
    state, as in the reparameterised policy N(μ_θ(s), σ_θ(s)) of [A p.153].
    The two critics have different random initialisations; that is all that
    decorrelates them [A p.137]. α is learned as log α so it stays positive.

    ``layernorm`` (M4X arm X1, default off): each critic hidden layer gets a
    LayerNorm with a learned scale and offset, as in RLPD (Ball et al. 2023).
    It adds parameters only; every random draw, and so the actor and critic
    weights for a given key, is the same as without it.
    """
    ka, k1, k2 = jax.random.split(key, 3)
    actor = mlp_init(ka, [obs_dim, hidden, hidden, 2 * act_dim])
    actor[-1]["w"] = actor[-1]["w"] * 0.01  # as PPO: start at μ ≈ 0, σ ≈ 1
    q = [mlp_init(k, [obs_dim + act_dim, hidden, hidden, 1]) for k in (k1, k2)]
    if layernorm:
        for layer in (layer for qk in q for layer in qk[:-1]):
            layer["ln_scale"], layer["ln_bias"] = jnp.ones(hidden), jnp.zeros(hidden)
    return {"actor": actor, "q": q, "log_alpha": jnp.log(jnp.asarray(alpha0))}


def actor_dist(actor, obs: jax.Array) -> tuple[jax.Array, jax.Array]:
    mu, log_std = jnp.split(mlp(actor, obs), 2, axis=-1)
    return mu, jnp.clip(log_std, LOG_STD_MIN, LOG_STD_MAX)


def sample_action(actor, obs: jax.Array, key: jax.Array) -> tuple[jax.Array, jax.Array]:
    """ã = tanh(μ_θ(s) + σ_θ(s)·ε), ε ~ N(0, I), and log π_θ(ã|s).

    The reparameterisation trick [A p.153–154]: ã is a differentiable
    function of θ given ε, so ∇_θ flows through the sampled action into Q.
    The tanh squashes to the rate interval [−1, 1]; its log-Jacobian is the
    same change-of-variables correction PPO uses (``ppo.tanh_log_det``).
    """
    mu, log_std = actor_dist(actor, obs)
    u = mu + jnp.exp(log_std) * jax.random.normal(key, mu.shape)
    return jnp.tanh(u), gaussian_log_prob(u, mu, log_std) - tanh_log_det(u)


LN_EPS = 1e-6  # flax.linen.LayerNorm's default, which RLPD uses


def critic(qk, x: jax.Array) -> jax.Array:
    """``ppo.mlp``, with RLPD's order Dense → LayerNorm → activation in every
    hidden layer that carries LayerNorm parameters (M4X X1)."""
    if "ln_scale" not in qk[0]:
        return mlp(qk, x)
    for layer in qk[:-1]:
        h = x @ layer["w"] + layer["b"]
        h = (h - h.mean(-1, keepdims=True)) / jnp.sqrt(
            h.var(-1, keepdims=True) + LN_EPS
        )
        x = jnp.tanh(h * layer["ln_scale"] + layer["ln_bias"])
    return x @ qk[-1]["w"] + qk[-1]["b"]


def q_values(q, obs: jax.Array, act: jax.Array) -> jax.Array:
    """(Q_1(s, a), Q_2(s, a)), shape (2, batch)."""
    x = jnp.concatenate([obs, act], axis=-1)
    return jnp.stack([critic(qk, x)[..., 0] for qk in q])


def mean_action(params: dict[str, Any], obs: jax.Array) -> jax.Array:
    """The deterministic action tanh(μ_θ(s)), used for evaluation."""
    return jnp.tanh(actor_dist(params["actor"], obs)[0])


# ---------------------------------------------------------------------------
# The three losses. Each takes the full parameter tree but, through
# stop_gradient, sends gradient only to its own parameters
# (tests/test_sac.py::test_gradient_destinations).
# ---------------------------------------------------------------------------


def soft_target(params, target_q, batch, key, gamma: float) -> jax.Array:
    """y = r + γ·b·(min_k Q̄_k(s', a') − α log π(a'|s')),  a' ~ π_θ(·|s').

    * Soft Bellman equation [A p.150–151]: the next state's value includes the
      entropy bonus −α log π, estimated with one fresh action a' from the
      **current** policy (SAC keeps no target policy [A p.159]).
    * Clipped double Q [A p.137]: the min of two critics counters the
      maximisation bias [F p.342] of bootstrapping from a noisy max.
    * Target networks Q̄ [A p.106]: y does not move with w, so the critic
      step is an ordinary regression.
    * ``bootstrap`` b = 0 removes the future after a genuine terminal only.

    The whole target is detached: neither θ, α nor w receives gradient from y.
    (The slides write −αγ log π outside the min; it is the same expression.)
    """
    a2, logp2 = sample_action(params["actor"], batch["next_obs"], key)
    q_next = jnp.min(q_values(target_q, batch["next_obs"], a2), axis=0)
    soft_v = q_next - jnp.exp(params["log_alpha"]) * logp2
    return stop_gradient(batch["reward"] + gamma * batch["bootstrap"] * soft_v)


def critic_loss(params, target_q, batch, key, gamma: float):
    """L_Q = Σ_k mean (y − Q_k(s, a))² over both critics [A p.155]."""
    y = soft_target(params, target_q, batch, key, gamma)
    q = q_values(params["q"], batch["obs"], batch["act"])
    per_q = jnp.mean((q - y) ** 2, axis=1)
    stats = {"q1_loss": per_q[0], "q2_loss": per_q[1]}
    # q_max/q_min: logged since M4X for its entropy-adjusted critic-scale test.
    scale = {"q_mean": q.mean(), "q_abs_max": jnp.abs(q).max()}
    return per_q.sum(), {**stats, **scale, "q_max": q.max(), "q_min": q.min()}


def actor_loss(params, batch, key):
    """L_π = mean (α log π_θ(ã|s) − min_k Q_k(s, ã)) [A p.155, p.158], minimised.

    This is −V^π(s) of [A p.152] under the reparameterisation [A p.154]. The
    critics are frozen with stop_gradient on their *parameters*, but ã itself
    is not detached, so ∂Q/∂ã still carries the gradient to θ. α is a
    constant here.
    """
    a, logp = sample_action(params["actor"], batch["obs"], key)
    q = jnp.min(q_values(stop_gradient(params["q"]), batch["obs"], a), axis=0)
    alpha = stop_gradient(jnp.exp(params["log_alpha"]))
    loss = jnp.mean(alpha * logp - q)
    stats = {"entropy": -logp.mean(), "saturation": jnp.mean(jnp.abs(a) > 0.99)}
    return loss, {**stats, "actor_loss": loss}


def alpha_loss(params, batch, key, target_entropy: float):
    """L(α) = mean −α (log π(ã|s) + H_min) [A p.157–158].

    Dual of the constraint E[−log π] ≥ H_min [A p.156]. With the bracket
    detached, ∂L/∂α = −mean(log π + H_min) = entropy − H_min (we step log α,
    whose gradient is α times that: same sign). When the
    policy's entropy is *below* H_min the gradient is negative, so descent
    *raises* α and the actor is pushed to explore more (and vice versa).
    """
    _, logp = sample_action(stop_gradient(params["actor"]), batch["obs"], key)
    loss = jnp.mean(
        -jnp.exp(params["log_alpha"]) * stop_gradient(logp + target_entropy)
    )
    return loss, {"alpha_loss": loss}


def polyak(target, online, tau: float):
    """w̄ ← (1 − τ)·w̄ + τ·w [A p.109]. The slides' m is 1 − τ (0.995 here)."""
    return jax.tree.map(lambda t, o: (1.0 - tau) * t + tau * o, target, online)


# ---------------------------------------------------------------------------
# One update = steps 2–6 of [A p.158]
# ---------------------------------------------------------------------------


def make_update(cfg: dict[str, Any], gamma: float):
    """Returns ``(init_opt, update)``. ``update(state, batch)`` does one
    critic, one actor and one temperature step, then the Polyak update.

    ``state`` = ``{"params", "target_q", "opt", "key"}``. Each part has its own
    Adam (with plan §4.5 gradient-norm clipping), stepped in the slide order:
    critics first, then the actor against the just-updated critics.
    """
    opt = make_optimizer(cfg)

    def init_opt(params):
        return {k: opt.init(params[k]) for k in ("q", "actor", "log_alpha")}

    def step(params, opt_state, part, loss_fn, *args):
        (loss, stats), grads = jax.value_and_grad(loss_fn, has_aux=True)(params, *args)
        updates, opt_state[part] = opt.update(grads[part], opt_state[part])
        params = {**params, part: optax.apply_updates(params[part], updates)}
        return params, {**stats, f"{part}_grad_norm": optax.tree.norm(grads[part])}

    @jax.jit
    def update(state, batch):
        params, target_q, opt_state = (
            state["params"],
            state["target_q"],
            dict(state["opt"]),
        )
        key, kq, kpi, ka = jax.random.split(state["key"], 4)
        params, sq = step(
            params, opt_state, "q", critic_loss, target_q, batch, kq, gamma
        )
        params, sp = step(params, opt_state, "actor", actor_loss, batch, kpi)
        params, sa = step(
            params, opt_state, "log_alpha", alpha_loss, batch, ka, cfg["target_entropy"]
        )
        target_q = polyak(target_q, params["q"], cfg["tau"])
        stats = {**sq, **sp, **sa, "alpha": jnp.exp(params["log_alpha"])}
        new = {"params": params, "target_q": target_q, "opt": opt_state, "key": key}
        return new, stats

    return init_opt, update


# ---------------------------------------------------------------------------
# Checkpoints and the evaluation controller
# ---------------------------------------------------------------------------


def load(path: Path) -> tuple[dict[str, Any], dict[str, Any]]:
    """Inverse of ``ppo.save`` for an SAC parameter tree (target Q not saved)."""
    meta = json.loads(path.with_suffix(".json").read_text())
    cfg = meta["config"]
    layernorm = cfg.get("critic_layernorm", False)  # absent before M4X: off
    template = init(
        jax.random.PRNGKey(0), meta["obs_dim"], 2, cfg["hidden"], 0.2, layernorm
    )
    with np.load(path.with_suffix(".npz")) as z:
        leaves = [jnp.asarray(z[f"arr_{i}"]) for i in range(len(z.files))]
    for a, b in zip(leaves, jax.tree.leaves(template), strict=True):
        assert a.shape == b.shape, (a.shape, b.shape)
    return jax.tree.unflatten(jax.tree.structure(template), leaves), meta


_mean_action = jax.jit(mean_action)


class SACPolicy(PPOPolicy):
    """A saved SAC actor as an ``evaluate.run_episode`` controller.

    Deterministic: executes tanh(μ_θ(s)), the mode of the pre-squash Gaussian.
    Shares PPOPolicy's observation path so both see identical features.
    """

    _act_fn = staticmethod(_mean_action)

    @staticmethod
    def load(path: Path | str) -> SACPolicy:
        path = Path(path)
        params, meta = load(path)
        return SACPolicy(params, meta, str(path))

    def identity(self) -> dict[str, Any]:
        return {**super().identity(), "controller": "sac", "version": SAC_VERSION}


__all__ = ["Replay", "SACPolicy", "init", "load", "make_update", "save"]
