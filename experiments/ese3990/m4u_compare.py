"""Frozen M4U held-out comparison on U: every predeclared checkpoint, no
held-out selection (plan M4U, E3).

Groups: the four learner arms × training seeds × saved checkpoints, the
validation-selected U MPC (three search seeds) and RuleSailor, on nominal +
test + shift with environment seed 0. freeze.json is written before test/shift
are opened, and a restart with any changed input is refused. Each group runs
in one worker process and writes its own store and episodes/<group>.jsonl,
because EpisodeWriter is not multi-process safe; a restart skips completed
episodes.
"""

from __future__ import annotations

import argparse
import json
import multiprocessing
import os
import platform
import time
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

from compare import digest, write_freeze
from evaluate import make_controller, run_episode, select_checkpoint
from mpc import CEMMPC, CEMConfig
from sailing import HERE, NOMINAL, REPO, Task, contract_identity, load_manifest
from viz.episode_store import EpisodeWriter, build_identity

# Arm -> saved policy class. sac_demo and ppo_bc are expert-data arms (§9).
ARMS = {"ppo": "ppo", "sac": "sac", "sac_demo": "sac", "ppo_bc": "ppo"}
SEEDS = (0, 1, 2)
MAIN = REPO / "runs/ese3990/M4U/main"
MANIFESTS = ("nominal", "test", "shift")
SELECTION = (
    "Validation only (evaluate.select_checkpoint): most validation finishes, then "
    "greatest validation mean return, then earliest checkpoint. Final checkpoints "
    "and PPO + BC's initial checkpoint (role bc_only) are reported too."
)


def rel(path: Path) -> str:
    return os.path.relpath(path, REPO)


def read_jsonl(path: Path) -> list[dict]:
    return (
        [json.loads(s) for s in path.read_text().splitlines()] if path.exists() else []
    )


def prepare(
    mpc_freeze: Path, train_freeze: Path, thresholds: Path, main: Path = MAIN
) -> dict:
    """Every input of the held-out run, from training and validation only."""
    task = Task.load("U")
    contract = contract_identity(task)
    ident = build_identity()
    mpc = json.loads(mpc_freeze.read_text())
    if (
        mpc.get("task") != "U"
        or mpc["identity"] != ident
        or {k: mpc.get(k) for k in contract} != contract
    ):
        raise ValueError(f"{mpc_freeze}: not a U MPC freeze on this build/contract")
    groups = [
        {
            "id": f"mpc_search{seed}",
            "arm": "mpc",
            "controller": "mpc",
            "search_seed": seed,
            "config": mpc["config"],
            "roles": ["selected"],
        }
        for seed in mpc["search_seeds"]
    ]
    groups.append(
        {
            "id": "rule_sailor",
            "arm": "rule_sailor",
            "controller": "rule_sailor",
            "roles": ["reference"],
        }
    )
    trained = json.loads(train_freeze.read_text())["configs"]
    hashes = {}
    for arm, controller in ARMS.items():
        settings = []
        for seed in SEEDS:
            run = main / f"{arm}_U_s{seed}"
            config = json.loads((run / "config.json").read_text())
            cfg = config["config"]
            summary = json.loads((run / "summary.json").read_text())
            curve = read_jsonl(run / "curve.jsonl")
            store = json.loads((run / "store" / "run.json").read_text())
            if (
                config["task"] != "U"
                or cfg.get("arm") != arm
                or {k: config.get(k) for k in contract} != contract
            ):
                raise ValueError(f"{run}: not a {arm} run under the current U contract")
            if cfg != trained[arm]:
                raise ValueError(f"{run}: settings differ from the training freeze")
            if store["identity"] != ident:
                raise ValueError(f"{run}: trained on a different build")
            if summary["decisions"] != cfg["decisions"] or len(curve) != len(
                cfg["checkpoints"]
            ):
                raise ValueError(f"{run}: unfinished run or missing checkpoints")
            settings.append(cfg)
            selected = select_checkpoint(curve)["decisions"]
            initial = min(r["decisions"] for r in curve)
            for row in curve:
                decisions = row["decisions"]
                ckpt = run / "ckpt" / f"{decisions:07d}"
                meta = json.loads(ckpt.with_suffix(".json").read_text())
                if meta["training_seed"] != seed or meta["config"] != cfg:
                    raise ValueError(f"{ckpt}: checkpoint metadata differs")
                for suffix in (".json", ".npz"):
                    path = ckpt.with_suffix(suffix)
                    hashes[rel(path)] = digest(path)
                roles = ["curve"]
                if decisions == selected:
                    roles.append("selected")
                if decisions == cfg["decisions"]:
                    roles.append("final")
                if arm == "ppo_bc" and decisions == initial:
                    roles.append("bc_only")  # BC + value pretraining, no PPO update
                groups.append(
                    {
                        "id": f"{arm}_s{seed}_d{decisions:07d}",
                        "arm": arm,
                        "controller": controller,
                        "training_seed": seed,
                        "checkpoint": rel(ckpt),
                        "decisions": decisions,
                        "training_wall_s": meta["wall_s"],
                        "roles": roles,
                    }
                )
        if any(cfg != settings[0] for cfg in settings):
            raise ValueError(f"{arm}: training seeds differ in settings")
    return {
        "task": "U",
        **contract,
        "identity": ident,
        "groups": groups,
        "checkpoint_hashes": hashes,
        "freeze_hashes": {rel(p): digest(p) for p in (mpc_freeze, train_freeze)},
        # Diagnostics thresholds (u_diagnostics.py) are fixed before outcomes.
        "diagnostics_thresholds": {rel(thresholds): digest(thresholds)},
        "condition_hashes": {
            name: digest(HERE / "configs/conditions" / f"{name}.json")
            for name in ("validation", "test", "shift")
        },
        "evaluation_manifests": list(MANIFESTS),
        "environment_seed": 0,
        "selection": SELECTION,
        "mpc": {k: mpc[k] for k in ("setting", "config", "label", "search_seeds")},
        "evaluator_sha256": digest(Path(__file__)),
    }


def run_group(out: Path, group: dict, task: Task, conditions: dict) -> int:
    """Every missing (manifest, condition) of one group; returns how many ran."""
    path = out / "episodes" / f"{group['id']}.jsonl"
    done = {(e["manifest"], e["condition"]["id"]) for e in read_jsonl(path)}
    todo = [(m, c) for m, cs in conditions.items() for c in cs if (m, c.id) not in done]
    if not todo:
        return 0
    if group["controller"] == "mpc":
        controller = CEMMPC(group["search_seed"], CEMConfig(**group["config"]))
    else:
        checkpoint = REPO / group["checkpoint"] if "checkpoint" in group else None
        controller = make_controller(group["controller"], 0, checkpoint)
    path.parent.mkdir(parents=True, exist_ok=True)
    store_meta = {"group": group, "freeze_sha256": digest(out / "freeze.json")}
    with EpisodeWriter(out / "store" / group["id"], store_meta, shard_size=1) as w:
        for manifest, cond in todo:
            meta = {
                "group": group["id"],
                "manifest": manifest,
                "kind": "evaluation",
                "roles": group["roles"],
            }
            ep = run_episode(task, cond, controller, seed=0, writer=w, log_meta=meta)
            ep.update(group=group["id"], manifest=manifest)
            with path.open("a") as f:
                f.write(json.dumps(ep) + "\n")
    print(f"completed {group['id']} ({len(todo)} episodes)", flush=True)
    return len(todo)


def run(
    out: Path,
    frozen: dict,
    jobs: int = 1,
    task: Task | None = None,
    conditions: dict | None = None,
) -> None:
    """``task``/``conditions`` default to U and nominal + test + shift; tests
    pass tiny ones."""
    write_freeze(out, frozen)
    # Selection, roles and hashes are durable before held-out outcomes exist.
    conditions = conditions or {
        "nominal": [NOMINAL],
        **{name: load_manifest(name) for name in MANIFESTS[1:]},
    }
    task = task or Task.load(frozen["task"], frozen["contract_version"])
    with (out / "launches.jsonl").open("a") as f:
        launch = {
            "time": time.time(),
            "jobs": jobs,
            "rayon_threads": os.environ.get("RAYON_NUM_THREADS", "automatic"),
            "xla_flags": os.environ.get("XLA_FLAGS"),
            "hardware": platform.processor(),
        }
        f.write(json.dumps(launch) + "\n")
    groups = frozen["groups"]  # MPC first: the longest groups start first
    n = len(groups)
    args = ([out] * n, groups, [task] * n, [conditions] * n)
    if jobs == 1:
        list(map(run_group, *args))
    else:
        spawn = multiprocessing.get_context("spawn")  # no inherited JAX/Rayon state
        with ProcessPoolExecutor(jobs, mp_context=spawn) as ex:
            list(ex.map(run_group, *args))
    episodes = sum(
        len(read_jsonl(out / "episodes" / f"{g['id']}.jsonl")) for g in groups
    )
    (out / "complete.json").write_text(
        json.dumps({"groups": n, "episodes": episodes}, indent=2) + "\n"
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument(
        "--thresholds", type=Path, required=True, help="frozen diagnostics JSON"
    )
    parser.add_argument(
        "--mpc-freeze", type=Path, default=HERE / "configs/M4U_mpc_freeze.json"
    )
    parser.add_argument(
        "--train-freeze", type=Path, default=HERE / "configs/M4U_train_freeze.json"
    )
    parser.add_argument("--jobs", type=int, default=1, help="worker processes")
    args = parser.parse_args()
    frozen = prepare(args.mpc_freeze, args.train_freeze, args.thresholds)
    run(args.out, frozen, args.jobs)
