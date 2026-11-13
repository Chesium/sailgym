"""Figures and numbers for the intermediate presentation, from saved runs only.

    env -u PYTHONPATH MPLCONFIGDIR=/tmp/ese3990-mpl uv run --frozen --no-sync \
        python docs/intermediate-presentation/make_figures.py

Writes ``figs/r_learning.png``, ``figs/u_tracks.png`` and ``numbers.tex``
(LaTeX macros for every count quoted on the slides). Rerun after the M4U
held-out evaluation finishes: the MPC U held-out counts are read live and
flagged as partial until then.
"""

from __future__ import annotations

import json
import sys
from collections import defaultdict
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
sys.path.insert(0, str(REPO / "experiments/ese3990"))
from sailing import RS, Task  # noqa: E402
from viz.episode_store import EpisodeStore  # noqa: E402
from viz.render import boom_world, gates, hull_world, plot_static  # noqa: E402
from viz.replay import replay  # noqa: E402

# One colour per controller, as in main.tex; RuleSailor is also dashed.
PPO, SAC, MPC, RULE = "#5FAE5F", "#1F66CC", "#C71585", "#6B6B6B"
SAC_DEMO, MPC_SHORT = "#86B3EE", "#EB9CCB"  # lighter variants of the same arm
DASH = (0, (5, 2.5))
GATE, HULL, SAIL = "#B22222", "#2B3A45", "#D64D3F"  # as main.tex and the web
RUNS = REPO / "runs/ese3990"
FIGS = HERE / "figs"


def jsonl(path: Path) -> list[dict]:
    return [json.loads(s) for s in path.read_text().splitlines() if s]


def counts(run: Path, roles=("selected",)) -> dict:
    """``{(arm, role, manifest): [finished, episodes]}`` of a frozen comparison."""
    frozen = json.loads((run / "freeze.json").read_text())
    groups = {g["id"]: g for g in frozen["groups"]}
    if (run / "episodes.jsonl").exists():
        episodes = jsonl(run / "episodes.jsonl")  # M4 (R)
    else:
        episodes = [e for p in (run / "episodes").glob("*.jsonl") for e in jsonl(p)]
    out = defaultdict(lambda: [0, 0])
    for e in episodes:
        g = groups[e["group"]]
        for role in g["roles"]:
            if role in roles:
                k = (g.get("arm", g["controller"]), role, e["manifest"])
                out[k][0] += e["outcome"] == "finished"
                out[k][1] += 1
    return out


def r_learning(path: Path) -> None:
    """R held-out test success against real decisions, per training seed."""
    run = RUNS / "M4"
    frozen = json.loads((run / "freeze.json").read_text())
    groups = {g["id"]: g for g in frozen["groups"]}
    rate = defaultdict(lambda: [0, 0])
    for e in jsonl(run / "episodes.jsonl"):
        g = groups[e["group"]]
        if e["manifest"] == "test" and "decisions" in g:
            k = (g["controller"], g["training_seed"], g["decisions"])
            rate[k][0] += e["outcome"] == "finished"
            rate[k][1] += 1
    fig, ax = plt.subplots(figsize=(5.2, 3.4), dpi=200)
    for learner, color, name in (("sac", SAC, "SAC"), ("ppo", PPO, "PPO")):
        curves = []
        for seed in (0, 1, 2):
            pts = sorted(
                (d, f / n)
                for (c, s, d), (f, n) in rate.items()
                if c == learner and s == seed
            )
            x, y = np.array(pts).T
            ax.plot(x / 1e6, y, color=color, alpha=0.35, lw=1)
            curves.append(y)
        ax.plot(
            x / 1e6,
            np.mean(curves, axis=0),
            color=color,
            lw=2.5,
            label=f"{name} (mean of 3 seeds)",
        )
    # Both finish every test condition; RuleSailor's dashes sit on top of MPC.
    ax.axhline(1.0, color=MPC, lw=2.5, zorder=3, label="True-model MPC")
    ax.axhline(1.0, color=RULE, ls=DASH, lw=2, zorder=4, label="RuleSailor")
    ax.set(
        xlabel="Real decisions (millions)",
        ylabel="Held-out test success",
        ylim=(-0.03, 1.05),
    )
    ax.grid(alpha=0.3)
    ax.legend(fontsize=8, loc="center right")
    fig.tight_layout()
    fig.savefig(path)
    plt.close(fig)


def load(store: Path, **query) -> object:
    s = EpisodeStore(store)
    (entry,) = s.query(**query)
    return replay(s.load(entry), s.identity)


def u_tracks(path: Path, condition: str = "validation-000") -> None:
    """One U validation document (the first by manifest ID), five controllers."""
    m4u = RUNS / "M4U"
    picks = [
        (
            "SAC, scratch",
            SAC,
            m4u / "main/sac_U_s0/store",
            {"checkpoint": 4_000_000},
        ),
        (
            "SAC + demos",
            SAC_DEMO,
            m4u / "main/sac_demo_U_s0/store",
            {"checkpoint": 1_000_000},
        ),
        ("MPC 2 s", MPC_SHORT, m4u / "mpc/groups/H40_K32_s0/episodes", {}),
        ("MPC 8 s", MPC, m4u / "mpc/groups/H160_K32_s0/episodes", {}),
        ("RuleSailor", RULE, m4u / "eval/store_rule_sailor_validation", {}),
    ]
    trajs, colors, labels = [], [], []
    for name, color, store, q in picks:
        tr = load(store, condition=condition, **q)
        m = tr.meta
        result = (
            f"{m['finish_time_s']:.0f} s"
            if m["outcome"] == "finished"
            else {"truncated": "timeout"}.get(m["outcome"], m["reason"])
        )
        trajs.append(tr)
        colors.append(color)
        labels.append(f"{name}: {result}")
    fig = plot_static(trajs, Task.load("U"), colors=colors, figsize=(4.4, 4.8))
    ax = fig.axes[0]
    ax.set_title("")
    for t in ax.texts:  # the wind label collides with the gate; the slide says it
        t.set_visible(False)
    # Course as the web frontend and the slides draw it: gate circle of the
    # half-width with a solid bar, and the boat itself at the start.
    for line in list(ax.lines):
        if line.get_marker() == "o" and line.get_markerfacecolor() in (
            "white",
            "#d1495b",
        ):
            line.remove()  # start ring and gate-post dots
        elif line.get_linestyle() != "-" and line.get_color() == "0.45":
            line.set(linestyle="-", color=GATE, lw=2.2)  # gate bar
    task = Task.load("U")
    for a, b in gates(task):
        ax.add_patch(
            plt.Circle((a + b) / 2, task.half_width, fill=False, ec=GATE, lw=1)
        )
    s0 = trajs[0].states[0]
    x, y, psi, beta = (s0[RS[k]] for k in ("x", "y", "psi", "beta"))
    ax.add_patch(
        plt.Polygon(hull_world(x, y, psi), fc="white", ec=HULL, lw=0.8, zorder=9)
    )
    bm = boom_world(x, y, psi, beta)
    ax.plot(bm[:, 0], bm[:, 1], color=SAIL, lw=1.4, zorder=10)
    for p in ax.patches:  # the wind arrow, in the slides' neutral grey
        if type(p).__name__ == "FancyArrow":
            p.set_color("#666666")
    for line in ax.lines:  # RuleSailor dashed and on top, as on every slide
        if line.get_color() == RULE:
            line.set(linestyle=DASH, zorder=20)
    for legend in [*fig.legends, ax.get_legend()]:
        if legend is not None:
            legend.remove()
    handles = [
        plt.Line2D([], [], color=c, lw=2, ls=DASH if c == RULE else "-") for c in colors
    ]
    ax.legend(
        handles, labels, loc="upper right", fontsize=8, framealpha=0.9,
        title="Finish time", title_fontsize=8,
    )  # fmt: skip
    fig.savefig(path, dpi=200, bbox_inches="tight")
    plt.close(fig)


def frac(c: list[int]) -> str:
    return f"{c[0]}/{c[1]}"


def numbers(path: Path) -> None:
    r = counts(RUNS / "M4", roles=("selected", "reference"))
    u = counts(RUNS / "M4U/heldout", roles=("selected", "reference", "bc_only"))
    sweep = json.loads((RUNS / "M4U/mpc/summary.json").read_text())["settings"]
    mpc_val = {s["setting"]: s for s in sweep}
    expected = 3 * 70  # three search seeds × (50 test + 20 shift)
    done = u["mpc", "selected", "test"][1] + u["mpc", "selected", "shift"][1]
    macros = {
        "rPPOtest": frac(r["ppo", "selected", "test"]),
        "rSACtest": frac(r["sac", "selected", "test"]),
        "rMPCtest": frac(r["mpc", "selected", "test"]),
        "rRULEtest": frac(r["rule_sailor", "reference", "test"]),
        "rPPOshift": frac(r["ppo", "selected", "shift"]),
        "rSACshift": frac(r["sac", "selected", "shift"]),
        "rMPCshift": frac(r["mpc", "selected", "shift"]),
        "rRULEshift": frac(r["rule_sailor", "reference", "shift"]),
        "uPPOtest": frac(u["ppo", "selected", "test"]),
        "uPPOBCtest": frac(u["ppo_bc", "selected", "test"]),
        "uBCtest": frac(u["ppo_bc", "bc_only", "test"]),
        "uSACtest": frac(u["sac", "selected", "test"]),
        "uSACDtest": frac(u["sac_demo", "selected", "test"]),
        "uMPCtest": frac(u["mpc", "selected", "test"]),
        "uMPCshift": frac(u["mpc", "selected", "shift"]),
        "uRULEtest": frac(u["rule_sailor", "reference", "test"]),
        "uRULEshift": frac(u["rule_sailor", "reference", "shift"]),
        "uMPCvalShort": frac([mpc_val["H40_K32"][k] for k in ("finished", "episodes")]),
        "uMPCvalLong": frac([mpc_val["H160_K32"][k] for k in ("finished", "episodes")]),
        "uMPCpLong": f"{mpc_val['H160_K32']['latency_ms']['p50']:.0f}",
        "uMPCpShort": f"{mpc_val['H40_K32']['latency_ms']['p50']:.0f}",
        "uMPCpartial": ""
        if done == expected
        else f"partial: {done}/{expected} episodes",
    }
    lines = ["% Generated by make_figures.py from runs/ese3990; do not edit."]
    lines += [f"\\newcommand{{\\{k}}}{{{v}}}" for k, v in macros.items()]
    path.write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    FIGS.mkdir(exist_ok=True)
    r_learning(FIGS / "r_learning.png")
    u_tracks(FIGS / "u_tracks.png")
    numbers(HERE / "numbers.tex")
    print((HERE / "numbers.tex").read_text())
