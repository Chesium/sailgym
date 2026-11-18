"""M5 results: one committed summary of the finished, audited M5 artifacts.

Reads the main/extended training audits, the held-out freeze/replay audit and
the training freeze under runs/ese3990/M5. Applies the frozen threshold and
readings and writes docs/ese3990/M5_results.json. Runs no episode.
"""

from __future__ import annotations

import argparse
import json
from collections import defaultdict
from pathlib import Path

import numpy as np
from compare import digest
from m5 import FREEZE, LABEL
from m5_study import ROOT
from sailing import REPO


def load(path):
    return json.loads(Path(path).read_text())


def view(d):
    return dict(
        episodes=d["episodes"],
        finished=d["finished"],
        tack_episodes=d["episodes_with_a_tack"],
        failures=d["out_of_bounds"] + d["capsized"] + d["mark_missed"],
        mean_max_y_m=d["mean_max_y_m"],
        mean_rudder_travel_rad=d["mean_rudder_travel_rad_decision_sampled"],
    )


def in_band(q, lo, hi):
    if "q_min" in q:
        return q["q_min"]["min"] >= lo and q["q_max"]["max"] <= hi
    # Reused M4U rows logged only max |Q|; None when it cannot decide.
    m = q["q_abs_max"]["max"]
    return True if m <= min(-lo, hi) else False if m > max(-lo, hi) else None


def last_metrics(run):
    lines = (REPO / run / "metrics.jsonl").read_text().splitlines()
    return json.loads(lines[-1])


def curve(run):
    rows = (REPO / run / "curve.jsonl").read_text().splitlines()
    return {
        r["decisions"]: {k: v for k, v in r.items() if k != "label"}
        for r in map(json.loads, rows)
    }


def training_outcomes(run):
    out = dict(episodes=0, random_starts=0, finished=0, finished_random_start=0)
    for line in (REPO / run / "store/index.jsonl").read_text().splitlines():
        e = json.loads(line)
        if e["kind"] != "training":
            continue
        rand = any(e.get("m5_start_xy", (0.0, 0.0)))
        done = e["outcome"] == "finished"
        out["episodes"] += 1
        out["random_starts"] += rand
        out["finished"] += done
        out["finished_random_start"] += done and rand
    return out


def row(r, heldout, frozen):
    lo, hi = frozen["q_feasible_band"]
    sel = r["selected"]["decisions"]
    final = max(map(int, r["validation_diagnostics"]))
    cal = frozen["configs"][f"{r['learner']}_{r['cell']}"]["calibration_decisions"]
    out = dict(
        run_id=r["run_id"],
        run=r["run"],
        stage=r["stage"],
        learner=r["learner"],
        cell=r["cell"],
        seed=r["seed"],
        budget=r["budget"],
        reused_m4u="/M4U/" in r["run"],
        wall_s=r["summary"]["wall_s"],
        exact_replays=r["exact_replays"],
        training=training_outcomes(r["run"]),
        selected_decisions=sel,
        selected_is_calibration=sel == cal,
        selected_val_mean_return=r["selected"]["val_mean_return"],
        validation={
            "selected": view(r["validation_diagnostics"][str(sel)]),
            "final": view(r["validation_diagnostics"][str(final)]),
        },
        heldout=heldout[r["run_id"]],
    )
    if "q_diagnostics" in r:
        q = r["q_diagnostics"]
        out["sac"] = dict(
            q_abs_max=q["q_abs_max"]["max"],
            q_min=q.get("q_min", {}).get("min"),
            q_max=q.get("q_max", {}).get("max"),
            final_mean_q=q["q_mean"]["last"],
            in_feasible_band=in_band(q, lo, hi),
            final_alpha=last_metrics(r["run"])["alpha"],
        )
    return out


def cell_summary(rows, threshold):
    sel = [r["validation"]["selected"] for r in rows]
    test = [r["heldout"]["selected"] for r in rows]
    seeds = sum(v["finished"] >= threshold["val_finishes_per_seed"] for v in sel)
    out = dict(
        seeds=len(rows),
        seeds_finishing=seeds,
        finishes=seeds >= threshold["seeds_finishing"],
        selected_val_finishes=sum(v["finished"] for v in sel),
        selected_val_episodes=sum(v["episodes"] for v in sel),
        selected_val_tack_episodes=sum(v["tack_episodes"] for v in sel),
        selected_val_mean_return=float(
            np.mean([r["selected_val_mean_return"] for r in rows])
        ),
        selected_val_mean_max_y_m=float(np.mean([v["mean_max_y_m"] for v in sel])),
        selected_at_calibration=sum(r["selected_is_calibration"] for r in rows),
        selected_val_rudder_travel_rad=[
            min(v["mean_rudder_travel_rad"] for v in sel),
            max(v["mean_rudder_travel_rad"] for v in sel),
        ],
        heldout_selected={
            m: {
                k: sum(t[m][k] for t in test)
                for k in ("episodes", "finished", "tack_episodes", "failures")
            }
            for m in ("nominal", "test", "shift")
        },
        training={k: sum(r["training"][k] for r in rows) for k in rows[0]["training"]},
        wall_min=[round(r["wall_s"] / 60, 1) for r in rows],
    )
    if rows[0]["learner"] == "sac":
        out["q_in_band_seeds"] = sum(r["sac"]["in_feasible_band"] is True for r in rows)
        out["q_abs_max_range"] = [
            min(r["sac"]["q_abs_max"] for r in rows),
            max(r["sac"]["q_abs_max"] for r in rows),
        ]
        out["final_alpha_range"] = [
            min(r["sac"]["final_alpha"] for r in rows),
            max(r["sac"]["final_alpha"] for r in rows),
        ]
    return out


def readings(cells, frozen):
    def done(stage, learner, cell):
        return cells[f"{stage}/{learner}/{cell}"]["finishes"]

    rules = {
        "B finishes, S does not": lambda f: f("B") and not f("S"),
        "S finishes, B does not": lambda f: f("S") and not f("B"),
        "Only B + S finishes": lambda f: f("BS") and not (f("B") or f("S")),
        "No cell finishes": lambda f: (
            not any(f(c) for c in ("neither", "B", "S", "BS"))
        ),
    }
    out = []
    for observation, reading in frozen["readings"]:
        if observation in rules:
            applies = {
                learner: rules[observation](lambda c, lr=learner: done("main", lr, c))
                for learner in ("ppo", "sac")
            }
        elif observation.startswith("A PPO cell finishes only at 40M"):
            applies = any(
                done("extended", "ppo", c) and not done("main", "ppo", c)
                for c in ("B", "BS")
            )
        else:
            applies = any(
                done("main", "ppo", c) != done("main", "sac", c)
                for c in ("neither", "B", "S", "BS")
            )
        out.append(dict(observation=observation, reading=reading, applies=applies))
    return out


def extended_replicates_main(rows):
    out = {}
    for r in rows:
        if r["stage"] != "extended":
            continue
        main = curve(r["run"].replace("/extended/", "/main/"))
        ext = curve(r["run"])
        shared = sorted(set(main) & set(ext))
        out[r["run_id"]] = dict(
            shared_checkpoints=shared,
            identical=all(main[d] == ext[d] for d in shared),
        )
    return out


def build(root=ROOT, freeze=FREEZE):
    frozen = load(freeze)
    main, ext = load(root / "main_audit.json"), load(root / "extended_audit.json")
    held_freeze = root / "heldout/freeze.json"
    held = load(root / "heldout/replay_audit.json")
    if held["freeze_sha256"] != digest(held_freeze):
        raise ValueError("held-out replay audit does not match its freeze")
    if held["exact_replays"] != held["episodes"]:
        raise ValueError("held-out episodes did not all replay exactly")
    heldout = defaultdict(lambda: {"selected": {}, "final": {}})
    for h in held["rows"]:
        for role in h["roles"]:
            heldout[h["run_id"]][role][h["manifest"]] = view(h)
    rows = [row(r, heldout, frozen) for r in main["runs"] + ext["runs"]]
    groups = defaultdict(list)
    for r in rows:
        groups[f"{r['stage']}/{r['learner']}/{r['cell']}"].append(r)
    cells = {
        k: cell_summary(sorted(v, key=lambda r: r["seed"]), frozen["threshold"])
        for k, v in sorted(groups.items())
    }
    inputs = [
        freeze,
        root / "main_audit.json",
        root / "extended_audit.json",
        held_freeze,
        root / "heldout/replay_audit.json",
    ]
    return dict(
        label=LABEL,
        task="U (contract 2, gamma = 0.99975), 20 Hz, frozen start",
        inputs={str(p.relative_to(REPO)): digest(p) for p in inputs},
        threshold=frozen["threshold"],
        q_feasible_band=frozen["q_feasible_band"],
        exact_replays=dict(
            main=main["exact_replays"],
            extended=ext["exact_replays"],
            heldout=held["exact_replays"],
            heldout_groups=held["groups"],
        ),
        readings=readings(cells, frozen),
        extended_replicates_main=extended_replicates_main(rows),
        cells=cells,
        rows=rows,
    )


def main():
    ap = argparse.ArgumentParser(__doc__)
    ap.add_argument("--root", type=Path, default=ROOT)
    ap.add_argument("--out", type=Path, default=REPO / "docs/ese3990")
    args = ap.parse_args()
    result = build(args.root.resolve())
    (args.out / "M5_results.json").write_text(json.dumps(result, indent=1) + "\n")


if __name__ == "__main__":
    main()
