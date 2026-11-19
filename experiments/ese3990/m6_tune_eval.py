"""Evaluate one m6_tune checkpoint from the original start, with stored,
replay-checked episodes and the frozen M4U tack diagnostics.

    .venv/bin/python experiments/ese3990/m6_tune_eval.py RUN_DIR CKPT SPLIT [--jobs 6]

SPLIT is ``panel`` (the pilot panel, training distribution), ``panel200``
(200 fresh training-distribution conditions), ``stress`` (32 conditions outside
every frozen range, exploratory), or a frozen manifest name
(``validation``, ``test``, ``shift``). Writes ``<RUN_DIR>/eval_<CKPT>_<SPLIT>/``.
"""

import argparse
import itertools
import json
from pathlib import Path

import numpy as np
import u_diagnostics
from evaluate import run_episode
from evaluate import summarize as ep_summary
from m6_tune import FIXED, Task, to_rates
from m6_tune_plot import load
from make_conditions import TrainingSampler
from sailing import Condition, features, load_manifest
from viz.episode_store import EpisodeWriter


class Tuned:
    """A m6_tune policy as an ``evaluate.run_episode`` controller."""

    action_dtype = "float64"

    def __init__(self, args, params, act, name):
        self.args, self.params, self.act_fn, self.name = args, params, act, name
        self.mean, self.std = np.asarray(FIXED["mean"]), np.asarray(FIXED["std"])

    def reset(self, env):
        self.k = 0

    def act(self, env):
        rs = env.research_state()
        if self.k % self.args["repeat"] == 0:
            o = np.clip((features(rs, env.task.target) - self.mean) / self.std, -10, 10)
            self.a = np.asarray(
                self.act_fn(self.params, o.astype(np.float32)), np.float64
            )
        self.k += 1
        if self.args["head"] == "target":
            return to_rates(self.a, rs, env.task.decision_dt)
        return self.a.copy()

    def identity(self):
        return {
            "controller": f"m6_tune_{self.args['learner']}",
            "checkpoint": self.name,
            "args": self.args,
        }


def conditions(split):
    if (
        split == "stress"
    ):  # outside every frozen range: wind 3.5-7 m/s, ±10-15°, 60/120°
        return [
            Condition(f"stress-{i:02d}", w, b, h)
            for i, (w, b, h) in enumerate(
                itertools.product((3.5, 4.0, 6.0, 7.0), (-15, -10, 10, 15), (60, 120))
            )
        ]
    if split.startswith("panel"):
        n = int(split[5:] or 40)
        s = TrainingSampler(90000)
        return [s.next() for _ in range(n)]
    return load_manifest(split)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("run", type=Path)
    p.add_argument("ckpt")
    p.add_argument("split")
    p.add_argument("--jobs", type=int, default=6)
    a = p.parse_args()
    args, params, act, f = load(a.run, a.ckpt)
    assert args["norm"] == "fixed"
    task = Task.load("U")
    out = a.run / f"eval_{f.stem}_{a.split}"
    ctl = Tuned(args, params, act, str(f))
    eps = []
    with EpisodeWriter(out / "store", {"checkpoint": str(f), "split": a.split}) as w:
        for c in conditions(a.split):
            e = run_episode(task, c, ctl, writer=w, log_meta={"kind": a.split})
            lat = np.asarray(e.pop("decision_latency_s"))
            e["latency_ms"] = {
                "p50": float(np.median(lat) * 1e3),
                "p95": float(np.percentile(lat, 95) * 1e3),
            }
            eps.append(e)
    diags = u_diagnostics.diagnose_store(out / "store", jobs=a.jobs)
    res = {
        "checkpoint": str(f),
        "split": a.split,
        "outcomes": ep_summary(eps),
        "diagnostics": u_diagnostics.summarize(list(diags.values())),
        "finished_with_tack": sum(
            d["outcome"] == "finished" and d["tacks"] > 0 for d in diags.values()
        ),
        "replayed_exactly": len(diags),
        "latency_ms_p50_median": float(
            np.median([e["latency_ms"]["p50"] for e in eps])
        ),
    }
    (out / "summary.json").write_text(json.dumps(res, indent=1))
    (out / "episodes.json").write_text(json.dumps(eps, indent=1))
    print(json.dumps(res, indent=1))


if __name__ == "__main__":
    main()
