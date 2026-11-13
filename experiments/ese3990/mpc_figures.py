"""Render M3/M4U MPC evidence from saved sweep summaries and replayed probes.

The task, contract and file prefix come from the study's own study.json.
"""

import argparse
import json
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402
from mpc_study import STUDIES, study_task  # noqa: E402
from sailing import RS  # noqa: E402
from viz.episode_store import EpisodeStore  # noqa: E402
from viz.render import plot_static  # noqa: E402
from viz.replay import replay  # noqa: E402


def figures(study: Path, out: Path):
    out.mkdir(parents=True, exist_ok=True)
    summary = json.loads((study / "summary.json").read_text())
    rows = summary["settings"]
    task = study_task(summary["meta"])
    prefix = STUDIES[task.name]["prefix"]
    fig, axes = plt.subplots(1, 2, figsize=(11, 4), layout="constrained")
    for i, row in enumerate(rows):
        cost = row["hypothetical_physics_steps"] / row["real_decisions"]
        axes[0].scatter(cost, 100 * row["success_rate"])
        axes[0].annotate(
            row["setting"],
            (cost, 100 * row["success_rate"]),
            xytext=(0, -15 - 12 * (i % 2)),
            textcoords="offset points",
            ha="center",
            fontsize=8,
        )
    axes[0].set(
        xlabel="Actual hypothetical physics steps / live decision",
        ylabel="Validation success (%)",
        ylim=(-5, 115),
        title="True-model CEM-MPC: 20 conditions × 3 search seeds",
    )
    x = np.arange(len(rows))
    for key, marker in (("p50", "o"), ("p95", "^"), ("max", "x")):
        axes[1].plot(x, [r["latency_ms"][key] for r in rows], marker=marker, label=key)
    axes[1].axhline(50, color="red", linestyle="--", label="50 ms deadline")
    axes[1].set(
        xticks=x,
        xticklabels=[r["setting"] for r in rows],
        ylabel="Decision latency (ms)",
        yscale="log",
        title="Sampling + Rust + scoring + refitting",
    )
    axes[1].tick_params(axis="x", rotation=25)
    axes[1].legend()
    fig.savefig(out / f"{prefix}_mpc_cost.png", dpi=150)
    plt.close(fig)

    store = EpisodeStore(study / "probes")
    entry = store.query(condition="nominal")[0]
    log = store.load(entry)
    tr = replay(log, store.identity, task)
    fig = plot_static(
        [tr], task, title="CEM-MPC: predicted plan versus replanned execution"
    )
    ax = fig.axes[0]
    k = min(round(5 / task.decision_dt), len(log.actions) - 1)
    plan = log.plans[k]
    n = np.isfinite(plan[:, 0]).sum()
    ax.plot(
        plan[:, 0],
        plan[:, 1],
        "--",
        color="#e76f51",
        lw=3,
        zorder=10,
        label=f"Plan at {tr.t[k]:.2f} s (open loop)",
    )
    actual = tr.states[k : k + n, :][:, [RS["x"], RS["y"]]]
    ax.plot(
        actual[:, 0],
        actual[:, 1],
        color="black",
        lw=1,
        zorder=11,
        label="Executed continuation (replan every 0.05 s)",
    )
    ax.scatter(*plan[0], color="black", zorder=12)
    ax.legend(fontsize=8)
    # The paths nearly overlap at course scale; show their difference too.
    # U's tall track leaves the upper right free; R keeps its M3 layout.
    box = [0.08, 0.10, 0.40, 0.25] if task.name == "R" else [0.64, 0.58, 0.31, 0.2]
    zoom = ax.inset_axes(box)
    count = min(n, len(actual))
    separation = np.linalg.norm(plan[:count] - actual[:count], axis=1)
    zoom.plot(np.arange(count) * task.decision_dt, 100 * separation, color="black")
    if task.name == "R":
        zoom.set(xlabel="Lookahead (s)", ylabel="Separation (cm)")
    else:
        zoom.set_xlabel("Lookahead (s)", fontsize=8)
        zoom.set_title("Plan − executed (cm)", fontsize=8)
    zoom.tick_params(labelsize=8)
    fig.savefig(out / f"{prefix}_mpc_prediction.png", dpi=150)
    plt.close(fig)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("study", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    figures(args.study, args.out)
