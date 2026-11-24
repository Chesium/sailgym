"""Figures for docs/ese3990/M8_report.md, from saved logs and held-out records.

.venv/bin/python experiments/ese3990/m8_figs.py ARM[=TITLE] ...
  curves: panel finishes (of 40) against decisions, one line per seed, per arm
  tracks: one selected checkpoint per arm on a few validation legs (leg frame)
"""

import json
import sys

import matplotlib

matplotlib.use("Agg")
import m8_leg as m8  # noqa: E402
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402
from m6_tune_figs import BLUE, FIGS, GRID, INK, INK2, SURFACE, style  # noqa: E402
from m8_leg import OUT, RS  # noqa: E402


def rows(run):
    return [json.loads(x) for x in (run / "log.jsonl").open()]


def curves(arms):
    fig, axes = plt.subplots(1, len(arms), figsize=(3.8 * len(arms), 3.4), sharey=True)
    fig.patch.set_facecolor(SURFACE)
    for ax, (arm, title) in zip(np.atleast_1d(axes), arms, strict=True):
        style(ax)
        runs = sorted(
            (OUT / "train").glob(f"{arm}_s*"),
            key=lambda p: int(p.name.rsplit("_s", 1)[1]),
        )
        for run in runs:
            r = rows(run)
            x = [q["decisions"] / 1e6 for q in r]
            ax.plot(
                x, [q["eval"]["finished"] for q in r], color=BLUE, lw=1.1, alpha=0.65
            )
            up = [q["eval"]["bins"]["upwind"][0] for q in r]
            if max(up):
                ax.plot(x, up, color=INK2, lw=0.9, ls="--", alpha=0.8)
        ax.axhline(36, color=GRID, lw=0.8)
        ax.set_ylim(0, 40.5)
        ax.set_title(f"{title}\n{len(runs)} seeds", color=INK, fontsize=9, loc="left")
        ax.set_xlabel("decisions (millions)", color=INK2, fontsize=8)
    np.atleast_1d(axes)[0].set_ylabel(
        "panel finishes / 40 (dashed: dead upwind / 8)", color=INK2, fontsize=8
    )
    fig.tight_layout()
    path = FIGS / "M8_curves.png"
    fig.savefig(path, dpi=130, facecolor=SURFACE)
    print(path)


def _xy(ep, actions, seed, task_fn):
    """Exact replay of recorded actions; world (x, y) every 10 decisions."""
    env = m8.TaskEnv(task_fn(ep), None, seed)
    rs = env.reset()
    a3, o = np.array([0.0, 0.0, -1.0]), np.empty(env.spec.obs_len, np.float32)
    f, pts = np.empty_like(o), [rs[[RS["x"], RS["y"]]]]
    for k, a in enumerate(actions):
        a3[:2] = a
        env.raw.step(a3, o, f)
        if k % 10 == 0:
            pts.append(env.research_state()[[RS["x"], RS["y"]]])
    return np.array(pts), env.outcome


def tracks(run_name):
    """Validation legs in the 10 m corridor (leg frame) and chained routes."""
    run = OUT / "train" / run_name
    act, a = m8.load_policy(run, m8.select(run)[0])
    legs = [g for g in m8.load_split("validation") if g.W == 10.0][::2]
    eps = [e for e in m8.route_eps(1) if not e.id.startswith("random5_1")]
    fig, (ax1, ax2) = plt.subplots(
        1, 2, figsize=(12, 6.2), gridspec_kw={"width_ratios": [1, 1.6]}
    )
    fig.patch.set_facecolor(SURFACE)
    for ax, items, fn in ((ax1, legs, m8.leg_task), (ax2, eps, m8.route_task)):
        style(ax)
        recs, vec = m8.run_legs(items, act, a["repeat"], a["head"], True, fn)
        for i, (ep, r) in enumerate(zip(items, recs, strict=True)):
            xy, oc = _xy(ep, vec.rec[i], r["env_seed"], fn)
            assert oc == r["outcome"]
            ls = "-" if oc == "finished" else ":"
            ax.plot(
                xy[:, 0], xy[:, 1], lw=1.0, ls=ls, label=f"{r['cell']} D={ep.D:.0f}"
            )
            pts = np.asarray(ep.points if fn is m8.route_task else [(0, 0), (0, ep.D)])
            ax.plot(pts[:, 0], pts[:, 1], "o", ms=3, color=INK2)
        ax.set_aspect("equal")
    ax1.axvline(-10, color=GRID)
    ax1.axvline(10, color=GRID)
    ax1.set_title("validation legs, 10 m corridor (leg frame; dotted = not finished)",
                  color=INK, fontsize=9, loc="left")  # fmt: skip
    ax2.set_title("chained routes, U test-000 wind (world frame)", color=INK,
                  fontsize=9, loc="left")  # fmt: skip
    for ax in (ax1, ax2):
        ax.legend(fontsize=6, frameon=False)
    fig.tight_layout()
    path = FIGS / "M8_tracks.png"
    fig.savefig(path, dpi=130, facecolor=SURFACE)
    print(path)


if __name__ == "__main__":
    if sys.argv[1] == "tracks":
        tracks(sys.argv[2])
    else:
        curves([tuple((a.split("=", 1) + [a])[:2]) for a in sys.argv[1:]])
