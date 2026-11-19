"""Plot deterministic eval tracks of one m6_tune checkpoint.

.venv/bin/python experiments/ese3990/m6_tune_plot.py RUN_DIR [CKPT] [OUT.png]
"""

import json
import sys
from pathlib import Path

import jax
import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402
import ppo  # noqa: E402
import sac  # noqa: E402
from m6_tune import (  # noqa: E402
    FIXED,
    RS,
    Task,
    Vec,
    eval_panel,
    make_equivariant,
    to_rates,
)
from sailing import features  # noqa: E402


def load(run: Path, ckpt: str | None):
    args = json.loads((run / "args.json").read_text())
    files = sorted(run.glob("ckpt_*.npz"))
    f = run / f"ckpt_{int(float(ckpt)):09d}.npz" if ckpt else files[-1]
    if args.get("equivariant"):
        make_equivariant()
    key = jax.random.PRNGKey(0)
    tmpl = (
        ppo.init(key, 17, 2, args["hidden"], args["log_std"])
        if args["learner"] == "ppo"
        else sac.init(key, 17, 2, args["hidden"], 0.2, args.get("layernorm", False))
    )
    with np.load(f) as z:
        leaves = [z[f"arr_{i}"] for i in range(len(z.files))]
    params = jax.tree.unflatten(jax.tree.structure(tmpl), leaves)
    act = ppo.mean_action if args["learner"] == "ppo" else sac.mean_action
    return args, params, jax.jit(act), f


def tracks(args, params, act, conds=None):
    assert args["norm"] == "fixed"
    task = Task.load("U")
    conds = conds or eval_panel()
    mean, std = np.asarray(FIXED["mean"]), np.asarray(FIXED["std"])
    vec = Vec(task, len(conds), 0, conds=conds)
    alive = np.ones(len(conds), bool)
    xs = [[r[[RS["x"], RS["y"]]].copy()] for r in vec.rs]
    out = [None] * len(conds)
    a, step = None, 0
    while alive.any():
        if step % args["repeat"] == 0:
            o = np.clip((features(vec.rs, task.target) - mean) / std, -10, 10).astype(
                np.float32
            )
            a = np.asarray(act(params, o))
        rates = to_rates(a, vec.rs, task.decision_dt) if args["head"] == "target" else a
        idx, _, code = vec.step(rates, alive)
        for i in idx:
            xs[i].append(vec.rs[i, [RS["x"], RS["y"]]].copy())
        for j in np.flatnonzero(code):
            alive[idx[j]] = False
            out[idx[j]] = vec.record(idx[j], code[j])
        step += 1
    return [np.asarray(x) for x in xs], out


def main():
    run = Path(sys.argv[1])
    args, params, act, f = load(run, sys.argv[2] if len(sys.argv) > 2 else None)
    xs, out = tracks(args, params, act)
    col = {
        "finished": "tab:green",
        "mark_missed": "tab:red",
        "out_of_bounds": "tab:orange",
        "capsized": "k",
    }
    fig, ax = plt.subplots(figsize=(7, 7))
    for x, o in zip(xs, out, strict=True):
        ax.plot(
            x[:, 0],
            x[:, 1],
            lw=0.8,
            color=col.get(o["reason"] or o["outcome"], "tab:blue"),
        )
    ax.plot([-5, 5], [40, 40], "k-", lw=3)
    ax.set_aspect("equal")
    ax.set_title(
        f"{run.name} {f.stem}: "
        + ", ".join(
            f"{k}={sum((o['reason'] or o['outcome']) == k for o in out)}"
            for k in sorted({o["reason"] or o["outcome"] for o in out})
        )
    )
    png = sys.argv[3] if len(sys.argv) > 3 else str(run / f"{f.stem}.png")
    fig.savefig(png, dpi=90)
    print(png)


if __name__ == "__main__":
    main()
