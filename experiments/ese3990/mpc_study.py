"""The five-setting E2 validation sweep (never reads test/shift conditions).

Resumes at completed episodes in --out. Run --probes after the sweep to freeze
the default and record nominal + five validation probes with planned paths.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import time
from dataclasses import asdict
from pathlib import Path

import numpy as np
from evaluate import run_episode, summarize
from mpc import CEMMPC, CEMConfig
from sailing import NOMINAL, REPO, RS, Task, load_manifest, run_metadata
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
    episodes = [
        json.loads(s) for s in (out / "episodes.jsonl").read_text().splitlines()
    ]
    rows = []
    for name, config in SETTINGS.items():
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
    result = {"meta": json.loads((out / "study.json").read_text()), "settings": rows}
    (out / "summary.json").write_text(json.dumps(result, indent=2))
    return result


def probes(out: Path):
    result = report(out)
    rows = result["settings"]
    if len(rows) != 5 or any(r["episodes"] != 60 for r in rows):
        raise ValueError("finish all 5 × 20 validation conditions × 3 seeds first")
    # Predeclared selection: success, then lower p50 online cost, then name.
    chosen = min(
        rows, key=lambda r: (-r["finished"], r["latency_ms"]["p50"], r["setting"])
    )
    frozen = {
        "selection": SELECTION,
        "setting": chosen["setting"],
        "config": chosen["config"],
        "search_seeds": list(SEEDS),
        "study": str(out),
        "privilege": "known-model planning with exact episode branches",
        "tail_value": 0,
        "validation_summary": chosen,
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
    (out / "M3_freeze.json").write_text(json.dumps(frozen, indent=2))
    task = Task.load("R")
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
                CEMMPC(0, SETTINGS[chosen["setting"]]),
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--probes", action="store_true")
    args = parser.parse_args()
    out = args.out
    if args.probes:
        probes(out)
        return
    out.mkdir(parents=True, exist_ok=True)
    task = Task.load("R")
    meta = run_metadata(
        task,
        settings={k: asdict(v) for k, v in SETTINGS.items()},
        search_seeds=SEEDS,
        manifest="validation",
        conditions=[c.to_dict() for c in load_manifest("validation")],
        selection=SELECTION,
        hardware=platform.processor(),
        platform=platform.platform(),
        rayon_threads=os.environ.get("RAYON_NUM_THREADS", "automatic"),
    )
    header = out / "study.json"
    if header.exists():
        if json.loads(header.read_text()) != json.loads(json.dumps(meta)):
            raise ValueError("study metadata changed; use a new directory")
    else:
        header.write_text(json.dumps(meta, indent=2))
    path = out / "episodes.jsonl"
    done = (
        {
            (e["setting"], e["search_seed"], e["condition"]["id"])
            for e in map(json.loads, path.read_text().splitlines())
        }
        if path.exists()
        else set()
    )
    with EpisodeWriter(out / "episodes", meta, shard_size=1) as writer:
        for name, config in SETTINGS.items():
            for seed in SEEDS:
                for cond in load_manifest("validation"):
                    key = (name, seed, cond.id)
                    if key in done:
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
            report(out)
    report(out)


if __name__ == "__main__":
    main()
