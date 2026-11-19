"""M6 first-band width and conditional heading pilot. Launch with 'campaign'."""

import os
import subprocess
import sys
import time
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

import m6_train
import numpy as np
from compare import digest
from evaluate import make_controller
from m6 import ROOT as FIRST
from m6 import Neutral, atomic_json, audit, draw, panel, read, score
from make_conditions import TrainingSampler
from sailing import Condition, Task, load_manifest
from viz.episode_store import EpisodeWriter

ROOT = FIRST.parent / "width"
WIDTH = {"broad": 10, "narrow": 3, "heading": 3}
SEEDS = (0, 1, 2)
SOURCE = FIRST / "pilot/ppo_s100"


def direct_heading(c, xy, tracks):
    # Use the original uniform heading draw to choose among feasible boards.
    preference = 1 if c.heading_deg < 180 else -1
    for board in (preference, -preference):
        course = c.wind_bearing_deg + tracks[str(board)]["angle"]
        projected = xy[0] + (40 - xy[1]) * np.tan(np.deg2rad(course))
        if abs(projected) <= 5:
            return replace(c, heading_deg=(c.wind_bearing_deg + board * 45) % 360)
    raise ValueError(f"no direct course from {xy}")


def fixed_panels(tracks):
    wind, rng = TrainingSampler(615100), np.random.default_rng(615101)
    broad, narrow, headed = [], [], []
    for pair in range(40):
        c, xy = draw(wind, rng, 0, 10, 1)
        # Stratify the absolute offset into two equally represented 5 m intervals.
        x = xy[0] / 2 + (5 if pair % 2 else 0)
        for sign in (1, -1):
            cond = replace(
                c,
                id=f"{c.id}-{'east' if sign > 0 else 'west'}",
                wind_bearing_deg=sign * c.wind_bearing_deg,
                heading_deg=(sign * c.heading_deg) % 360,
            )
            row = dict(seed=pair, condition=cond.to_dict(), xy=[sign * x, xy[1]])
            broad.append(row)
            close = {**row, "xy": [sign * x * 0.3, xy[1]]}
            narrow.append(close)
            heading = direct_heading(cond, close["xy"], tracks)
            assert cond.key() not in wind.exclude and heading.key() not in wind.exclude
            headed.append({**close, "condition": heading.to_dict()})
    return dict(broad=broad, narrow=narrow, heading=headed)


def conditions(arm, seed, count, tracks):
    rows = panel(0, seed, WIDTH[arm], count)
    if arm == "heading":
        for row in rows:
            row["condition"] = direct_heading(
                Condition.from_dict(row["condition"]), row["xy"], tracks
            ).to_dict()
    return rows


def controls(arm):
    freeze = read(ROOT / "protocol.json")
    where = ROOT / arm
    where.mkdir()
    rows = freeze["control_panels"][arm]
    task = Task.load("U")
    with EpisodeWriter(where / "controls/store", shard_size=20) as writer:
        counts = {
            k: score(task, writer, policy, rows, k, 0)
            for k, policy in [("neutral", Neutral()), ("pink", "pink")]
        }
    old = read(FIRST / "freeze.json")
    atomic_json(
        where / "freeze.json",
        {
            **old,
            "widths": [WIDTH[arm]],
            "controls": [counts],
            "arm": arm,
            "sources": {**old["sources"], Path(__file__).name: digest(Path(__file__))},
            "parent_protocol": digest(ROOT / "protocol.json"),
        },
    )
    atomic_json(where / "clock.json", {"start": time.time()})


class FirstBand(m6_train.Curriculum):
    """Keep training on stage zero, retaining the first passing snapshot."""

    def ended(self, stage, steps):
        assert stage == 0
        self.completed_steps += steps
        self.episodes += 1
        if self.episodes % 25:
            return
        self.round += 1
        seed = 1000000 + self.seed * 10000 + self.round
        rows = conditions(self.freeze["arm"], seed, 20, self.freeze["tracks"])
        row = self.probe("promotion", 0, rows, self.completed_steps)
        wins = row["finishes"]
        controls = self.freeze["controls"][0].values()
        passed = wins >= 6 and all(
            wins / 20 > r["finishes"] / r["attempts"] for r in controls
        )
        self.streak = self.streak + 1 if passed else 0
        if self.streak == 2 and not self.qualified:
            self.qualified = True
            atomic_json(
                self.out / "qualifying.json", {"decisions": self.completed_steps}
            )


def fit(arm, learner, seed):
    where = ROOT / arm
    tracks = read(where / "freeze.json")["tracks"]

    def training_draw(wind, rng, stage, width):
        c, xy = draw(wind, rng, stage, width)
        if arm == "heading":
            c = direct_heading(c, xy, tracks)
            assert c.key() not in wind.exclude
        return c, xy

    # Each fit is a separate process. Supply the fixed-band curriculum to the
    # original hash-frozen training loop, including its calibration and updates.
    with (
        patch.object(m6_train, "Curriculum", FirstBand),
        patch.object(m6_train, "draw", training_draw),
    ):
        m6_train.run(where, learner, seed)
    out = where / "pilot" / f"{learner}_s{seed}"
    summ = read(out / "summary.json")
    if not summ["complete"] or summ["decisions"] != 400000:
        raise ValueError(f"incomplete training {out}")
    evaluate(arm, learner, seed)
    audit(out)


def evaluate(arm, learner, seed):
    out = ROOT / arm / "pilot" / f"{learner}_s{seed}"
    freeze = read(ROOT / "protocol.json")
    step = 400000
    policy = make_controller(learner, seed, out / "ckpt" / f"{step:07d}")
    task = Task.load("U")
    with EpisodeWriter(out / "evaluation/store", shard_size=20) as writer:
        for name, rows in freeze["panels"].items():
            score(task, writer, policy, rows, name, 0, step)
        rows = [
            dict(seed=0, xy=[0, 0], condition=c.to_dict())
            for c in load_manifest("validation")
        ]
        score(task, writer, policy, rows, "original_validation", 5, step)
    audit(out / "evaluation")


def rescore(stem):
    seal = read(ROOT / "protocol.json")
    path = SOURCE / seal["checkpoints"][stem]["path"]
    for ext in (".json", ".npz"):
        assert digest(path.with_suffix(ext)) == seal["checkpoints"][stem][ext]
    policy = make_controller("ppo", 100, path)
    out = ROOT / "retention" / stem
    with EpisodeWriter(out / "store", shard_size=20) as writer:
        score(
            Task.load("U"),
            writer,
            policy,
            seal["panels"]["broad"],
            "common",
            0,
            int(stem),
        )
    audit(out)


def transfer(arm, learner, seed):
    out = ROOT / arm / "pilot" / f"{learner}_s{seed}"
    step = read(out / "qualifying.json")["decisions"]
    policy = make_controller(learner, seed, out / "probes" / f"{step:07d}")
    with EpisodeWriter(out / "transfer/store", shard_size=20) as writer:
        score(
            Task.load("U"),
            writer,
            policy,
            read(ROOT / "protocol.json")["transfer"],
            "paired_before",
            1,
            step,
        )
    audit(out / "transfer")


def batch(commands):
    cpus = sorted(os.sched_getaffinity(0))
    for start in range(0, len(commands), 6):
        jobs = []
        for i, args in enumerate(commands[start : start + 6]):
            key = "_".join(map(str, args))
            ids = cpus[3 * i : 3 * i + 3] or cpus[:3]
            with (ROOT / (key + ".log")).open("a") as log:
                jobs.append(
                    (
                        key,
                        subprocess.Popen(
                            [
                                "taskset",
                                "-c",
                                ",".join(map(str, ids)),
                                sys.executable,
                                __file__,
                                *map(str, args),
                            ],
                            stdout=log,
                            stderr=log,
                        ),
                    )
                )
        failed = [k for k, p in jobs if p.wait()]
        if failed:
            raise RuntimeError(f"failed workers {failed}")


def campaign():
    ROOT.mkdir(parents=True, exist_ok=False)
    old = read(FIRST / "freeze.json")
    tracks = old["tracks"]
    ckpts = {}
    for p in sorted([*SOURCE.glob("probes/*.npz"), *SOURCE.glob("ckpt/*.npz")]):
        ckpts[p.stem] = {
            "path": str(p.relative_to(SOURCE).with_suffix("")),
            **{e: digest(p.with_suffix(e)) for e in (".json", ".npz")},
        }
    assert len(ckpts) == 6 and len(old["paired"][1]) == 20
    atomic_json(
        ROOT / "protocol.json",
        dict(
            start=time.time(),
            seeds=SEEDS,
            panels=fixed_panels(tracks),
            control_panels={a: conditions(a, 615110, 100, tracks) for a in WIDTH},
            checkpoints=ckpts,
            transfer=old["paired"][1],
            tracks=tracks,
            decisions=400000,
            required_seeds=2,
            sources=old["sources"],
            original_freeze=digest(FIRST / "freeze.json"),
            runner=digest(Path(__file__)),
        ),
    )

    def status(phase, **kwargs):
        atomic_json(
            ROOT / "status.json", dict(phase=phase, updated=time.time(), **kwargs)
        )

    status("controls and retention")
    batch(
        [
            ("controls", "broad"),
            ("controls", "narrow"),
            *[("rescore", s) for s in ckpts],
        ]
    )
    status("six PPO width runs")
    batch([("fit", a, "ppo", s) for a in ("narrow", "broad") for s in SEEDS])

    def passing(arm):
        return [
            s
            for s in SEEDS
            if (ROOT / arm / "pilot" / f"ppo_s{s}" / "qualifying.json").exists()
        ]

    arm = "narrow"
    if len(passing(arm)) < 2:
        arm = "heading"
        status("conditional heading pilot", narrow_passing=passing("narrow"))
        batch([("controls", arm)])
        batch([("fit", arm, "ppo", s) for s in SEEDS])
    winners = passing(arm)
    if len(winners) >= 2:
        status("transfer and SAC", arm=arm, passing=winners)
        batch(
            [*[("transfer", arm, "ppo", s) for s in winners], ("fit", arm, "sac", 100)]
        )
        if (ROOT / arm / "pilot/sac_s100/qualifying.json").exists():
            batch([("transfer", arm, "sac", 100)])
    arms = [a for a in WIDTH if (ROOT / a).exists()]
    batch([("audit_controls", a) for a in arms])
    status("complete", arms=arms, conditional_arm=arm, passing=winners)


if __name__ == "__main__":
    cmd, *args = sys.argv[1:]
    if cmd == "campaign":
        campaign()
    elif cmd == "controls":
        controls(*args)
    elif cmd == "audit_controls":
        audit(ROOT / args[0] / "controls")
    elif cmd == "rescore":
        rescore(*args)
    else:
        {"fit": fit, "transfer": transfer}[cmd](args[0], args[1], int(args[2]))
