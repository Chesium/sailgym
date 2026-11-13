"""Batch exact-replay overlays from saved JSON video specs (M4/§10).

No controller or simulation collection runs here. Every selector must identify
exactly one stored episode, and all boats in an overlay share one condition.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import matplotlib  # noqa: E402

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
from sailing import REPO, Task  # noqa: E402

from viz.episode_store import EpisodeStore  # noqa: E402
from viz.render import animate, plot_static, plot_timeseries  # noqa: E402
from viz.replay import replay  # noqa: E402


def trajectories(spec: dict):
    result = []
    for item in spec["episodes"]:
        store = EpisodeStore(REPO / item["store"])
        entries = store.query(**item["query"])
        if len(entries) != 1:
            raise ValueError(
                f"selector must match one episode, got {len(entries)}: {item}"
            )
        tr = replay(store.load(entries[0]), store.identity)
        tr.meta["label"] = item["label"]
        result.append(tr)
    if (
        not result
        or len({json.dumps(t.meta["condition"], sort_keys=True) for t in result}) != 1
    ):
        raise ValueError("an overlay needs episodes from one identical condition")
    return result


def compose(spec: dict, out: Path, formats=("gif", "mp4")) -> dict:
    name = spec["id"]
    if Path(name).name != name:
        raise ValueError("video id must be a filename")
    fps, speed = spec.get("fps", 10), spec.get("speed", 6)
    if not (0 < fps < float("inf") and 0 < speed < float("inf")):
        raise ValueError("fps and speed must be finite and positive")
    trajs = trajectories(spec)
    tasks = {tr.meta["task"] for tr in trajs}
    if len(tasks) != 1:
        raise ValueError("an overlay needs one task")
    task = Task.load(tasks.pop())
    out.mkdir(parents=True, exist_ok=True)
    plot_static(trajs, task, out / f"{name}.png", title=spec["title"])
    if spec.get("timeseries"):
        for i, tr in enumerate(trajs):
            plot_timeseries(tr, out / f"{name}_{i}_controls.png")
    frames = {
        ext: animate(
            trajs,
            task,
            out / f"{name}.{ext}",
            fps=fps,
            speed=speed,
            title=spec["title"],
            dpi=75,
        )
        for ext in formats
    }
    return {
        "id": name,
        "frames": frames,
        "episodes": [
            {
                k: tr.meta[k]
                for k in (
                    "episode_id",
                    "label",
                    "final_checksum",
                    "condition",
                    "outcome",
                )
            }
            for tr in trajs
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("spec", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    specs = json.loads(args.spec.read_text())
    if not specs or len({s["id"] for s in specs}) != len(specs):
        raise ValueError("need at least one spec and unique video ids")
    results = []
    for spec in specs:
        results.append(compose(spec, args.out))
        print(f"rendered {spec['id']}", flush=True)
    # A static small-multiples view reuses the exact same rendered overlays.
    count = (len(specs) + 1) // 2
    fig, axes = plt.subplots(count, 2, squeeze=False, figsize=(14, 5 * count))
    for ax in axes.flat:
        ax.set_axis_off()
    for ax, spec in zip(axes.flat, specs, strict=False):
        ax.imshow(plt.imread(args.out / f"{spec['id']}.png"))
    fig.tight_layout(pad=0.2)
    fig.savefig(args.out / "grid.png", dpi=120)
    plt.close(fig)
    (args.out / "render.json").write_text(
        json.dumps(
            {
                "spec_sha256": hashlib.sha256(args.spec.read_bytes()).hexdigest(),
                "renderer_sha256": {
                    p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                    for p in (Path(__file__), Path(__file__).with_name("render.py"))
                },
                "videos": results,
            },
            indent=2,
        )
        + "\n"
    )


if __name__ == "__main__":
    main()
