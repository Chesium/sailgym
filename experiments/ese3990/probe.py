"""M4P exploration probe (plan §6): random policies on U, no learning.

Can undirected exploration ever produce a tack? Every cell drives the same 100
training conditions with rate noise of one colour (power spectrum ∝ 1/f^β),
sample-and-hold interval and scale. M1's calibration controller
(``SmoothRandom(hold=10)``) is the reference cell. Every episode is stored;
the report replays each one exactly and counts tacks with ``u_diagnostics``
under the frozen M4U thresholds. Post-hoc and exploratory.

    python experiments/ese3990/probe.py freeze        # before any probe episode
    python experiments/ese3990/probe.py run --out runs/ese3990/M4P --jobs 12
    python experiments/ese3990/probe.py report --run runs/ese3990/M4P --out docs/ese3990
"""

from __future__ import annotations

import argparse
import json
import multiprocessing
import os
import time
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

import numpy as np
import u_diagnostics
from baselines import SmoothRandom
from compare import digest
from evaluate import run_episode
from make_conditions import TrainingSampler
from sailing import HERE, REPO, Task, contract_identity
from viz.episode_store import EpisodeStore, EpisodeWriter, build_identity

FREEZE = HERE / "configs/M4P_freeze.json"
DIAGNOSTICS = HERE / "configs/M4U_diagnostics.json"
LABEL = "M4P exploration probe (post-hoc, exploratory; no learning)"
VERSION = 1
BETAS, HOLDS, SIGMAS = (0, 1, 2), (1, 10, 20, 40), (0.3, 0.6, 1.0)
EPISODES = 100
PROBE_SEED = 100  # condition sampler and controller seeds; training seeds are 0-2
TACK_MIN, FAILURE_MAX = 0.10, 0.10
SOURCES = [
    "probe.py",
    "baselines.py",
    "evaluate.py",
    "make_conditions.py",
    "sailing.py",
    "u_diagnostics.py",
    "viz/episode_store.py",
    "viz/replay.py",
]
READINGS = [
    [
        "White noise at a 0.05 s hold already reaches",
        "Exploration noise is not U's bottleneck; the next arm targets the critic "
        "and the decision horizon",
    ],
    [
        "White at 0.05 s does not reach, but a correlated or held process does",
        "Per-decision white noise is a cause; that process is the exploration "
        "setting for the next RL arm",
    ],
    [
        "Tacks appear only together with more than 10% failures",
        "Coherent exploration runs into terminal states; start-state curricula or "
        "resets come before noise changes",
    ],
    [
        "No cell reaches 10% tacks",
        "Undirected exploration cannot find the tack; next is reset-based "
        "discovery (reverse-start curriculum, Go-Explore)",
    ],
    [
        "Any random finish",
        "Reported with its process: the success rate a sparse-outcome learner "
        "would start from",
    ],
]


def rel(p: Path) -> str:
    return os.path.relpath(p, REPO)


# ---------------------------------------------------------------------------
# The noise and the controller
# ---------------------------------------------------------------------------


def powerlaw_noise(rng: np.random.Generator, beta: float, n: int) -> np.ndarray:
    """Unit-variance Gaussian noise of length ``n`` with power ∝ 1/f^β (β = 0
    white, 1 pink, 2 red), by shaping a random spectrum (Timmer & König 1995)."""
    if n < 2:  # one sample has no spectrum
        return rng.standard_normal(n)
    f = np.fft.rfftfreq(n)
    f[0] = 1.0 / n  # the lowest resolvable frequency stands in for DC
    z = rng.standard_normal(f.size) + 1j * rng.standard_normal(f.size)
    x = np.fft.irfft(f ** (-beta / 2) * z, n)
    return x / x.std()


class NoiseProbe:
    """Rates σ·noise clipped to [−1, 1]. One noise realisation per episode and
    action dimension, at one sample per ``hold`` decisions, each held."""

    version = VERSION

    def __init__(
        self, seed: int, beta: float, hold: int, sigma: float, decisions: int
    ) -> None:
        self.seed, self.beta, self.hold, self.sigma = seed, beta, hold, sigma
        self.decisions = decisions
        self.rng = np.random.default_rng(seed)

    def reset(self, env: object) -> None:
        n = -(-self.decisions // self.hold)
        x = np.stack([powerlaw_noise(self.rng, self.beta, n) for _ in range(2)], 1)
        self._seq = np.clip(self.sigma * x, -1.0, 1.0)
        self._k = 0

    def act(self, env: object) -> np.ndarray:
        a = self._seq[self._k // self.hold]
        self._k += 1
        return a

    def identity(self) -> dict:
        return {
            "controller": "noise_probe",
            "version": self.version,
            "beta": self.beta,
            "hold": self.hold,
            "sigma": self.sigma,
        }


def cells() -> list[dict]:
    out = [{"id": "smooth_random_hold10", "controller": "smooth_random", "hold": 10}]
    out += [
        {
            "id": f"beta{b}_hold{h:02d}_sigma{s}",
            "controller": "noise_probe",
            "beta": b,
            "hold": h,
            "sigma": s,
        }
        for b in BETAS
        for h in HOLDS
        for s in SIGMAS
    ]
    return [c | {"index": i} for i, c in enumerate(out)]


def controller(cell: dict, episode: int, decisions: int):
    """A fresh controller per episode, seeded by (cell, episode), so a resumed
    cell draws exactly the actions an uninterrupted one would."""
    seed = 1_000_000 * PROBE_SEED + 1_000 * cell["index"] + episode
    if cell["controller"] == "smooth_random":
        return SmoothRandom(seed, cell["hold"]), seed
    return NoiseProbe(seed, cell["beta"], cell["hold"], cell["sigma"], decisions), seed


def conditions(n: int = EPISODES) -> list:
    """The same n training conditions for every cell (paired across cells)."""
    s = TrainingSampler(PROBE_SEED)
    return [s.next() for _ in range(n)]


# ---------------------------------------------------------------------------
# Freeze, run
# ---------------------------------------------------------------------------


def make_freeze() -> dict:
    task = Task.load("U")
    return {
        "label": LABEL,
        "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "task": "U",
        **contract_identity(task),
        "identity": build_identity(),
        "cells": cells(),
        "episodes_per_cell": EPISODES,
        "conditions": {
            "sampler": "make_conditions.TrainingSampler",
            "seed": PROBE_SEED,
            "ids": [c.id for c in conditions()],
        },
        "seeds": "controller 1e6·100 + 1e3·cell + episode; environment seed = "
        "episode index (paired across cells)",
        "threshold": {
            "reaches": f"≥ {TACK_MIN:.0%} of episodes with a settled tack and "
            f"≤ {FAILURE_MAX:.0%} ending in a failure (out of bounds, capsize, "
            "mark missed)",
            "tack_min": TACK_MIN,
            "failure_max": FAILURE_MAX,
        },
        "readings": READINGS,
        "diagnostics_thresholds": {rel(DIAGNOSTICS): digest(DIAGNOSTICS)},
        "source_sha256": {
            f"experiments/ese3990/{s}": digest(HERE / s) for s in SOURCES
        },
    }


def prepare() -> dict:
    fz = json.loads(FREEZE.read_text())
    for name, h in fz["source_sha256"].items():
        if digest(REPO / name) != h:
            raise ValueError(f"source changed since the freeze: {name}")
    if fz["identity"] != build_identity():
        raise ValueError("build identity differs from the freeze")
    return fz


def read_jsonl(path: Path) -> list[dict]:
    return (
        [json.loads(x) for x in path.read_text().splitlines()] if path.exists() else []
    )


def run_cell(out: Path, cell: dict, task: Task, conds: list) -> int:
    """One cell's missing episodes (resumable by episode index)."""
    path = out / "episodes" / f"{cell['id']}.jsonl"
    done = {e["episode_index"] for e in read_jsonl(path)}
    todo = [(i, c) for i, c in enumerate(conds) if i not in done]
    if not todo:
        return 0
    path.parent.mkdir(parents=True, exist_ok=True)
    decisions = task.max_steps // task.cadence
    meta = {"cell": cell, "freeze_sha256": digest(out / "freeze.json")}
    with EpisodeWriter(out / "store" / cell["id"], meta, shard_size=1) as w:
        for i, cond in todo:
            ctrl, seed = controller(cell, i, decisions)
            log = {"cell": cell["id"], "kind": "exploration_probe", "seed_ctrl": seed}
            ep = run_episode(task, cond, ctrl, seed=i, writer=w, log_meta=log)
            ep.pop("decision_latency_s")
            ep.update(cell=cell["id"], episode_index=i, controller_seed=seed)
            with path.open("a") as f:
                f.write(json.dumps(ep) + "\n")
    return len(todo)


def _run_cell(args: tuple[str, dict, int]) -> tuple[str, int, float]:
    out, cell, n = args
    t = time.perf_counter()
    k = run_cell(Path(out), cell, Task.load("U"), conditions(n))
    return cell["id"], k, time.perf_counter() - t


def run(out: Path, jobs: int, only: list[str] | None = None) -> None:
    fz = prepare()
    out.mkdir(parents=True, exist_ok=True)
    frozen = out / "freeze.json"
    if frozen.exists() and json.loads(frozen.read_text()) != fz:
        raise ValueError(f"{frozen} differs from {FREEZE}; use a new run directory")
    frozen.write_text(json.dumps(fz, indent=1) + "\n")
    todo = [c for c in fz["cells"] if not only or c["id"] in only]
    work = [(str(out), c, fz["episodes_per_cell"]) for c in todo]
    launch = {"time": time.time(), "jobs": jobs, "cells": len(work)}
    with (out / "launches.jsonl").open("a") as f:
        f.write(json.dumps(launch | {"load_average": os.getloadavg()}) + "\n")
    spawn = multiprocessing.get_context("spawn")
    with ProcessPoolExecutor(jobs, mp_context=spawn) as ex:
        for cid, k, wall in ex.map(_run_cell, work):
            print(f"{cid}: {k} episodes in {wall:.0f} s", flush=True)
    if not only:
        (out / "complete.json").write_text(json.dumps({"time": time.time()}) + "\n")


# ---------------------------------------------------------------------------
# Report: exact replay of every episode, tack diagnostics, the grid figure
# ---------------------------------------------------------------------------


def report(run_dir: Path, out: Path, jobs: int) -> dict:
    fz = json.loads((run_dir / "freeze.json").read_text())
    if not (run_dir / "complete.json").exists():
        raise ValueError(f"{run_dir} is incomplete")
    (path,) = fz["diagnostics_thresholds"]
    if digest(REPO / path) != fz["diagnostics_thresholds"][path]:
        raise ValueError(f"frozen input changed: {path}")
    cfg = u_diagnostics.load_config(REPO / path)
    work = []
    for c in fz["cells"]:
        store = run_dir / "store" / c["id"]
        if EpisodeStore(store).identity != fz["identity"]:
            raise ValueError(f"{c['id']}: store built on a different build")
        k = len(EpisodeStore(store).index)
        if k != fz["episodes_per_cell"]:
            raise ValueError(
                f"{c['id']}: {k} episodes, expected {fz['episodes_per_cell']}"
            )
        work += [(str(store), lo, min(lo + 10, k), cfg) for lo in range(0, k, 10)]
    spawn = multiprocessing.get_context("spawn")
    with ProcessPoolExecutor(jobs, mp_context=spawn) as ex:
        parts = list(ex.map(u_diagnostics._chunk, work))  # raises unless exact
    diags: dict[str, list] = {}
    for w, part in zip(work, parts, strict=True):
        diags.setdefault(Path(w[0]).name, []).extend(part.values())
    rows, flags = cell_rows(fz["cells"], diags)
    result = {
        "label": LABEL,
        "freeze": {rel(FREEZE): digest(FREEZE)},
        "flags": flags,
        "cells": rows,
    }
    (out / "figs").mkdir(parents=True, exist_ok=True)
    grid_figure(rows, out / "figs" / "M4P_tack_grid.png")
    (out / "M4P_results.json").write_text(json.dumps(result, indent=1) + "\n")
    return result


def cell_rows(cells: list[dict], diags: dict[str, list]) -> tuple[list, dict]:
    """Per-cell u_diagnostics summary, tack and failure rates, the declared
    threshold, and the flags the readings are checked against."""
    rows = []
    for c in cells:
        d = diags[c["id"]]
        s = u_diagnostics.summarize(d)
        n = s["episodes"]
        fail = sum(s[r] for r in u_diagnostics.FAILURE_REASONS)
        tack = s["episodes_with_a_tack"] / n
        rows.append(
            {
                **c,
                **s,
                "tack_rate": tack,
                "failure_rate": fail / n,
                "share_max_y_ge_20m": float(np.mean([x["max_y_m"] >= 20 for x in d])),
                "reaches": tack >= TACK_MIN and fail / n <= FAILURE_MAX,
            }
        )
    white = [r for r in rows if r.get("beta") == 0 and r["hold"] == 1]
    flags = {
        "white_per_decision_reaches": any(r["reaches"] for r in white),
        "cells_reaching": [r["id"] for r in rows if r["reaches"]],
        "cells_with_tacks_but_failures": [
            r["id"] for r in rows if r["tack_rate"] >= TACK_MIN and not r["reaches"]
        ],
        "finishes": {r["id"]: r["finished"] for r in rows if r["finished"]},
        "episodes_replayed_exactly": sum(r["episodes"] for r in rows),
    }
    return rows, flags


def grid_figure(rows: list[dict], path: Path) -> None:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    def at(b: int, h: int, s: float) -> dict:
        return next(
            r
            for r in rows
            if r.get("beta") == b and r["hold"] == h and r.get("sigma") == s
        )

    fig, axes = plt.subplots(1, len(SIGMAS), figsize=(4.2 * len(SIGMAS), 3.4))
    for ax, s in zip(axes, SIGMAS, strict=True):
        m = np.array([[at(b, h, s)["tack_rate"] for h in HOLDS] for b in BETAS])
        ax.imshow(100 * m, vmin=0, vmax=100, cmap="viridis")
        for i, b in enumerate(BETAS):
            for j, h in enumerate(HOLDS):
                r = at(b, h, s)
                text = (
                    f"{100 * r['tack_rate']:.0f}%\nfail {100 * r['failure_rate']:.0f}%"
                )
                ax.text(j, i, text, ha="center", va="center", fontsize=7, color="w")
        ax.set_xticks(range(len(HOLDS)), [f"{0.05 * h:g} s" for h in HOLDS])
        ax.set_yticks(range(len(BETAS)), ["white", "pink", "red"])
        ax.set_xlabel("hold")
        ax.set_title(f"σ = {s}", fontsize=9)
    ref = next(r for r in rows if r["controller"] == "smooth_random")
    fig.suptitle(
        "Episodes with a settled tack (and failures), 100 U episodes per cell, no "
        f"learning. Reference SmoothRandom(hold 0.5 s): {100 * ref['tack_rate']:.0f}% "
        f"tacks, {100 * ref['failure_rate']:.0f}% failures. M4P, exploratory.",
        fontsize=8,
    )
    fig.tight_layout()
    fig.savefig(path, dpi=150)
    plt.close(fig)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("freeze")
    r = sub.add_parser("run")
    r.add_argument("--out", type=Path, required=True)
    r.add_argument("--jobs", type=int, default=os.cpu_count())
    r.add_argument("--only", nargs="*", help="cell ids (a timing pilot)")
    p = sub.add_parser("report")
    p.add_argument("--run", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--jobs", type=int, default=os.cpu_count())
    a = ap.parse_args()
    if a.cmd == "freeze":
        if FREEZE.exists():
            raise SystemExit(f"{FREEZE} exists; a freeze is written once")
        FREEZE.write_text(json.dumps(make_freeze(), indent=1) + "\n")
        print(f"wrote {FREEZE}")
    elif a.cmd == "run":
        run(a.out, a.jobs, a.only)
    else:
        print(json.dumps(report(a.run, a.out, a.jobs)["flags"], indent=1))


if __name__ == "__main__":
    main()
