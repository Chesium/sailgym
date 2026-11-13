"""Verify saved learner runs without opening test conditions (M2).

Usage: python experiments/ese3990/audit_runs.py RUN... --out summary.json
Checks recorded costs/diagnostics, replays selected/final probes exactly, and
reproduces both checkpoints' nominal evaluation with the saved actor.
"""

from __future__ import annotations

import argparse
import json
from collections import Counter
from pathlib import Path

import numpy as np
from evaluate import make_controller, run_episode, select_checkpoint
from sailing import NOMINAL, Task, load_manifest
from viz.episode_store import EpisodeStore
from viz.replay import replay


def rows(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text().splitlines()]


def audit(run: Path) -> dict:
    config = json.loads((run / "config.json").read_text())
    summary = json.loads((run / "summary.json").read_text())
    curve = rows(run / "curve.jsonl")
    metrics = rows(run / "metrics.jsonl")
    store = EpisodeStore(run / "store")
    cfg = config["config"]
    learner = config["learner"]
    selected = select_checkpoint(curve)
    final = curve[-1]
    assert summary["decisions"] == final["decisions"] == cfg["decisions"]
    for row in metrics:
        for key, value in row.items():
            if isinstance(value, (int, float)):
                assert np.isfinite(value), (run, key, row["decisions"])
    counts = Counter()
    for episode in store.index:
        counts[episode["kind"]] += episode["decisions"]
    assert counts["calibration"] == cfg["calibration_decisions"]
    # M4U expert-data arms charge their demonstrations to the budget too.
    demos = cfg.get("demos", {}).get("decisions", 0)
    assert counts["training"] + counts["calibration"] + demos == cfg["decisions"]
    assert counts.keys() <= {"calibration", "training", "probe", "validation"}
    validation_ids = {c.id for c in load_manifest("validation")}
    for row in curve:
        episodes = store.query(checkpoint=row["decisions"])
        val = [e for e in episodes if e["condition"]["id"] != "nominal"]
        assert len(episodes) == len(val) + 1 == len(validation_ids) + 1
        assert {e["condition"]["id"] for e in val} == validation_ids
        assert sum(e["outcome"] == "finished" for e in val) == row["val_finished"]
        assert float(np.mean([e["return"] for e in val])) == row["val_mean_return"]
        assert (
            dict(Counter(e["reason"] or e["outcome"] for e in val))
            == row["val_outcome_counts"]
        )

    task = Task.load(cfg["task"])
    replayed = replay_decisions = evaluation_decisions = 0
    for checkpoint in {selected["decisions"], final["decisions"]}:
        probes = store.query(kind="probe", checkpoint=checkpoint)
        assert len(probes) == len(config["probe_ids"])
        for episode in probes:
            replay(store.load(episode), store.identity, task)
            replayed += 1
            replay_decisions += episode["decisions"]
        policy = make_controller(learner, 0, run / "ckpt" / f"{checkpoint:07d}")
        actual = run_episode(task, NOMINAL, policy)
        evaluation_decisions += actual["decisions"]
        recorded = next(row for row in curve if row["decisions"] == checkpoint)
        assert actual["return"] == recorded["nominal_return"]
        assert actual["outcome"] == recorded["nominal_outcome"]

    training = store.query(kind="training")
    diagnostic_keys = (
        (
            "q1_loss",
            "q2_loss",
            "actor_loss",
            "alpha",
            "entropy",
            "q_abs_max",
            "saturation",
        )
        if learner == "sac"
        else ("loss", "approx_kl", "clip_frac", "saturation")
    )
    return {
        "run": str(run),
        "learner": learner,
        "seed": config["training_seed"],
        "config": cfg,
        "identity": store.identity,
        "summary": summary,
        "curve": curve,
        "selected": selected,
        "real_decisions_by_kind": dict(counts),
        "training_outcomes": dict(
            Counter(e["reason"] or e["outcome"] for e in training)
        ),
        "probe_replays_verified": replayed,
        "verification_cost": {
            "actor_evaluation_decisions": evaluation_decisions,
            "replayed_decisions": replay_decisions,
        },
        "all_curve_rows_match_episode_index": True,
        "nominal_reload_verified": True,
        "finite_metrics_verified": True,
        "diagnostics": {
            k: {
                "last": metrics[-1][k],
                "min": min(r[k] for r in metrics),
                "max": max(r[k] for r in metrics),
            }
            for k in diagnostic_keys
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("runs", nargs="+", type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    results = []
    for run in args.runs:
        results.append(audit(run))
        print(f"verified {run}", flush=True)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(results, indent=2, allow_nan=False) + "\n")


if __name__ == "__main__":
    main()
