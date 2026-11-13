"""Learner entry point: calibration, collection, PPO updates, checkpoints.

Plan §4.1, §4.5 and E1.

Usage::

    uv run --frozen --no-sync python experiments/ese3990/train.py --seed 0 \
        --decisions 100000 --checkpoints 10000,25000,50000,75000,100000 \
        --out runs/ese3990/ppo/R_pilot_s0

A run directory holds ``config.json``, ``metrics.jsonl`` (one line per PPO
update), ``curve.jsonl`` (one line per checkpoint: validation outcomes),
``ckpt/<decisions>.{npz,json}`` and ``store/`` — every real episode the run
touched (calibration, training, checkpoint evaluations), replayable with
``viz/show.py`` (plan §10).

The budget counts **every real decision**, calibration included (plan §4.5).
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path
from typing import Any

import jax
import numpy as np
import ppo
from baselines import SmoothRandom
from evaluate import run_episode, summarize
from make_conditions import TrainingSampler
from sailing import (
    NOMINAL,
    RUNNING,
    Task,
    TaskEnv,
    TransitionReward,
    features,
    load_manifest,
    run_metadata,
)
from viz.episode_store import EpisodeRecorder, EpisodeWriter

# Plan §4.5 starting values. Frozen for M1 (docs/ese3990/M1_report.md); any
# change before the final runs is recorded there.
CONFIG: dict[str, Any] = {
    "task": "R",
    "decisions": 500_000,
    "checkpoints": [10_000, 100_000, 250_000, 500_000],
    "calibration_decisions": 10_000,
    "calibration_hold": 10,
    "norm_clip": 5.0,
    "std_floor": 1e-2,
    "rollout": 2048,
    "minibatch": 256,
    "epochs": 4,
    "clip": 0.2,
    "lam": 0.95,
    "lr": 3e-4,
    "max_grad_norm": 1.0,
    "vf_coef": 0.5,
    "ent_coef": 0.001,
    "log_std": -0.5,
    "hidden": 64,
}
PROBE_IDS = ("nominal", *[c.id for c in load_manifest("validation")[:5]])
"""The probe set (plan §10.2): nominal plus the first 5 validation documents."""


class Collector:
    """Explicit-reset, single-environment collection (plan §3.2).

    Episodes carry across rollout boundaries. Every episode is logged to the
    store when it ends (or, if still running, on :meth:`close`), so every
    training rollout replays (plan §10.2).
    """

    def __init__(self, task, sampler, writer, meta: dict[str, Any], dtype: str) -> None:
        self.task, self.sampler, self.writer = task, sampler, writer
        self.meta, self.dtype = meta, dtype
        self.episodes = 0
        self._begin()

    def _begin(self) -> None:
        self.env = TaskEnv(self.task, self.sampler.next(), seed=self.episodes)
        self.rs = self.env.reset()
        self.recorder = EpisodeRecorder(self.env, self.dtype)
        self.recorder.start(self.rs)
        self.ret = 0.0

    def _log(self) -> None:
        self.writer.add(self.recorder.finish(**self.meta))

    def run(self, n: int, choose) -> tuple[dict[str, np.ndarray], list]:
        """``n`` real decisions; ``choose(rs) -> (action, extra)``."""
        keys = ("rs", "next_rs", "reward", "bootstrap", "cont")
        rows: dict[str, list] = {k: [] for k in keys}
        extras, ended = [], []
        for _ in range(n):
            a, extra = choose(self.rs)
            rec = self.env.step(a)
            self.recorder.step(rec)
            for k in keys:
                rows[k].append(getattr(rec, k))
            extras.append(extra)
            self.ret += rec.reward
            self.rs = rec.next_rs
            if rec.outcome != RUNNING:
                self._log()
                ended.append({"outcome": rec.outcome, "return": self.ret})
                self.episodes += 1
                self._begin()
        data = {k: np.asarray(v) for k, v in rows.items()}
        data["extra"] = np.asarray(extras)
        return data, ended

    def close(self) -> None:
        if self.recorder._k:  # log the cut-off episode too; it replays as "running"
            self._log()


def append_jsonl(path: Path, row: dict[str, Any]) -> None:
    with open(path, "a") as f:
        f.write(json.dumps(row) + "\n")


def calibrate(
    task: Task, seed: int, cfg: dict[str, Any], sampler, writer
) -> dict[str, Any]:
    """10k smooth-random decisions → frozen feature normalisation (plan §4.5).

    A function of the training seed only, so PPO and SAC see identical data.
    """
    rnd = SmoothRandom(seed, cfg["calibration_hold"])
    meta = {"controller": "smooth_random", "kind": "calibration"}
    col = Collector(task, sampler, writer, meta, "float64")
    data, _ = col.run(cfg["calibration_decisions"], lambda rs: (rnd.act(None), 0.0))
    col.close()
    return ppo.normalization_from(features(data["rs"], task.target), cfg["std_floor"])


def evaluate_checkpoint(task, policy, decisions, writer) -> dict[str, Any]:
    """Deterministic policy on nominal + the 20 validation documents.

    All are stored; the probe subset is tagged ``kind="probe"`` (plan §10.2).
    Test documents stay closed (plan E3).
    """
    eps = []
    for c in [NOMINAL, *load_manifest("validation")]:
        kind = "probe" if c.id in PROBE_IDS else "validation"
        eps.append(
            run_episode(
                task,
                c,
                policy,
                writer=writer,
                log_meta={"kind": kind, "checkpoint": decisions},
            )
        )
    val = [e for e in eps if e["condition"]["id"] != "nominal"]
    return {
        "decisions": decisions,
        "nominal_outcome": eps[0]["outcome"],
        "nominal_return": eps[0]["return"],
        "val_mean_return": float(np.mean([e["return"] for e in val])),
        **{f"val_{k}": v for k, v in summarize(val).items()},
    }


def train(seed: int, out: Path, cfg: dict[str, Any]) -> Path:
    out.mkdir(parents=True, exist_ok=True)
    task = Task.load(cfg["task"])
    gamma = TransitionReward(task).gamma  # the task's discount, not a PPO knob
    meta0 = run_metadata(
        task,
        learner="ppo",
        ppo_version=ppo.PPO_VERSION,
        training_seed=seed,
        config=cfg,
        probe_ids=list(PROBE_IDS),
    )
    (out / "config.json").write_text(json.dumps(meta0, indent=1))
    writer = EpisodeWriter(out / "store", {"learner": "ppo", "training_seed": seed})
    sampler = TrainingSampler(seed)
    t0 = time.perf_counter()

    norm = calibrate(task, seed, cfg, sampler, writer)
    decisions = cfg["calibration_decisions"]
    norm_obs = lambda rs: ppo.normalize(rs, task.target, norm, cfg["norm_clip"])  # noqa: E731

    key = jax.random.PRNGKey(seed)
    params = ppo.init(key, len(norm["mean"]), 2, cfg["hidden"], cfg["log_std"])
    opt, update = ppo.make_update(cfg)
    opt_state = opt.init(params)
    rng = np.random.default_rng([seed, 1])  # exploration noise and minibatch order
    actor_mean = jax.jit(lambda p, o: ppo.mlp(p["actor"], o))
    log_prob = jax.jit(ppo.log_prob)
    value = jax.jit(ppo.value)

    pending = sorted(cfg["checkpoints"])
    col = Collector(
        task, sampler, writer, {"controller": "ppo", "kind": "training"}, "float32"
    )
    timing = {"collect_s": 0.0, "update_s": 0.0, "eval_s": 0.0}

    def checkpoint() -> None:
        ts = time.perf_counter()
        meta = {
            **meta0,
            "decisions": decisions,
            "obs_dim": len(norm["mean"]),
            "normalization": norm,
            "wall_s": time.perf_counter() - t0,
        }
        path = out / "ckpt" / f"{decisions:07d}"
        ppo.save(path, params, meta)
        row = evaluate_checkpoint(
            task, ppo.PPOPolicy(params, meta, str(path)), decisions, writer
        )
        append_jsonl(out / "curve.jsonl", row)
        timing["eval_s"] += time.perf_counter() - ts
        print(
            f"[ckpt {decisions:>7}] val success {row['val_success_rate']:.2f} "
            f"return {row['val_mean_return']:+.2f} nominal {row['nominal_outcome']}",
            flush=True,
        )

    while True:
        while pending and decisions >= pending[0]:
            pending.pop(0)
            checkpoint()
        if decisions >= cfg["decisions"]:
            break
        n = min(cfg["rollout"], cfg["decisions"] - decisions)

        # 1. Roll out π_θ: u ~ N(μ_θ(s), σ²), execute a = tanh(u) [A p.57].
        ts = time.perf_counter()
        std = np.exp(np.asarray(params["log_std"]))

        def choose(rs, params=params, std=std):
            u = np.asarray(
                actor_mean(params, norm_obs(rs))
            ) + std * rng.standard_normal(2)
            u = u.astype(np.float32)
            return np.tanh(u), u

        data, ended = col.run(n, choose)
        decisions += n
        obs, next_obs = norm_obs(data["rs"]), norm_obs(data["next_rs"])
        # log π_θ(a|s) and V(s), V(s') for the whole batch, with the same
        # functions the loss uses, so the first ratio is exactly 1.
        u = data["extra"]
        logp = np.asarray(log_prob(params, obs, u))
        v, v_next = np.asarray(value(params, obs)), np.asarray(value(params, next_obs))
        timing["collect_s"] += time.perf_counter() - ts

        # 2. GAE with the plan §3.4 masks [F p.515–516].
        adv, ret = ppo.gae(
            data["reward"],
            v,
            v_next,
            data["bootstrap"],
            data["cont"],
            gamma,
            cfg["lam"],
        )

        # 3. Epochs of minibatch PPO-Clip updates [A p.55, p.57].
        ts = time.perf_counter()
        batch = {
            "obs": obs,
            "u": u,
            "logp": logp,
            "adv": adv.astype(np.float32),
            "ret": ret.astype(np.float32),
        }
        params, opt_state, stats = update(params, opt_state, batch, rng)
        timing["update_s"] += time.perf_counter() - ts

        fin = [e for e in ended if e["outcome"] == "finished"]
        append_jsonl(
            out / "metrics.jsonl",
            {
                "decisions": decisions,
                "wall_s": time.perf_counter() - t0,
                "episodes_ended": len(ended),
                "train_success_rate": len(fin) / len(ended) if ended else None,
                "train_mean_return": float(np.mean([e["return"] for e in ended]))
                if ended
                else None,
                "std": np.exp(np.asarray(params["log_std"])).tolist(),
                "saturation": float(np.mean(np.abs(np.tanh(u)) > 0.99)),
                **stats,
                **timing,
            },
        )

    col.close()
    writer.close()
    summary = {
        "decisions": decisions,
        "wall_s": time.perf_counter() - t0,
        **timing,
        "training_episodes": col.episodes,
    }
    (out / "summary.json").write_text(json.dumps(summary, indent=1))
    print(json.dumps(summary, indent=1))
    return out


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--decisions", type=int, default=CONFIG["decisions"])
    ap.add_argument("--checkpoints", help="comma-separated decision counts")
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument(
        "--set",
        action="append",
        default=[],
        metavar="KEY=JSON",
        help="override a CONFIG entry, e.g. --set log_std=0.0 (pilots only)",
    )
    args = ap.parse_args(argv)
    cfg = {**CONFIG, "decisions": args.decisions}
    if args.checkpoints:
        cfg["checkpoints"] = [int(x) for x in args.checkpoints.split(",")]
    for kv in args.set:
        k, v = kv.split("=", 1)
        if k not in CONFIG:
            raise SystemExit(f"unknown config key {k!r}")
        cfg[k] = json.loads(v)
    train(args.seed, args.out, cfg)
    return 0


if __name__ == "__main__":
    sys.exit(main())
