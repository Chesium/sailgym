"""Replay every episode of one or more stores exactly, in parallel (plan §10.4).

    uv run --frozen --no-sync python experiments/ese3990/replay_audit.py \
        runs/ese3990/M4/store [more stores] [--out audit.json] [--jobs 24]

Each episode re-simulates under the contract it recorded and must reproduce its
logged outcome, physics steps, return and final checksum. Exits non-zero on
any mismatch; the JSON lists every failure.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

from viz.episode_store import EpisodeStore
from viz.replay import replay


def _audit(args: tuple[str, int, int]) -> tuple[int, int, int, list[str]]:
    path, lo, hi = args
    store = EpisodeStore(Path(path))
    n = decisions = steps = 0
    bad = []
    for e in store.index[lo:hi]:
        try:
            replay(store.load(e), store.identity)
        except Exception as err:  # noqa: BLE001 — every failure is reported
            bad.append(f"{path}:{e['episode_id']}: {err}")
            continue
        n += 1
        decisions += e["decisions"]
        steps += e["physics_steps"]
    return n, decisions, steps, bad


def audit(stores: list[Path], jobs: int, chunk: int = 64) -> dict:
    t0 = time.perf_counter()
    work = []
    total = 0
    for s in stores:
        k = len(EpisodeStore(s).index)
        total += k
        work += [(str(s), lo, min(lo + chunk, k)) for lo in range(0, k, chunk)]
    n = decisions = steps = 0
    bad: list[str] = []
    with ProcessPoolExecutor(jobs) as ex:
        for a, b, c, d in ex.map(_audit, work):
            n, decisions, steps, bad = n + a, decisions + b, steps + c, bad + d
    return {
        "stores": [str(s) for s in stores],
        "episodes": total,
        "exact_replays": n,
        "replay_decisions": decisions,
        "replay_physics_steps": steps,
        "failures": bad,
        "wall_s": time.perf_counter() - t0,
        "jobs": jobs,
    }


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("stores", nargs="+", type=Path)
    ap.add_argument("--out", type=Path)
    ap.add_argument("--jobs", type=int, default=os.cpu_count())
    args = ap.parse_args(argv)
    res = audit(args.stores, args.jobs)
    text = json.dumps(res, indent=2)
    print(text)
    if args.out:
        args.out.write_text(text + "\n")
    return 0 if res["exact_replays"] == res["episodes"] else 1


if __name__ == "__main__":
    sys.exit(main())
