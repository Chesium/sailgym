"""Zero-shot: M7 PPO policies in longer ±10 m channels (gate further upwind).

.venv/bin/python experiments/ese3990/m7_channel_long.py [--runs RUN...] [--ys 40 80 ...]
    [--relay 40] [--plot]

Same channel, start, gate width, dynamics and reward; only the gate's y, the
top bound (gate + 60 m) and the time limit (360 s, so time does not hide the
result) change. The policy never sees time; its distance-to-gate features go
beyond the ≤ 40 m seen in training. U test conditions, deterministic policy,
selected held-out checkpoints. Writes runs/ese3990/M7/long/results.json.
"""

import argparse
import json
from concurrent.futures import ProcessPoolExecutor
from dataclasses import replace
from multiprocessing import get_context
from types import SimpleNamespace

import numpy as np
from m7_channel import OUT
from m7_channel_mpc import channel
from sailing import REPO

import sailgym_core

LONG = REPO / "runs/ese3990/M7/long"
HELDOUT = OUT / "heldout/c9_ppo_openu40_40M.json"


def long_channel(y, half=10.0, budget_s=360.0):
    t = channel(half)
    x0, y0, x1, _ = t.bounds
    route = sailgym_core.route_waypoints(t.start, [(0.0, y)], t.half_width)
    t = replace(
        t,
        target=(0.0, y),
        bounds=(x0, y0, x1, y + 60.0),
        budget_s=budget_s,
        route_json=route,
    )
    t.max_steps = round(budget_s / t.dt)
    assert t.max_steps % t.cadence == 0
    return t


def reversals(x, h=3.0):
    """Board changes: turning points of x that reverse by at least h m."""
    n, trend, ext = 0, 0, x[0]
    for v in x:
        if trend == 0:
            if abs(v - ext) >= h:
                trend, ext = (1 if v > ext else -1), v
        elif trend * (v - ext) > 0:
            ext = v
        elif abs(v - ext) >= h:
            n, trend, ext = n + 1, -trend, v
    return n


def rollout(args, params, act, task, conds, relay=None):
    """m6_tune_plot.tracks, keeping every research state for the classifier.

    With ``relay`` L, the policy sees a virtual gate at (0, k·L), advanced by L
    each time the boat passes it (capped at the real gate): legs like training's.
    """
    from m6_tune import FIXED, Vec, to_rates
    from sailing import RS, features

    mean, std = np.asarray(FIXED["mean"]), np.asarray(FIXED["std"])
    vec = Vec(task, len(conds), 0, conds=conds)
    alive = np.ones(len(conds), bool)
    st, out = [[r.copy()] for r in vec.rs], [None] * len(conds)
    a, step = None, 0
    gy = task.target[1]
    vt = np.full(len(conds), min(relay, gy) if relay else gy)
    while alive.any():
        if relay:
            hit = vec.rs[:, RS["y"]] >= vt
            vt[hit] = np.minimum(vt[hit] + relay, gy)
        if step % args["repeat"] == 0:
            f = features(vec.rs, (0.0, 0.0))
            f[:, 1] += vt  # dy to the (virtual) gate; the gate is at x = 0
            o = np.clip((f - mean) / std, -10, 10)
            a = np.asarray(act(params, o.astype(np.float32)))
        rates = to_rates(a, vec.rs, task.decision_dt) if args["head"] == "target" else a
        idx, _, code = vec.step(rates, alive)
        for i in idx:
            st[i].append(vec.rs[i].copy())
        for j in np.flatnonzero(code):
            alive[idx[j]] = False
            out[idx[j]] = vec.record(idx[j], code[j])
        step += 1
    return [np.asarray(s) for s in st], out


def one(job):
    run, ckpt, ys, relay = job
    from m6_tune_plot import load
    from sailing import RS, load_manifest
    from u_diagnostics import diagnose

    args, params, act, _ = load(OUT / run, str(ckpt))
    conds = load_manifest("test")
    res = {}
    for y in ys:
        task = long_channel(y)
        sts, outs = rollout(args, params, act, task, conds, relay)
        eps = []
        for c, s, o in zip(conds, sts, outs, strict=True):
            tr = SimpleNamespace(
                exact=True,
                states=s,
                t=s[:, RS["t"]],
                meta=dict(
                    outcome=o["outcome"], reason=o["reason"], condition=c.to_dict()
                ),
            )
            d = diagnose(tr, task)
            eps.append(
                dict(
                    outcome=o["reason"] or o["outcome"],
                    t=o["t"],
                    max_y=float(s[:, RS["y"]].max()),
                    reversals=reversals(s[:, RS["x"]]),
                    tacks=d["tacks"],
                    gybes=d["gybes"],
                    failed_tacks=d["failed_tacks"],
                )
            )
        fin = [e for e in eps if e["outcome"] == "finished"]
        res[y] = dict(
            finished=len(fin),
            episodes=len(eps),
            outcomes={
                k: sum(e["outcome"] == k for e in eps)
                for k in {e["outcome"] for e in eps}
            },
            finish_time_mean=float(np.mean([e["t"] for e in fin])) if fin else None,
            reversals_mean=float(np.mean([e["reversals"] for e in fin]))
            if fin
            else None,
            reversals_hist={
                str(k): sum(e["reversals"] == k for e in fin)
                for k in sorted({e["reversals"] for e in fin})
            },
            tacks_mean=float(np.mean([e["tacks"] for e in fin])) if fin else None,
            gybes_total=sum(e["gybes"] for e in eps),
            max_y_mean=float(np.mean([e["max_y"] for e in eps])),
        )
        np.savez_compressed(
            LONG / f"{run}_y{y:g}{f'_relay{relay:g}' if relay else ''}_xy.npz",
            *[s[::10][:, [RS["x"], RS["y"]]] for s in sts],
        )
    return run, ckpt, res


def plot(panels, png):
    """Tracks from the saved npz files: (run, gate y, relay or None, title)."""
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from m6_tune_figs import BLUE, GRID, INK, INK2, SURFACE, style

    ymax = max(y for _, y, _, _ in panels) + 6
    fig, axes = plt.subplots(1, len(panels), figsize=(2.6 * len(panels), 8.4))
    fig.patch.set_facecolor(SURFACE)
    for ax, (run, y, relay, title) in zip(axes, panels, strict=True):
        tag = f"_relay{relay:g}" if relay else ""
        z = np.load(LONG / f"{run}_y{y:g}{tag}_xy.npz")
        res = json.loads((LONG / f"results{tag}.json").read_text())[run]["by_gate_y"]
        r = res[str(float(y))]
        style(ax)
        ax.grid(False)
        for side in (-1, 1):
            ax.axvspan(side * 10, side * 14, color=GRID, lw=0)
            ax.axvline(side * 10, color=INK2, lw=1.0)
        for k in z.files:
            ax.plot(z[k][:, 0], z[k][:, 1], color=BLUE, lw=0.7, alpha=0.5)
        ax.plot([-5, 5], [y, y], color=INK, lw=3, solid_capstyle="round")
        if relay:
            for v in np.arange(relay, y, relay):
                ax.axhline(v, color=INK2, lw=0.6, ls=":")
        ax.plot(0, 0, "o", color=INK, ms=3)
        ax.set_title(
            f"{title}\n{r['finished']}/{r['episodes']} finish",
            color=INK,
            fontsize=8,
            loc="left",
        )
        ax.set_xlim(-14, 14)
        ax.set_ylim(-4, ymax)
        ax.set_aspect("equal")
        ax.set_xlabel("x (m)", color=INK2, fontsize=8)
    axes[0].set_ylabel("y (m)", color=INK2, fontsize=8)
    fig.tight_layout()
    fig.savefig(png, dpi=130, facecolor=SURFACE)
    print(png)


def main():
    p = argparse.ArgumentParser()
    p.add_argument(
        "--runs", nargs="+", default=[f"c9_ppo_openu40_s{s}" for s in range(10)]
    )
    p.add_argument("--ys", type=float, nargs="+", default=[40.0, 80.0, 120.0, 160.0])
    p.add_argument("--jobs", type=int, default=10)
    p.add_argument("--relay", type=float, help="virtual-gate leg length (m)")
    p.add_argument("--plot", action="store_true", help="draw the saved tracks only")
    a = p.parse_args()
    if a.plot:
        s7, s1 = "c9_ppo_openu40_s7", "c9_ppo_openu40_s1"
        return plot(
            [
                (s7, 80, None, "seed 7, 80 m gate\nas trained"),
                (s7, 80, 40, "seed 7, 80 m gate\nrelay every 40 m"),
                (s1, 120, 40, "seed 1, 120 m gate\nrelay"),
                (s1, 160, 40, "seed 1, 160 m gate\nrelay"),
            ],
            REPO / "docs/ese3990/figs/M7_channel_long_tracks.png",
        )
    LONG.mkdir(parents=True, exist_ok=True)
    sel = json.loads(HELDOUT.read_text())
    jobs = [(r, sel[r]["selected_ckpt"], a.ys, a.relay) for r in a.runs]
    out = {}
    with ProcessPoolExecutor(a.jobs, mp_context=get_context("spawn")) as ex:
        for run, ckpt, res in ex.map(one, jobs):
            out[run] = dict(ckpt=ckpt, by_gate_y=res)
            print(run, flush=True)
            for y, r in res.items():
                print(
                    f"  y {y:g}: {r['finished']}/{r['episodes']}"
                    f" t {r['finish_time_mean'] or 0:.0f}s"
                    f" reversals {r['reversals_mean'] or 0:.1f}"
                    f" tacks {r['tacks_mean'] or 0:.1f} {r['outcomes']}",
                    flush=True,
                )
    name = f"results_relay{a.relay:g}.json" if a.relay else "results.json"
    (LONG / name).write_text(json.dumps(out, indent=1))


if __name__ == "__main__":
    main()
