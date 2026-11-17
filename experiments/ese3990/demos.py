"""RuleSailor demonstrations for M4U's expert-data arms (plan §6, M4U).

Usage::

    env -u PYTHONPATH uv run --frozen --no-sync python experiments/ese3990/demos.py \
        --out runs/ese3990/M4U/demos/store

The demonstrations are an ordinary episode store (``kind="demo"``): 50
RuleSailor episodes on ``TrainingSampler(1000)`` conditions (training
distribution, held-out documents excluded), environment seed = episode index.
Every outcome is kept. :func:`load` turns the store back into transitions by
exact replay; there is no second data format. The store hash printed here is
what a training run is given and verifies.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from collections import Counter
from pathlib import Path

import numpy as np
from baselines import RuleSailorController
from evaluate import run_episode
from make_conditions import TrainingSampler
from ppo import gae
from sailing import OUTCOME_CODE, RUNNING, Task, TransitionReward, masks
from viz.episode_store import EpisodeStore, EpisodeWriter
from viz.replay import ReplayMismatch, replay

SAMPLER_SEED = 1000
EPISODES = 50


def collect(out: Path, task: Task, episodes: int = EPISODES) -> str:
    """Record ``episodes`` RuleSailor episodes into a new store; returns its hash."""
    out.mkdir(
        parents=True, exist_ok=False
    )  # a frozen demonstration set is never appended to
    sampler = TrainingSampler(SAMPLER_SEED)
    with EpisodeWriter(
        out, {"kind": "demo", "controller": "rule_sailor", "sampler_seed": SAMPLER_SEED}
    ) as w:
        for i in range(episodes):
            run_episode(
                task,
                sampler.next(),
                RuleSailorController(),
                seed=i,
                writer=w,
                log_meta={"kind": "demo"},
            )
    return store_sha256(out)


def store_sha256(store: Path) -> str:
    """SHA-256 over run.json, index.jsonl and the shards, in that order."""
    h = hashlib.sha256()
    for p in [
        store / "run.json",
        store / "index.jsonl",
        *sorted(store.glob("shard-*.npz")),
    ]:
        h.update(p.name.encode())
        h.update(p.read_bytes())
    return h.hexdigest()


def load(
    store: Path, sha256: str, task: Task, kind: str = "demo"
) -> dict[str, np.ndarray]:
    """Every demonstration transition, by exact replay, in store order.

    ``kind="evaluation"`` reads an evaluation store the same way (M4X's V
    check on RuleSailor's validation episodes).

    Keys: ``rs``, ``action`` (the executed two rates), ``reward``, ``next_rs``,
    ``bootstrap``, ``cont`` (as ``Collector.run``) and ``return_to_go``. The
    demonstration decision count is ``len(reward)``.
    """
    if store_sha256(store) != sha256:
        raise ValueError(f"{store}: store hash differs from {sha256}")
    s = EpisodeStore(store)
    reward_fn = TransitionReward(task)
    keys = ("rs", "action", "reward", "next_rs", "bootstrap", "cont")
    parts: dict[str, list[np.ndarray]] = {k: [] for k in keys}
    for e in s.index:
        got = (e["kind"], e["task"], e.get("contract_version", 1))
        if got != (kind, task.name, task.contract_version):
            raise ValueError(f"{e['episode_id']}: {got} is not a {kind} of this task")
        # replay checks outcome, physics steps, return and final checksum.
        traj = replay(s.load(e), s.identity, task)
        n = e["decisions"]
        code = np.full(n, OUTCOME_CODE[RUNNING])
        code[-1] = OUTCOME_CODE[e["outcome"]]
        # Only the terminal decision can run short of the cadence.
        steps = np.full(n, task.cadence)
        steps[-1] = e["physics_steps"] - task.cadence * (n - 1)
        r = reward_fn.batch(traj.states[:-1], traj.states[1:], code, steps)
        if np.cumsum(r)[-1] != e["return"]:  # sequential, as the recorder summed
            raise ReplayMismatch(f"{e['episode_id']}: rewards do not sum to the log")
        b, c = np.ones(n), np.ones(n)
        b[-1], c[-1] = masks(e["outcome"])
        for k, v in zip(
            keys,
            (traj.states[:-1], traj.actions, r, traj.states[1:], b, c),
            strict=True,
        ):
            parts[k].append(v)
    data = {k: np.concatenate(v) for k, v in parts.items()}
    # Discounted return-to-go within each episode: GAE with V ≡ 0 and λ = 1 is
    # the Monte Carlo return. A timeout's is the truncated sum, with no
    # bootstrap from its final state (there is no value function to use).
    zero = np.zeros(len(data["reward"]))
    data["return_to_go"], _ = gae(
        data["reward"],
        zero,
        zero,
        data["bootstrap"],
        data["cont"],
        reward_fn.gamma,
        1.0,
    )
    return data


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args(argv)
    sha = collect(args.out, Task.load("U"))
    index = EpisodeStore(args.out).index
    report = {
        "store": str(args.out),
        "sha256": sha,
        "episodes": len(index),
        "decisions": sum(e["decisions"] for e in index),
        "outcomes": dict(Counter(e["reason"] or e["outcome"] for e in index)),
    }
    print(json.dumps(report, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
