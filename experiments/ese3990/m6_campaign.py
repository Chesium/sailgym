"""Run the budgeted M6 campaign. Each worker has its own episode store."""

import json
import os
import subprocess
import sys
import time
from pathlib import Path

from compare import digest
from evaluate import make_controller, run_episode, select_checkpoint
from m6 import ROOT, atomic_json, audit, prepare, read
from sailing import HERE, NOMINAL, Task, load_manifest
from viz.episode_store import EpisodeWriter


def lines(path):
    return (
        [json.loads(s) for s in path.read_text().splitlines()] if path.exists() else []
    )


def workers(commands):
    jobs = []
    available = sorted(os.sched_getaffinity(0))
    for i, (name, command) in enumerate(commands):
        with (ROOT / f"{name}.log").open("a") as log:
            cpus = available[i * 3 : i * 3 + 3] or available[:3]
            proc = subprocess.Popen(
                ["taskset", "-c", ",".join(map(str, cpus)), sys.executable, *command],
                cwd=HERE,
                stdout=log,
                stderr=subprocess.STDOUT,
            )
        jobs.append((name, proc))
    failed = [name for name, proc in jobs if proc.wait()]
    if failed:
        raise RuntimeError(f"workers failed: {failed}")


def evaluate_run(path, group):
    task = Task.load("U")
    seal = read(ROOT / "selection.json")[str(path.relative_to(ROOT))]
    manifest = seal[group]
    ckpt = path / "ckpt" / f"{manifest['decisions']:07d}"
    if (
        digest(ckpt.with_suffix(".npz")) != manifest["sha256"]
        or digest(ckpt.with_suffix(".json")) != manifest["metadata_sha256"]
    ):
        raise ValueError("checkpoint changed")
    out = path / group
    policy = make_controller(path.name.split("_")[0], 0, ckpt)
    suites = ["validation"] if group == "pilot_final" else ["test", "shift"]
    with EpisodeWriter(out / "store", manifest, shard_size=12) as writer:
        for suite in suites:
            conditions = ([NOMINAL] if suite == "test" else []) + load_manifest(suite)
            for c in conditions:
                run_episode(
                    task,
                    c,
                    policy,
                    writer=writer,
                    log_meta=dict(
                        kind="nominal" if c.id == "nominal" else suite,
                        checkpoint=manifest["decisions"],
                    ),
                )
    audit(out)


def campaign():
    prepare(ROOT)
    clock = read(ROOT / "clock.json")["start"]

    def status(phase, **kw):
        atomic_json(
            ROOT / "status.json", dict(phase=phase, elapsed_s=time.time() - clock, **kw)
        )

    status("pilots")
    workers([(alg, ["m6_train.py", alg, "100"]) for alg in ("ppo", "sac")])
    pilot = read(ROOT / "pilot/ppo_s100/summary.json")
    predicted = pilot["wall_s"] * 10
    main = pilot["pilot_pass"] and predicted <= min(
        60 * 60, clock + 110 * 60 - time.time()
    )
    status("pilot decision", ppo_main=main, forecast_s=predicted)
    atomic_json(
        ROOT / "main_decision.json",
        dict(
            ppo_main=main,
            forecast_s=predicted,
            pilot=pilot,
            elapsed_s=time.time() - clock,
            freeze_sha256=digest(ROOT / "freeze.json"),
        ),
    )
    if main:
        workers(
            [(f"ppo_s{s}", ["m6_train.py", "ppo", str(s), "--main"]) for s in range(5)]
        )
    runs = sorted(ROOT.glob("pilot/*")) + sorted(ROOT.glob("main/*"))
    seal = {}
    for path in runs:
        summary = read(path / "summary.json")
        groups = {
            "pilot_final" if path.parent.name == "pilot" else "final": summary[
                "decisions"
            ]
        }
        if path.parent.name == "main" and summary["complete"]:
            groups["selected"] = select_checkpoint(lines(path / "curve.jsonl"))[
                "decisions"
            ]
        if path.parent.name == "main" and not summary["complete"]:
            groups = {}
        seal[str(path.relative_to(ROOT))] = {}
        for name, steps in groups.items():
            ckpt = path / "ckpt" / f"{steps:07d}"
            seal[str(path.relative_to(ROOT))][name] = dict(
                decisions=steps,
                sha256=digest(ckpt.with_suffix(".npz")),
                metadata_sha256=digest(ckpt.with_suffix(".json")),
            )
    atomic_json(ROOT / "selection.json", seal)
    status("evaluation and replay")
    tasks = [
        (
            f"eval_{Path(p).name}_{group}",
            ["m6_campaign.py", "eval", str(ROOT / p), group],
        )
        for p, groups in seal.items()
        for group in groups
    ]
    tasks.extend(
        (f"audit_{p.name}", ["m6.py", "audit", str(p)])
        for p in [*runs, *sorted(ROOT.glob("controls_*"))]
    )
    for i in range(0, len(tasks), 5):
        if time.time() >= clock + 150 * 60:
            status("time limit", pending=[name for name, _ in tasks[i:]])
            return
        workers(tasks[i : i + 5])
    status("complete", ppo_main=main)


if __name__ == "__main__":
    if sys.argv[1:2] == ["eval"]:
        evaluate_run(Path(sys.argv[2]), sys.argv[3])
    else:
        campaign()
