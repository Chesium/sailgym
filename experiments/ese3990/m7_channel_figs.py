"""Figures for docs/ese3990/M7_channel_report.md, from saved logs and checkpoints.

.venv/bin/python experiments/ese3990/m7_channel_figs.py PPO_RUN SAC_RUN
(tracks use each run's last checkpoint on the U test set in the ±10 m channel)
"""

import json
import sys

import matplotlib

matplotlib.use("Agg")
import m7_channel  # noqa: E402
import matplotlib.pyplot as plt  # noqa: E402
from m6_tune_figs import BLUE, FIGS, GRID, INK, INK2, SURFACE, style  # noqa: E402
from m6_tune_plot import load, tracks  # noqa: E402
from m7_channel import OUT  # noqa: E402
from sailing import load_manifest  # noqa: E402

PANELS = [
    ("PPO · M6 configuration", "c1_ppo"),
    ("PPO · width curriculum (2.5 m steps)", "c4_ppo_curr25"),
    ("PPO · U first, then curriculum + back-off", "c9_ppo_openu40"),
    ("SAC · M6 configuration", "c1_sac"),
]


def rows(run):
    return [json.loads(x) for x in (run / "log.jsonl").open()]


def curves():
    fig, axes = plt.subplots(
        2, len(PANELS), figsize=(15, 5.6), sharex=True, height_ratios=[3, 2]
    )
    fig.patch.set_facecolor(SURFACE)
    for col, (title, prefix) in enumerate(PANELS):
        top, bot = axes[0, col], axes[1, col]
        style(top)
        style(bot)
        runs = sorted(OUT.glob(f"{prefix}_s[0-9]"))
        for run in runs:
            rs = rows(run)
            x = [r["decisions"] / 1e6 for r in rs]
            top.plot(
                x,
                [r["eval"].get("finished", 0) for r in rs],
                color=BLUE,
                lw=1.4,
                alpha=0.7,
            )
            w = [(r["eval"].get("curriculum") or {}).get("half", 10) for r in rs]
            bot.step(
                x, [min(h, 35) for h in w], where="post", color=BLUE, lw=1.2, alpha=0.6
            )
        top.set_title(f"{title}\n{len(runs)} seeds", color=INK, fontsize=9, loc="left")
        top.set_ylim(-1, 42)
        bot.set_ylim(8, 37)
        bot.set_yticks([10, 20, 30])
        bot.axhline(10, color=GRID, lw=0.8)
        if prefix.startswith("c9"):
            bot.text(
                12,
                35.5,
                "open-water U drawn at 35",
                color=INK2,
                fontsize=7,
                va="bottom",
            )
        bot.set_xlabel("decisions (millions)", color=INK2, fontsize=8)
    axes[0, 0].set_ylabel("finishes / 40 in the ±10 m channel", color=INK2, fontsize=8)
    axes[1, 0].set_ylabel("training half-width (m)", color=INK2, fontsize=8)
    fig.tight_layout()
    fig.savefig(FIGS / "M7_channel_curves.png", dpi=130, facecolor=SURFACE)


def track_figure(runs):
    m7_channel.use_channel(10.0)
    fig, axes = plt.subplots(1, len(runs), figsize=(4.2 * len(runs), 6.2))
    fig.patch.set_facecolor(SURFACE)
    for ax, name in zip(axes, runs, strict=True):
        args, params, act, f = load(OUT / name, None)
        xs, out = tracks(args, params, act, load_manifest("test"))
        style(ax)
        ax.grid(False)
        for side in (-1, 1):
            ax.axvspan(side * 10, side * 14, color=GRID, lw=0)
            ax.axvline(side * 10, color=INK2, lw=1.2)
        for x in xs:
            ax.plot(x[:, 0], x[:, 1], color=BLUE, lw=0.8, alpha=0.55)
        ax.plot([-5, 5], [40, 40], color=INK, lw=3, solid_capstyle="round")
        ax.plot(0, 0, "o", color=INK, ms=4)
        fin = sum(o["outcome"] == "finished" for o in out)
        ax.set_title(
            f"{args['learner'].upper()} {name}, {int(f.stem[5:]) / 1e6:.0f}M\n"
            f"U test set in the channel: {fin}/{len(out)} finish",
            color=INK,
            fontsize=9,
            loc="left",
        )
        ax.set_xlim(-14, 14)
        ax.set_ylim(-4, 44)
        ax.set_aspect("equal")
        ax.set_xlabel("x (m)", color=INK2, fontsize=8)
    axes[0].set_ylabel("y (m)", color=INK2, fontsize=8)
    fig.tight_layout()
    fig.savefig(FIGS / "M7_channel_tracks.png", dpi=130, facecolor=SURFACE)


if __name__ == "__main__":
    curves()
    track_figure(sys.argv[1:])
    print(FIGS / "M7_channel_curves.png", FIGS / "M7_channel_tracks.png")
