"""M5 selected/final held-out evaluation, with the original 20 Hz contract.

Selection uses validation only. Every selected checkpoint, action hold, source
and condition manifest is sealed before any test/shift episode runs. Episode
stores are the resume ledger; rebuilding JSON summaries never repeats episodes.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import multiprocessing
import os
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

import numpy as np
import u_diagnostics
from compare import digest
from evaluate import make_controller, run_episode, select_checkpoint
from m5 import FREEZE, LABEL, HeldAction, replay_episode
from m5_study import ROOT, jobs, read_rows, verify_freeze
from sailing import HERE, NOMINAL, REPO, RS, Task, contract_identity, load_manifest
from viz.episode_store import EpisodeStore, EpisodeWriter, build_identity


def atomic_text(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f"{path.name}.{os.getpid()}.tmp")
    tmp.write_text(text)
    tmp.replace(path)


def atomic_json(path, value):
    atomic_text(path, json.dumps(value, indent=1) + "\n")


def rel(path):
    return os.path.relpath(path, REPO)


def all_runs(root, frozen, extended=True):
    specs = []
    for stage in ("main", "extended") if extended else ("main",):
        for job in jobs(stage, root):
            specs.append(
                {
                    **job,
                    "stage": stage,
                    "id": f"{stage}_{job['out'].name}",
                    "budget": 40_000_000 if stage == "extended" else 4_000_000,
                }
            )
    for run in frozen["reuse"]:
        specs.append(
            dict(
                learner=run["learner"],
                cell="neither",
                seed=run["seed"],
                out=REPO / run["run"],
                stage="main",
                budget=4_000_000,
                id=f"main_{run['learner']}_neither_s{run['seed']}",
            )
        )
    return specs


def run_fingerprint(run):
    """Include shard bytes so a cached replay audit cannot hide changed actions."""
    paths = [
        run / n for n in ("config.json", "summary.json", "curve.jsonl", "metrics.jsonl")
    ]
    paths += sorted((run / "store").glob("*"))
    paths += sorted((run / "ckpt").glob("*"))
    hashes = {str(p.relative_to(run)): digest(p) for p in paths if p.is_file()}
    return hashlib.sha256(json.dumps(hashes, sort_keys=True).encode()).hexdigest()


def validate_run(spec, frozen):
    run = spec["out"]
    meta = json.loads((run / "config.json").read_text())
    summary = json.loads((run / "summary.json").read_text())
    curve = read_rows(run / "curve.jsonl")
    key = f"{spec['learner']}_{spec['cell']}"
    if spec["stage"] == "extended":
        key += "_extended"
    cfg = frozen["configs"][key]
    if (
        meta["config"] != cfg
        or meta["learner"] != spec["learner"]
        or meta["training_seed"] != spec["seed"]
        or meta["task"] != "U"
        or summary["decisions"] != cfg["decisions"]
        or len(curve) != len(cfg["checkpoints"])
        or curve[-1]["decisions"] != cfg["decisions"]
    ):
        raise ValueError(f"{run}: incomplete run or settings differ from M5 freeze")
    task = Task.load("U")
    if any(meta.get(k) != v for k, v in contract_identity(task).items()):
        raise ValueError(f"{run}: wrong U contract")
    if EpisodeStore(run / "store").identity != build_identity():
        raise ValueError(f"{run}: different physics build")
    return meta, curve


def prepare(root=ROOT):
    frozen = verify_freeze()
    groups, hashes = [], {rel(FREEZE): digest(FREEZE)}
    for name in ("main_audit.json", "extended_audit.json", "followup_plan.json"):
        path = root / name
        hashes[rel(path)] = digest(path)
    for spec in all_runs(root, frozen):
        meta, curve = validate_run(spec, frozen)
        run, cfg = spec["out"], meta["config"]
        selected = select_checkpoint(curve)["decisions"]
        final = cfg["decisions"]
        for name in ("config.json", "summary.json", "curve.jsonl", "store/run.json"):
            path = run / name
            hashes[rel(path)] = digest(path)
        for decisions in sorted({selected, final}):
            ckpt = run / "ckpt" / f"{decisions:07d}"
            saved = json.loads(ckpt.with_suffix(".json").read_text())
            if (
                saved["config"] != cfg
                or saved["decisions"] != decisions
                or saved["training_seed"] != spec["seed"]
                or saved["learner"] != spec["learner"]
            ):
                raise ValueError(f"{ckpt}: checkpoint metadata differs")
            for suffix in (".json", ".npz"):
                path = ckpt.with_suffix(suffix)
                hashes[rel(path)] = digest(path)
            groups.append(
                dict(
                    id=f"{spec['id']}_d{decisions:08d}",
                    run_id=spec["id"],
                    learner=spec["learner"],
                    cell=spec["cell"],
                    seed=spec["seed"],
                    budget=cfg["decisions"],
                    stage=spec["stage"],
                    decisions=decisions,
                    checkpoint=rel(ckpt),
                    hold=cfg.get("m5", {}).get("hold", 1),
                    roles=[
                        role
                        for role, d in (("selected", selected), ("final", final))
                        if d == decisions
                    ],
                    label=LABEL,
                )
            )
    for path in (
        Path(__file__),
        HERE / "u_diagnostics.py",
        HERE / "configs/M4U_diagnostics.json",
    ):
        hashes[rel(path)] = digest(path)
    for name in ("validation", "test", "shift"):
        path = HERE / f"configs/conditions/{name}.json"
        hashes[rel(path)] = digest(path)
    return dict(
        label=LABEL,
        task="U",
        **contract_identity(Task.load("U")),
        identity=build_identity(),
        groups=groups,
        input_hashes=hashes,
        environment_seed=0,
        manifests=["nominal", "test", "shift"],
        selection=(
            "validation finishes, mean return, earliest checkpoint; "
            "selected and final only"
        ),
        evaluation=(
            "frozen scenario start, 20 Hz rewards; hold B/BS actions for 10 decisions"
        ),
    )


def verify_inputs(frozen):
    verify_freeze()
    if frozen["identity"] != build_identity():
        raise ValueError("held-out physics identity changed")
    for name, expected in frozen["input_hashes"].items():
        if digest(REPO / name) != expected:
            raise ValueError(f"held-out input changed: {name}")


def seal(out, frozen):
    path = out / "freeze.json"
    if path.exists():
        if json.loads(path.read_text()) != frozen:
            raise ValueError("held-out frozen inputs changed; use a new directory")
    else:
        atomic_json(path, frozen)


def run_group(args):
    out, group, task, conditions = args
    store_dir = out / "store" / group["id"]
    target = {(m, c.id) for m, cs in conditions.items() for c in cs}
    entries = EpisodeStore(store_dir).index if (store_dir / "run.json").exists() else []
    keys = [(e["manifest"], e["condition"]["id"]) for e in entries]
    if len(keys) != len(set(keys)):
        raise ValueError(f"duplicate held-out episode in {store_dir}")
    if set(keys) - target:
        raise ValueError(f"unexpected held-out conditions in {store_dir}")
    done = set(keys)
    missing = [
        (m, c) for m, cs in conditions.items() for c in cs if (m, c.id) not in done
    ]
    if missing:
        policy = make_controller(group["learner"], 0, REPO / group["checkpoint"])
        held = HeldAction(policy, group["hold"])
        with EpisodeWriter(
            store_dir,
            {"group": group, "freeze_sha256": digest(out / "freeze.json")},
            shard_size=1,
        ) as w:
            for manifest, cond in missing:
                run_episode(
                    task,
                    cond,
                    held,
                    seed=0,
                    writer=w,
                    log_meta=dict(
                        group=group["id"],
                        manifest=manifest,
                        kind="evaluation",
                        roles=group["roles"],
                        label=LABEL,
                    ),
                )
    entries = EpisodeStore(store_dir).index
    atomic_text(
        out / "episodes" / f"{group['id']}.jsonl",
        "".join(json.dumps(e) + "\n" for e in entries),
    )
    print(f"held-out {group['id']}: {len(entries)} episodes", flush=True)
    return len(entries)


def parallel_map(fn, work, workers):
    if workers == 1:
        return list(map(fn, work))
    with ProcessPoolExecutor(
        workers, mp_context=multiprocessing.get_context("spawn")
    ) as ex:
        return list(ex.map(fn, work))


def evaluate(out, frozen, workers=6, task=None, conditions=None):
    verify_inputs(frozen)
    seal(out, frozen)  # before load_manifest or the first policy query
    conditions = conditions or {
        "nominal": [NOMINAL],
        **{n: load_manifest(n) for n in ("test", "shift")},
    }
    task = task or Task.load("U", frozen["contract_version"])
    work = [(out, group, task, conditions) for group in frozen["groups"]]
    counts = parallel_map(run_group, work, workers)
    result = dict(
        label=LABEL,
        groups=len(counts),
        episodes=sum(counts),
        freeze_sha256=digest(out / "freeze.json"),
    )
    atomic_json(out / "complete.json", result)
    return result


def episode_diagnostics(store, entries, task=None):
    rows = []
    cfg = u_diagnostics.load_config()
    for entry in entries:
        episode_task = task or Task.load(
            entry["task"], entry.get("contract_version", 1)
        )
        tr = replay_episode(store.load(entry), store.identity, episode_task)
        row = u_diagnostics.diagnose(tr, episode_task, cfg)
        row.update(
            manifest=entry.get("manifest", "validation"),
            checkpoint=entry.get("checkpoint"),
            rudder_travel_rad_decision_sampled=float(
                np.abs(np.diff(tr.states[:, RS["delta_r"]])).sum()
            ),
        )
        rows.append(row)
    return rows


def diagnostic_summary(rows):
    result = u_diagnostics.summarize(rows)
    result["mean_rudder_travel_rad_decision_sampled"] = (
        float(np.mean([r["rudder_travel_rad_decision_sampled"] for r in rows]))
        if rows
        else None
    )
    return result


def audit_group(args):
    out, group, task = args
    store = EpisodeStore(out / "store" / group["id"])
    rows = episode_diagnostics(store, store.index, task)
    result = dict(
        group=group,
        exact_replays=len(rows),
        episodes=rows,
        summaries={
            m: diagnostic_summary([r for r in rows if r["manifest"] == m])
            for m in sorted({r["manifest"] for r in rows})
        },
    )
    atomic_json(out / "diagnostics" / f"{group['id']}.json", result)
    return result


def audit(out, workers=6, task=None):
    frozen = json.loads((out / "freeze.json").read_text())
    verify_inputs(frozen)
    complete = json.loads((out / "complete.json").read_text())
    results = parallel_map(
        audit_group, [(out, g, task) for g in frozen["groups"]], workers
    )
    exact = sum(r["exact_replays"] for r in results)
    if exact != complete["episodes"]:
        raise ValueError("held-out evaluation and replay episode counts differ")
    rows = [
        {**r["group"], "manifest": m, **s}
        for r in results
        for m, s in r["summaries"].items()
    ]
    result = dict(
        label=LABEL,
        exact_replays=exact,
        episodes=complete["episodes"],
        groups=len(results),
        rows=rows,
        freeze_sha256=digest(out / "freeze.json"),
    )
    atomic_json(out / "replay_audit.json", result)
    return result


def main():
    ap = argparse.ArgumentParser(__doc__)
    ap.add_argument("command", choices=("evaluate", "audit"))
    ap.add_argument("--root", type=Path, default=ROOT)
    ap.add_argument("--jobs", type=int, default=6)
    args = ap.parse_args()
    if args.jobs < 1:
        ap.error("jobs must be positive")
    root = args.root.resolve()
    out = root / "heldout"
    if args.command == "evaluate":
        evaluate(out, prepare(root), args.jobs)
    else:
        audit(out, args.jobs)


if __name__ == "__main__":
    main()
