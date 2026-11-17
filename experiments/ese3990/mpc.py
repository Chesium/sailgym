"""CEM-MPC with the true Rust model; no learned dynamics. No terminal critic
unless M4X's X2 passes one as ``tail`` (default None: the M3/M4U planner).

Slide references below are PDF pages in
docs/ese3990/ESE3990_slides_L14-L19_advanced_topics.pdf.
The slides motivate sampling/refitting and receding-horizon control; blocking,
clipping and the variance floor are our explicit implementation choices (§4.3).
"""

from __future__ import annotations

import time
from dataclasses import asdict, dataclass
from math import ceil

import numpy as np
from sailing import OUTCOME_CODE, RS, RUNNING, TaskEnv


@dataclass(frozen=True)
class CEMConfig:
    horizon: int = 40  # decisions, each 0.05 s on the frozen benchmark
    candidates: int = 64
    iterations: int = 3
    block_s: float = 0.5
    initial_std: float = 0.6
    min_std: float = 0.1

    def __post_init__(self):
        for n in (self.horizon, self.candidates, self.iterations):
            if not isinstance(n, int) or n < 1:
                raise ValueError("H, K and I must be positive integers")
        if self.candidates < 2:
            raise ValueError("CEM needs at least two candidates")
        if not (0 < self.min_std <= self.initial_std < float("inf")):
            raise ValueError("require 0 < min_std <= initial_std < infinity")


def fit_elites(samples: np.ndarray, scores: np.ndarray, count: int, floor: float):
    # Advanced p. 120: sample, rank, then improve the sampling distribution.
    # Refit each rate/block using the elite mean and population std.
    elite = samples[np.argsort(-scores, kind="stable")[:count]]
    return elite.mean(axis=0), np.maximum(elite.std(axis=0), floor)


class CEMMPC:
    action_dtype = "float64"

    def __init__(self, seed: int = 0, config: CEMConfig | None = None, tail=None):
        # tail: M4X X2's V(s_H), a callable on research states (K, n) -> (K,)
        # with an identity(). None keeps every frozen M3/M4U planner unchanged.
        self.seed, self.config, self.tail = seed, config or CEMConfig(), tail
        self.plan = None

    def reset(self, env: TaskEnv):
        c = self.config
        ratio = c.block_s / env.task.decision_dt
        if not np.isfinite(ratio) or ratio < 1 or not np.isclose(ratio, round(ratio)):
            raise ValueError("block_s must be a positive multiple of decision_dt")
        self.block = round(ratio)
        if c.horizon % self.block:
            raise ValueError("horizon must contain whole action blocks")
        self.rng = np.random.default_rng(self.seed)
        self.plan = None  # never carry a plan or search RNG across episodes
        self.last_plan_xy = None
        self.physics_steps = self.rollout_calls = self.candidate_rollouts = 0
        self.predicted = []  # best plan's score per decision, tail included
        self.tail_s = 0.0  # wall time inside the tail value

    def act(self, env: TaskEnv) -> np.ndarray:
        if env.outcome != "running":
            raise ValueError("reset at the episode boundary before planning")
        c, b = self.config, self.block
        shape = (c.horizon // b, 2)
        if self.plan is None:
            mean = np.zeros(shape)
        else:
            # Advanced p. 201: reuse the previous solution. Shift ONE decision,
            # repeat the tail, then project onto constant 0.5 s block means.
            shifted = np.concatenate([self.plan[1:], self.plan[-1:]])
            mean = shifted.reshape(-1, b, 2).mean(axis=1)
        std = np.full(shape, c.initial_std)  # restore exploration at every replan
        discount = env.reward_fn.gamma ** np.arange(c.horizon)
        best_score = -np.inf
        for _ in range(c.iterations):
            samples = np.clip(
                self.rng.normal(mean, std, (c.candidates, *shape)), -1.0, 1.0
            )
            samples[0] = mean  # keep the warm start as a feasible candidate
            expanded = np.repeat(samples, b, axis=1)
            # Advanced pp. 178–179: shooting with a model. Rust forks preserve
            # gate/capsize history and stop at outcomes/the remaining live budget.
            # rollout uses the SAME reward + release=-1 adapter as live steps.
            states, code, steps, rewards = env.rollout(expanded)
            # Finite discounted return; no tail value, even at a time limit.
            # Post-terminal rewards are zero (steps=0), never reset transitions.
            scores = rewards @ discount
            if self.tail is not None:
                # X2: + γ^H·V(s_H) only for candidates still running after H
                # decisions; none after a finish, failure or the time limit.
                ts = time.perf_counter()
                alive = code[:, -1] == OUTCOME_CODE[RUNNING]
                v = np.asarray(self.tail(states[:, -1]), dtype=np.float64)
                scores = scores + np.where(
                    alive, discount[-1] * env.reward_fn.gamma * v, 0.0
                )
                self.tail_s += time.perf_counter() - ts
            j = int(np.argmax(scores))
            if scores[j] > best_score:
                best_score = scores[j]
                self.plan = expanded[j].copy()
                valid = steps[j] > 0
                self.last_plan_xy = np.full((c.horizon + 1, 2), np.nan)
                self.last_plan_xy[0] = env.research_state()[[RS["x"], RS["y"]]]
                self.last_plan_xy[1:][valid] = states[j, valid][:, [RS["x"], RS["y"]]]
            mean, std = fit_elites(
                samples, scores, max(2, ceil(c.candidates / 8)), c.min_std
            )
            self.physics_steps += int(steps.sum())
            self.rollout_calls += 1
            self.candidate_rollouts += c.candidates
        self.predicted.append(float(best_score))
        # Advanced pp. 199–200: execute just the first action, observe, replan.
        return self.plan[0].copy()

    def identity(self):
        return {
            "controller": "cem_mpc",
            "version": 1,
            "search_seed": self.seed,
            "config": asdict(self.config),
            "model": "true Rust episode branches",
            "tail_value": 0 if self.tail is None else self.tail.identity(),
            "release": "forced off",
        }
