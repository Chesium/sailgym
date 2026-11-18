"""M5's exploratory training contract, without changing any frozen M4 source.

Episodes always execute and record the original 20 Hz transitions. Only the
learner boundary aggregates actions/rewards; routes, rewards and outcomes stay
with the frozen TaskEnv. Random starts are scenario initial positions, not state
restores. Use replay_episode for M5 training episodes with nonzero starts.
"""

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass, fields, replace
from pathlib import Path

import numpy as np
import train
from compare import digest
from evaluate import run_episode, summarize
from make_conditions import TrainingSampler
from probe import NoiseProbe
from sailing import (
    HERE,
    REPO,
    RUNNING,
    Task,
    TaskEnv,
    TransitionReward,
    scenario_json,
)
from viz.episode_store import EpisodeRecorder, EpisodeStore, EpisodeWriter
from viz.replay import replay

import sailgym_core

LABEL = "M5 upwind without demonstrations (post-hoc, exploratory)"
DEFAULT_BOX = (-25.0, 25.0, 0.0, 35.0)  # xmin, xmax, ymin, ymax
CELLS = {"neither": (1, False), "B": (10, False), "S": (1, True), "BS": (10, True)}
FREEZE = HERE / "configs/M5_freeze.json"


@dataclass
class StartTask(Task):
    initial_xy: tuple[float, float] = (0.0, 0.0)

    @classmethod
    def at(cls, task: Task, xy) -> StartTask:
        return cls(
            **{f.name: getattr(task, f.name) for f in fields(Task)},
            initial_xy=tuple(xy),
        )

    def spec(self, cond):
        doc = json.loads(scenario_json(cond))
        doc["initial_state"].update(x=self.initial_xy[0], y=self.initial_xy[1])
        return sailgym_core.Spec(
            json.dumps(doc, sort_keys=True),
            max_steps=self.max_steps,
            autoreset="Disabled",
            cadence=self.cadence,
            bounds=self.bounds,
            route_json=self.route_json,
            log_decisions=False,
        )


class StartSampler:
    """Alternate frozen/random starts, with an independent RNG for position.

    Wind draws still come from TrainingSampler. Changed headings are checked
    against every held-out condition, after randomization too.
    """

    def __init__(self, seed, box=DEFAULT_BOX, *, all_random=False):
        xmin, xmax, ymin, ymax = box
        if not np.isfinite(box).all() or not xmin < xmax or not ymin < ymax:
            raise ValueError("start box must have finite increasing bounds")
        if xmin < -80 or xmax > 120 or ymin < -80 or ymax >= 40:
            raise ValueError("start box must be in bounds and below the U gate")
        self.box = tuple(box)
        self.wind = TrainingSampler(seed)
        self.rng = np.random.default_rng([seed, 5])
        self.all_random = all_random
        self.count = 0
        self.xy = (0.0, 0.0)

    def next(self):
        cond = self.wind.next()
        random_start = self.all_random or self.count % 2 == 1
        self.count += 1
        self.xy = (0.0, 0.0)
        if random_start:
            xmin, xmax, ymin, ymax = self.box
            self.xy = (
                float(self.rng.uniform(xmin, xmax)),
                float(self.rng.uniform(ymin, ymax)),
            )
            while True:
                cond = replace(cond, heading_deg=float(self.rng.uniform(0, 360)))
                if cond.key() not in self.wind.exclude:
                    break
        return cond


class HeldAction:
    """Query a saved policy once every hold original decisions, reset per episode."""

    def __init__(self, policy, hold):
        if hold < 1:
            raise ValueError("hold must be positive")
        self.policy, self.hold = policy, hold
        self.action_dtype = getattr(policy, "action_dtype", "float64")

    def reset(self, env):
        self.policy.reset(env)
        self.k = 0

    def act(self, env):
        if self.k % self.hold == 0:
            self.action = self.policy.act(env).copy()
        self.k += 1
        return self.action.copy()

    def identity(self):
        return {**self.policy.identity(), "m5_hold": self.hold, "label": LABEL}


class BlockCollector(train.Collector):
    def __init__(self, task, sampler, writer, meta, dtype, hold):
        if hold < 1:
            raise ValueError("hold must be positive")
        self.hold = hold
        self.gamma = TransitionReward(task).gamma
        self.decisions = 0
        super().__init__(task, sampler, writer, meta, dtype)

    def _begin(self):
        cond = self.sampler.next()
        xy = getattr(self.sampler, "xy", (0.0, 0.0))
        self.env = TaskEnv(StartTask.at(self.task, xy), cond, self.episodes)
        self.rs = self.env.reset()
        self.recorder = EpisodeRecorder(self.env, self.dtype)
        self.recorder.start(self.rs)
        self.ret = 0.0

    def _log(self):
        self.writer.add(
            self.recorder.finish(
                **self.meta,
                m5_start_xy=list(self.env.task.initial_xy),
                m5_hold=self.hold,
                label=LABEL,
            )
        )

    def run(self, n, choose, *, max_decisions=None):
        """At most n learner transitions, bounded by actual 20 Hz decisions.

        Never cross an episode boundary inside a block. The final block may be
        shortened by a terminal, timeout or collection budget. Its own discount
        is used for bootstrapping, including a shortened timeout block.
        """
        rows = []
        ended = []
        remaining = n * self.hold if max_decisions is None else max_decisions
        for _ in range(n):
            if remaining <= 0:
                break
            action, extra = choose(self.rs)
            first = self.rs
            reward = 0.0
            count = 0
            for j in range(min(self.hold, remaining)):
                rec = self.env.step(action)
                self.recorder.step(rec)
                reward += self.gamma**j * rec.reward
                self.ret += rec.reward
                self.rs = rec.next_rs
                count += 1
                if rec.outcome != RUNNING:
                    break
            remaining -= count
            self.decisions += count
            rows.append(
                dict(
                    rs=first,
                    action=rec.action,
                    next_rs=rec.next_rs,
                    reward=reward,
                    bootstrap=rec.bootstrap,
                    cont=rec.cont,
                    extra=extra,
                    discount=self.gamma**count,
                    raw_decisions=count,
                )
            )
            if rec.outcome != RUNNING:
                self._log()
                ended.append({"outcome": rec.outcome, "return": self.ret})
                self.episodes += 1
                self._begin()
        return {k: np.asarray([r[k] for r in rows]) for k in rows[0]}, ended


def calibration_blocks(data, hold, gamma):
    """Pack the unchanged 10k raw calibration decisions for SAC's block replay.

    SmoothRandom holds globally, so an episode boundary can split its hold.
    Split again whenever its action changes; never merge two different actions.
    """
    out = []
    i = 0
    while i < len(data["reward"]):
        end = i + 1
        while (
            end < min(i + hold, len(data["reward"]))
            and data["cont"][end - 1]
            and np.array_equal(data["action"][i], data["action"][end])
        ):
            end += 1
        n = end - i
        out.append(
            dict(
                rs=data["rs"][i],
                action=data["action"][i],
                next_rs=data["next_rs"][end - 1],
                reward=float(np.dot(gamma ** np.arange(n), data["reward"][i:end])),
                bootstrap=data["bootstrap"][end - 1],
                cont=data["cont"][end - 1],
                discount=gamma**n,
                raw_decisions=n,
            )
        )
        i = end
    return {k: np.asarray([r[k] for r in out]) for k in out[0]}


def block_gae(data, v, v_next, lam):
    adv = np.zeros(len(v), np.float64)
    a = 0.0
    for t in reversed(range(len(v))):
        discount = data["discount"][t]
        delta = data["reward"][t] + discount * data["bootstrap"][t] * v_next[t] - v[t]
        a = delta + discount * lam * data["cont"][t] * a
        adv[t] = a
    return adv, adv + v


def replay_episode(log, identity, task=None):
    task = task or Task.load(log.meta["task"], log.meta.get("contract_version", 1))
    task = StartTask.at(task, log.meta.get("m5_start_xy", (0.0, 0.0)))
    return replay(log, identity, task)


def check_start_box(out: Path, box=DEFAULT_BOX, episodes=100):
    """No learning, all starts randomized, fixed pink/0.5 s/σ=.3 controller."""
    out.mkdir(parents=True, exist_ok=False)
    task = Task.load("U")
    sampler = StartSampler(100, box, all_random=True)
    protocol = dict(
        label=LABEL,
        box=list(box),
        episodes=episodes,
        seed=100,
        beta=1,
        hold=10,
        sigma=0.3,
        sources={
            str(p.relative_to(REPO)): digest(p)
            for p in (
                Path(__file__),
                HERE / "probe.py",
                HERE / "sailing.py",
                HERE / "evaluate.py",
                HERE / "make_conditions.py",
            )
        },
    )
    (out / "protocol.json").write_text(json.dumps(protocol, indent=1) + "\n")
    results = []
    with EpisodeWriter(out / "store", protocol, shard_size=10) as writer:
        for i in range(episodes):
            cond = sampler.next()
            started = StartTask.at(task, sampler.xy)
            ctrl = NoiseProbe(100_000_000 + i, 1, 10, 0.3, task.max_decisions)
            ep = run_episode(
                started,
                cond,
                ctrl,
                i,
                writer=writer,
                log_meta={
                    "kind": "start_box_check",
                    "label": LABEL,
                    "m5_start_xy": list(sampler.xy),
                },
            )
            ep.pop("decision_latency_s")
            results.append(ep)
    store = EpisodeStore(out / "store")
    for entry in store.index:
        replay_episode(store.load(entry), store.identity)
    result = dict(
        protocol=protocol,
        summary=summarize(results),
        episodes=results,
        exact_replays=len(store.index),
        passes=sum(e["outcome"] == "finished" for e in results) >= episodes * 0.01,
    )
    (out / "results.json").write_text(json.dumps(result, indent=1) + "\n")
    print(
        json.dumps(
            {k: result[k] for k in ("summary", "exact_replays", "passes")}, indent=1
        )
    )
    return result


def main():
    ap = argparse.ArgumentParser(__doc__)
    sub = ap.add_subparsers(dest="command", required=True)
    check = sub.add_parser("start-box")
    check.add_argument("--out", type=Path, required=True)
    check.add_argument("--box", type=float, nargs=4, default=DEFAULT_BOX)
    args = ap.parse_args()
    if args.command == "start-box":
        check_start_box(args.out, args.box)


if __name__ == "__main__":
    main()
