"""Freeze, run and audit the M5b exploration and learning-dynamics arms."""

import argparse
import fcntl
import json
import os
import subprocess
import sys
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor
from datetime import UTC, datetime

import m5_compare as compare
import m5_report as report
from compare import digest
from m5_followup import audit_cached
from m5_study import audit_run, verify_freeze
from m5b import ARMS, FREEZE, LABEL, ROOT, config
from sailing import HERE, REPO, Task, contract_identity

DOC = REPO / "docs/ese3990"
PARENT = HERE / "configs/M5_freeze.json"
BASE = DOC / "M5_results.json"
SEEDS = list(range(5))


def now():
    return datetime.now(UTC).isoformat()


def specs():
    return [
        dict(
            id=f"{arm}_s{s}",
            stage="main",
            out=ROOT / "main" / f"{arm}_s{s}",
            learner=ARMS[arm],
            cell=arm,
            seed=s,
            budget=4_000_000,
        )
        for arms in (("SAC-E", "PPO-8"), ("SAC-P", "PPO-sigma"))
        for s in SEEDS
        for arm in arms
    ]


def freeze():
    parent = verify_freeze()
    pilots = [ROOT / "smoke" / f"{arm}_s100" for arm in ARMS]
    for arm, path in zip(ARMS, pilots, strict=True):
        meta = report.load(path / "config.json")
        if meta["config"] != config(arm, parent, True) or meta["training_seed"] != 100:
            raise ValueError(f"Invalid smoke config: {path}")
    audited = compare.parallel_map(audit_run, pilots, 4)
    compare.atomic_json(ROOT / "smoke_audit.json", audited)
    cells = report.load(BASE)["cells"]
    best = {}
    for learner in ("ppo", "sac"):
        scores = {c: cells[f"main/{learner}/{c}"] for c in ("neither", "S", "B", "BS")}
        best[learner] = max(
            scores,
            key=lambda c: (
                scores[c]["seeds_finishing"],
                scores[c]["selected_val_tack_episodes"],
                scores[c]["selected_val_mean_max_y_m"],
            ),
        )
    if set(best.values()) != {"neither"}:
        raise ValueError(f"Declared cell rule selected {best}")
    paths = [
        PARENT,
        BASE,
        ROOT / "smoke_audit.json",
        *[
            HERE / f
            for f in (
                "m5b.py",
                "m5b_study.py",
                "m5_compare.py",
                "m5_followup.py",
                "m5_report.py",
                "u_diagnostics.py",
                "tests/test_m5b.py",
            )
        ],
    ]
    f = dict(
        label=LABEL,
        frozen_at_utc=now(),
        identity=parent["identity"],
        task="U",
        training_seeds=SEEDS,
        pilot_seed=100,
        decision_budget=4_000_000,
        configs={
            f"{learner}_{arm}": config(arm, parent) for arm, learner in ARMS.items()
        },
        threshold=parent["threshold"],
        q_feasible_band=parent["q_feasible_band"],
        baseline="M5 4M neither rows on the same five seeds",
        best_cell_rule=(
            "Qualifying seeds, settled-tack episodes, then mean maximum y on "
            "selected validation checkpoints"
        ),
        best_cells=best,
        treatments={
            "SAC-E": "Omit critic-target entropy; retain actor and temperature losses",
            "PPO-8": (
                "Eight persistent environments, 256 decisions each, "
                "shared condition stream and per-lane GAE boundaries"
            ),
            "SAC-P": (
                "clip(tanh(mu) + 0.3 * pink, -1, 1), independent "
                "beta=1 sequences per episode and action dimension"
            ),
            "PPO-sigma": "Initial log_std=ln(0.3)",
        },
        selection=parent["selection"],
        evaluation=parent["evaluation"],
        readings=[
            (
                "If an arm qualifies, compare with its same-seed baseline and "
                "report test and shift transfer."
            ),
            (
                "If SAC-E keeps Q inside the reference band without qualifying, "
                "correcting critic scale alone was insufficient."
            ),
            "If both arms of one learner qualify, report them separately.",
            (
                "If no arm qualifies, close M5 at these budgets. Reverse "
                "curricula, demonstrations and reset exploration are later work."
            ),
        ],
        sources={compare.rel(p): digest(p) for p in paths},
    )
    with FREEZE.open("x") as out:
        json.dump(f, out, indent=2)
        out.write("\n")
    return verify()


def verify():
    parent = verify_freeze()
    f = report.load(FREEZE)
    if f["identity"] != parent["identity"]:
        raise ValueError("Simulator identity differs")
    for name, h in f["sources"].items():
        if digest(REPO / name) != h:
            raise ValueError(f"Frozen input changed: {name}")
    return f


def worker(spec):
    out = spec["out"]
    if (out / "summary.json").is_file():
        compare.validate_run(spec, verify())
        return
    if out.exists():
        raise RuntimeError(f"Incomplete training directory retained at {out}")
    out.parent.mkdir(exist_ok=True, parents=True)
    with out.with_suffix(".log").open("a") as log:
        subprocess.run(
            [
                sys.executable,
                str(HERE / "m5b.py"),
                "main",
                spec["cell"],
                str(spec["seed"]),
            ],
            stdout=log,
            stderr=subprocess.STDOUT,
            check=True,
            cwd=REPO,
        )


def evaluate(f):
    jobs = specs()
    hashes = {
        **verify_freeze()["sources"],
        **f["sources"],
        compare.rel(FREEZE): digest(FREEZE),
    }
    runs = compare.parallel_map(audit_cached, [(ROOT, s, f, hashes) for s in jobs], 6)
    groups = []
    hashes = dict(hashes)
    for spec, run in zip(jobs, runs, strict=True):
        selected = run["selected"]["decisions"]
        hashes[compare.rel(ROOT / "audits" / f"{spec['id']}.json")] = digest(
            ROOT / "audits" / f"{spec['id']}.json"
        )
        for d in sorted({selected, spec["budget"]}):
            ckpt = spec["out"] / "ckpt" / f"{d:07d}"
            meta = report.load(ckpt.with_suffix(".json"))
            if (
                meta["decisions"] != d
                or meta["training_seed"] != spec["seed"]
                or meta["config"] != f["configs"][f"{spec['learner']}_{spec['cell']}"]
            ):
                raise ValueError(f"Wrong checkpoint: {ckpt}")
            for suffix in (".npz", ".json"):
                p = ckpt.with_suffix(suffix)
                hashes[compare.rel(p)] = digest(p)
            groups.append(
                dict(
                    id=f"{spec['id']}_d{d:08d}",
                    run_id=spec["id"],
                    learner=spec["learner"],
                    cell=spec["cell"],
                    seed=spec["seed"],
                    stage="main",
                    budget=spec["budget"],
                    decisions=d,
                    checkpoint=compare.rel(ckpt),
                    hold=1,
                    label=LABEL,
                    roles=[
                        r
                        for r, k in (("selected", selected), ("final", spec["budget"]))
                        if k == d
                    ],
                )
            )
    held = ROOT / "heldout"
    plan = dict(
        label=LABEL,
        task="U",
        **contract_identity(Task.load("U")),
        identity=f["identity"],
        groups=groups,
        input_hashes=hashes,
        environment_seed=0,
        manifests=["nominal", "test", "shift"],
        selection=f["selection"],
    )
    compare.evaluate(held, plan, 6)
    audit = compare.audit(held, 6)
    heldout = defaultdict(lambda: {"selected": {}, "final": {}})
    for h in audit["rows"]:
        for role in h["roles"]:
            heldout[h["run_id"]][role][h["manifest"]] = report.view(h)
    rows = [report.row(r, heldout, f) for r in runs]
    baseline = [
        r
        for r in report.load(BASE)["rows"]
        if r["stage"] == "main" and r["cell"] == "neither"
    ]
    cells = {
        arm: report.cell_summary([r for r in rows if r["cell"] == arm], f["threshold"])
        for arm in ARMS
    }
    summaries = {
        **{
            f"{learner.upper()} baseline": report.cell_summary(
                [r for r in baseline if r["learner"] == learner], f["threshold"]
            )
            for learner in ("sac", "ppo")
        },
        **cells,
    }
    winners = [arm for arm in ARMS if cells[arm]["finishes"]]
    reading = (
        ("Arms meeting the validation criterion: " + ", ".join(winners) + ".")
        if winners
        else (
            "M5 closes. None of the four arms met the validation finish "
            "threshold at 4M decisions."
        )
    )
    exact = sum(r["exact_replays"] for r in runs)
    result = dict(
        label=LABEL,
        reading=reading,
        rows=rows,
        baseline=baseline,
        summaries=summaries,
        threshold=f["threshold"],
        training_exact_replays=exact,
        heldout_exact_replays=audit["exact_replays"],
        heldout_freeze_sha256=digest(held / "freeze.json"),
    )
    compare.atomic_json(DOC / "M5b_results.json", result)
    lines = [
        "# M5b exploratory upwind study",
        "",
        reading,
        "",
        (
            "Each arm had five seeds. The frozen threshold required at least "
            "three seeds with at least one finish in 20 selected validation "
            "episodes. Held-out columns use those selected checkpoints."
        ),
        "",
        (
            "| Arm | Qualifying seeds | Validation finishes | Episodes with a "
            "tack | Mean max y (m) | Test finishes | Shift finishes | "
            "Training finishes |"
        ),
        "|---|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for name, c in summaries.items():
        t = c["heldout_selected"]
        lines.append(
            f"| {name} | {c['seeds_finishing']}/5 | {c['selected_val_finishes']}/100 "
            f"| {c['selected_val_tack_episodes']}/100 | "
            f"{c['selected_val_mean_max_y_m']:.1f} | {t['test']['finished']}/250 | "
            f"{t['shift']['finished']}/100 | {c['training']['finished']} |"
        )
    lines += [
        "",
        "SAC-E kept Q inside the reference band in "
        f"{cells['SAC-E']['q_in_band_seeds']}/5 seeds. "
        f"The audit reproduced {exact:,} training-run and "
        f"{audit['exact_replays']:,} held-out episodes. "
        "The paired seed data include selected and final checkpoints, "
        "critic scale, entropy temperature and rudder travel. "
        "[Results](M5b_results.json) · [Claims](M5b_claims.json)",
    ]
    compare.atomic_json(
        DOC / "M5b_claims.json",
        dict(
            label=LABEL,
            reading=reading,
            source_sha256=digest(DOC / "M5b_results.json"),
            inputs=hashes,
            heldout_audit=compare.rel(held / "replay_audit.json"),
            reproduce=(
                "env -u PYTHONPATH XLA_FLAGS=--xla_cpu_multi_thread_eigen=false "
                ".venv/bin/python experiments/ese3990/m5b_study.py evaluate"
            ),
        ),
    )
    compare.atomic_text(DOC / "M5b_report.md", "\n".join(lines) + "\n")
    compare.atomic_text(
        ROOT / "completion.md", reading + "\n\nReport: docs/ese3990/M5b_report.md\n"
    )


def supervise():
    with (ROOT / "study.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)

        def status(stage, **details):
            compare.atomic_json(
                ROOT / "status.json",
                dict(stage=stage, updated_utc=now(), pid=os.getpid(), **details),
            )

        try:
            verify()
            status("training")
            with ThreadPoolExecutor(max_workers=6) as pool:
                list(pool.map(worker, specs()))
            status("audit_and_evaluation")
            evaluate(verify())
            status("complete")
        except Exception as exc:
            status("failed", error=str(exc))
            raise


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["freeze", "verify", "run", "evaluate"])
    cmd = parser.parse_args().command
    if cmd == "evaluate":
        with (ROOT / "study.lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            evaluate(verify())
    elif cmd == "run":
        supervise()
    elif cmd == "freeze":
        freeze()
    else:
        verify()
