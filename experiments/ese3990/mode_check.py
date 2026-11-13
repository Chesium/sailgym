"""Is RuleSailor's manoeuvre mode predictable from the 17 learner features?
(plan M4U, optional mode check)

    env -u PYTHONPATH uv run --frozen --no-sync python \
        experiments/ese3990/mode_check.py STORE [STORE ...] --out MODE.json

RuleSailor is re-run along each stored episode's exact replay. At every
decision its emitted action must equal the stored one bit for bit, so the
recorded mode sequence is the one that generated the data. A softmax
classifier is then fitted on an episode-wise split, in two sizes: linear, and
the learners' own 2x64 tanh MLP (``ppo.mlp``). If the modes the classifier
cannot recover carry the actions, RuleSailor's memory holds information the
learners' features lack.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

import jax
import jax.numpy as jnp
import numpy as np
import optax
from baselines import RuleSailorController
from ppo import mlp, mlp_init
from sailing import FEATURE_NAMES, Condition, Task, TaskEnv, features
from viz.episode_store import EpisodeStore, state_checksum
from viz.replay import ReplayMismatch, identity_problems


def rule_modes(
    store: EpisodeStore, entry: dict[str, Any]
) -> tuple[np.ndarray, list[str]]:
    """Research states at each decision ``(n, len(RS))`` and RuleSailor's mode
    after deciding there, i.e. the mode that produced the stored action."""
    if entry["controller"] != "rule_sailor":
        raise ValueError(f"{entry['episode_id']} is not a RuleSailor episode")
    if probs := identity_problems(store.identity):
        raise ReplayMismatch("; ".join(probs))
    task = Task.load(entry["task"], entry.get("contract_version", 1))
    env = TaskEnv(task, Condition.from_dict(entry["condition"]), entry["seed"])
    states = [env.reset()]
    ctrl = RuleSailorController()
    ctrl.reset(env)
    modes = []
    for k, a in enumerate(store.load(entry).actions.astype(np.float64)):
        # The clip is ActionAdapter's: the store holds what was sent.
        if not np.array_equal(np.clip(ctrl.act(env)[:2], -1.0, 1.0), a):
            raise ReplayMismatch(f"{entry['episode_id']}: action {k} differs")
        modes.append(ctrl.inner.mode)
        states.append(env.step(a).next_rs)
    if state_checksum(env.research_state()) != entry["final_checksum"]:
        raise ReplayMismatch(f"{entry['episode_id']}: final checksum differs")
    return np.stack(states[:-1]), modes


def fit(
    x: np.ndarray, y: np.ndarray, sizes: list[int], steps: int, seed: int = 0
) -> list[dict[str, jax.Array]]:
    """Full-batch Adam on softmax cross-entropy."""
    params = mlp_init(jax.random.PRNGKey(seed), sizes)
    opt = optax.adam(3e-3)
    state = opt.init(params)
    x, y = jnp.asarray(x), jnp.asarray(y)

    def loss(p):
        return optax.softmax_cross_entropy_with_integer_labels(mlp(p, x), y).mean()

    @jax.jit
    def step(p, s):
        g = jax.grad(loss)(p)
        u, s = opt.update(g, s)
        return optax.apply_updates(p, u), s

    for _ in range(steps):
        params, state = step(params, state)
    return params


def scores(
    y: np.ndarray, pred: np.ndarray, names: list[str], scored: np.ndarray
) -> dict[str, Any]:
    """Balanced accuracy is the mean recall over ``scored`` modes: those in
    both splits, since a mode never seen in training cannot be predicted."""
    recall = {names[c]: float(np.mean(pred[y == c] == c)) for c in np.unique(y)}
    return {
        "accuracy": float(np.mean(pred == y)),
        "balanced_accuracy": float(np.mean([recall[names[c]] for c in scored])),
        "recall": recall,
    }


def mode_check(
    stores: list[Path], test_frac: float = 0.3, steps: int = 3000, seed: int = 0
) -> dict[str, Any]:
    eps = []
    for path in stores:
        s = EpisodeStore(path)
        for e in s.index:
            st, modes = rule_modes(s, e)
            task = Task.load(e["task"], e.get("contract_version", 1))
            eps.append((f"{path}:{e['episode_id']}", features(st, task.target), modes))
    names = sorted({m for _, _, ms in eps for m in ms})
    order = np.random.default_rng(seed).permutation(len(eps))
    n_test = max(1, round(test_frac * len(eps)))
    split = {"test": order[:n_test], "train": order[n_test:]}

    def stack(idx):
        x = np.concatenate([eps[i][1] for i in idx])
        y = np.array([names.index(m) for i in idx for m in eps[i][2]])
        return x, y

    (xtr, ytr), (xte, yte) = stack(split["train"]), stack(split["test"])
    mu, sd = xtr.mean(0), np.maximum(xtr.std(0), 1e-6)
    ztr, zte = (xtr - mu) / sd, (xte - mu) / sd
    majority = np.bincount(ytr, minlength=len(names)).argmax()
    scored = np.intersect1d(ytr, yte)
    res: dict[str, Any] = {
        "stores": [str(p) for p in stores],
        "episodes": {k: [eps[i][0] for i in v] for k, v in split.items()},
        "decisions": {"train": len(ytr), "test": len(yte)},
        "features": FEATURE_NAMES,
        "modes": names,
        "mode_share_test": {names[c]: float(np.mean(yte == c)) for c in np.unique(yte)},
        "scored_modes": [names[c] for c in scored],
        "test_share_unseen_in_train": float(np.mean(~np.isin(yte, ytr))),
        "majority": scores(yte, np.full_like(yte, majority), names, scored),
    }
    for label, hidden in (("linear", []), ("mlp_2x64", [64, 64])):
        p = fit(ztr, ytr, [len(FEATURE_NAMES), *hidden, len(names)], steps, seed)
        res[label] = {
            "train": scores(ytr, np.asarray(mlp(p, ztr).argmax(-1)), names, scored),
            "test": scores(yte, np.asarray(mlp(p, zte).argmax(-1)), names, scored),
        }
    return res


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("stores", nargs="+", type=Path)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--steps", type=int, default=3000)
    args = ap.parse_args(argv)
    res = mode_check(args.stores, steps=args.steps)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(res, indent=1) + "\n")
    print(json.dumps({k: res[k] for k in res if k != "episodes"}, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
