"""Audit the five completed blocked-SAC budget runs and publish the paired comparison.

Run with --jobs 5 after training. Uses the existing M5 replay, selection and
held-out routines. Writes only under this budget study's output directory.
"""

from __future__ import annotations

import argparse
import fcntl
from collections import defaultdict
from pathlib import Path
from statistics import mean

import m5_compare as compare
import m5_report as report
import m5_study
from compare import digest
from m5_followup import audit_cached
from sailing import HERE, REPO, Task, contract_identity

BASE_RESULTS = REPO / "docs/ese3990/M5_results.json"
FREEZE = HERE / "configs/M5_sac_budget_freeze.json"


def check_prefix(run, baseline_run):
    """The shorter run can clip the block at its budget, unlike the long run."""
    baseline = report.curve(baseline_run)
    extension = report.curve(run)
    shorter_budget = max(baseline)
    shared = [d for d in sorted(baseline) if d < shorter_budget]
    crossing = min(d for d in extension if d >= shorter_budget)
    if crossing - shorter_budget >= 10:
        raise ValueError(f"{run}: missed the 4M checkpoint")
    matched = shared + ([crossing] if crossing == shorter_budget else [])
    for d in matched:
        checkpoint = Path("ckpt") / f"{d:07d}.npz"
        if baseline[d] != extension.get(d) or digest(run / checkpoint) != digest(
            baseline_run / checkpoint
        ):
            raise ValueError(f"{run}: parameters or validation diverged at {d}")
    return dict(
        matched_decisions=matched,
        baseline_final=shorter_budget,
        extension_crossing=crossing,
        baseline_finishes=baseline[shorter_budget]["val_finished"],
        crossing_finishes=extension[crossing]["val_finished"],
    )


def heldout_freeze(runs, config, hashes, label):
    """Seal validated checkpoints before loading any held-out conditions."""
    groups = []
    for run in runs:
        final = run["summary"]["decisions"]
        selected = run["selected"]["decisions"]
        for d in sorted({selected, final}):
            checkpoint = REPO / run["run"] / "ckpt" / f"{d:07d}"
            meta = report.load(checkpoint.with_suffix(".json"))
            if (
                meta["learner"] != "sac"
                or meta["training_seed"] != run["seed"]
                or meta["config"] != config
                or meta["decisions"] != d
            ):
                raise ValueError(f"{checkpoint}: checkpoint metadata differs")
            for suffix in (".json", ".npz"):
                path = checkpoint.with_suffix(suffix)
                hashes[compare.rel(path)] = digest(path)
            groups.append(
                dict(
                    id=f"{run['run_id']}_d{d:08d}",
                    run_id=run["run_id"],
                    learner="sac",
                    cell="B",
                    seed=run["seed"],
                    budget=final,
                    stage="extended",
                    decisions=d,
                    checkpoint=compare.rel(checkpoint),
                    hold=config["m5"]["hold"],
                    label=label,
                    roles=[
                        k
                        for k, v in (("selected", selected), ("final", final))
                        if d == v
                    ],
                )
            )
    return dict(
        label=label,
        task="U",
        **contract_identity(Task.load("U")),
        identity=compare.build_identity(),
        groups=groups,
        input_hashes=hashes,
        environment_seed=0,
        manifests=["nominal", "test", "shift"],
        selection="M5 validation selection, selected and final checkpoints only",
    )


def add_approach(row, audit):
    for role, decisions in (
        ("selected", row["selected_decisions"]),
        ("final", row["budget"]),
    ):
        dist = [
            e["min_distance_m"]
            for e in audit["validation_episode_diagnostics"]
            if e["checkpoint"] == decisions
        ]
        if len(dist) != row["validation"][role]["episodes"]:
            raise ValueError(
                f"missing validation trajectories for {row['run_id']} at {decisions}"
            )
        row["validation"][role]["mean_min_distance_m"] = mean(dist)


def markdown(result):
    threshold = result["threshold"]
    n = result["qualifying_seeds"]
    status = "met" if n >= threshold["seeds_finishing"] else "missed"
    lines = [
        "# Blocked SAC on U at 4M and 40M decisions",
        "",
        f"The 40M arm {status} the exploratory finish threshold. "
        f"{n} of {threshold['seeds']} seeds finished at least "
        f"{threshold['val_finishes_per_seed']} of {threshold['val_episodes']} "
        "selected validation episodes. "
        f"The threshold is {threshold['seeds_finishing']} seeds.",
        "",
        "The selected policies had six checkpoint opportunities at 4M and ten at 40M. "
        "The final rows compare fixed decision budgets. All distances are in meters. "
        "Gate distance averages each validation episode’s closest approach "
        "to the gate center.",
        "",
        "| Budget | Checkpoint | Validation finishes | Validation tacks | "
        "Max upwind | Gate distance | Test finishes | Shift finishes |",
        "|---|---|---:|---:|---:|---:|---:|---:|",
    ]
    for name in ("4M", "40M"):
        rows = result["rows"][name]
        for role in ("selected", "final"):
            val = [r["validation"][role] for r in rows]

            def counts(items, field="finished"):
                count = sum(i[field] for i in items)
                total = sum(i["episodes"] for i in items)
                return f"{count}/{total}"

            lines.append(
                f"| {name} | {role} | {counts(val)} | {counts(val, 'tack_episodes')} | "
                f"{mean(i['mean_max_y_m'] for i in val):.1f} | "
                f"{mean(i['mean_min_distance_m'] for i in val):.1f} | "
                f"{counts([r['heldout'][role]['test'] for r in rows])} | "
                f"{counts([r['heldout'][role]['shift'] for r in rows])} |"
            )
    lines += [
        "",
        f"All {result['training_exact_replays']:,} stored training-run episodes and "
        f"{result['heldout_exact_replays']:,} new held-out episodes replayed exactly. "
        "Shared checkpoint comparisons, training finishes, Q ranges, entropy "
        "temperatures and rudder travel are in results.json.",
        "",
    ]
    return "\n".join(lines)


def evaluate_study(root, frozen, workers):
    previous = m5_study.verify_freeze()
    baseline = report.load(BASE_RESULTS)
    baseline_audit_path = m5_study.ROOT / "main_audit.json"
    if (
        digest(baseline_audit_path)
        != baseline["inputs"][compare.rel(baseline_audit_path)]
    ):
        raise ValueError("4M training audit changed since the M5 report")
    seeds = frozen["training_seeds"]
    specs = [
        dict(
            id=f"extended_sac_B_s{s}",
            out=root / "extended" / f"sac_B_s{s}",
            learner="sac",
            cell="B",
            seed=s,
            stage="extended",
            budget=frozen["decision_budget"],
        )
        for s in seeds
    ]
    prefix = {}
    for spec in specs:
        compare.validate_run(spec, frozen)
        prefix[spec["seed"]] = check_prefix(
            spec["out"], m5_study.ROOT / "main" / spec["out"].name
        )
    paths = [
        FREEZE,
        BASE_RESULTS,
        baseline_audit_path,
        Path(__file__),
        HERE / "m5_compare.py",
        HERE / "m5_report.py",
        HERE / "m5_followup.py",
        HERE / "u_diagnostics.py",
    ]
    hashes = {**previous["sources"], **{compare.rel(p): digest(p) for p in paths}}
    runs = compare.parallel_map(
        audit_cached, [(root, spec, frozen, hashes) for spec in specs], workers
    )
    for run in runs:
        p = root / "audits" / f"{run['run_id']}.json"
        hashes[compare.rel(p)] = digest(p)
    held = root / "heldout"
    plan = heldout_freeze(
        runs, frozen["configs"]["sac_B_extended"], hashes, frozen["label"]
    )
    compare.evaluate(held, plan, workers)
    audited = compare.audit(held, workers)
    heldout = defaultdict(lambda: {"selected": {}, "final": {}})
    for h in audited["rows"]:
        for role in h["roles"]:
            heldout[h["run_id"]][role][h["manifest"]] = report.view(h)
    old = [
        r
        for r in baseline["rows"]
        if r["learner"] == "sac" and r["cell"] == "B" and r["stage"] == "main"
    ]
    old.sort(key=lambda r: r["seed"])
    if [r["seed"] for r in old] != seeds:
        raise ValueError("missing frozen 4M baseline seeds")
    new = [report.row(r, heldout, previous) for r in runs]
    old_audits = {r["run_id"]: r for r in report.load(baseline_audit_path)["runs"]}
    for row in old:
        add_approach(row, old_audits[row["run_id"]])
    for row, audited_run in zip(new, runs, strict=True):
        add_approach(row, audited_run)
    threshold = frozen["threshold"]
    result = dict(
        label=frozen["label"],
        threshold=threshold,
        prefix=prefix,
        training_exact_replays=sum(r["exact_replays"] for r in runs),
        heldout_exact_replays=audited["exact_replays"],
        qualifying_seeds=sum(
            r["validation"]["selected"]["finished"]
            >= threshold["val_finishes_per_seed"]
            for r in new
        ),
        rows={"4M": old, "40M": new},
        heldout_freeze_sha256=digest(held / "freeze.json"),
    )
    compare.atomic_json(root / "results.json", result)
    compare.atomic_text(root / "report.md", markdown(result))


def main():
    from m5_sac_budget import ROOT, verify

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--jobs", type=int, default=5)
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error("jobs must be positive")
    with (ROOT / "evaluation.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        evaluate_study(ROOT, verify(), args.jobs)


if __name__ == "__main__":
    main()
