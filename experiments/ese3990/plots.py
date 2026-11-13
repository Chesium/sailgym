"""Figures from raw run summaries (plan §5, §7).

Usage::

    uv run --frozen --no-sync python experiments/ese3990/plots.py \
        runs/ese3990/ppo/R_s0_1M --out docs/ese3990/figs/ppo_R_curve.png
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402


def learning_curve(runs: list[Path], out: Path) -> None:
    """Validation success rate and mean return against real decisions, one
    line per run (individual seeds, never only a mean: plan E3)."""
    fig, (ax_s, ax_r) = plt.subplots(1, 2, figsize=(10, 3.6), layout="constrained")
    for run in runs:
        rows = [json.loads(x) for x in (run / "curve.jsonl").read_text().splitlines()]
        d = [r["decisions"] for r in rows]
        ax_s.plot(d, [r["val_success_rate"] for r in rows], marker="o", label=run.name)
        ax_r.plot(d, [r["val_mean_return"] for r in rows], marker="o", label=run.name)
    ax_s.set(xlabel="real decisions (incl. calibration)", ylabel="validation success")
    ax_s.set_ylim(-0.05, 1.05)
    ax_r.set(
        xlabel="real decisions (incl. calibration)", ylabel="validation mean return"
    )
    ax_s.legend(fontsize="small")
    out.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(out, dpi=150)


def tuning_row(run: Path) -> str:
    """One Markdown row of ``docs/rl/ppo_tuning_log.md`` for a finished run."""
    from train import CONFIG

    cfg = json.loads((run / "config.json").read_text())
    diff = {k: v for k, v in cfg["config"].items() if CONFIG.get(k) != v}
    for k in ("checkpoints", "decisions"):
        diff.pop(k, None)
    curve = [json.loads(x) for x in (run / "curve.jsonl").read_text().splitlines()]

    def at(d: int) -> str:
        r = min(curve, key=lambda r: abs(r["decisions"] - d))
        return f"{r['val_finished']}/{r['val_episodes']}"

    best = max(curve, key=lambda r: (r["val_finished"], r["val_mean_return"]))
    idx = [
        json.loads(x) for x in (run / "store" / "index.jsonl").read_text().splitlines()
    ]
    tr = [e for e in idx if e["kind"] == "training"]
    n_fin = sum(e["outcome"] == "finished" for e in tr)
    n_miss = sum(e["reason"] == "mark_missed" for e in tr)
    last = json.loads((run / "metrics.jsonl").read_text().splitlines()[-1])
    summ = json.loads((run / "summary.json").read_text())
    return (
        f"| `{run.name}` | {cfg['training_seed']} | "
        f"{', '.join(f'{k}={v}' for k, v in diff.items()) or 'baseline'} | "
        f"{at(100_000)} | {at(cfg['config']['decisions'])} | "
        f"{at(best['decisions'])} @ {best['decisions'] // 1000}k | "
        f"{n_fin} / {n_miss} / {len(tr)} | "
        f"{', '.join(f'{x:.2f}' for x in last['std'])} | {summ['wall_s']:.0f} |"
    )


# Light-mode chart tokens (dataviz reference palette, categorical slots 1-2).
INK, INK2, GRID = "#0b0b0b", "#52514e", "#e6e5e1"
BLUE, ORANGE, GREY = "#2a78d6", "#eb6834", "#b3b2ad"


def _style(ax) -> None:
    ax.spines[["top", "right"]].set_visible(False)
    ax.spines[["left", "bottom"]].set_color(INK2)
    ax.tick_params(colors=INK2, labelsize=9)
    ax.grid(True, color=GRID, lw=0.6)
    ax.set_axisbelow(True)


def baseline_runs(root: Path) -> list[Path]:
    """Every baseline-config PPO run on R with a 1M budget (tuning log P1, P5, M1)."""
    runs = [root / "R_s0_1M", *sorted((root / "pilots").glob("P1_base_s*/"))]
    return runs + sorted((root / "pilots").glob("P5_base_s*/"))


def _seed(run: Path) -> int:
    return json.loads((run / "config.json").read_text())["training_seed"]


def _curve(run: Path, upto: int = 1_000_000) -> list[dict]:
    rows = [json.loads(x) for x in (run / "curve.jsonl").read_text().splitlines()]
    return [r for r in rows if r["decisions"] <= upto]


def seed_curves(runs: list[Path], out: Path) -> None:
    """Validation finished vs decisions, one line per seed, coloured by fate."""
    fig, ax = plt.subplots(figsize=(7.5, 4.2), layout="constrained")
    groups = {"solved, kept at 1M": [], "solved, then lost": [], "not solved": []}
    for run in runs:
        c = _curve(run)
        best = max(r["val_finished"] for r in c)
        g = "not solved" if best < 18 else (
            "solved, kept at 1M" if c[-1]["val_finished"] >= 18 else "solved, then lost"
        )  # fmt: skip
        groups[g].append(c)
    style = {
        "not solved": (GREY, 1.0, 1),
        "solved, then lost": (ORANGE, 2.0, 2),
        "solved, kept at 1M": (BLUE, 2.0, 3),
    }
    for g in ("not solved", "solved, then lost", "solved, kept at 1M"):
        color, lw, z = style[g]
        for i, c in enumerate(groups[g]):
            ax.plot(
                [r["decisions"] / 1e6 for r in c], [r["val_finished"] for r in c],
                color=color, lw=lw, zorder=z,
                label=f"{g} ({len(groups[g])} seeds)" if i == 0 else None,
            )  # fmt: skip
    _style(ax)
    ax.set(xlim=(0, 1.0), ylim=(-0.5, 20.5), yticks=[0, 5, 10, 15, 20])
    ax.set_xlabel("real decisions (millions, incl. 10k calibration)", color=INK2)
    ax.set_ylabel("validation conditions finished (of 20)", color=INK2)
    ax.set_title(
        f"PPO on task R: {len(runs)} training seeds, same settings",
        color=INK, loc="left", fontsize=11,
    )  # fmt: skip
    ax.legend(
        loc="lower left", bbox_to_anchor=(0, 1.0), ncol=3, frameon=False,
        fontsize=9, labelcolor=INK, handlelength=1.5,
    )  # fmt: skip
    ax.set_title(ax.get_title(), color=INK, loc="left", fontsize=11, pad=24)
    fig.savefig(out, dpi=200, facecolor="#fcfcfb")


def selection_dumbbell(runs: list[Path], out: Path) -> None:
    """Each seed at 1M vs at its validation-selected checkpoint."""
    from evaluate import select_checkpoint

    rows = []
    for run in runs:
        c = _curve(run)
        rows.append(
            (_seed(run), c[-1]["val_finished"], select_checkpoint(c)["val_finished"])
        )
    rows.sort(key=lambda r: (r[2], r[1]))
    fig, ax = plt.subplots(figsize=(7.5, 5.2), layout="constrained")
    for i, (_, last, sel) in enumerate(rows):
        ax.plot([last, sel], [i, i], color=GRID, lw=2, zorder=1)
    y = range(len(rows))
    ax.scatter(
        [r[1] for r in rows], y, s=40, color=GREY, zorder=2,
        edgecolor="#fcfcfb", linewidth=1.5, label="last (1M)",
    )  # fmt: skip
    ax.scatter(
        [r[2] for r in rows], y, s=40, color=BLUE, zorder=3,
        edgecolor="#fcfcfb", linewidth=1.5, label="validation-selected checkpoint",
    )  # fmt: skip
    _style(ax)
    ax.set_yticks(list(y), [f"seed {r[0]}" for r in rows], fontsize=8)
    ax.set(xlim=(-0.8, 20.8), xticks=[0, 5, 10, 15, 20])
    ax.set_xlabel("validation conditions finished (of 20)", color=INK2)
    n_last = sum(r[1] >= 18 for r in rows)
    n_sel = sum(r[2] >= 18 for r in rows)
    ax.set_title(
        f"Seeds finishing ≥ 18/20: {n_last} at 1M, {n_sel} with checkpoint selection",
        color=INK, loc="left", fontsize=11,
    )  # fmt: skip
    ax.legend(loc="lower right", frameon=False, fontsize=9, labelcolor=INK)
    fig.savefig(out, dpi=200, facecolor="#fcfcfb")


def comparison_gif(root: Path, good: int, bad: int, out: Path) -> None:
    """Nominal R: RuleSailor vs PPO seed ``good`` at its validation-selected
    checkpoint vs PPO seed ``bad`` at its last checkpoint (a failing seed's
    "selected" one is just the least-bad, often untrained), replayed exactly."""
    import tempfile

    from baselines import RuleSailorController
    from evaluate import run_episode, select_checkpoint
    from sailing import NOMINAL, Task
    from viz.episode_store import EpisodeStore, EpisodeWriter
    from viz.render import animate
    from viz.replay import replay

    task = Task.load("R")
    trajs = []
    with tempfile.TemporaryDirectory() as tmp:
        with EpisodeWriter(tmp) as w:
            run_episode(task, NOMINAL, RuleSailorController(), writer=w)
        s = EpisodeStore(tmp)
        tr = replay(s.load(s.index[0]), s.identity, task)
        tr.meta["label"] = "RuleSailor (hand-written reference)"
        trajs.append(tr)
    for seed in (good, bad):
        run = next(r for r in baseline_runs(root) if _seed(r) == seed)
        c = _curve(run)
        ck = (select_checkpoint(c) if seed == good else c[-1])["decisions"]
        s = EpisodeStore(run / "store")
        (e,) = s.query(kind="probe", condition="nominal", checkpoint=ck)
        tr = replay(s.load(e), s.identity, task)
        tr.meta["label"] = f"PPO seed {seed} @ {ck // 1000}k"
        trajs.append(tr)
    n = animate(
        trajs, task, out, fps=10, speed=3.0, colors=[GREY, BLUE, ORANGE],
        title="Task R, nominal wind: reach the gate at (30, 0)", dpi=110,
    )  # fmt: skip
    print(f"wrote {out}: {n} frames")


def intermediate(root: Path, out: Path) -> None:
    """Minute-3 assets for the intermediate video."""
    out.mkdir(parents=True, exist_ok=True)
    runs = baseline_runs(root)
    seed_curves(runs, out / "ppo_seed_curves.png")
    selection_dumbbell(runs, out / "ppo_checkpoint_selection.png")
    comparison_gif(root, 111, 0, out / "R_rule_vs_ppo.gif")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("runs", nargs="*", type=Path)
    ap.add_argument("--intermediate", type=Path, metavar="ROOT", help="ppo run root")
    ap.add_argument("--out", type=Path)
    ap.add_argument("--table", action="store_true", help="print tuning-log rows")
    args = ap.parse_args(argv)
    if args.intermediate:
        intermediate(args.intermediate, args.out)
        return 0
    if args.table:
        print("\n".join(tuning_row(r) for r in args.runs))
    if args.out:
        learning_curve(args.runs, args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
