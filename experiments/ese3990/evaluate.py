"""One real-simulator evaluator for every controller (plan §4.4, E3).

Usage::

    uv run python experiments/ese3990/evaluate.py --task R --controller rule_sailor \
        --manifest nominal --out runs/ese3990/eval/R_rule_nominal.json

A controller is any object with ``reset(env)`` and ``act(env) -> action``.
Failed episodes are reported alongside successes; completion time is reported
conditionally on success and never averaged over failures.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path
from typing import Any

import numpy as np
from baselines import RuleSailorController, SmoothRandom
from sailing import (
    FINISHED,
    NOMINAL,
    RS,
    Condition,
    Task,
    TaskEnv,
    distance_to_target,
    load_manifest,
    run_metadata,
)
from viz.episode_store import EpisodeRecorder, EpisodeWriter


def run_episode(
    task: Task,
    cond: Condition,
    controller: Any,
    seed: int = 0,
    keep_trajectory: bool = False,
    writer: EpisodeWriter | None = None,
    log_meta: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """One episode. With ``writer`` it is also logged for replay (plan §10);
    ``log_meta`` adds fields such as run, kind or checkpoint."""
    env = TaskEnv(task, cond, seed)
    rs0 = env.reset()
    controller.reset(env)
    recorder = None
    if writer is not None:
        recorder = EpisodeRecorder(env, getattr(controller, "action_dtype", "float64"))
        recorder.start(rs0)
    ret = 0.0
    decisions = 0
    steps = 0
    peak_heel = abs(rs0[RS["phi"]])
    rudder_travel = 0.0
    sheet_travel = 0.0
    traj = [rs0.tolist()] if keep_trajectory else None
    actions = [] if keep_trajectory else None
    t0 = time.perf_counter()
    while env.outcome == "running":
        a = controller.act(env)
        rec = env.step(a)
        if recorder is not None:
            recorder.step(rec)
        ret += rec.reward
        decisions += 1
        steps += rec.steps
        peak_heel = max(peak_heel, abs(rec.next_rs[RS["phi"]]))
        rudder_travel += abs(rec.next_rs[RS["delta_r"]] - rec.rs[RS["delta_r"]])
        sheet_travel += abs(rec.next_rs[RS["l_sheet"]] - rec.rs[RS["l_sheet"]])
        if keep_trajectory:
            traj.append(rec.next_rs.tolist())
            actions.append(rec.action.tolist())
    wall = time.perf_counter() - t0
    final = env.research_state()
    out = {
        "condition": cond.to_dict(),
        "seed": seed,
        "outcome": env.outcome,
        "reason": env.raw.termination_reason,
        "finish_time_s": env.raw.finish_time,
        "decisions": decisions,
        "physics_steps": steps,
        "return": ret,
        "final_distance_m": distance_to_target(final, task),
        "peak_heel_deg": float(np.degrees(peak_heel)),
        "rudder_travel_rad_decision_sampled": rudder_travel,
        "sheet_travel_m_decision_sampled": sheet_travel,
        "wall_s": wall,
    }
    if hasattr(controller, "release_requests"):
        out["release_requests_overridden"] = controller.release_requests
    if recorder is not None:
        ident = controller.identity()
        log = recorder.finish(
            controller=ident["controller"],
            controller_identity=ident,
            **(log_meta or {}),
        )
        out["episode_id"] = writer.add(log)
    if keep_trajectory:
        out["trajectory"] = traj
        out["actions"] = actions
    return out


def summarize(episodes: list[dict[str, Any]]) -> dict[str, Any]:
    n = len(episodes)
    fin = [e for e in episodes if e["outcome"] == FINISHED]
    reasons: dict[str, int] = {}
    for e in episodes:
        k = e["reason"] or e["outcome"]
        reasons[k] = reasons.get(k, 0) + 1
    times = [e["finish_time_s"] for e in fin]
    return {
        "episodes": n,
        "finished": len(fin),
        "success_rate": len(fin) / n if n else float("nan"),
        "outcome_counts": reasons,
        "finish_time_s_given_success": {
            "mean": float(np.mean(times)) if times else None,
            "min": float(np.min(times)) if times else None,
            "max": float(np.max(times)) if times else None,
        },
        "peak_heel_deg_max": max(e["peak_heel_deg"] for e in episodes),
        "release_requests_overridden": sum(
            e.get("release_requests_overridden", 0) for e in episodes
        ),
    }


def select_checkpoint(curve: list[dict[str, Any]]) -> dict[str, Any]:
    """The validation-selected checkpoint of one training run (plan E3).

    Most validation documents finished; ties go to the higher mean validation
    return, then to the earlier checkpoint. Validation only: the selected
    checkpoint's validation score is a selection score, not a held-out result.
    The same rule applies to every learner.
    """
    return min(
        curve,
        key=lambda r: (-r["val_finished"], -r["val_mean_return"], r["decisions"]),
    )


def make_controller(name: str, seed: int, checkpoint: Path | None = None) -> Any:
    if name == "ppo":
        from ppo import PPOPolicy

        return PPOPolicy.load(checkpoint)
    if name == "sac":
        from sac import SACPolicy

        return SACPolicy.load(checkpoint)
    if name == "rule_sailor":
        return RuleSailorController()
    if name == "random":
        return SmoothRandom(seed)
    raise ValueError(name)


def evaluate(
    task_name: str,
    controller_name: str,
    manifest: str,
    seed: int = 0,
    store: Path | None = None,
    checkpoint: Path | None = None,
) -> dict[str, Any]:
    task = Task.load(task_name)
    conds = [NOMINAL] if manifest == "nominal" else load_manifest(manifest)
    ctrl = make_controller(controller_name, seed, checkpoint)
    writer = None
    if store is not None:
        writer = EpisodeWriter(
            store, {"task": task_name, "controller": controller_name}
        )
    meta = {"kind": "evaluation", "manifest": manifest}
    eps = [
        run_episode(task, c, ctrl, seed, writer=writer, log_meta=meta) for c in conds
    ]
    if writer is not None:
        writer.close()
    return {
        "meta": run_metadata(
            task, controller=ctrl.identity(), manifest=manifest, evaluation_seed=seed
        ),
        "summary": summarize(eps),
        "episodes": eps,
    }


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--task", default="R")
    ap.add_argument(
        "--controller",
        default="rule_sailor",
        choices=["rule_sailor", "random", "ppo", "sac"],
    )
    ap.add_argument("--manifest", default="nominal")
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--out", type=Path)
    ap.add_argument("--store", type=Path, help="log episodes for replay (plan §10)")
    ap.add_argument("--checkpoint", type=Path, help="ppo/sac: ckpt path without suffix")
    args = ap.parse_args(argv)
    a = args
    report = evaluate(a.task, a.controller, a.manifest, a.seed, a.store, a.checkpoint)
    print(json.dumps(report["summary"], indent=2))
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(report, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
