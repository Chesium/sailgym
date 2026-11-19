"""Tracks of the planner's channel episodes (from m7_channel_mpc.py JSON).

.venv/bin/python experiments/ese3990/m7_channel_plot.py [HALF] [SEED]
"""

import json
import sys

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402
from m6_tune_figs import BLUE, FIGS, GRID, INK, INK2, SURFACE, style  # noqa: E402
from m7_channel_mpc import OUT  # noqa: E402


def main(half=10.0, seed=0):
    eps = [
        json.loads(p.read_text())
        for p in sorted((OUT / f"half{half:g}_s{seed}").glob("*.json"))
    ]
    fig, ax = plt.subplots(figsize=(4.4, 6.4))
    fig.patch.set_facecolor(SURFACE)
    style(ax)
    ax.grid(False)
    for side in (-1, 1):
        ax.axvspan(side * half, side * (half + 4), color=GRID, lw=0)
        ax.axvline(side * half, color=INK2, lw=1.2)
    ax.text(half + 0.4, 2, "wall", color=INK2, fontsize=8, rotation=90, va="bottom")
    for e in eps:
        xy = np.asarray(e["xy"])
        ax.plot(xy[:, 0], xy[:, 1], color=BLUE, lw=1.3, alpha=0.7)
    ax.plot([-5, 5], [40, 40], color=INK, lw=3, solid_capstyle="round")
    ax.text(0, 41.2, "gate", color=INK2, fontsize=8, ha="center")
    ax.plot(0, 0, "o", color=INK, ms=4)
    ax.text(0.8, -2.2, "start", color=INK2, fontsize=8)
    ax.annotate(
        "wind",
        (-7, 30),
        (-7, 36),
        color=INK2,
        fontsize=8,
        ha="center",
        arrowprops=dict(arrowstyle="->", color=INK2),
    )
    t = [e["finish_time_s"] for e in eps if e["outcome"] == "finished"]
    ax.set_title(
        f"Planner (H=160) in a ±{half:g} m channel\n"
        f"{len(t)}/{len(eps)} validation episodes finish, mean {np.mean(t):.1f} s",
        color=INK,
        fontsize=9,
        loc="left",
    )
    ax.set_xlim(-half - 4, half + 4)
    ax.set_ylim(-4, 44)
    ax.set_aspect("equal")
    ax.set_xlabel("x (m)", color=INK2, fontsize=8)
    ax.set_ylabel("y (m)", color=INK2, fontsize=8)
    fig.tight_layout()
    png = FIGS / f"M7_channel_mpc_half{half:g}_tracks.png"
    fig.savefig(png, dpi=150, facecolor=SURFACE)
    print(png)


if __name__ == "__main__":
    main(*(float(a) for a in sys.argv[1:2]), *(int(a) for a in sys.argv[2:3]))
