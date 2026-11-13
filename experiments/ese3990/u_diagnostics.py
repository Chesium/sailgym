"""Upwind (task U) behaviour diagnostics from exact replay (plan M4U).

    env -u PYTHONPATH uv run --frozen --no-sync python \
        experiments/ese3990/u_diagnostics.py STORE --out DIAG.json [--jobs 6]

``diagnose`` is a pure function of an exactly replayed ``Trajectory``. Its
thresholds come from ``configs/M4U_diagnostics.json``, which is frozen before
held-out results. Definitions:

* **TWA**: sailgym-task's ``true_wind_angle``, the FROM direction off the bow,
  positive to starboard, 0 head to wind (F2). It is unwrapped along the episode
  so crossings of head to wind (multiples of 2π) and dead downwind (odd
  multiples of π) can be counted.
* **Settled on a tack**: |TWA| >= ``head_to_wind_deg`` on one side with surge
  ``u >= way_mps``, held for ``settle_s``. The start counts as settled on the
  side of its initial TWA. A gate passage settles the boat wherever it is.
* Between two settled spells the boat's net path through the wind classifies
  the manoeuvre. A **tack** crosses head to wind exactly once net and settles
  on the other side. A **gybe** does the same through dead downwind. A
  **failed tack** crosses head to wind and falls back to the side it left. A
  **turn** goes all the way round. A tack still unsettled when the episode
  times out or terminates is reported as ``tack_unsettled_at_end`` and is not
  counted.
* **In irons**: |TWA| < ``head_to_wind_deg`` with ``u < way_mps`` (stopped or
  making sternway with the bow near the wind). **Sternway**: ``u < 0``.
"""

from __future__ import annotations

import argparse
import json
import math
import multiprocessing
import sys
from concurrent.futures import ProcessPoolExecutor
from functools import cache
from itertools import pairwise
from pathlib import Path
from typing import Any

import numpy as np
from sailing import HERE, RS, TRUNCATED, Task, sha256_text
from viz.episode_store import EpisodeStore
from viz.replay import Trajectory, replay

CONFIG = HERE / "configs" / "M4U_diagnostics.json"
FAILURE_REASONS = ("mark_missed", "out_of_bounds", "capsized")


def load_config(path: Path | str = CONFIG) -> dict[str, float]:
    return {
        k: float(v["value"])
        for k, v in json.loads(Path(path).read_text())["thresholds"].items()
    }


def true_wind_angle(states: np.ndarray) -> np.ndarray:
    """``sailgym_task::true_wind_angle`` on research states: ``atan2(h_y, -h_x)``
    with ``h = R_z(-ψ) W`` (``frames::world_to_body``)."""
    psi = states[:, RS["psi"]]
    wx, wy = states[:, RS["wind_x"]], states[:, RS["wind_y"]]
    hx = np.cos(psi) * wx + np.sin(psi) * wy
    hy = -np.sin(psi) * wx + np.cos(psi) * wy
    return np.arctan2(hy, -hx)


def _runs(mask: np.ndarray) -> list[tuple[int, int]]:
    """Inclusive ``(start, end)`` index pairs of the True runs of ``mask``."""
    d = np.diff(np.r_[0, mask.astype(np.int8), 0])
    return list(zip(np.flatnonzero(d == 1), np.flatnonzero(d == -1) - 1, strict=True))


def _classify(th: np.ndarray, a: int, b: int) -> str | None:
    """The manoeuvre between settled indices ``a`` and ``b`` of unwrapped TWA."""
    # Lines kπ crossed net: boundaries between the cells [kπ, (k+1)π) of a and
    # b, so an end sitting exactly on a line is counted once, consistently.
    lo, hi = sorted((math.floor(th[a] / math.pi), math.floor(th[b] / math.pi)))
    ks = np.arange(lo + 1, hi + 1)
    if len(ks) == 1:
        return "tack" if ks[0] % 2 == 0 else "gybe"
    if len(ks) >= 2:
        return "turn"
    # Net zero: a failed tack only if the bow did cross head to wind.
    crossed = np.any(np.diff(np.floor(th[a : b + 1] / (2 * math.pi))) != 0)
    return "failed_tack" if crossed else None


def diagnose(
    tr: Trajectory, task: Task, cfg: dict[str, float] | None = None
) -> dict[str, Any]:
    if not tr.exact:
        raise ValueError("U diagnostics need an exact replay, not the stored pose")
    cfg = cfg or load_config()
    st, t, m = tr.states, tr.t, tr.meta
    n = len(st)
    twa = true_wind_angle(st)
    th = np.unwrap(twa)
    u = st[:, RS["u"]]
    w = np.r_[np.diff(t), 0.0]  # state k holds for the decision that follows it
    h2w = math.radians(cfg["head_to_wind_deg"])
    way = cfg["way_mps"]
    finished = m["outcome"] == "finished"

    # Settled spells as (start, end, settled); sides are read from th.
    spells = []
    if abs(twa[0]) >= h2w:
        spells.append((0, 0, True))
    ok = (np.abs(twa) >= h2w) & (u >= way)
    for side in (twa > 0, twa < 0):
        spells += [
            (s, e, True) for s, e in _runs(ok & side) if t[e] - t[s] >= cfg["settle_s"]
        ]
    spells.sort()
    if not spells or spells[-1][1] < n - 1:
        spells.append((n - 1, n - 1, finished))

    events = []
    for (_, a, _), (b, _, settled) in pairwise(spells):
        kind = _classify(th, a, b) if b > a else None
        if kind:
            events.append(
                {"type": kind, "t0": float(t[a]), "t1": float(t[b]), "settled": settled}
            )

    def count(kind: str, settled_only: bool = True) -> int:
        return sum(
            e["type"] == kind and (e["settled"] or not settled_only) for e in events
        )

    irons = (np.abs(twa) < h2w) & (u < way)
    d = np.hypot(st[:, RS["x"]] - task.target[0], st[:, RS["y"]] - task.target[1])
    return {
        "episode_id": m.get("episode_id"),
        "condition": m["condition"]["id"],
        "outcome": m["outcome"],
        "reason": m["reason"],
        "duration_s": float(t[-1]),
        "tacks": count("tack"),
        "failed_tacks": count("failed_tack", False),
        "gybes": count("gybe"),
        "turns": count("turn", False),
        "tack_unsettled_at_end": any(
            e["type"] == "tack" and not e["settled"] for e in events
        ),
        "irons_s": float(w[irons].sum()),
        "irons_longest_s": max(
            (float(w[s : e + 1].sum()) for s, e in _runs(irons)), default=0.0
        ),
        "sternway_s": float(w[u < 0].sum()),
        "timeout": m["outcome"] == TRUNCATED,
        **{r: m["reason"] == r for r in FAILURE_REASONS},
        "final_distance_m": float(d[-1]),
        "min_distance_m": float(d.min()),
        "max_y_m": float(st[:, RS["y"]].max()),
        "final_x_m": float(st[-1, RS["x"]]),
        "final_y_m": float(st[-1, RS["y"]]),
        "events": events,
    }


def summarize(diags: list[dict[str, Any]]) -> dict[str, Any]:
    """Counts and per-episode means for one group of episodes. Timeout final
    distance is over timeouts only, never mixed with finishes."""
    k = len(diags)
    timeouts = [d["final_distance_m"] for d in diags if d["timeout"]]

    def mean(key: str) -> float | None:
        return float(np.mean([d[key] for d in diags])) if k else None

    return {
        "episodes": k,
        "finished": sum(d["outcome"] == "finished" for d in diags),
        "timeouts": len(timeouts),
        **{r: sum(d[r] for d in diags) for r in FAILURE_REASONS},
        "tacks_histogram": {
            str(v): sum(d["tacks"] == v for d in diags)
            for v in sorted({d["tacks"] for d in diags})
        },
        "episodes_with_a_tack": sum(d["tacks"] > 0 for d in diags),
        **{
            f"mean_{key}": mean(key)
            for key in (
                "tacks",
                "failed_tacks",
                "gybes",
                "irons_s",
                "sternway_s",
                "max_y_m",
            )
        },  # fmt: skip
        "timeout_final_distance_m": {
            "median": float(np.median(timeouts)) if timeouts else None,
            "min": min(timeouts, default=None),
            "max": max(timeouts, default=None),
        },
    }


# ---------------------------------------------------------------------------
# A whole store, replayed exactly, in parallel
# ---------------------------------------------------------------------------


@cache
def _task(name: str, version: int) -> Task:
    return Task.load(name, version)


def _chunk(args: tuple[str, int, int, dict[str, float]]) -> dict[str, Any]:
    path, lo, hi, cfg = args
    store = EpisodeStore(Path(path))
    out = {}
    for e in store.index[lo:hi]:
        task = _task(e["task"], e.get("contract_version", 1))
        tr = replay(store.load(e), store.identity, task)  # raises unless exact
        out[e["episode_id"]] = diagnose(tr, task, cfg)
    return out


def diagnose_store(
    store: Path | str,
    jobs: int = 6,
    cfg: dict[str, float] | None = None,
    chunk: int = 8,
) -> dict[str, dict[str, Any]]:
    """``{episode_id: diagnose(...)}`` for every episode of ``store``."""
    cfg = cfg or load_config()
    k = len(EpisodeStore(Path(store)).index)
    work = [(str(store), lo, min(lo + chunk, k), cfg) for lo in range(0, k, chunk)]
    out: dict[str, dict[str, Any]] = {}
    spawn = multiprocessing.get_context("spawn")  # no inherited JAX/Rayon state
    with ProcessPoolExecutor(jobs, mp_context=spawn) as ex:
        for part in ex.map(_chunk, work):
            out.update(part)
    return out


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("store", type=Path)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--config", type=Path, default=CONFIG)
    ap.add_argument("--jobs", type=int, default=6)
    args = ap.parse_args(argv)
    cfg = load_config(args.config)
    diags = diagnose_store(args.store, args.jobs, cfg)
    res = {
        "store": str(args.store),
        "config": str(args.config),
        "config_sha256": sha256_text(args.config.read_text()),
        "thresholds": cfg,
        "summary": summarize(list(diags.values())),
        "episodes": diags,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(res, indent=1) + "\n")
    print(json.dumps(res["summary"], indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
