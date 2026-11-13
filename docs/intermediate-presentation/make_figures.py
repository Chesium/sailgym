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
from matplotlib.patches import FancyArrow  # noqa: E402

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
sys.path.insert(0, str(REPO / "experiments/ese3990"))
import u_diagnostics  # noqa: E402
from sailing import RS, Task  # noqa: E402
from viz.episode_store import EpisodeStore  # noqa: E402
from viz.render import boom_world, gates, hull_world, plot_static  # noqa: E402
from viz.replay import replay  # noqa: E402

# One colour per controller, as in main.tex; RuleSailor is also dashed.
PPO, SAC, MPC = "#5FAE5F", "#1F66CC", "#C71585"
SAC_DEMO, MPC_SHORT = "#86B3EE", "#EB9CCB"  # lighter variants of the same arm
DASH = (0, (5, 2.5))
GATE, HULL, SAIL = "#B22222", "#2B3A45", "#D64D3F"  # as main.tex and the web
WIND, WIND_TEXT = "#56B4E9", "#3C7EA3"  # sky blue, as \\wind in main.tex
RULE_FIG = "black"  # RuleSailor is grey in slide text, black in graphs
OK, FAIL = "#2E9D3A", "#D62728"
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


def pilot_curves(learner: str, count: int) -> list[tuple[np.ndarray, np.ndarray]]:
    """Validation success of every R pilot run whose settings equal the
    M2-frozen config of ``learner`` (budget aside): chosen by config, not by
    outcome. Pilots were scored on the 20 validation documents only."""
    frozen = json.loads(
        (REPO / "experiments/ese3990/configs/M2_freeze.json").read_text()
    )["configs"][learner]
    same = lambda c: all(c[k] == v for k, v in frozen.items() if k != "checkpoints")  # noqa: E731
    out = []
    for run in sorted((RUNS / learner / "pilots").glob("*/config.json")):
        cfg = json.loads(run.read_text())
        if cfg["task"] == "R" and cfg["training_seed"] >= 100 and same(cfg["config"]):
            curve = jsonl(run.parent / "curve.jsonl")
            out.append(
                (
                    np.array([r["decisions"] for r in curve]),
                    np.array([r["val_finished"] / 20 for r in curve]),
                )
            )
    # PPO seeds 100-120 (docs/rl/ppo_tuning_log.md), SAC 100-111 (M2 report)
    assert len(out) == count, (learner, len(out))
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
            label=f"{name}, mean of 3 main seeds (test)",
        )
    # PPO pilot seeds with exactly the frozen PPO settings at 1M (21 seeds).
    # They were only ever scored on the 20 validation documents: test stayed
    # closed during the pilots, so they are drawn and labelled separately.
    for learner, color, name, count in (
        ("ppo", PPO, "PPO", 21),
        ("sac", SAC, "SAC", 12),
    ):
        pilots = pilot_curves(learner, count)
        seeds, mean = f"{name}, {count} pilot seeds", f"{name}, pilot mean"
        for i, (x, y) in enumerate(pilots):
            ax.plot(x / 1e6, y, color=color, alpha=0.4, lw=0.7,
                    label=None if i else f"{seeds} (validation)")  # fmt: skip
        x = pilots[0][0]
        assert all(np.array_equal(x, px) for px, _ in pilots)  # common checkpoints
        y = np.mean([y for _, y in pilots], axis=0)
        ax.plot(x / 1e6, y, color=color, lw=1.8, ls=(0, (4, 1.5, 1, 1.5)),
                label=f"{mean} (validation)")  # fmt: skip
    # The 0 of 3 main PPO seeds, read against the pilots' per-seed rate
    # (seeds ending 1M at >= 18/20, as in docs/rl/ppo_tuning_log.md).
    finals = [y[-1] for _, y in pilot_curves("ppo", 21)]
    learned = sum(f >= 0.9 for f in finals)
    chance = (1 - learned / len(finals)) ** 3
    ax.annotate(
        f"0 of 3 main PPO seeds learned. With {learned} of {len(finals)} pilot seeds\n"
        f"learning, all 3 failing has a {chance:.0%} chance.",
        xy=(0.62, 0.0), xytext=(0.36, 0.1), fontsize=6.5, color="#2F6B2F",
        arrowprops={"arrowstyle": "-", "color": "#2F6B2F", "lw": 0.6},
        bbox={"boxstyle": "round,pad=0.25", "fc": "white", "ec": "none", "alpha": 0.85},
        zorder=30,
    )  # fmt: skip
    # Both finish every test condition; RuleSailor's dashes sit on top of MPC.
    ax.axhline(1.0, color=MPC, lw=2.5, zorder=3, label="True-model MPC")
    ax.axhline(1.0, color=RULE_FIG, ls=DASH, lw=1.6, zorder=4, label="RuleSailor")
    ax.set(
        xlabel="Real decisions (millions)",
        ylabel="Success rate",
        xlim=(0, 1.0),  # the budget: curves end at the frame, not short of it
        ylim=(-0.03, 1.05),
    )
    ax.grid(alpha=0.3)
    ax.legend(fontsize=7, loc="upper center", bbox_to_anchor=(0.5, -0.2), ncol=2,
              frameon=False)  # fmt: skip
    fig.savefig(path, bbox_inches="tight")
    plt.close(fig)


def load(store: Path, **query) -> object:
    s = EpisodeStore(store)
    (entry,) = s.query(**query)
    return replay(s.load(entry), s.identity)


def selected(arm: str, seed: int = 0) -> int:
    """The validation-selected checkpoint (decisions) frozen for held-out use."""
    frozen = json.loads((RUNS / "M4U/heldout/freeze.json").read_text())
    (g,) = [
        g
        for g in frozen["groups"]
        if g["arm"] == arm
        and g.get("training_seed") == seed
        and "selected" in g["roles"]
    ]
    return g["decisions"]


def wind_arrow(ax, top: tuple[float, float], length: float) -> None:
    """The slides' wind marker: a wide sky-blue arrow, always labelled."""
    ax.add_patch(
        FancyArrow(
            top[0], top[1], 0, -length, width=1.6, head_width=4.2, head_length=3,
            length_includes_head=True, color=WIND, zorder=12,
        )
    )  # fmt: skip
    ax.text(top[0], top[1] - length - 1.5, "wind", color=WIND_TEXT, fontsize=9,
            ha="center", va="top", zorder=12)  # fmt: skip


def u_tracks(
    path: Path, conditions=("validation-000", "validation-001", "validation-002")
) -> None:
    """The first three U validation documents (by manifest ID) per controller,
    seed 0; learners at their validation-selected checkpoint."""
    m4u = RUNS / "M4U"
    main = m4u / "main"
    cases = [
        ("PPO", PPO, main / "ppo_U_s0/store", {"checkpoint": selected("ppo")}),
        ("SAC", SAC, main / "sac_U_s0/store", {"checkpoint": selected("sac")}),
        ("SAC + demos", SAC_DEMO, main / "sac_demo_U_s0/store",
         {"checkpoint": selected("sac_demo")}),
        ("MPC 2 s", MPC_SHORT, m4u / "mpc/groups/H40_K32_s0/episodes", {}),
        ("MPC 8 s", MPC, m4u / "mpc/groups/H160_K32_s0/episodes", {}),
        ("RuleSailor", RULE_FIG, m4u / "eval/store_rule_sailor_validation", {}),
    ]  # fmt: skip
    trajs, colors = [], []
    for _, color, store, q in cases:
        for c in conditions:
            trajs.append(load(store, condition=c, **q))
            colors.append(color)
    fig = plot_static(trajs, Task.load("U"), colors=colors, figsize=(5.6, 4.4))
    ax = fig.axes[0]
    ax.set_title("")
    task = Task.load("U")
    booms = 0  # plot_static draws one final hull + boom per trajectory, in order
    for line in list(ax.lines):
        if line.get_marker() in ("o", "x"):
            line.remove()  # start ring, gate-post dots and the old end markers
        elif line.get_linestyle() != "-" and line.get_color() == "0.45":
            line.set(linestyle="-", color=GATE, lw=2.2)  # gate bar
        elif line.get_color() == "0.15":  # a final boom: sail in its hull's colour
            line.set(color=colors[booms], lw=1.2)
            booms += 1
        elif line.get_color() in colors:
            line.set(lw=0.9)
            if line.get_color() == RULE_FIG:  # dashed and on top, as on every slide
                line.set(linestyle=DASH, zorder=20)
    assert booms == len(trajs), booms
    for p in list(ax.patches) + list(ax.texts):  # replaced by wind_arrow below
        if type(p).__name__ in ("FancyArrow", "Annotation"):
            p.remove()
    # Course as the web frontend draws it: gate circle of the half-width with
    # a solid bar, and the boat itself at the start.
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
    # Outcome dots: small enough to sit inside the final hull outline.
    for tr in trajs:
        ok = tr.meta["outcome"] == "finished"
        ax.plot(tr.col("x")[-1], tr.col("y")[-1], "o", ms=2.6, mew=0,
                color=OK if ok else FAIL, zorder=6.5)  # fmt: skip
    ax.tick_params(labelsize=9)
    ax.xaxis.label.set_size(10)
    ax.yaxis.label.set_size(10)
    x0, x1 = ax.get_xlim()
    y0, y1 = ax.get_ylim()
    wind_arrow(ax, (x0 + 0.08 * (x1 - x0), y1 - 0.06 * (y1 - y0)), 9)
    handles = [
        plt.Line2D([], [], color=c, lw=1.6, ls=DASH if c == RULE_FIG else "-")
        for _, c, _, _ in cases
    ] + [
        plt.Line2D([], [], color=OK, marker="o", ls="", ms=4),
        plt.Line2D([], [], color=FAIL, marker="o", ls="", ms=4),
    ]
    labels = [name for name, *_ in cases] + ["finished", "failed"]
    ax.legend(
        handles, labels, loc="upper center", bbox_to_anchor=(0.5, -0.17), ncol=4,
        fontsize=9, frameon=False,
    )  # fmt: skip
    fig.savefig(path, dpi=200, bbox_inches="tight")
    plt.close(fig)


def tacked(arm: str) -> str:
    """Held-out runs (nominal + test + shift, 3 seeds) of the arm's selected
    checkpoints with at least one completed tack, by exact replay under the
    frozen thresholds (configs/M4U_diagnostics.json)."""
    frozen = json.loads((RUNS / "M4U/heldout/freeze.json").read_text())
    k = n = 0
    for g in frozen["groups"]:
        if g["arm"] == arm and "selected" in g["roles"]:
            d = u_diagnostics.diagnose_store(RUNS / "M4U/heldout/store" / g["id"])
            k += sum(v["tacks"] > 0 for v in d.values())
            n += len(d)
    return f"{k}/{n}"


def frac(c: list[int]) -> str:
    return f"{c[0]}/{c[1]}"


def numbers(path: Path) -> None:
    r = counts(RUNS / "M4", roles=("selected", "reference"))
    u = counts(RUNS / "M4U/heldout", roles=("selected", "reference", "bc_only"))
    sweep = json.loads((RUNS / "M4U/mpc/summary.json").read_text())["settings"]
    mpc_val = {s["setting"]: s for s in sweep}
    # The slide shows MPC test only; flag it until all 3 × 50 test episodes exist.
    done = u["mpc", "selected", "test"][1]
    expected = 3 * 50
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
        "uSACtacked": tacked("sac"),
        "uSACDtacked": tacked("sac_demo"),
        "uMPCpartial": ""
        if done == expected
        else rf"{{\scriptsize partial: {done}/{expected} test episodes}}\\[0.6em]",
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
