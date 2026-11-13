"""Learner entry point: calibration, collection, PPO or SAC updates, checkpoints.

Plan §4.1, §4.2, §4.5 and E1.

Usage::

    uv run --frozen --no-sync python experiments/ese3990/train.py --seed 0 \
        --decisions 100000 --checkpoints 10000,25000,50000,75000,100000 \
        --out runs/ese3990/ppo/R_pilot_s0 [--learner sac]

M4U: ``--task U`` selects the U settings; ``--demos <store> --demos-sha256
<hash>`` turns SAC into SAC + demonstrations and PPO into PPO + BC.

A run directory holds ``config.json``, ``metrics.jsonl`` (one line per PPO
update), ``curve.jsonl`` (one line per checkpoint: validation outcomes),
``ckpt/<decisions>.{npz,json}`` and ``store/`` — every real episode the run
touched (calibration, training, checkpoint evaluations), replayable with
``viz/show.py`` (plan §10). PPO + BC adds ``bc.jsonl`` (one line per epoch).

The budget counts **every real decision**, calibration included (plan §4.5),
and any demonstrations (M4U).
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path
from typing import Any

import demos
import jax
import numpy as np
import ppo
import sac
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
# Plan §4.5 SAC starting values. Shared with PPO: calibration, normalisation,
# hidden width, optimizer. Exploration warmup = the 10k calibration decisions,
# which go into replay; updates start right after them.
SAC_CONFIG: dict[str, Any] = {
    **{k: CONFIG[k] for k in ("task", "decisions", "checkpoints", "lr", "hidden")},
    **{k: CONFIG[k] for k in ("calibration_decisions", "calibration_hold")},
    **{k: CONFIG[k] for k in ("norm_clip", "std_floor", "max_grad_norm")},
    "replay": 100_000,
    "minibatch": 256,
    "utd": 1,  # gradient updates per real decision
    "tau": 0.005,
    "alpha0": 0.2,
    "target_entropy": -2.0,  # H_min = −dim(a), on the squashed actions
    "log_every": 2048,  # decisions per metrics.jsonl row (PPO: one per rollout)
}
# M4U (plan §6): U changes only these M2-freeze settings; its γ = 0.99975 is
# the task contract's (configs/tasks_v2.json). Checkpoint 0 is "initial": it
# is saved as soon as calibration and any demonstrations/BC are charged, i.e.
# at 10k decisions from scratch and 10k + D in an expert-data arm.
U_SCHEDULE: dict[str, Any] = {
    "task": "U",
    "decisions": 4_000_000,
    "checkpoints": [0, 400_000, 1_000_000, 2_000_000, 3_000_000, 4_000_000],
}
U_CONFIG = {**CONFIG, **U_SCHEDULE, "arm": "ppo"}
U_SAC_CONFIG = {**SAC_CONFIG, **U_SCHEDULE, "replay": 400_000, "arm": "sac"}
# What an expert-data arm adds to its from-scratch config (plan §6, M4U). The
# SAC minibatch becomes half demonstrations, half online; BC uses PPO's Adam
# lr and gradient clip with its own optimizer state.
DEMO_ARMS: dict[str, dict[str, Any]] = {
    "sac": {"arm": "sac_demo"},
    "ppo": {"arm": "ppo_bc", "bc_epochs": 20, "bc_minibatch": 256},
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
        keys = ("rs", "action", "next_rs", "reward", "bootstrap", "cont")
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
) -> tuple[dict[str, Any], dict[str, np.ndarray]]:
    """10k smooth-random decisions → frozen feature normalisation (plan §4.5).

    A function of the training seed only, so PPO and SAC see identical data.
    Returns the normalisation and the transitions (SAC puts them in replay).
    """
    rnd = SmoothRandom(seed, cfg["calibration_hold"])
    meta = {"controller": "smooth_random", "kind": "calibration"}
    col = Collector(task, sampler, writer, meta, "float64")
    data, _ = col.run(cfg["calibration_decisions"], lambda rs: (rnd.act(None), 0.0))
    col.close()
    norm = ppo.normalization_from(features(data["rs"], task.target), cfg["std_floor"])
    return norm, data


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


def checkpoint(out, task, writer, policy_cls, params, meta) -> None:
    """Save ``params`` and evaluate them on nominal + validation (plan E1)."""
    decisions = meta["decisions"]
    path = out / "ckpt" / f"{decisions:07d}"
    ppo.save(path, params, meta)
    row = evaluate_checkpoint(
        task, policy_cls(params, meta, str(path)), decisions, writer
    )
    append_jsonl(out / "curve.jsonl", row)
    print(
        f"[ckpt {decisions:>7}] val success {row['val_success_rate']:.2f} "
        f"return {row['val_mean_return']:+.2f} nominal {row['nominal_outcome']}",
        flush=True,
    )


def train(
    seed: int, out: Path, cfg: dict[str, Any], demo: dict[str, np.ndarray] | None = None
) -> Path:
    """PPO; with ``demo`` (``demos.load``), PPO + BC: the actor mean and critic
    are first fitted to the demonstrations, which are charged to the budget."""
    start = cfg["calibration_decisions"] + (0 if demo is None else len(demo["reward"]))
    if 0 < (cfg["decisions"] - start) % cfg["rollout"] < cfg["minibatch"]:
        # ppo.make_update needs one full minibatch; fail now, not after the run.
        raise ValueError("the budget's last rollout is shorter than one minibatch")
    out.mkdir(parents=True, exist_ok=False)  # never mix or overwrite recorded runs
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

    norm, _ = calibrate(task, seed, cfg, sampler, writer)
    decisions = cfg["calibration_decisions"]
    norm_obs = lambda rs: ppo.normalize(rs, task.target, norm, cfg["norm_clip"])  # noqa: E731

    key = jax.random.PRNGKey(seed)
    params = ppo.init(key, len(norm["mean"]), 2, cfg["hidden"], cfg["log_std"])
    opt, update = ppo.make_update(cfg)
    opt_state = opt.init(params)
    rng = np.random.default_rng([seed, 1])  # exploration noise and minibatch order
    if demo is not None:
        # opt_state is still Adam's zero state, so PPO starts fresh after BC.
        params, history = ppo.behavior_clone(
            params,
            norm_obs(demo["rs"]),
            demo["action"],
            demo["return_to_go"],
            cfg,
            rng,
        )
        for epoch, row in enumerate(history, 1):
            append_jsonl(out / "bc.jsonl", {"epoch": epoch, **row})
        decisions += len(demo["reward"])
    actor_mean = jax.jit(lambda p, o: ppo.mlp(p["actor"], o))
    log_prob = jax.jit(ppo.log_prob)
    value = jax.jit(ppo.value)

    pending = sorted(cfg["checkpoints"])
    col = Collector(
        task, sampler, writer, {"controller": "ppo", "kind": "training"}, "float32"
    )
    timing = {"collect_s": 0.0, "update_s": 0.0, "eval_s": 0.0}

    while True:
        while pending and decisions >= pending[0]:
            pending.pop(0)
            ts = time.perf_counter()
            meta = {
                **meta0,
                "decisions": decisions,
                "obs_dim": len(norm["mean"]),
                "normalization": norm,
                "wall_s": ts - t0,
            }
            checkpoint(out, task, writer, ppo.PPOPolicy, params, meta)
            timing["eval_s"] += time.perf_counter() - ts
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


def train_sac(
    seed: int, out: Path, cfg: dict[str, Any], demo: dict[str, np.ndarray] | None = None
) -> Path:
    """SAC through the same contract as :func:`train`: identical calibration
    data and normalisation, collector, episode store and checkpoint evaluation.

    The loop is the outer loop of [A p.158] with one real decision per
    iteration: act with a ~ π_θ(·|s), store the transition, then ``utd``
    gradient updates on minibatches from replay. With ``demo``
    (``demos.load``), SAC + demonstrations: every minibatch is half
    demonstrations (``sac.symmetric_sample``), which are charged to the budget.
    """
    out.mkdir(parents=True, exist_ok=False)  # never mix or overwrite recorded runs
    task = Task.load(cfg["task"])
    gamma = TransitionReward(task).gamma
    meta0 = run_metadata(
        task,
        learner="sac",
        sac_version=sac.SAC_VERSION,
        training_seed=seed,
        config=cfg,
        probe_ids=list(PROBE_IDS),
    )
    (out / "config.json").write_text(json.dumps(meta0, indent=1))
    writer = EpisodeWriter(out / "store", {"learner": "sac", "training_seed": seed})
    sampler = TrainingSampler(seed)
    t0 = time.perf_counter()

    norm, calib = calibrate(task, seed, cfg, sampler, writer)
    decisions = cfg["calibration_decisions"]
    norm_obs = lambda rs: ppo.normalize(rs, task.target, norm, cfg["norm_clip"])  # noqa: E731
    replay = sac.Replay(cfg["replay"], len(norm["mean"]))

    def store(buf, data):
        buf.add(
            obs=norm_obs(data["rs"]),
            act=data["action"][:, :2],  # the executed rates (release dropped)
            reward=data["reward"],
            next_obs=norm_obs(data["next_rs"]),
            bootstrap=data["bootstrap"],
            cont=data["cont"],
        )

    store(replay, calib)  # calibration doubles as the exploration warmup (plan §4.5)
    if demo is not None:
        demo_buf = sac.Replay(len(demo["reward"]), len(norm["mean"]))
        store(demo_buf, demo)  # exactly full; never added to again
        decisions += len(demo["reward"])

    key = jax.random.PRNGKey(seed)
    params = sac.init(key, len(norm["mean"]), 2, cfg["hidden"], cfg["alpha0"])
    init_opt, update = sac.make_update(cfg, gamma)
    state = {
        "params": params,
        "target_q": params["q"],  # Q̄ starts equal to Q
        "opt": init_opt(params),
        "key": jax.random.fold_in(key, 1),
    }
    rng = np.random.default_rng([seed, 1])  # exploration noise and minibatches
    actor_dist = jax.jit(sac.actor_dist)

    pending = sorted(cfg["checkpoints"])
    col = Collector(
        task, sampler, writer, {"controller": "sac", "kind": "training"}, "float32"
    )
    timing = {"collect_s": 0.0, "update_s": 0.0, "eval_s": 0.0}
    window: list[dict] = []
    ended_window: list[dict] = []

    def choose(rs):
        # a = tanh(μ_θ(s) + σ_θ(s)·ε): the same stochastic actor the loss uses.
        mu, log_std = (
            np.asarray(x) for x in actor_dist(state["params"]["actor"], norm_obs(rs))
        )
        u = (mu + np.exp(log_std) * rng.standard_normal(2)).astype(np.float32)
        return np.tanh(u), 0.0

    while True:
        while pending and decisions >= pending[0]:
            pending.pop(0)
            ts = time.perf_counter()
            meta = {
                **meta0,
                "decisions": decisions,
                "obs_dim": len(norm["mean"]),
                "normalization": norm,
                "wall_s": ts - t0,
            }
            checkpoint(out, task, writer, sac.SACPolicy, state["params"], meta)
            timing["eval_s"] += time.perf_counter() - ts
        if decisions >= cfg["decisions"]:
            break

        ts = time.perf_counter()
        data, ended = col.run(1, choose)
        store(replay, data)
        decisions += 1
        ended_window += ended
        timing["collect_s"] += time.perf_counter() - ts

        ts = time.perf_counter()
        for _ in range(cfg["utd"]):
            if demo is None:
                batch = replay.sample(rng, cfg["minibatch"])
            else:
                batch = sac.symmetric_sample(rng, demo_buf, replay, cfg["minibatch"])
            state, stats = update(state, batch)
            window.append(stats)
        timing["update_s"] += time.perf_counter() - ts

        if decisions % cfg["log_every"] == 0 or decisions == cfg["decisions"]:
            stats = jax.device_get(window)
            fin = [e for e in ended_window if e["outcome"] == "finished"]
            append_jsonl(
                out / "metrics.jsonl",
                {
                    "decisions": decisions,
                    "wall_s": time.perf_counter() - t0,
                    "episodes_ended": len(ended_window),
                    "train_success_rate": len(fin) / len(ended_window)
                    if ended_window
                    else None,
                    "train_mean_return": float(
                        np.mean([e["return"] for e in ended_window])
                    )
                    if ended_window
                    else None,
                    "replay_size": replay.size,
                    **{k: float(np.mean([s[k] for s in stats])) for k in stats[0]},
                    "q_abs_max": float(np.max([s["q_abs_max"] for s in stats])),
                    **timing,
                },
            )
            window, ended_window = [], []

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
    ap.add_argument("--learner", choices=["ppo", "sac"], default="ppo")
    ap.add_argument("--task", choices=["R", "U"], default="R")
    ap.add_argument("--demos", type=Path, help="expert-data arm: a demos.py store")
    ap.add_argument("--demos-sha256", help="that store's frozen hash, verified")
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--decisions", type=int)
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
    base = {
        ("ppo", "R"): CONFIG,
        ("sac", "R"): SAC_CONFIG,
        ("ppo", "U"): U_CONFIG,
        ("sac", "U"): U_SAC_CONFIG,
    }[args.learner, args.task]
    cfg = {**base, "decisions": args.decisions or base["decisions"]}
    if args.checkpoints:
        cfg["checkpoints"] = [int(x) for x in args.checkpoints.split(",")]
    for kv in args.set:
        k, v = kv.split("=", 1)
        if k not in base:
            raise SystemExit(f"unknown config key {k!r}")
        cfg[k] = json.loads(v)
    demo = None
    if args.demos:
        demo = demos.load(args.demos, args.demos_sha256, Task.load(cfg["task"]))
        cfg.update(DEMO_ARMS[args.learner])
        cfg["demos"] = {
            "store": str(args.demos),
            "sha256": args.demos_sha256,
            "decisions": len(demo["reward"]),
        }
    (train_sac if args.learner == "sac" else train)(args.seed, args.out, cfg, demo)
    return 0


if __name__ == "__main__":
    sys.exit(main())
