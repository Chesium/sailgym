"""Render episodes from a store.

    uv run python experiments/ese3990/viz/show.py STORE [--condition ID ...]
        [--manifest NAME] [--limit N] --out DIR [--gif] [--mp4] [--timeseries]

Every selected episode is replayed exactly (and verified) before rendering;
``--allow-pose-only`` falls back to the stored 5 Hz pose for static figures
when the store came from another build.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from sailing import Task  # noqa: E402

from viz.episode_store import EpisodeStore  # noqa: E402
from viz.render import animate, plot_static, plot_timeseries  # noqa: E402
from viz.replay import ReplayMismatch, pose_only, replay  # noqa: E402


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("store", type=Path)
    ap.add_argument("--condition", nargs="*")
    ap.add_argument("--manifest")
    ap.add_argument("--limit", type=int)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--name", default="episodes")
    ap.add_argument("--title")
    ap.add_argument("--gif", action="store_true")
    ap.add_argument("--mp4", action="store_true")
    ap.add_argument("--timeseries", action="store_true")
    ap.add_argument("--speed", type=float, default=2.0)
    ap.add_argument("--fps", type=float, default=20.0, help="mp4 frame rate")
    ap.add_argument("--gif-fps", type=float, default=10.0)
    ap.add_argument("--allow-pose-only", action="store_true")
    args = ap.parse_args(argv)

    store = EpisodeStore(args.store)
    filters = {}
    if args.manifest:
        filters["manifest"] = args.manifest
    if args.condition:
        wanted = set(args.condition)
        filters["condition"] = lambda c: c in wanted
    entries = store.query(**filters)[: args.limit]
    if not entries:
        print("no episodes match", file=sys.stderr)
        return 1
    tasks = {e["task"] for e in entries}
    if len(tasks) != 1:
        print(f"select one task, got {sorted(tasks)}", file=sys.stderr)
        return 1
    task = Task.load(tasks.pop())

    trajs = []
    for e in entries:
        log = store.load(e)
        try:
            trajs.append(replay(log, store.identity, task))
        except ReplayMismatch as err:
            if not args.allow_pose_only:
                raise
            print(f"{e['episode_id']}: {err} — using stored pose", file=sys.stderr)
            trajs.append(pose_only(log))
    print(f"replayed {sum(t.exact for t in trajs)}/{len(trajs)} episodes exactly")

    args.out.mkdir(parents=True, exist_ok=True)
    title = args.title or f"Task {task.name}"
    plot_static(trajs, task, args.out / f"{args.name}.png", title=title)
    if args.timeseries:
        for tr in trajs:
            plot_timeseries(
                tr, args.out / f"{args.name}_{tr.meta['episode_id']}_ts.png"
            )
    # gif is palette-per-frame and grows fast, so it gets fewer, smaller frames
    # than mp4 by default.
    for ext, on, fps, dpi in (
        ("gif", args.gif, args.gif_fps, 70),
        ("mp4", args.mp4, args.fps, 100),
    ):
        if on:
            n = animate(
                trajs, task, args.out / f"{args.name}.{ext}", fps=fps, speed=args.speed,
                title=title, dpi=dpi,
            )  # fmt: skip
            print(f"wrote {args.name}.{ext}: {n} frames")
    return 0


if __name__ == "__main__":
    sys.exit(main())
