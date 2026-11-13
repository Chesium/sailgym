"""Verify and summarize the frozen M4U held-out run; figures and video specs.

    python experiments/ese3990/m4u_report.py runs/ese3990/M4U/heldout --out docs/ese3990

Reuses M4's aggregate (outcomes, conditional finish times, costs, latency).
Expert-data arms are labeled as such in every row, figure and video title.
"""

from __future__ import annotations

import argparse
import json
import multiprocessing
import os
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402
import u_diagnostics  # noqa: E402
from compare import digest  # noqa: E402
from comparison_report import aggregate  # noqa: E402
from m4u_compare import ARMS, read_jsonl, rel  # noqa: E402
from sailing import REPO, Task, contract_identity, load_manifest  # noqa: E402
from viz.episode_store import EpisodeStore  # noqa: E402

LABELS = {
    "ppo": "PPO",
    "sac": "SAC",
    "sac_demo": "SAC + demonstrations (expert data)",
    "ppo_bc": "PPO + BC (expert data)",
    "mpc": "True-model MPC",
    "rule_sailor": "RuleSailor",
}
EXPERT = ("sac_demo", "ppo_bc")
# The controllers compared in media and first failures; BC only is PPO + BC's
# initial checkpoint (no extra run).
CONTROLLERS = [
    *[(arm, "selected") for arm in ARMS],
    ("ppo_bc", "bc_only"),
    ("mpc", "selected"),
    ("rule_sailor", "reference"),
]
ROWS = [
    *[(arm, role) for arm in ARMS for role in ("selected", "final")],
    *CONTROLLERS[len(ARMS) :],
]


def label(arm: str, role: str) -> str:
    return "BC only (expert data)" if role == "bc_only" else LABELS[arm]


def groups_of(frozen: dict, arm: str, role: str) -> list[dict]:
    return [g for g in frozen["groups"] if g["arm"] == arm and role in g["roles"]]


def load(run: Path) -> tuple[dict, list[dict]]:
    frozen = json.loads((run / "freeze.json").read_text())
    episodes = [
        e
        for g in frozen["groups"]
        for e in read_jsonl(run / "episodes" / f"{g['id']}.jsonl")
    ]
    return frozen, episodes


def verify(run: Path, frozen: dict, episodes: list[dict]) -> dict:
    """Every frozen hash, the episode set, the stores and nominal reloads."""
    for name, expected in {
        **frozen["checkpoint_hashes"],
        **frozen["freeze_hashes"],
        **frozen["diagnostics_thresholds"],
        **{
            f"experiments/ese3990/configs/conditions/{name}.json": h
            for name, h in frozen["condition_hashes"].items()
        },
        "experiments/ese3990/m4u_compare.py": frozen["evaluator_sha256"],
    }.items():
        if digest(REPO / name) != expected:
            raise ValueError(f"frozen input changed: {name}")
    contract = contract_identity(Task.load(frozen["task"], frozen["contract_version"]))
    if contract != {k: frozen[k] for k in contract}:
        raise ValueError("contract amendment changed")
    expected = {
        (g["id"], m, c)
        for g in frozen["groups"]
        for m in frozen["evaluation_manifests"]
        for c in (["nominal"] if m == "nominal" else [x.id for x in load_manifest(m)])
    }
    actual = [(e["group"], e["manifest"], e["condition"]["id"]) for e in episodes]
    if len(actual) != len(set(actual)) or set(actual) != expected:
        raise ValueError("missing, duplicate or unexpected evaluation episodes")
    unreported = reloads = 0
    for group in frozen["groups"]:
        store = EpisodeStore(run / "store" / group["id"])
        if store.identity != frozen["identity"]:
            raise ValueError(f"{group['id']}: store and frozen build/contract differ")
        stored = {e["episode_id"] for e in store.index}
        reported = {e["episode_id"] for e in episodes if e["group"] == group["id"]}
        if not reported <= stored:
            raise ValueError(f"{group['id']}: reported episodes missing from store")
        # Only an interruption between storing and reporting leaves extras.
        unreported += len(stored - reported)
        if "checkpoint" not in group:
            continue
        run_dir = (REPO / group["checkpoint"]).parent.parent
        curve = read_jsonl(run_dir / "curve.jsonl")
        recorded = next(r for r in curve if r["decisions"] == group["decisions"])
        nominal = next(
            e
            for e in episodes
            if e["group"] == group["id"] and e["manifest"] == "nominal"
        )
        if nominal["return"] != recorded["nominal_return"]:
            raise ValueError(f"{group['id']}: nominal reload differs from training")
        reloads += 1
    return {
        "expected_episodes": len(expected),
        "frozen_hashes_unchanged": True,
        "nominal_checkpoint_reloads_verified": reloads,
        "unreported_store_entries": unreported,
    }


def summarize_rows(frozen: dict, episodes: list[dict]) -> list[dict]:
    result = []
    for arm, role in ROWS:
        groups = groups_of(frozen, arm, role)
        ids = {g["id"] for g in groups}
        for manifest in frozen["evaluation_manifests"] if groups else ():
            rows = [
                e for e in episodes if e["group"] in ids and e["manifest"] == manifest
            ]
            result.append(
                {
                    "arm": arm,
                    "role": role,
                    "label": label(arm, role),
                    "manifest": manifest,
                    **aggregate(rows),
                    "per_seed": {
                        g["id"]: aggregate([e for e in rows if e["group"] == g["id"]])
                        for g in groups
                    },
                }
            )
    return result


def video_specs(run: Path, frozen: dict, episodes: list[dict]):
    """viz/compose.py specs from stored episodes, and the first held-out failure
    (by manifest ID) of each controller."""

    def item(group, condition="nominal"):
        tag = " [expert data]" if group["arm"] in EXPERT else ""
        return {
            "store": rel(run / "store" / group["id"]),
            "query": {"group": group["id"], "condition": condition},
            "label": group["id"] + tag,
        }

    picked = {c: groups_of(frozen, *c) for c in CONTROLLERS}
    specs = [
        {
            "id": "controllers_nominal",
            "title": "U: validation-selected controllers, BC only and references",
            "episodes": [item(gs[0]) for gs in picked.values() if gs],
            "timeseries": True,
        }
    ]
    for arm in ARMS:
        specs += [
            {
                "id": f"{arm}_checkpoints",
                "title": f"U: {LABELS[arm]} seed 0, predeclared checkpoints",
                "episodes": [
                    item(g)
                    for g in frozen["groups"]
                    if g["arm"] == arm and g["training_seed"] == 0
                ],
            },
            {
                "id": f"{arm}_seeds",
                "title": f"U: {LABELS[arm]}, validation-selected training seeds",
                "episodes": [item(g) for g in picked[(arm, "selected")]],
            },
        ]
    specs.append(
        {
            "id": "mpc_search_seeds",
            "title": "U: MPC optimizer repetitions (not training seeds)",
            "episodes": [item(g) for g in picked[("mpc", "selected")]],
        }
    )
    failures = {}
    for (arm, role), gs in picked.items():
        name = "bc_only" if role == "bc_only" else arm
        ids = {g["id"] for g in gs}
        bad = [
            e
            for e in episodes
            if e["group"] in ids
            and e["manifest"] != "nominal"
            and e["outcome"] != "finished"
        ]
        first = min(bad, key=lambda e: (e["condition"]["id"], e["group"]), default=None)
        failures[name] = (
            {
                k: first[k]
                for k in ("group", "condition", "manifest", "outcome", "reason")
            }
            if first
            else None
        )
        if first:
            group = next(g for g in gs if g["id"] == first["group"])
            specs.append(
                {
                    "id": f"{name}_first_failure",
                    "title": f"U: {label(arm, role)} first failure "
                    f"({first['condition']['id']})",
                    "episodes": [item(group, first["condition"]["id"])],
                    "timeseries": True,
                }
            )
    for spec in specs:
        spec.update(fps=10, speed=6)
    return [s for s in specs if s["episodes"]], failures


def learning_figure(frozen: dict, episodes: list[dict], summary: list[dict], path):
    fig, ax = plt.subplots(figsize=(9, 4.5), layout="constrained")
    colors = ("#2a78d6", "#eb6834", "#8a3ffc", "#1d9a6c")
    for arm, color in zip(ARMS, colors, strict=True):
        seeds = sorted(
            {g["training_seed"] for g in frozen["groups"] if g["arm"] == arm}
        )
        xs, ys = [], []
        for seed in seeds:
            groups = sorted(
                (
                    g
                    for g in frozen["groups"]
                    if g["arm"] == arm and g["training_seed"] == seed
                ),
                key=lambda g: g["decisions"],
            )
            xs.append([g["decisions"] / 1e6 for g in groups])
            ys.append(
                [
                    np.mean(
                        [
                            e["outcome"] == "finished"
                            for e in episodes
                            if e["group"] == g["id"] and e["manifest"] == "test"
                        ]
                    )
                    for g in groups
                ]
            )
            ax.plot(xs[-1], ys[-1], color=color, alpha=0.35, lw=1, marker=".")
        if not xs:
            continue
        x, y = np.mean(xs, axis=0), np.array(ys)
        ax.plot(
            x,
            y.mean(axis=0),
            color=color,
            lw=2,
            label=f"{LABELS[arm]} mean ({len(seeds)} training seeds)",
        )
        ax.fill_between(x, y.min(axis=0), y.max(axis=0), color=color, alpha=0.12)
        if arm in EXPERT:
            first = min(
                (g for g in frozen["groups"] if g["arm"] == arm),
                key=lambda g: g["decisions"],
            )
            meta = json.loads(
                (REPO / first["checkpoint"]).with_suffix(".json").read_text()
            )
            calibration = meta["config"]["calibration_decisions"]
            ax.annotate(
                f"initial = {calibration:,} calibration + "
                f"{first['decisions'] - calibration:,} demonstration decisions",
                (x[0], y.mean(axis=0)[0]),
                xytext=(8, 8 if arm == "sac_demo" else -14),
                textcoords="offset points",
                fontsize=7,
                color=color,
            )
    for arm, role, style in (
        ("mpc", "selected", (0, (6, 6))),
        ("rule_sailor", "reference", (6, (6, 6))),
    ):
        row = next(
            (
                r
                for r in summary
                if (r["arm"], r["role"], r["manifest"]) == (arm, role, "test")
            ),
            None,
        )
        if row:
            ax.axhline(
                row["success_rate"],
                color="#228833" if arm == "mpc" else "#777777",
                ls=style,
                lw=1,
                label=f"{LABELS[arm]} reference (fixed; not a learning curve)",
            )
    ax.set(
        xlabel="Real decisions (millions; calibration and demonstrations included)",
        ylabel="Held-out test success (50 conditions)",
        ylim=(-0.04, 1.04),
        title="U: success versus real decisions",
    )
    ax.grid(alpha=0.2)
    ax.legend(fontsize=7, loc="upper left")
    fig.savefig(path, dpi=150)
    plt.close(fig)


def diagnostics(
    run: Path, frozen: dict, episodes: list[dict], jobs: int
) -> tuple[dict, dict]:
    """``u_diagnostics`` on every stored held-out episode (each replayed
    exactly), under the frozen thresholds file whose hash ``verify`` checked.
    Returns the per-row summaries and the per-episode results."""
    (path,) = frozen["diagnostics_thresholds"]
    cfg = u_diagnostics.load_config(REPO / path)
    work = []
    for g in frozen["groups"]:
        store = run / "store" / g["id"]
        k = len(EpisodeStore(store).index)
        work += [(str(store), lo, min(lo + 8, k), cfg) for lo in range(0, k, 8)]
    if jobs == 1:
        parts = map(u_diagnostics._chunk, work)
    else:
        spawn = multiprocessing.get_context("spawn")  # no inherited JAX/Rayon state
        ex = ProcessPoolExecutor(jobs, mp_context=spawn)
        parts = ex.map(u_diagnostics._chunk, work)
    per = {}
    for (store, *_), part in zip(work, parts, strict=True):
        per.update({f"{Path(store).name}/{eid}": d for eid, d in part.items()})
    if jobs != 1:
        ex.shutdown()
    rows = []
    for arm, role in ROWS:
        ids = {g["id"] for g in groups_of(frozen, arm, role)}
        for manifest in frozen["evaluation_manifests"] if ids else ():
            d = [
                per[f"{e['group']}/{e['episode_id']}"]
                for e in episodes
                if e["group"] in ids and e["manifest"] == manifest
            ]
            rows.append(
                {
                    "arm": arm,
                    "role": role,
                    "label": label(arm, role),
                    "manifest": manifest,
                    **u_diagnostics.summarize(d),
                }
            )
    return {"thresholds_file": path, "thresholds": cfg, "rows": rows}, per


def diagnostics_table(rows: list[dict]) -> str:
    lines = [
        "| Controller / role | Set | Episodes with a tack | Mean tacks | "
        "Mean failed tacks | Mean irons (s) | Mark missed | Out of bounds | "
        "Timeouts | Timeout final distance, median (m) |",
        "|---|---|---|---|---|---|---|---|---|---|",
    ]
    for r in rows:
        dist = r["timeout_final_distance_m"]["median"]
        lines.append(
            f"| {r['label']} / {r['role']} | {r['manifest']} | "
            f"{r['episodes_with_a_tack']}/{r['episodes']} | {r['mean_tacks']:.2f} | "
            f"{r['mean_failed_tacks']:.2f} | {r['mean_irons_s']:.1f} | "
            f"{r['mark_missed']} | {r['out_of_bounds']} | {r['timeouts']} | "
            f"{'—' if dist is None else f'{dist:.1f}'} |"
        )
    return "\n".join(lines) + "\n"


def outcome_table(summary: list[dict]) -> str:
    lines = [
        "| Controller / role | Set | Finishes | Per seed | Outcomes "
        "| Successful mean (s) |",
        "|---|---|---|---|---|---|",
    ]
    for row in summary:
        mean = row["finish_time_s_given_success"]["mean"]
        per_seed = ", ".join(
            f"{v['finished']}/{v['episodes']}" for v in row["per_seed"].values()
        )
        outcomes = ", ".join(
            f"{k} {v}" for k, v in sorted(row["outcome_counts"].items())
        )
        time = "—" if mean is None else f"{mean:.2f}"
        lines.append(
            f"| {row['label']} / {row['role']} | {row['manifest']} | "
            f"{row['finished']}/{row['episodes']} | {per_seed} | {outcomes} | {time} |"
        )
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--jobs", type=int, default=os.cpu_count())
    args = parser.parse_args()
    frozen, episodes = load(args.run)
    checks = verify(args.run, frozen, episodes)
    diag, per_episode = diagnostics(args.run, frozen, episodes, args.jobs)
    (args.run / "diagnostics.json").write_text(json.dumps(per_episode) + "\n")
    summary = summarize_rows(frozen, episodes)
    specs, failures = video_specs(args.run, frozen, episodes)
    arms = {g["id"]: g["arm"] for g in frozen["groups"]}
    result = {
        "summary": summary,
        "evaluation_cost": {
            "all": aggregate(episodes),
            "per_arm": {
                arm: aggregate([e for e in episodes if arms[e["group"]] == arm])
                for arm in dict.fromkeys(arms.values())
            },
            "launches": read_jsonl(args.run / "launches.jsonl"),
        },
        "selection": frozen["selection"],
        "mpc": frozen["mpc"],
        "first_failures": failures,
        "diagnostics": diag,
        "checks": checks,
    }
    out = args.out
    (out / "figs").mkdir(parents=True, exist_ok=True)
    (out / "M4U_results.json").write_text(json.dumps(result, indent=2) + "\n")
    (out / "M4U_videos.json").write_text(json.dumps(specs, indent=2) + "\n")
    (out / "M4U_outcomes.md").write_text(outcome_table(summary))
    (out / "M4U_diagnostics.md").write_text(diagnostics_table(diag["rows"]))
    learning_figure(frozen, episodes, summary, out / "figs" / "M4U_learning.png")
    print(f"verified and summarized {len(episodes)} episodes")


if __name__ == "__main__":
    main()
