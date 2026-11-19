"""Figures for docs/ese3990/M6_tune_report.md, from saved logs and checkpoints.

.venv/bin/python experiments/ese3990/m6_tune_figs.py
"""

import json

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
from m6_tune import OUT  # noqa: E402
from m6_tune_plot import load, tracks  # noqa: E402
from sailing import REPO, load_manifest  # noqa: E402

FIGS = REPO / "docs/ese3990/figs"
SURFACE, INK, INK2, GRID, BLUE = "#fcfcfb", "#0b0b0b", "#52514e", "#e4e3df", "#2a78d6"
PANELS = [
    ("PPO · target head · 256 units", [f"b9_tgt_k10_h256_s{s}" for s in range(10)]),
    ("PPO · target head · 64 units", [f"b1_tgt_fixed_k10_s{s}" for s in range(5)]),
    (
        "PPO · rate head · 64 units",
        ["b1_rate_fixed_k10_s0", "b1_rate_fixed_k10_s1"]
        + [f"b3_rate_k10_s{s}" for s in (2, 3, 4)],
    ),
    (
        "PPO · rate head · 64 units · calibration scaling",
        ["b3_rate_calib_k10_s0", "b3_rate_calib_k10_s1"],
    ),
    ("SAC · target head · 256 units", [f"b5_sac_tgt_h256_s{s}" for s in range(5)]),
]


def style(ax):
    ax.set_facecolor(SURFACE)
    for side in ("top", "right"):
        ax.spines[side].set_visible(False)
    for side in ("left", "bottom"):
        ax.spines[side].set_color(INK2)
    ax.tick_params(colors=INK2, labelsize=8)
    ax.grid(axis="y", color=GRID, lw=0.8)
    ax.set_axisbelow(True)


def curves():
    fig, axes = plt.subplots(1, len(PANELS), figsize=(15, 3.4), sharey=True)
    fig.patch.set_facecolor(SURFACE)
    for ax, (title, runs) in zip(axes, PANELS, strict=True):
        style(ax)
        n = 0
        for r in runs:
            p = OUT / r / "log.jsonl"
            if not p.exists():
                continue
            rows = [json.loads(x) for x in p.read_text().splitlines()]
            x = [row["decisions"] / 1e6 for row in rows]
            y = [row["eval"].get("finished", 0) for row in rows]
            ax.plot(x, y, color=BLUE, lw=1.5, alpha=0.75, solid_capstyle="round")
            n += 1
        ax.axvline(4, color=INK2, lw=1, ls=":")
        ax.text(4.3, 41.5, "4M main budget", color=INK2, fontsize=7, va="bottom")
        ax.set_title(f"{title}\n{n} seeds", color=INK, fontsize=9, loc="left")
        ax.set_xlim(0, 20)
        ax.set_ylim(-1, 44)
        ax.set_xlabel("decisions (millions)", color=INK2, fontsize=8)
    axes[0].set_ylabel("finishes / 40, original start", color=INK2, fontsize=8)
    fig.tight_layout()
    fig.savefig(FIGS / "M6_tune_curves.png", dpi=130, facecolor=SURFACE)


def track_figure():
    args, params, act, f = load(OUT / "b9_tgt_k10_h256_s0", "10004660")
    xs, out = tracks(args, params, act, load_manifest("test"))
    fig, ax = plt.subplots(figsize=(4.6, 5.2))
    fig.patch.set_facecolor(SURFACE)
    style(ax)
    ax.grid(False)
    for x in xs:
        ax.plot(x[:, 0], x[:, 1], color=BLUE, lw=0.8, alpha=0.6)
    ax.plot([-5, 5], [40, 40], color=INK, lw=3, solid_capstyle="round")
    ax.text(6, 40, "gate", color=INK2, fontsize=8, va="center")
    ax.plot(0, 0, "o", color=INK, ms=4)
    ax.text(1.5, -2.5, "start", color=INK2, fontsize=8)
    ax.annotate(
        "wind from north",
        (-6, 30),
        (-6, 36),
        color=INK2,
        fontsize=8,
        arrowprops=dict(arrowstyle="->", color=INK2),
        ha="center",
    )
    fin = sum(o["outcome"] == "finished" for o in out)
    ax.set_title(
        f"PPO seed 0 at 10M, U test set: {fin}/{len(out)} finish",
        color=INK,
        fontsize=9,
        loc="left",
    )
    ax.set_aspect("equal")
    ax.set_xlabel("x (m)", color=INK2, fontsize=8)
    ax.set_ylabel("y (m)", color=INK2, fontsize=8)
    fig.tight_layout()
    fig.savefig(FIGS / "M6_tune_tracks.png", dpi=130, facecolor=SURFACE)


if __name__ == "__main__":
    curves()
    track_figure()
    print(FIGS / "M6_tune_curves.png", FIGS / "M6_tune_tracks.png")
