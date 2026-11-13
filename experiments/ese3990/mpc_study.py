"""MPC validation sweeps (never read test/shift conditions).

R: the five-setting E2 sweep (M3). U: the M4U H=40/H=160 sweep under the U
contract. Resumes at completed episodes in --out. ``--jobs N`` runs the
(setting, search seed) groups as separate processes, each with its own store.
Run --probes after the sweep to freeze the selected setting and record
nominal + five validation probes with planned paths.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import multiprocessing
import os
import platform
import time
from concurrent.futures import ProcessPoolExecutor
from dataclasses import asdict, replace
from pathlib import Path

import numpy as np
from evaluate import run_episode, summarize
from mpc import CEMMPC, CEMConfig
from sailing import (
    HERE,
    NOMINAL,
    REPO,
    RS,
    Task,
    contract_identity,
    load_manifest,
    run_metadata,
)
from viz.episode_store import EpisodeStore, EpisodeWriter, build_identity
from viz.replay import replay

SETTINGS = {
    "H40_K64": CEMConfig(),
    "H80_K64": CEMConfig(horizon=80),
    "H160_K64": CEMConfig(horizon=160),
    "H40_K32": CEMConfig(candidates=32),
    "H40_K128": CEMConfig(candidates=128),
}
SEEDS = (0, 1, 2)
SELECTION = "most validation finishes, then lower p50 latency, then setting name"
# M4U: the frozen M3 default (K=32, I=3, four elites) at H=40 and H=160.
M3_DEFAULT = CEMConfig(
    **json.loads((HERE / "configs/M3_freeze.json").read_text())["config"]
)
STUDIES = {
    "R": {"settings": SETTINGS, "freeze": "M3_freeze.json", "prefix": "M3"},
    "U": {
        "settings": {
            "H40_K32": M3_DEFAULT,
            "H160_K32": replace(M3_DEFAULT, horizon=160),
        },
        "freeze": "M4U_mpc_freeze.json",
        "prefix": "M4U",
        # Declared before any U outcome: H=160 is never presented as real-time.
        "labels": {
            "H40_K32": "real-time target (50 ms); see measured latency",
            "H160_K32": "offline, non-real-time",
        },
    },
}


def select(rows):
    # Predeclared selection: success, then lower p50 online cost, then name.
    return min(
        rows, key=lambda r: (-r["finished"], r["latency_ms"]["p50"], r["setting"])
    )


def load_episodes(out: Path) -> list[dict]:
    """The serial sweep's episodes.jsonl plus every parallel group's."""
    paths = [out / "episodes.jsonl", *sorted(out.glob("groups/*/episodes.jsonl"))]
    return [
        json.loads(s) for p in paths if p.exists() for s in p.read_text().splitlines()
    ]


def study_task(meta: dict) -> Task:
    return Task.load(meta["task"], meta.get("contract_version", 1))


def cost_summary(episodes):
    lat = np.concatenate([e["decision_latency_s"] for e in episodes])
    return {
        **summarize(episodes),
        "real_decisions": sum(e["decisions"] for e in episodes),
        "real_physics_steps": sum(e["physics_steps"] for e in episodes),
        "hypothetical_physics_steps": sum(
            e["planning"]["hypothetical_physics_steps"] for e in episodes
        ),
        "rollout_calls": sum(e["planning"]["rollout_calls"] for e in episodes),
        "candidate_rollouts": sum(
            e["planning"]["candidate_rollouts"] for e in episodes
        ),
        "wall_s": sum(e["wall_s"] for e in episodes),
        "latency_ms": dict(
            zip(
                ("p50", "p95", "max"),
                (1000 * np.percentile(lat, [50, 95, 100])).tolist(),
                strict=True,
            )
        ),
        "deadline_misses_50ms": int((lat > 0.05).sum()),
        "deadline_miss_rate": float((lat > 0.05).mean()),
    }


def report(out: Path):
    episodes = load_episodes(out)
    meta = json.loads((out / "study.json").read_text())
    rows = []
    for name, config in STUDIES[meta["task"]]["settings"].items():
        eps = [e for e in episodes if e["setting"] == name]
        if not eps:
            continue
        rows.append(
            {
                "setting": name,
                "config": asdict(config),
                **cost_summary(eps),
                "per_search_seed": {
                    str(s): cost_summary([e for e in eps if e["search_seed"] == s])
                    for s in SEEDS
                    if any(e["search_seed"] == s for e in eps)
                },
                "mean_return": float(np.mean([e["return"] for e in eps])),
                "failures": [
                    {
                        k: e[k]
                        for k in (
                            "condition",
                            "search_seed",
                            "outcome",
                            "reason",
                            "final_distance_m",
                        )
                    }
                    for e in eps
                    if e["outcome"] != "finished"
                ],
            }
        )
    result = {"meta": meta, "settings": rows}
    (out / "summary.json").write_text(json.dumps(result, indent=2))
    return result


def probes(out: Path):
    result = report(out)
    rows = result["settings"]
    study = STUDIES[result["meta"]["task"]]
    task = study_task(result["meta"])
    per_setting = len(load_manifest("validation")) * len(SEEDS)
    if len(rows) != len(study["settings"]) or any(
        r["episodes"] != per_setting for r in rows
    ):
        raise ValueError("finish every setting × validation condition × seed first")
    chosen = select(rows)
    labels = study.get("labels", {})
    frozen = {
        "selection": SELECTION,
        "setting": chosen["setting"],
        "config": chosen["config"],
        "search_seeds": list(SEEDS),
        "study": str(out),
        "privilege": "known-model planning with exact episode branches",
        "tail_value": 0,
        "validation_summary": chosen,
        **({"task": task.name, "label": labels[chosen["setting"]]} if labels else {}),
        **contract_identity(task),
        "identity": build_identity(),
        "sources_sha256": {
            name: hashlib.sha256((REPO / name).read_bytes()).hexdigest()
            for name in (
                "experiments/ese3990/mpc.py",
                "experiments/ese3990/mpc_study.py",
                "experiments/ese3990/evaluate.py",
                "experiments/ese3990/sailing.py",
                "crates/sailgym-py/src/env.rs",
                "crates/sailgym-env/src/episode.rs",
            )
        },
    }
    (out / study["freeze"]).write_text(json.dumps(frozen, indent=2))
    probe_dir = out / "probes"
    done = (
        {e["condition"]["id"] for e in EpisodeStore(probe_dir).index}
        if (probe_dir / "run.json").exists()
        else set()
    )
    # Probe search seed 0 is declared before viewing outcomes.
    with EpisodeWriter(probe_dir, frozen, shard_size=1) as writer:
        for cond in [NOMINAL, *load_manifest("validation")[:5]]:
            if cond.id in done:
                continue
            ep = run_episode(
                task,
                cond,
                CEMMPC(0, study["settings"][chosen["setting"]]),
                writer=writer,
                log_meta={
                    "kind": "probe",
                    "manifest": "nominal" if cond == NOMINAL else "validation",
                },
            )
            with (out / "probe_episodes.jsonl").open("a") as f:
                f.write(json.dumps(ep) + "\n")
            print(json.dumps({"probe": cond.id, "outcome": ep["outcome"]}), flush=True)
    store = EpisodeStore(out / "probes")
    errors = []
    for entry in store.index:
        log = store.load(entry)
        trajectory = replay(log, store.identity, task)
        # The selected candidate's first transition must equal the live one;
        # later points differ because MPC replans instead of committing.
        error = np.max(
            np.abs(log.plans[:, 1] - trajectory.states[1:, [RS["x"], RS["y"]]])
        )
        if error != 0:
            raise AssertionError(f"first-step prediction differs: {error}")
        errors.append(
            {
                "condition": entry["condition"]["id"],
                "max_first_step_error_m": float(error),
            }
        )
    (out / "probe_checks.json").write_text(json.dumps(errors, indent=2))
    eps = [
        json.loads(s) for s in (out / "probe_episodes.jsonl").read_text().splitlines()
    ]
    (out / "probe_summary.json").write_text(json.dumps(cost_summary(eps), indent=2))


def run_group(out: Path, dest: Path, name: str, seed: int):
    """One (setting, search seed) over the validation documents, resumably.

    ``dest`` holds this group's episodes.jsonl and store: ``out`` itself when
    serial, its own directory when it runs beside other processes.
    """
    meta = json.loads((out / "study.json").read_text())
    task = study_task(meta)
    config = STUDIES[meta["task"]]["settings"][name]
    done = {
        e["condition"]["id"]
        for e in load_episodes(out)
        if (e["setting"], e["search_seed"]) == (name, seed)
    }
    dest.mkdir(parents=True, exist_ok=True)
    path = dest / "episodes.jsonl"
    with EpisodeWriter(dest / "episodes", meta, shard_size=1) as writer:
        for cond in load_manifest("validation"):
            if cond.id in done:
                continue
            t0 = time.perf_counter()
            ep = run_episode(
                task,
                cond,
                CEMMPC(seed, config),
                seed=0,
                writer=writer,
                log_meta={
                    "setting": name,
                    "search_seed": seed,
                    "manifest": "validation",
                    "kind": "evaluation",
                },
            )
            ep.update(setting=name, search_seed=seed)
            with path.open("a") as f:
                f.write(json.dumps(ep) + "\n")
            print(
                json.dumps(
                    {
                        "setting": name,
                        "search_seed": seed,
                        "condition": cond.id,
                        "outcome": ep["outcome"],
                        "wall_s": time.perf_counter() - t0,
                    }
                ),
                flush=True,
            )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--task", choices=STUDIES, default="R")
    parser.add_argument("--jobs", type=int, default=1, help="processes")
    parser.add_argument("--probes", action="store_true")
    args = parser.parse_args()
    out = args.out
    if args.probes:
        probes(out)
        return
    out.mkdir(parents=True, exist_ok=True)
    task = Task.load(args.task)
    settings = STUDIES[args.task]["settings"]
    meta = run_metadata(
        task,
        settings={k: asdict(v) for k, v in settings.items()},
        search_seeds=SEEDS,
        manifest="validation",
        conditions=[c.to_dict() for c in load_manifest("validation")],
        selection=SELECTION,
        hardware=platform.processor(),
        platform=platform.platform(),
        rayon_threads=os.environ.get("RAYON_NUM_THREADS", "automatic"),
        # Latency depends on concurrent load, so a resume must keep the split.
        **({"processes": args.jobs} if args.jobs > 1 else {}),
    )
    header = out / "study.json"
    if header.exists():
        if json.loads(header.read_text()) != json.loads(json.dumps(meta)):
            raise ValueError("study metadata changed; use a new directory")
    else:
        header.write_text(json.dumps(meta, indent=2))
    groups = [(name, seed) for name in settings for seed in SEEDS]
    if args.jobs == 1:
        for name, seed in groups:
            run_group(out, out, name, seed)
            report(out)
    else:
        # Longest horizons first; K=32 caps each group's Rayon fan-out.
        groups.sort(key=lambda g: -settings[g[0]].horizon)
        spawn = multiprocessing.get_context("spawn")
        with ProcessPoolExecutor(args.jobs, mp_context=spawn) as ex:
            list(
                ex.map(
                    run_group,
                    [out] * len(groups),
                    [out / "groups" / f"{n}_s{s}" for n, s in groups],
                    *zip(*groups, strict=True),
                )
            )
    report(out)


if __name__ == "__main__":
    main()
