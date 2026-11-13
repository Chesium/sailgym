"""Frozen M4 comparison: all predeclared checkpoints, no held-out selection.

Writes the freeze before opening test/shift manifests. Every live episode is
stored for exact replay; restarting with the same inputs skips completed keys.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path

from evaluate import make_controller, run_episode, select_checkpoint
from sailing import HERE, NOMINAL, REPO, Task, load_manifest
from viz.episode_store import EpisodeWriter, build_identity

MAIN = REPO / "runs/ese3990/main"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def prepare(main: Path = MAIN) -> dict:
    m2_path, m3_path = (HERE / "configs" / f"M{i}_freeze.json" for i in (2, 3))
    m2 = json.loads(m2_path.read_text())
    m3 = json.loads(m3_path.read_text())
    if not (build_identity() == m2["identity"] == m3["identity"]):
        raise ValueError("the learners and planner must use the same build/contract")
    groups, hashes = [], {}
    for seed in m3["search_seeds"]:
        groups.append(
            {
                "id": f"mpc_search{seed}",
                "controller": "mpc",
                "search_seed": seed,
                "roles": ["selected"],
            }
        )
    groups.append(
        {"id": "rule_sailor", "controller": "rule_sailor", "roles": ["reference"]}
    )
    for learner, cfg in m2["configs"].items():
        for seed in m2["training_seeds"]:
            run = main / f"{learner}_R_s{seed}"
            config = json.loads((run / "config.json").read_text())
            summary = json.loads((run / "summary.json").read_text())
            curve = [
                json.loads(s) for s in (run / "curve.jsonl").read_text().splitlines()
            ]
            if config["config"] != cfg or summary["decisions"] != cfg["decisions"]:
                raise ValueError(f"{run}: settings/budget differ from M2 freeze")
            selected = select_checkpoint(curve)["decisions"]
            if len(curve) != len(cfg["checkpoints"]):
                raise ValueError(f"{run}: missing predeclared checkpoints")
            for row in curve:
                decisions = row["decisions"]
                ckpt = run / "ckpt" / f"{decisions:07d}"
                meta = json.loads(ckpt.with_suffix(".json").read_text())
                if meta["training_seed"] != seed or meta["config"] != cfg:
                    raise ValueError(f"{ckpt}: checkpoint metadata differs")
                for suffix in (".json", ".npz"):
                    path = ckpt.with_suffix(suffix)
                    hashes[str(path.relative_to(REPO))] = digest(path)
                roles = ["curve"]
                if decisions == selected:
                    roles.append("selected")
                if decisions == cfg["decisions"]:
                    roles.append("final")
                groups.append(
                    {
                        "id": f"{learner}_s{seed}_d{decisions:07d}",
                        "controller": learner,
                        "training_seed": seed,
                        "checkpoint": str(ckpt.relative_to(REPO)),
                        "decisions": decisions,
                        "training_wall_s": meta["wall_s"],
                        "roles": roles,
                    }
                )
    return {
        "task": "R",
        "identity": build_identity(),
        "groups": groups,
        "checkpoint_hashes": hashes,
        "freeze_hashes": {
            str(p.relative_to(REPO)): digest(p) for p in (m2_path, m3_path)
        },
        "condition_hashes": {
            name: digest(HERE / "configs/conditions" / f"{name}.json")
            for name in ("validation", "test", "shift")
        },
        "evaluation_manifests": ["nominal", "test", "shift"],
        "environment_seed": 0,
        "rayon_threads": os.environ.get("RAYON_NUM_THREADS", "automatic"),
        "selection": m2["checkpoint_selection"],
        "mpc": m3["config"],
        "upwind": "Deferred to keep M4 focused on the required R comparison.",
        "evaluator_sha256": digest(Path(__file__)),
    }


def write_freeze(out: Path, frozen: dict) -> None:
    out.mkdir(parents=True, exist_ok=True)
    path = out / "freeze.json"
    if path.exists() and json.loads(path.read_text()) != frozen:
        raise ValueError("frozen inputs changed; use a new comparison directory")
    path.write_text(json.dumps(frozen, indent=2) + "\n")


def compare(out: Path):
    frozen = prepare()
    write_freeze(out, frozen)
    # The selection and checkpoint hashes above are durable before held-out
    # outcomes exist. Test/shift results never change roles or parameters.
    manifests = {
        "nominal": [NOMINAL],
        **{n: load_manifest(n) for n in ("test", "shift")},
    }
    task = Task.load(frozen["task"])
    path = out / "episodes.jsonl"
    done = (
        {
            (e["group"], e["manifest"], e["condition"]["id"])
            for e in map(json.loads, path.read_text().splitlines())
        }
        if path.exists()
        else set()
    )
    with EpisodeWriter(out / "store", frozen, shard_size=1) as writer:
        for group in frozen["groups"]:
            controller = make_controller(
                group["controller"],
                group.get("search_seed", 0),
                REPO / group["checkpoint"] if "checkpoint" in group else None,
            )
            for manifest, conditions in manifests.items():
                for cond in conditions:
                    key = (group["id"], manifest, cond.id)
                    if key in done:
                        continue
                    meta = {
                        "group": group["id"],
                        "manifest": manifest,
                        "kind": "evaluation",
                        "roles": group["roles"],
                    }
                    ep = run_episode(
                        task, cond, controller, seed=0, writer=writer, log_meta=meta
                    )
                    ep.update(group=group["id"], manifest=manifest)
                    with path.open("a") as f:
                        f.write(json.dumps(ep) + "\n")
                print(f"completed {group['id']} {manifest}", flush=True)
    (out / "complete.json").write_text(
        json.dumps(
            {
                "groups": len(frozen["groups"]),
                "episodes": sum(1 for _ in path.open()),
            },
            indent=2,
        )
        + "\n"
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    compare(args.out)
