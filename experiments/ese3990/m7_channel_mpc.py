"""Feasibility of a channel beat: the frozen M4U planner (H160_K32) on U with
the x bounds narrowed to ±HALF m. Validation conditions only.

RAYON_NUM_THREADS=3 .venv/bin/python experiments/ese3990/m7_channel_mpc.py \
    --half 10 --n 8 --jobs 8
"""

import argparse
import json
import os
from concurrent.futures import ProcessPoolExecutor
from dataclasses import replace
from multiprocessing import get_context
from types import SimpleNamespace

import numpy as np
from evaluate import run_episode
from mpc import CEMMPC, CEMConfig
from sailing import REPO, RS, Task, load_manifest
from u_diagnostics import diagnose

OUT = REPO / "runs/ese3990/M7/channel_mpc"
FREEZE = json.loads(
    (REPO / "experiments/ese3990/configs/M4U_mpc_freeze.json").read_text()
)


def channel(half: float) -> Task:
    u = Task.load("U")
    _, y0, _, y1 = u.bounds
    return replace(u, bounds=(-half, y0, half, y1))


def one(job):
    half, cond, seed, path = job
    task = channel(half)
    ep = run_episode(task, cond, CEMMPC(seed, CEMConfig(**FREEZE["config"])), 0, True)
    st = np.asarray(ep.pop("trajectory"))
    ep.pop("actions")
    lat = np.asarray(ep.pop("decision_latency_s"))
    tr = SimpleNamespace(
        exact=True,
        states=st,
        t=st[:, RS["t"]],
        meta=dict(outcome=ep["outcome"], reason=ep["reason"], condition=cond.to_dict()),
    )
    d = diagnose(tr, task)
    d.pop("events")
    ep.update(
        half=half,
        diagnostics=d,
        max_abs_x=float(np.abs(st[:, RS["x"]]).max()),
        latency_ms_p50=float(np.median(lat) * 1e3),
        xy=st[::10][:, [RS["x"], RS["y"]]].round(2).tolist(),
    )
    path.write_text(json.dumps(ep))
    return path


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--half", type=float, nargs="+", default=[10.0])
    p.add_argument("--n", type=int, default=8)
    p.add_argument("--seed", type=int, default=0)
    p.add_argument("--jobs", type=int, default=8)
    a = p.parse_args()
    conds = load_manifest("validation")[: a.n]
    jobs = []
    for half in a.half:
        d = OUT / f"half{half:g}_s{a.seed}"
        d.mkdir(parents=True, exist_ok=True)
        jobs += [
            (half, c, a.seed, d / f"{c.id}.json")
            for c in conds
            if not (d / f"{c.id}.json").exists()
        ]
    os.environ.setdefault("RAYON_NUM_THREADS", "3")
    with ProcessPoolExecutor(a.jobs, mp_context=get_context("spawn")) as ex:
        for path in ex.map(one, jobs):
            e = json.loads(path.read_text())
            g = e["diagnostics"]
            print(
                f"±{e['half']:g} {e['condition']['id']} {e['reason'] or e['outcome']}"
                f" t={e['finish_time_s'] or g['duration_s']:.1f}s tacks={g['tacks']}"
                f" failed={g['failed_tacks']} gybes={g['gybes']}"
                f" max|x|={e['max_abs_x']:.1f} maxy={g['max_y_m']:.1f}"
                f" p50={e['latency_ms_p50']:.0f}ms",
                flush=True,
            )


if __name__ == "__main__":
    main()
