"""M6 reverse-start curriculum, controls and transfer probes on the frozen U task."""

from __future__ import annotations

import argparse
import json
import time
from dataclasses import replace
from pathlib import Path

import numpy as np
import u_diagnostics
from evaluate import run_episode
from m5 import StartTask, replay_episode
from m5_compare import atomic_json
from make_conditions import TrainingSampler
from probe import NoiseProbe
from sailing import HERE, REPO, Condition, Task
from viz.episode_store import EpisodeStore, EpisodeWriter

ROOT = REPO / "runs/ese3990/M6/first"
BANDS = [(1, 3), (3, 7), (7, 15), (15, 30), (30, 40), (40, 40)]
FINAL = len(BANDS) - 1


def read(path):
    return json.loads(path.read_text())


def draw(wind, rng, stage, width, side=None):
    c = wind.next()
    if stage == FINAL:
        return c, [0.0, 0.0]
    x = rng.uniform(-width, width) if side is None else side * rng.uniform(0, width)
    depth = rng.uniform(*BANDS[stage])
    while True:
        c = replace(c, heading_deg=float(rng.uniform(0, 360)))
        if c.key() not in wind.exclude:
            return c, [float(x), 40.0 - depth]


def panel(stage, seed, width, count=20):
    wind, rng = TrainingSampler(seed), np.random.default_rng([seed, 6])
    rows = []
    for i in range(count):
        c, xy = draw(wind, rng, stage, width, 1 if i % 2 else -1)
        rows.append(dict(condition=c.to_dict(), xy=xy, seed=i))
    return rows


class Neutral:
    def reset(self, env):
        self.action = np.zeros(2)

    def act(self, env):
        return self.action

    def identity(self):
        return {"controller": "neutral"}


def score(task, writer, policy, rows, kind, stage, checkpoint=0):
    wins = decisions = 0
    for i, row in enumerate(rows):
        agent = policy
        if policy == "pink":
            agent = NoiseProbe(100_000_000 + i, 1, 10, 0.3, task.max_decisions)
        env_task = StartTask.at(task, row["xy"])
        meta = dict(
            kind=kind, stage=stage, checkpoint=checkpoint, m5_start_xy=row["xy"]
        )
        meta.update({k: row[k] for k in ("pair", "geometry", "board") if k in row})
        ep = run_episode(
            env_task,
            Condition.from_dict(row["condition"]),
            agent,
            row["seed"],
            writer=writer,
            log_meta=meta,
        )
        if ep["decisions"] == 0:
            raise ValueError("reset already terminal")
        wins += ep["outcome"] == "finished"
        decisions += ep["decisions"]
    writer.flush()
    return dict(finishes=wins, attempts=len(rows), decisions=decisions)


def control(root, stage, width, attempt):
    task = Task.load("U")
    rows = panel(stage, 61000 + stage + 100 * attempt, width, 100 if stage == 0 else 20)
    path = root / f"controls_{attempt}_{stage}"
    path.mkdir()
    atomic_json(path / "conditions.json", rows)
    with EpisodeWriter(path / "store", shard_size=20) as writer:
        result = {
            name: score(task, writer, agent, rows, name, stage)
            for name, agent in [("neutral", Neutral()), ("pink", "pink")]
        }
    atomic_json(path / "result.json", result)
    return result


def ground_tracks(root):
    # Ground-track angles measured on ordinary controls, independent of all learners.
    from sailing import RS

    angles = {-1: [], 1: []}
    store = EpisodeStore(root / "store")
    for e in store.index:
        tr = replay_episode(store.load(e), store.identity)
        x, y = tr.col("x"), tr.col("y")
        bearing = e["condition"]["wind_bearing_deg"]
        for a in range(0, len(x) - 40, 40):
            b = a + 40
            dx, dy = x[b] - x[a], y[b] - y[a]
            if dy < 0.5 or tr.states[a:b, RS["u"]].min() < 0.1:
                continue
            angle = (np.degrees(np.arctan2(dx, dy)) - bearing + 180) % 360 - 180
            if 35 <= abs(angle) <= 65:
                angles[int(np.sign(angle))].append(float(angle))
    return {
        str(k): dict(n=len(v), angle=float(np.median(v)) if v else None)
        for k, v in angles.items()
    }


def paired(stage, tracks, width):
    if stage == FINAL or any(v["angle"] is None for v in tracks.values()):
        return []
    rows = []
    wind = TrainingSampler(62000 + stage)
    rng = np.random.default_rng([62000, stage])
    for _ in range(2000):
        sign = 1 if (len(rows) // 2) % 2 else -1
        c = wind.next()
        heading = c.wind_bearing_deg + 45 * sign
        c = replace(c, heading_deg=heading % 360)
        if c.key() in wind.exclude:
            continue
        depth = rng.uniform(*BANDS[stage])
        slope = np.tan(np.deg2rad(c.wind_bearing_deg + tracks[str(sign)]["angle"]))
        other = np.tan(np.deg2rad(c.wind_bearing_deg + tracks[str(-sign)]["angle"]))
        # Endpoints along either board bound the attainable two-leg intersection.
        offsets = np.linspace(-width, width, 201)
        direct_end = offsets + depth * slope
        opposite_end = offsets + depth * other
        direct = offsets[np.abs(direct_end) <= 4]
        miss = offsets[(sign * direct_end >= 6) & (sign * opposite_end <= 4)]
        if not len(direct) or not len(miss):
            continue
        pair = len(rows) // 2
        for label, xs in [("direct", direct), ("switch", miss)]:
            x = float(rng.choice(xs))
            rows.append(
                dict(
                    condition=c.to_dict(),
                    seed=pair,
                    pair=pair,
                    board=sign,
                    geometry=label,
                    xy=[x, 40 - depth],
                    projected_gate_x=x + depth * slope,
                    opposite_gate_x=x + depth * other,
                )
            )
        if len(rows) == 20:
            return rows
    return []


def prepare(root):
    from concurrent.futures import ProcessPoolExecutor

    from compare import digest

    root.mkdir(parents=True, exist_ok=False)
    atomic_json(root / "clock.json", {"start": time.time(), "budget_min": 150})
    # Narrow the first box only if both controls cannot reach the gate. If neutral
    # regularly succeeds, widen it within the original M5 interval on the next try.
    width = 10
    for attempt in range(2):
        first = control(root, 0, width, attempt)
        n, p = first["neutral"]["finishes"], first["pink"]["finishes"]
        if n < 30 and max(n, p) >= 1:
            break
        if attempt or n >= 30:
            raise ValueError(f"first box needs redesign: {first}")
        width = 5
    widths = [width] + [10] * (FINAL - 1) + [0]
    with ProcessPoolExecutor(max_workers=5) as pool:
        futures = [
            pool.submit(control, root, s, widths[s], attempt)
            for s in range(1, FINAL + 1)
        ]
        tracks = ground_tracks(root / f"controls_{attempt}_0")
        controls = [first] + [f.result() for f in futures]
    freeze = dict(
        bands=BANDS,
        widths=widths,
        controls=controls,
        control_attempt=attempt,
        promotion="6/20 and above both controls twice, every 25 current episodes",
        pilot_gate="promote stage 0 and finish on a fresh stage 1 promotion probe",
        pilot_seed=100,
        pilot_decisions=400000,
        main_seeds=list(range(5)),
        tracks=tracks,
        paired=[paired(s, tracks, widths[s]) for s in range(FINAL + 1)],
        sources={
            p.name: digest(p)
            for p in [
                Path(__file__),
                HERE / "m6_train.py",
                HERE / "train.py",
                HERE / "ppo.py",
                HERE / "sac.py",
                HERE / "m5.py",
                HERE / "configs/M4U_train_freeze.json",
            ]
        },
    )
    atomic_json(root / "freeze.json", freeze)


def audit(path):
    task = Task.load("U")
    store = EpisodeStore(path / "store")
    output = path / "audit.jsonl"
    temp = output.with_suffix(".tmp")
    with temp.open("w") as file:
        for entry in store.index:
            tr = replay_episode(store.load(entry), store.identity, task)
            d = u_diagnostics.diagnose(tr, task)
            x, y = tr.col("x"), tr.col("y")
            closest = np.argmin(np.hypot(x, y - 40))
            d.update(
                {
                    k: entry[k]
                    for k in (
                        "kind",
                        "stage",
                        "checkpoint",
                        "pair",
                        "geometry",
                        "board",
                        "m5_start_xy",
                        "decisions",
                    )
                    if k in entry
                }
            )
            d.update(
                x_closest=float(x[closest]),
                rudder_travel=float(abs(np.diff(tr.col("delta_r"))).sum()),
            )
            first = next(
                (e for e in d["events"] if e["settled"] and e["type"] == "tack"), None
            )
            if first:
                a, b = np.searchsorted(tr.t, [first["t0"], first["t1"]])
                angle = (
                    np.degrees(np.arctan2(-x[a], 40 - y[a]))
                    - entry["condition"]["wind_bearing_deg"]
                )
                d["first_tack"] = dict(
                    time=first["t0"],
                    x=float(x[a]),
                    y=float(y[a]),
                    bearing_to_gate_relative_to_wind=(angle + 180) % 360 - 180,
                    speed=float(tr.col("u")[a]),
                    exit_speed=float(tr.col("u")[b]),
                    regained_way=bool(np.any(tr.col("u")[b:] >= 0.1)),
                )
            file.write(json.dumps(d) + "\n")
    temp.replace(output)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument("command", choices=["prepare", "audit"])
    parser.add_argument("path", type=Path, nargs="?", default=ROOT)
    args = parser.parse_args()
    (prepare if args.command == "prepare" else audit)(args.path)
