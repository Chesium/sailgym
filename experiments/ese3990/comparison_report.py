"""Summaries, figures and stored-episode video specs for the frozen M4 run."""

from __future__ import annotations

import argparse
import json
import time
from collections import Counter
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402
from compare import digest  # noqa: E402
from evaluate import summarize  # noqa: E402
from sailing import REPO, ActionAdapter, load_manifest  # noqa: E402
from viz.episode_store import EpisodeStore  # noqa: E402


def aggregate(episodes: list[dict]) -> dict:
    latency = np.concatenate([e["decision_latency_s"] for e in episodes])
    return {
        **summarize(episodes),
        "real_decisions": sum(e["decisions"] for e in episodes),
        "physics_steps": sum(e["physics_steps"] for e in episodes),
        "hypothetical_physics_steps": sum(
            e.get("planning", {}).get("hypothetical_physics_steps", 0) for e in episodes
        ),
        "candidate_rollouts": sum(
            e.get("planning", {}).get("candidate_rollouts", 0) for e in episodes
        ),
        "rollout_calls": sum(
            e.get("planning", {}).get("rollout_calls", 0) for e in episodes
        ),
        "wall_s": sum(e["wall_s"] for e in episodes),
        "latency_ms": dict(
            zip(
                ("p50", "p95", "max"),
                (1000 * np.percentile(latency, [50, 95, 100])).tolist(),
                strict=True,
            )
        ),
        "deadline_misses_50ms": int((latency > 0.05).sum()),
        "deadline_miss_rate": float((latency > 0.05).mean()),
        "first_decision_latency_ms_max": 1000
        * max(e["decision_latency_s"][0] for e in episodes),
        "peak_heel_deg_mean": float(np.mean([e["peak_heel_deg"] for e in episodes])),
        "rudder_travel_rad_decision_sampled_mean": float(
            np.mean([e["rudder_travel_rad_decision_sampled"] for e in episodes])
        ),
        "sheet_travel_m_decision_sampled_mean": float(
            np.mean([e["sheet_travel_m_decision_sampled"] for e in episodes])
        ),
    }


def adapter_benchmark() -> dict:
    """The shared live adapter is outside act(); measure it separately.

    These small CPU timings include timer overhead, no physics or inference.
    Quantiles must not be added to act() quantiles as if jointly measured.
    """
    adapter = ActionAdapter(3)
    result = {}
    for name, action in (
        ("learned_f32_pair", np.array([0.2, -0.3], np.float32)),
        ("mpc_f64_pair", np.array([0.2, -0.3])),
        ("rule_f64_triple", np.array([0.2, -0.3, -1.0])),
    ):
        samples = []
        for _ in range(10_000):
            start = time.perf_counter()
            out = adapter(action)
            samples.append(time.perf_counter() - start)
        assert out[2] == -1 and (np.abs(out[:2]) <= 1).all()
        result[name] = {
            "calls": len(samples),
            "p50_us": float(np.percentile(samples, 50) * 1e6),
            "p95_us": float(np.percentile(samples, 95) * 1e6),
        }
    return result


def summarize_groups(frozen: dict, episodes: list[dict]) -> list[dict]:
    result = []
    for method, role in (
        ("ppo", "selected"),
        ("ppo", "final"),
        ("sac", "selected"),
        ("sac", "final"),
        ("mpc", "selected"),
        ("rule_sailor", "reference"),
    ):
        groups = [
            g
            for g in frozen["groups"]
            if g["controller"] == method and role in g["roles"]
        ]
        for manifest in frozen["evaluation_manifests"]:
            rows = [
                e
                for e in episodes
                if e["group"] in {g["id"] for g in groups} and e["manifest"] == manifest
            ]
            result.append(
                {
                    "controller": method,
                    "role": role,
                    "manifest": manifest,
                    **aggregate(rows),
                    "per_seed": {
                        g["id"]: aggregate([e for e in rows if e["group"] == g["id"]])
                        for g in groups
                    },
                }
            )
    return result


def video_specs(out: Path, frozen: dict, episodes: list[dict]):
    store = str((out / "store").resolve().relative_to(REPO))
    groups = frozen["groups"]
    selected = {
        method: [
            g
            for g in groups
            if g["controller"] == method
            and ("selected" in g["roles"] or "reference" in g["roles"])
        ]
        for method in ("ppo", "sac", "mpc", "rule_sailor")
    }

    def item(group, condition="nominal"):
        return {
            "store": store,
            "query": {"group": group["id"], "condition": condition},
            "label": group["id"],
        }

    specs = [
        {
            "id": "controllers_nominal",
            "title": "R: validation-selected controllers (seed 0)",
            "episodes": [item(gs[0]) for gs in selected.values()],
            "timeseries": True,
        }
    ]
    for method in ("ppo", "sac"):
        specs.append(
            {
                "id": f"{method}_checkpoints",
                "title": f"R: {method.upper()} seed 0, predeclared checkpoints",
                "episodes": [
                    item(g)
                    for g in groups
                    if g["controller"] == method and g["training_seed"] == 0
                ],
            }
        )
        specs.append(
            {
                "id": f"{method}_seeds",
                "title": f"R: {method.upper()} validation-selected training seeds",
                "episodes": [item(g) for g in selected[method]],
            }
        )
    specs.append(
        {
            "id": "mpc_search_seeds",
            "title": "R: MPC optimizer repetitions (not training seeds)",
            "episodes": [item(g) for g in selected["mpc"]],
        }
    )
    failures = {}
    for method, gs in selected.items():
        bad = [
            e
            for e in episodes
            if e["group"] in {g["id"] for g in gs}
            and e["manifest"] != "nominal"
            and e["outcome"] != "finished"
        ]
        first = (
            min(bad, key=lambda e: (e["condition"]["id"], e["group"])) if bad else None
        )
        failures[method] = (
            {
                k: first[k]
                for k in ("group", "condition", "manifest", "outcome", "reason")
            }
            if first
            else None
        )
        if first:
            g = next(g for g in gs if g["id"] == first["group"])
            specs.append(
                {
                    "id": f"{method}_first_failure",
                    "title": f"{method}: first failure ({first['condition']['id']})",
                    "episodes": [item(g, first["condition"]["id"])],
                    "timeseries": True,
                }
            )
    for spec in specs:
        spec.update(fps=10, speed=6)
    return specs, failures


def learning_figure(frozen, episodes, summary, path):
    fig, axes = plt.subplots(1, 2, figsize=(11, 4), sharey=True, layout="constrained")
    for method, color in (("ppo", "#2a78d6"), ("sac", "#eb6834")):
        seeds = sorted(
            {g["training_seed"] for g in frozen["groups"] if g["controller"] == method}
        )
        ys, xs, times = [], [], []
        for seed in seeds:
            groups = sorted(
                [
                    g
                    for g in frozen["groups"]
                    if g["controller"] == method and g["training_seed"] == seed
                ],
                key=lambda g: g["decisions"],
            )
            x = np.array([g["decisions"] for g in groups]) / 1e6
            wall = np.array([g["training_wall_s"] for g in groups])
            y = np.array(
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
            for ax, xx in zip(axes, (x, wall), strict=True):
                ax.plot(xx, y, color=color, alpha=0.35, lw=1, marker=".")
            ys.append(y)
            xs.append(x)
            times.append(wall)
        for ax, xx in zip(axes, (xs, times), strict=True):
            xmean = np.mean(xx, axis=0)
            ax.plot(
                xmean,
                np.mean(ys, axis=0),
                color=color,
                lw=2,
                label=f"{method.upper()} mean (3 training seeds)",
            )
            ax.fill_between(
                xmean, np.min(ys, axis=0), np.max(ys, axis=0), color=color, alpha=0.12
            )
    for method, label, color in (
        ("mpc", "Fixed true-model MPC (3 search seeds)", "#228833"),
        ("rule_sailor", "RuleSailor engineering reference", "#777777"),
    ):
        row = next(
            r for r in summary if r["controller"] == method and r["manifest"] == "test"
        )
        for ax in axes:
            ax.axhline(
                row["success_rate"],
                color=color,
                ls=(0 if method == "mpc" else 6, (6, 6)),
                lw=1,
                label=label,
            )
    axes[0].set(
        xlabel="Real decisions (millions, including calibration)",
        ylabel="Held-out test success",
        ylim=(-0.04, 1.04),
    )
    axes[1].set(xlabel="Checkpoint elapsed wall time (s; log scale)", xscale="log")
    axes[1].set_title(
        "Includes collection, updates and preceding validation", fontsize=9
    )
    for ax in axes:
        ax.grid(alpha=0.2)
    axes[0].legend(fontsize=7, loc="center left")
    fig.savefig(path, dpi=150)
    plt.close(fig)


def outcome_figure(summary, path):
    methods = ("ppo", "sac", "mpc", "rule_sailor")
    fig, ax = plt.subplots(figsize=(8, 4), layout="constrained")
    x = np.arange(len(methods))
    for offset, manifest in ((-0.18, "test"), (0.18, "shift")):
        rows = [
            next(
                r
                for r in summary
                if r["controller"] == m
                and r["role"] != "final"
                and r["manifest"] == manifest
            )
            for m in methods
        ]
        ax.bar(
            x + offset, [r["success_rate"] for r in rows], width=0.34, label=manifest
        )
        for pos, r in zip(x + offset, rows, strict=True):
            values = [s["success_rate"] for s in r["per_seed"].values()]
            ax.scatter(np.full(len(values), pos), values, color="black", s=12, zorder=5)
    ax.set(
        xticks=x,
        xticklabels=["PPO", "SAC", "True-model MPC", "RuleSailor"],
        ylabel="Success rate",
        ylim=(-0.04, 1.05),
    )
    ax.set_title(
        "Validation-selected policies; dots are training/search repetitions",
        fontsize=10,
    )
    ax.legend()
    fig.savefig(path, dpi=150)
    plt.close(fig)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    out = args.out
    out.mkdir(parents=True, exist_ok=True)
    frozen = json.loads((args.run / "freeze.json").read_text())
    episodes = [
        json.loads(s) for s in (args.run / "episodes.jsonl").read_text().splitlines()
    ]
    for name, expected in frozen["checkpoint_hashes"].items():
        if digest(REPO / name) != expected:
            raise ValueError(f"frozen checkpoint changed: {name}")
    for name, expected in frozen["freeze_hashes"].items():
        if digest(REPO / name) != expected:
            raise ValueError(f"controller freeze changed: {name}")
    for name, expected in frozen["condition_hashes"].items():
        if (
            digest(REPO / f"experiments/ese3990/configs/conditions/{name}.json")
            != expected
        ):
            raise ValueError(f"condition manifest changed: {name}")
    if digest(REPO / "experiments/ese3990/compare.py") != frozen["evaluator_sha256"]:
        raise ValueError("frozen evaluator changed")
    expected = {
        (g["id"], m, c)
        for g in frozen["groups"]
        for m in frozen["evaluation_manifests"]
        for c in (["nominal"] if m == "nominal" else [x.id for x in load_manifest(m)])
    }
    actual = [(e["group"], e["manifest"], e["condition"]["id"]) for e in episodes]
    if len(actual) != len(set(actual)) or set(actual) != expected:
        raise ValueError("missing, duplicate or unexpected evaluation episodes")
    store = EpisodeStore(args.run / "store")
    if store.identity != frozen["identity"]:
        raise ValueError("store and frozen build/contract differ")
    if {e["episode_id"] for e in store.index} != {e["episode_id"] for e in episodes}:
        raise ValueError("episode reports and store differ")
    reloads = 0
    for group in frozen["groups"]:
        if "checkpoint" not in group:
            continue
        run = (REPO / group["checkpoint"]).parent.parent
        curve = [json.loads(s) for s in (run / "curve.jsonl").read_text().splitlines()]
        recorded = next(r for r in curve if r["decisions"] == group["decisions"])
        actual_nominal = next(
            e
            for e in episodes
            if e["group"] == group["id"] and e["manifest"] == "nominal"
        )
        if actual_nominal["return"] != recorded["nominal_return"]:
            raise ValueError(f"{group['id']}: nominal reload differs from training")
        reloads += 1
    summary = summarize_groups(frozen, episodes)
    specs, failures = video_specs(args.run, frozen, episodes)
    audit = json.loads((REPO / "runs/ese3990/main/M4_audit.json").read_text())
    paired = []
    for seed in (0, 1, 2):
        runs = [REPO / f"runs/ese3990/main/{m}_R_s{seed}" for m in ("ppo", "sac")]
        stores = [EpisodeStore(run / "store") for run in runs]
        calibration = [s.query(kind="calibration") for s in stores]
        left = [
            (e["condition"], e["seed"], e["final_checksum"]) for e in calibration[0]
        ]
        right = [
            (e["condition"], e["seed"], e["final_checksum"]) for e in calibration[1]
        ]
        if left != right:
            raise ValueError(f"seed {seed}: learner calibration episodes differ")
        for a, b in zip(*calibration, strict=True):
            np.testing.assert_array_equal(
                stores[0].load(a).actions, stores[1].load(b).actions
            )
        norm = [
            json.loads((run / "ckpt/0010000.json").read_text())["normalization"]
            for run in runs
        ]
        if norm[0] != norm[1]:
            raise ValueError(f"seed {seed}: learner normalization differs")
        paired.append(seed)
    counts = Counter()
    main_physics = Counter()
    for run in audit:
        counts.update(run["real_decisions_by_kind"])
        index = EpisodeStore(REPO / run["run"] / "store").index
        for episode in index:
            main_physics[episode["kind"]] += episode["physics_steps"]
    first_calls = {}
    methods = {g["id"]: g["controller"] for g in frozen["groups"]}
    for episode in episodes:
        method = methods[episode["group"]]
        if method not in first_calls:
            first_calls[method] = {
                "group": episode["group"],
                "first_act_ms": 1000 * episode["decision_latency_s"][0],
            }
    result = {
        "summary": summary,
        "evaluation_cost": aggregate(episodes),
        "main_real_decisions_by_kind": dict(counts),
        "main_physics_steps_by_kind": dict(main_physics),
        "main_audit_verification_cost": {
            k: sum(r["verification_cost"][k] for r in audit)
            for k in audit[0]["verification_cost"]
        },
        "main_summed_run_wall_s": sum(r["summary"]["wall_s"] for r in audit),
        "shared_live_adapter_benchmark": adapter_benchmark(),
        "first_observed_act_calls": first_calls,
        "selection": frozen["selection"],
        "first_failures": failures,
        "group_checks": {
            "expected_episodes": len(expected),
            "unique_stored_episodes": len(store.index),
            "checkpoint_hashes_unchanged": True,
            "nominal_checkpoint_reloads_verified": reloads,
            "exact_paired_calibration_and_normalization_seeds": paired,
        },
    }
    (out / "M4_results.json").write_text(json.dumps(result, indent=2) + "\n")
    (out / "M4_videos.json").write_text(json.dumps(specs, indent=2) + "\n")
    figures = out / "figs"
    figures.mkdir(parents=True, exist_ok=True)
    learning_figure(frozen, episodes, summary, figures / "M4_learning.png")
    outcome_figure(summary, figures / "M4_outcomes.png")
    lines = [
        "| Controller / role | Set | Finishes | Per repetition | Successful mean (s) |",
        "|---|---|---|---|---|",
    ]
    for row in summary:
        mean = row["finish_time_s_given_success"]["mean"]
        per_seed = ", ".join(
            f"{v['finished']}/{v['episodes']}" for v in row["per_seed"].values()
        )
        time = "—" if mean is None else f"{mean:.2f}"
        lines.append(
            f"| {row['controller']} / {row['role']} | {row['manifest']} | "
            f"{row['finished']}/{row['episodes']} | {per_seed} | {time} |"
        )
    (out / "M4_outcomes.md").write_text("\n".join(lines) + "\n")
    print(f"verified and summarized {len(episodes)} episodes")


if __name__ == "__main__":
    main()
