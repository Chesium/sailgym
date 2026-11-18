"""Wait for M5 main, then extended PPO, replay audits and frozen held-out work.

    m5_followup.py run --jobs 6

Only this follow-on process owns followup.lock; the existing main supervisor
continues untouched. Each dependent stage must succeed. Restarting skips finished
training runs and reuses replay audits only when every input byte still matches.
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
from datetime import UTC, datetime
from pathlib import Path

import m5_study
from compare import digest
from m5 import FREEZE, LABEL
from m5_compare import (
    all_runs,
    atomic_json,
    diagnostic_summary,
    episode_diagnostics,
    parallel_map,
    rel,
    run_fingerprint,
    validate_run,
)
from sailing import HERE, REPO
from viz.episode_store import EpisodeStore, build_identity

STEPS = (
    "wait_main",
    "extended",
    "main_audit",
    "extended_audit",
    "heldout_evaluate",
    "heldout_audit",
)


def protocol(root, workers):
    m5_study.verify_freeze()
    paths = (FREEZE, Path(__file__), HERE / "m5_compare.py", HERE / "u_diagnostics.py")
    return dict(
        label=LABEL,
        root=str(root),
        workers=workers,
        stages=list(STEPS),
        identity=build_identity(),
        sources={rel(p): digest(p) for p in paths},
        heldout="selected/final only; validation selection sealed before test/shift",
        failure="stop; retain results and incomplete runs; no second-stage tuning",
    )


def seal_protocol(root, planned):
    path = root / "followup_plan.json"
    if path.exists():
        if json.loads(path.read_text()) != planned:
            raise ValueError("follow-up sources/settings changed since launch")
    else:
        atomic_json(path, planned)


def wait_for_main(root, poll_seconds=10):
    """Do not start another training writer until the original lock is released."""
    while True:
        with (root / "queue.lock").open("a") as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                pass
            else:
                incomplete = [
                    str(j["out"])
                    for j in m5_study.jobs("main", root)
                    if not (j["out"] / "summary.json").exists()
                ]
                if incomplete:
                    raise RuntimeError(
                        f"main supervisor exited with incomplete runs: {incomplete}"
                    )
                return
        time.sleep(poll_seconds)


def audit_cached(args):
    root, spec, frozen, sources = args
    run = spec["out"]
    _, curve = validate_run(spec, frozen)
    signature = run_fingerprint(run)
    path = root / "audits" / f"{spec['id']}.json"
    if path.exists():
        cached = json.loads(path.read_text())
        if cached["fingerprint"] == signature and cached["sources"] == sources:
            print(f"cached audit {spec['id']}", flush=True)
            return cached["result"]
    result = m5_study.audit_run(run)
    # audit_run checks all episodes; diagnose only selected/final validation
    # episodes, keeping the main analyses on their declared frozen-start task.
    checkpoints = {result["selected"]["decisions"], curve[-1]["decisions"]}
    store = EpisodeStore(run / "store")
    entries = [
        e
        for e in store.index
        if e.get("checkpoint") in checkpoints
        and e["condition"]["id"] != "nominal"
        and e["kind"] in ("probe", "validation")
    ]
    rows = episode_diagnostics(store, entries)
    result.update(
        run_id=spec["id"],
        learner=spec["learner"],
        cell=spec["cell"],
        seed=spec["seed"],
        stage=spec["stage"],
        budget=spec["budget"],
        validation_diagnostics={
            str(d): diagnostic_summary([r for r in rows if r["checkpoint"] == d])
            for d in sorted(checkpoints)
        },
        validation_episode_diagnostics=rows,
    )
    if spec["learner"] == "sac":
        metrics = m5_study.read_rows(run / "metrics.jsonl")
        result["q_diagnostics"] = {
            key: dict(min=min(values), max=max(values), last=values[-1])
            for key in ("q_min", "q_max", "q_mean", "q_abs_max")
            if (values := [r[key] for r in metrics if key in r])
        }
        result["q_feasible_band"] = frozen["q_feasible_band"]
    atomic_json(path, dict(fingerprint=signature, sources=sources, result=result))
    print(f"audited {spec['id']}: {result['exact_replays']} exact episodes", flush=True)
    return result


def audit_stage(root, stage, workers):
    frozen = m5_study.verify_freeze()
    sources = protocol(root, workers)["sources"]
    specs = [s for s in all_runs(root, frozen) if s["stage"] == stage]
    results = parallel_map(
        audit_cached, [(root, s, frozen, sources) for s in specs], workers
    )
    result = dict(
        label=LABEL,
        stage=stage,
        sources=sources,
        runs=results,
        exact_replays=sum(r["exact_replays"] for r in results),
        note="main includes six reused M4U runs as M5 rows",
    )
    atomic_json(root / f"{stage}_audit.json", result)
    return result


def commands(root, workers):
    common = ["--root", str(root), "--jobs", str(workers)]
    python = sys.executable
    return {
        "extended": [
            python,
            str(HERE / "m5_study.py"),
            "run",
            "--stage",
            "extended",
            *common,
        ],
        "main_audit": [
            python,
            str(Path(__file__)),
            "audit-stage",
            "--stage",
            "main",
            *common,
        ],
        "extended_audit": [
            python,
            str(Path(__file__)),
            "audit-stage",
            "--stage",
            "extended",
            *common,
        ],
        "heldout_evaluate": [python, str(HERE / "m5_compare.py"), "evaluate", *common],
        "heldout_audit": [python, str(HERE / "m5_compare.py"), "audit", *common],
    }


def run_pipeline(root, workers):
    root.mkdir(parents=True, exist_ok=True)
    with (root / "followup.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        planned = protocol(root, workers)
        seal_protocol(root, planned)
        state = dict(
            label=LABEL,
            pid=os.getpid(),
            status="running",
            completed=[],
            started_utc=datetime.now(UTC).isoformat(),
        )
        child = None

        def stop(signum, frame):
            raise KeyboardInterrupt("follow-up stop requested")

        signal.signal(signal.SIGTERM, stop)
        try:
            for stage in STEPS:
                state.update(
                    stage=stage,
                    child_pid=None,
                    updated_utc=datetime.now(UTC).isoformat(),
                )
                atomic_json(root / "followup.json", state)
                print(f"starting stage {stage}", flush=True)
                if protocol(root, workers) != planned:
                    raise ValueError(
                        "follow-up sources/settings changed during execution"
                    )
                if stage == "wait_main":
                    wait_for_main(root)
                else:
                    env = {
                        **os.environ,
                        "XLA_FLAGS": "--xla_cpu_multi_thread_eigen=false",
                    }
                    env.pop("PYTHONPATH", None)
                    child = subprocess.Popen(
                        commands(root, workers)[stage],
                        cwd=REPO,
                        env=env,
                        start_new_session=True,
                    )
                    state["child_pid"] = child.pid
                    atomic_json(root / "followup.json", state)
                    code = child.wait()
                    if code:
                        raise RuntimeError(
                            f"stage {stage} failed with exit code {code}"
                        )
                    child = None
                state["completed"].append(stage)
            state.update(status="complete", stage="complete", child_pid=None)
        except BaseException as error:
            state.update(status="failed", error=str(error))
            raise
        finally:
            if child is not None and child.poll() is None:
                os.killpg(child.pid, signal.SIGTERM)
                try:
                    child.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait()
            state["updated_utc"] = datetime.now(UTC).isoformat()
            atomic_json(root / "followup.json", state)
    return state


def main():
    ap = argparse.ArgumentParser(__doc__)
    ap.add_argument("command", choices=("run", "audit-stage"))
    ap.add_argument("--root", type=Path, default=m5_study.ROOT)
    ap.add_argument("--jobs", type=int, default=6)
    ap.add_argument("--stage", choices=("main", "extended"))
    args = ap.parse_args()
    if args.jobs < 1:
        ap.error("jobs must be positive")
    root = args.root.resolve()
    if args.command == "run":
        run_pipeline(root, args.jobs)
    else:
        if args.stage is None:
            ap.error("audit-stage needs --stage")
        planned = protocol(root, args.jobs)
        seal_protocol(root, planned)
        audit_stage(root, args.stage, args.jobs)


if __name__ == "__main__":
    main()
