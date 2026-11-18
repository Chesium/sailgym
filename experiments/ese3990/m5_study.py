"""M5 pilot/main queue, replay audit and write-once training freeze.

    m5_study.py run --stage smoke --box -10 10 25 39 --jobs 4
    m5_study.py audit --stage smoke
    m5_study.py freeze --box-check runs/ese3990/M5/start_box_near_gate
    m5_study.py run --stage main --jobs 6

Main and extended queues are separate. No held-out evaluation runs here.
Completed jobs are skipped; failed/incomplete run directories are retained and
require an explicit new directory before retrying. There is only one queue
writer per root (an advisory process lock prevents concurrent launchers).
"""

from __future__ import annotations

import argparse
import fcntl
import json
import os
import signal
import subprocess
import sys
import time
from collections import Counter
from datetime import UTC, datetime
from pathlib import Path

import numpy as np
import ppo
import sac
from compare import digest
from evaluate import run_episode, select_checkpoint
from m5 import CELLS, FREEZE, LABEL, HeldAction, replay_episode
from m5_train import config
from sailing import HERE, NOMINAL, REPO, Task, contract_identity
from viz.episode_store import EpisodeStore, build_identity

ROOT = REPO / "runs/ese3990/M5"
START_BOX = (-10.0, 10.0, 25.0, 39.0)
SOURCES = [
    "m5.py",
    "m5_train.py",
    "m5_study.py",
    "train.py",
    "ppo.py",
    "sac.py",
    "probe.py",
    "baselines.py",
    "evaluate.py",
    "sailing.py",
    "make_conditions.py",
    "compare.py",
    "viz/episode_store.py",
    "viz/replay.py",
    "configs/tasks.json",
    "configs/tasks_v2.json",
    "configs/M4U_train_freeze.json",
    "configs/M4U_diagnostics.json",
    *[
        f"configs/conditions/{s}.json"
        for s in ("training", "validation", "test", "shift")
    ],
]
READINGS = [
    ["B finishes, S does not", "The decision interval was the bottleneck"],
    ["S finishes, B does not", "Discovering a first finish was the bottleneck"],
    ["Only B + S finishes", "Both are needed, as in Bink's setup"],
    ["No cell finishes", "Neither change is sufficient; run the second stage"],
    [
        "A PPO cell finishes only at 40M",
        "PPO's B result is update-bound, not decision-interval-bound",
    ],
    [
        "One learner finishes where the other does not",
        "Report Q band and tack diagnostics; no ranking claim beyond these seeds",
    ],
]


def source_hashes():
    return {str((HERE / p).relative_to(REPO)): digest(HERE / p) for p in SOURCES}


def read_rows(path):
    return [json.loads(line) for line in path.read_text().splitlines() if line]


def jobs(stage, root=ROOT):
    seeds = [100] if stage == "smoke" else range(5)
    learners = ["ppo"] if stage == "extended" else ["ppo", "sac"]
    cells = ["B", "BS"] if stage == "extended" else CELLS
    result = []
    for learner in learners:
        for cell in cells:
            for seed in seeds:
                if stage == "main" and cell == "neither" and seed < 3:
                    continue  # declared reuse, never launch these again
                result.append(
                    dict(
                        learner=learner,
                        cell=cell,
                        seed=seed,
                        out=root / stage / f"{learner}_{cell}_s{seed}",
                    )
                )
    return result


def audit_run(out):
    """Every stored episode replays; budgets and validation rows agree exactly."""
    meta = json.loads((out / "config.json").read_text())
    summary = json.loads((out / "summary.json").read_text())
    cfg, learner = meta["config"], meta["learner"]
    store = EpisodeStore(out / "store")
    counts = Counter()
    for entry in store.index:
        replay_episode(store.load(entry), store.identity)
        counts[entry["kind"]] += entry["decisions"]
    if counts["training"] + counts["calibration"] != cfg["decisions"]:
        raise ValueError(f"{out}: stored decisions differ from budget")
    if counts["calibration"] != 10_000 or summary["decisions"] != cfg["decisions"]:
        raise ValueError(f"{out}: calibration/summary budget differs")
    for row in read_rows(out / "metrics.jsonl"):
        if any(not np.isfinite(v) for v in row.values() if isinstance(v, (int, float))):
            raise ValueError(f"{out}: nonfinite training metric")
    curve = read_rows(out / "curve.jsonl")
    for row in curve:
        episodes = store.query(checkpoint=row["decisions"])
        val = [e for e in episodes if e["condition"]["id"] != "nominal"]
        assert len(episodes) == 21 and len(val) == 20
        assert sum(e["outcome"] == "finished" for e in val) == row["val_finished"]
        assert np.mean([e["return"] for e in val]) == row["val_mean_return"]
    assert curve[-1]["decisions"] == cfg["decisions"]
    policy_cls = ppo.PPOPolicy if learner == "ppo" else sac.SACPolicy
    hold = cfg.get("m5", {}).get("hold", 1)
    for row in {
        r["decisions"]: r for r in (select_checkpoint(curve), curve[-1])
    }.values():
        path = out / "ckpt" / f"{row['decisions']:07d}"
        ep = run_episode(
            Task.load("U"), NOMINAL, HeldAction(policy_cls.load(path), hold)
        )
        assert ep["return"] == row["nominal_return"]
        assert ep["outcome"] == row["nominal_outcome"]
    return dict(
        run=str(out.relative_to(REPO)) if out.is_relative_to(REPO) else str(out),
        label=LABEL,
        summary=summary,
        exact_replays=len(store.index),
        raw_decisions_by_kind=dict(counts),
        selected=select_checkpoint(curve),
        nominal_reloads_verified=True,
    )


def audit_stage(stage, root=ROOT):
    results = [audit_run(job["out"]) for job in jobs(stage, root)]
    result = dict(label=LABEL, stage=stage, sources=source_hashes(), runs=results)
    (root / f"{stage}_audit.json").write_text(json.dumps(result, indent=1) + "\n")
    replays = sum(r["exact_replays"] for r in results)
    print(f"{stage}: {len(results)} runs, {replays} exact replays", flush=True)
    return result


def freeze(box_check, root=ROOT):
    if FREEZE.exists():
        raise ValueError("M5 freeze exists and is never rewritten")
    checked = json.loads((box_check / "results.json").read_text())
    if not checked["passes"] or checked["exact_replays"] != 100:
        raise ValueError("start box did not pass the declared 100-episode gate")
    box = checked["protocol"]["box"]
    audit = json.loads((root / "smoke_audit.json").read_text())
    if audit["sources"] != source_hashes() or len(audit["runs"]) != 8:
        raise ValueError("need eight audited smoke runs under the current sources")
    for job in jobs("smoke", root):
        cfg = json.loads((job["out"] / "config.json").read_text())["config"]
        if cfg != config(job["learner"], job["cell"], box, 100_000):
            raise ValueError("smoke settings differ from the checked box/config")
    configs = {
        f"{learner}_{cell}": config(learner, cell, box)
        for learner in ("ppo", "sac")
        for cell in CELLS
    }
    configs.update(
        {
            f"ppo_{cell}_extended": config("ppo", cell, box, 40_000_000)
            for cell in ("B", "BS")
        }
    )
    reuse = []
    for learner in ("ppo", "sac"):
        for seed in range(3):
            run = REPO / f"runs/ese3990/M4U/main/{learner}_U_s{seed}"
            meta = json.loads((run / "config.json").read_text())
            if meta["config"] != configs[f"{learner}_neither"]:
                raise ValueError("reused M4U run has different settings")
            reuse.append(
                dict(
                    learner=learner,
                    cell="neither",
                    seed=seed,
                    run=str(run.relative_to(REPO)),
                    hashes={
                        name: digest(run / name)
                        for name in ("config.json", "summary.json", "curve.jsonl")
                    },
                )
            )
    frozen = dict(
        label=LABEL,
        frozen_at_utc=datetime.now(UTC).isoformat(),
        task="U",
        **contract_identity(Task.load("U")),
        identity=build_identity(),
        training_seeds=list(range(5)),
        pilot_seed=100,
        start_box=box,
        starts=(
            "alternate frozen/random training episodes; uniform heading on "
            "random starts; calibration/evaluation frozen"
        ),
        cells={c: dict(hold=h, randomized_starts=s) for c, (h, s) in CELLS.items()},
        configs=configs,
        reuse=reuse,
        sources=source_hashes(),
        evidence={
            str(p.relative_to(REPO)): digest(p)
            for p in (
                box_check / "results.json",
                root / "smoke_audit.json",
                REPO / "scenarios/free_sail.json",
                REPO / "pyproject.toml",
                REPO / "uv.lock",
            )
        },
        selection="most validation finishes, highest mean return, earliest checkpoint",
        threshold=dict(
            seeds_finishing=3, seeds=5, val_finishes_per_seed=1, val_episodes=20
        ),
        q_feasible_band=[-22.3, 15.7],
        readings=READINGS,
        evaluation=(
            "frozen start/20 Hz/U gamma; B actions held for 10 decisions; "
            "test/shift only after held-out freeze"
        ),
        budget=(
            "4M actual raw decisions including 10k calibration; "
            "PPO B/BS additional 40M rows separate"
        ),
        checkpoint_rule=(
            "M4U targets evaluated at the next completed PPO rollout, "
            "as in M4U; final budget exact"
        ),
        second_stage=(
            "only if called for by first-stage readings; separate freeze required"
        ),
    )
    FREEZE.write_text(json.dumps(frozen, indent=1) + "\n")
    print(f"wrote {FREEZE}", flush=True)
    return frozen


def verify_freeze(path=FREEZE):
    fz = json.loads(path.read_text())
    for name, expected in {**fz["sources"], **fz["evidence"]}.items():
        if digest(REPO / name) != expected:
            raise ValueError(f"changed frozen input: {name}")
    if fz["identity"] != build_identity():
        raise ValueError("M5 physics identity changed")
    for reused in fz.get("reuse", []):
        for name, expected in reused["hashes"].items():
            if digest(REPO / reused["run"] / name) != expected:
                raise ValueError(f"changed reused M4U run: {reused['run']}/{name}")
    return fz


def run_queue(stage, root, workers, box):
    if workers < 1:
        raise ValueError("jobs must be positive")
    root.mkdir(parents=True, exist_ok=True)

    def stop(signum, frame):
        raise KeyboardInterrupt("queue stop requested; terminating owned trainers")

    signal.signal(signal.SIGTERM, stop)
    with (root / "queue.lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if stage != "smoke":
            verify_freeze()
        pending = []
        for job in jobs(stage, root):
            out = job["out"]
            if (out / "summary.json").exists():
                continue
            if out.exists():
                raise ValueError(
                    f"incomplete run retained at {out}; inspect before retry"
                )
            pending.append(job)
        active = []
        failed = []
        try:
            while pending or active:
                while pending and len(active) < workers:
                    job = pending.pop(0)
                    out = job["out"]
                    out.parent.mkdir(parents=True, exist_ok=True)
                    cmd = [
                        sys.executable,
                        str(HERE / "m5_train.py"),
                        "--learner",
                        job["learner"],
                        "--cell",
                        job["cell"],
                        "--seed",
                        str(job["seed"]),
                        "--out",
                        str(out),
                    ]
                    if stage == "smoke":
                        cmd += ["--smoke", "--box", *map(str, box)]
                    elif stage == "extended":
                        cmd += ["--extended"]
                    env = {
                        **os.environ,
                        "XLA_FLAGS": "--xla_cpu_multi_thread_eigen=false",
                    }
                    env.pop("PYTHONPATH", None)
                    with out.with_suffix(".log").open("w") as log:
                        proc = subprocess.Popen(
                            cmd, cwd=REPO, env=env, stdout=log, stderr=subprocess.STDOUT
                        )
                    active.append((proc, job))
                    print(f"started {out.name}: pid {proc.pid}", flush=True)
                for proc, job in list(active):
                    code = proc.poll()
                    if code is not None:
                        active.remove((proc, job))
                        print(f"finished {job['out'].name}: exit {code}", flush=True)
                        if code:
                            failed.append(str(job["out"]))
                            pending.clear()  # stop launching after any failure
                (root / "queue.json").write_text(
                    json.dumps(
                        dict(
                            label=LABEL,
                            stage=stage,
                            supervisor_pid=os.getpid(),
                            active=[
                                dict(pid=p.pid, run=str(j["out"])) for p, j in active
                            ],
                            pending=[str(j["out"]) for j in pending],
                            failed=failed,
                            updated_utc=datetime.now(UTC).isoformat(),
                        ),
                        indent=1,
                    )
                    + "\n"
                )
                if active:
                    time.sleep(2)
        finally:
            for proc, _ in active:
                proc.terminate()
            for proc, _ in active:
                proc.wait()
        if failed:
            raise RuntimeError(f"failed runs retained: {failed}")


def main():
    ap = argparse.ArgumentParser(__doc__)
    sub = ap.add_subparsers(dest="command", required=True)
    for cmd in ("run", "audit"):
        parser = sub.add_parser(cmd)
        parser.add_argument(
            "--stage", choices=("smoke", "main", "extended"), required=True
        )
        parser.add_argument("--root", type=Path, default=ROOT)
        if cmd == "run":
            parser.add_argument("--jobs", type=int, default=4)
            parser.add_argument("--box", type=float, nargs=4, default=START_BOX)
    parser = sub.add_parser("freeze")
    parser.add_argument("--box-check", type=Path, required=True)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = ap.parse_args()
    if args.command == "run":
        run_queue(args.stage, args.root.resolve(), args.jobs, args.box)
    elif args.command == "audit":
        audit_stage(args.stage, args.root.resolve())
    else:
        freeze(args.box_check.resolve(), args.root.resolve())


if __name__ == "__main__":
    main()
