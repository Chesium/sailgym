"""Exact re-simulation of a stored episode (§10.1).

``replay`` rebuilds the episode from (task, condition, seed, actions) through
the same ``TaskEnv`` every controller uses and checks it against the log:
outcome, executed physics steps, return and the final-state checksum. A store
written by a different physics build or contract is refused; ``pose_only``
gives the stored 5 Hz poses instead, labelled as such.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

import numpy as np
from sailing import RS, RS_NAMES, Condition, Task, TaskEnv

from viz.episode_store import POSE_FIELDS, EpisodeLog, build_identity, state_checksum


class ReplayMismatch(RuntimeError):
    """The log cannot be reproduced on this build: refuse rather than guess."""


@dataclass
class Trajectory:
    """Research states at every decision boundary, ``states[0]`` the reset."""

    meta: dict[str, Any]
    states: (
        np.ndarray
    )  # (decisions + 1, len(RS_NAMES)); pose-only: NaN outside POSE_FIELDS
    actions: np.ndarray  # (decisions, 2)
    exact: bool  # False when built from the stored low-rate pose

    @property
    def t(self) -> np.ndarray:
        return self.states[:, RS["t"]]

    def col(self, name: str) -> np.ndarray:
        return self.states[:, RS[name]]


def identity_problems(stored: dict[str, Any]) -> list[str]:
    now = build_identity()
    probs = [
        f"{k}: stored {stored.get(k)!r}, now {now[k]!r}"
        for k in now
        if stored.get(k) != now[k]
    ]
    return probs


def replay(
    log: EpisodeLog, store_identity: dict[str, Any], task: Task | None = None
) -> Trajectory:
    probs = identity_problems(store_identity)
    if probs:
        raise ReplayMismatch(
            "store identity differs from this build: " + "; ".join(probs)
        )
    m = log.meta
    version = m.get(
        "contract_version", 1
    )  # the contract the episode was recorded under
    if task is None or task.contract_version != version:
        task = Task.load(m["task"], version)
    env = TaskEnv(task, Condition.from_dict(m["condition"]), m["seed"])
    states = [env.reset()]
    ret = 0.0
    steps = 0
    for a in log.actions:
        if env.outcome != "running":
            raise ReplayMismatch(
                "the replayed episode ended before its actions ran out"
            )
        rec = env.step(np.asarray(a, dtype=np.float64))
        states.append(rec.next_rs)
        ret += rec.reward
        steps += rec.steps
    final = env.research_state()
    checks = {
        "outcome": (env.outcome, m["outcome"]),
        "physics_steps": (steps, m["physics_steps"]),
        "final_checksum": (state_checksum(final), m["final_checksum"]),
        "return": (ret, m["return"]),
    }
    bad = [f"{k}: replay {a!r} vs log {b!r}" for k, (a, b) in checks.items() if a != b]
    if bad:
        raise ReplayMismatch("replay diverged from the log: " + "; ".join(bad))
    return Trajectory(dict(m), np.stack(states), log.actions.astype(np.float64), True)


def pose_only(log: EpisodeLog) -> Trajectory:
    """The stored low-rate pose as a trajectory, for a build that cannot replay."""
    s = np.full((len(log.pose), len(RS_NAMES)), np.nan)
    for j, f in enumerate(POSE_FIELDS):
        s[:, RS[f]] = log.pose[:, j]
    m = log.meta
    task = Task.load(m["task"], m.get("contract_version", 1))
    t = np.arange(len(log.pose)) * m["pose_every_decisions"] * task.decision_dt
    # The last pose is the terminal state, which can fall between pose ticks.
    t[-1] = m["physics_steps"] * task.dt
    s[:, RS["t"]] = t
    return Trajectory(dict(log.meta), s, log.actions.astype(np.float64), False)
