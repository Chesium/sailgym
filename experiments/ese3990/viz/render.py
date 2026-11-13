"""Top-down rendering of replayed episodes: static figures and synced overlays.

Display only. Poses come from ``viz.replay`` (exact) or the stored 5 Hz pose
(fallback); nothing here integrates or evaluates physics. Hull length/beam,
mast position and boom length are read from the Rust parameter catalogue
(``Spec.parameters_json``), as the web renderer does.

M1 provides the static figure and a basic synced overlay animation; M4's
compositor (checkpoint/controller/seed overlays, grids) builds on these.
"""

from __future__ import annotations

import json
from collections.abc import Sequence
from dataclasses import dataclass
from functools import cache
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402
from matplotlib import animation  # noqa: E402
from matplotlib.collections import LineCollection  # noqa: E402
from matplotlib.patches import FancyArrow, Polygon  # noqa: E402
from sailing import RS, Task  # noqa: E402

import sailgym_core  # noqa: E402
from viz.replay import Trajectory  # noqa: E402

# Hull outline as fractions of (loa, beam), x forward, y to port. Copied from
# `web/src/render/geometry.ts` HULL_OUTLINE (dimensionless display data).
HULL_OUTLINE = np.array(
    [
        [0.5, 0.0], [0.42, 0.18], [0.25, 0.38], [0.05, 0.48], [-0.15, 0.5],
        [-0.35, 0.45], [-0.5, 0.36], [-0.5, -0.36], [-0.35, -0.45], [-0.15, -0.5],
        [0.05, -0.48], [0.25, -0.38], [0.42, -0.18],
    ]
)  # fmt: skip


@dataclass(frozen=True)
class BoatDims:
    loa: float
    beam: float
    mast_x: float
    boom_length: float


@cache
def boat_dims() -> BoatDims:
    p = json.loads(sailgym_core.Spec("free_sail").parameters_json)
    return BoatDims(
        p["hull"]["loa"],
        p["hull"]["beam"],
        p["sail"]["mast_pos_b"]["x"],
        p["sail"]["boom_length"],
    )


def _to_world(pts_b: np.ndarray, x: float, y: float, psi: float) -> np.ndarray:
    c, s = np.cos(psi), np.sin(psi)
    return np.c_[
        x + pts_b[:, 0] * c - pts_b[:, 1] * s, y + pts_b[:, 0] * s + pts_b[:, 1] * c
    ]


def hull_world(x: float, y: float, psi: float, scale: float = 1.0) -> np.ndarray:
    d = boat_dims()
    return _to_world(HULL_OUTLINE * [d.loa * scale, d.beam * scale], x, y, psi)


def boom_world(
    x: float, y: float, psi: float, beta: float, scale: float = 1.0
) -> np.ndarray:
    """Gooseneck → clew; ``b̂(β) = (−cos β, −sin β)`` in the body frame (F2.1)."""
    d = boat_dims()
    a = np.array([d.mast_x * scale, 0.0])
    b = a + d.boom_length * scale * np.array([-np.cos(beta), -np.sin(beta)])
    return _to_world(np.stack([a, b]), x, y, psi)


# ---------------------------------------------------------------------------
# Course and extent
# ---------------------------------------------------------------------------


def gates(task: Task) -> list[tuple[np.ndarray, np.ndarray]]:
    route = json.loads(task.route_json)
    out = []
    for m in route["marks"]:
        g = m["rounding"].get("gate") if isinstance(m["rounding"], dict) else None
        if g:
            out.append(
                (np.array([g[0]["x"], g[0]["y"]]), np.array([g[1]["x"], g[1]["y"]]))
            )
    return out


def draw_course(ax, task: Task) -> None:
    sx, sy = task.start
    ax.plot([sx], [sy], marker="o", ms=6, mfc="white", mec="0.3", zorder=3)
    for a, b in gates(task):
        ax.plot(
            [a[0], b[0]], [a[1], b[1]], ls=(0, (3, 2)), color="0.45", lw=1.2, zorder=2
        )
        for p in (a, b):
            ax.plot([p[0]], [p[1]], marker="o", ms=7, color="#d1495b", zorder=3)


def extent(trajs: Sequence[Trajectory], task: Task, margin: float = 8.0):
    xs = [task.start[0], task.target[0]]
    ys = [task.start[1], task.target[1]]
    for a, b in gates(task):
        xs += [a[0], b[0]]
        ys += [a[1], b[1]]
    for tr in trajs:
        xs += [np.nanmin(tr.col("x")), np.nanmax(tr.col("x"))]
        ys += [np.nanmin(tr.col("y")), np.nanmax(tr.col("y"))]
    x0, x1 = min(xs) - margin, max(xs) + margin
    y0, y1 = min(ys) - margin, max(ys) + margin
    return x0, x1, y0, y1


def draw_wind(
    ax, wind_xy: np.ndarray, where: tuple[float, float], length: float
) -> None:
    """Arrow pointing where the wind blows TO, labelled with speed."""
    w = np.asarray(wind_xy, dtype=float)
    sp = float(np.hypot(*w))
    if sp == 0:
        return
    d = w / sp * length
    ax.add_patch(
        FancyArrow(
            where[0], where[1], d[0], d[1], width=length * 0.06, color="#3a86ff",
            length_includes_head=True, zorder=4,
        )
    )  # fmt: skip
    ax.annotate(
        f"wind {sp:.1f} m/s", (where[0], where[1]), xytext=(4, 4),
        textcoords="offset points", fontsize=8, color="#3a86ff",
    )  # fmt: skip


def label_of(tr: Trajectory) -> str:
    m = tr.meta
    who = m.get("label") or m.get("controller", "?")
    if m["outcome"] == "finished":
        res = f"finished {m['finish_time_s']:.1f} s"
    else:
        res = m.get("reason") or m["outcome"]
    tag = "" if tr.exact else " [pose only]"
    return f"{who} · {m['condition']['id']} · {res}{tag}"


DEFAULT_COLORS = plt.get_cmap("tab10").colors


def _legend(ax, n: int) -> None:
    """Inside for one episode; below the axes for an overlay."""
    if n <= 2:
        ax.legend(loc="lower right", fontsize=7, framealpha=0.9)
    else:
        ax.legend(
            loc="upper center", bbox_to_anchor=(0.5, -0.12), ncol=2, fontsize=7,
            frameon=False,
        )  # fmt: skip


def _setup(ax, trajs, task, title):
    x0, x1, y0, y1 = extent(trajs, task)
    ax.set_xlim(x0, x1)
    ax.set_ylim(y0, y1)
    ax.set_aspect("equal")
    ax.set_xlabel("x east (m)")
    ax.set_ylabel("y north (m)")
    ax.grid(True, color="0.92", lw=0.6)
    ax.set_facecolor("#f7fbff")
    draw_course(ax, task)
    w = trajs[0].states[0, [RS["wind_x"], RS["wind_y"]]]
    if not np.any(np.isnan(w)):
        span = max(x1 - x0, y1 - y0)
        draw_wind(ax, w, (x0 + 0.06 * (x1 - x0), y1 - 0.18 * (y1 - y0)), 0.1 * span)
    if title:
        ax.set_title(title, fontsize=10)
    return x0, x1, y0, y1


# ---------------------------------------------------------------------------
# Static figure
# ---------------------------------------------------------------------------


def plot_static(
    trajs: Sequence[Trajectory],
    task: Task,
    path: Path | str | None = None,
    title: str | None = None,
    colors: Sequence | None = None,
    boat_every_s: float | None = None,
    figsize: tuple[float, float] = (8.0, 6.0),
):
    """Tracks for one or more episodes.

    A single episode gets a hull outline every ``boat_every_s`` (default 3 s);
    an overlay gets only its final hulls, so many tracks stay legible.
    """
    colors = colors or DEFAULT_COLORS
    if boat_every_s is None and len(trajs) == 1:
        boat_every_s = 3.0
    fig, ax = plt.subplots(figsize=figsize, dpi=120)
    _setup(ax, trajs, task, title)
    for i, tr in enumerate(trajs):
        c = colors[i % len(colors)]
        x, y, t = tr.col("x"), tr.col("y"), tr.t
        ax.plot(x, y, color=c, lw=1.6, label=label_of(tr), zorder=5)
        if tr.exact:
            marks = (
                np.unique(np.searchsorted(t, np.arange(0, t[-1], boat_every_s)))
                if boat_every_s
                else []
            )
            for k in [*marks, len(t) - 1]:
                s = tr.states[k]
                ax.add_patch(
                    Polygon(
                        hull_world(s[RS["x"]], s[RS["y"]], s[RS["psi"]]),
                        closed=True, fc=c, ec="0.2", lw=0.5, alpha=0.55, zorder=6,
                    )
                )  # fmt: skip
                bm = boom_world(s[RS["x"]], s[RS["y"]], s[RS["psi"]], s[RS["beta"]])
                ax.plot(bm[:, 0], bm[:, 1], color="0.15", lw=1.0, zorder=7)
        end = "o" if tr.meta["outcome"] == "finished" else "x"
        ax.plot([x[-1]], [y[-1]], marker=end, color=c, ms=7, mew=2, zorder=8)
    _legend(ax, len(trajs))
    fig.tight_layout()
    if path:
        Path(path).parent.mkdir(parents=True, exist_ok=True)
        fig.savefig(path)
        plt.close(fig)
    return fig


def plot_timeseries(tr: Trajectory, path: Path | str) -> None:
    """Heel, speed, rudder, sheet and the commanded rates for one episode."""
    t = tr.t
    fig, axes = plt.subplots(4, 1, figsize=(8, 7), sharex=True, dpi=120)
    speed = np.hypot(tr.col("u"), tr.col("v"))
    axes[0].plot(t, np.degrees(tr.col("phi")), color="#d1495b")
    axes[0].set_ylabel("heel (deg)")
    axes[1].plot(t, speed, color="#2a9d8f")
    axes[1].set_ylabel("speed (m/s)")
    axes[2].plot(t, np.degrees(tr.col("delta_r")), label="rudder (deg)")
    axes[2].plot(t, tr.col("l_sheet"), label="sheet (m)")
    axes[2].legend(fontsize=7)
    ta = t[:-1]
    axes[3].step(ta, tr.actions[:, 0], where="post", label="rudder rate cmd")
    axes[3].step(ta, tr.actions[:, 1], where="post", label="sheet rate cmd")
    axes[3].set_ylim(-1.1, 1.1)
    axes[3].legend(fontsize=7)
    axes[3].set_xlabel("t (s)")
    axes[0].set_title(label_of(tr), fontsize=9)
    for a in axes:
        a.grid(True, color="0.92")
    fig.tight_layout()
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(path)
    plt.close(fig)


# ---------------------------------------------------------------------------
# Synced overlay animation
# ---------------------------------------------------------------------------


def frame_times(
    trajs: Sequence[Trajectory], fps: float, speed: float, tail_s: float
) -> np.ndarray:
    t_end = max(float(tr.t[-1]) for tr in trajs)
    return np.arange(0.0, t_end + tail_s + 1e-9, speed / fps)


def animate(
    trajs: Sequence[Trajectory],
    task: Task,
    path: Path | str,
    fps: float = 20.0,
    speed: float = 2.0,
    title: str | None = None,
    colors: Sequence | None = None,
    boat_scale: float = 1.5,
    trail_s: float | None = None,
    tail_s: float = 1.0,
    figsize: tuple[float, float] = (7.0, 5.5),
    dpi: int = 90,
) -> int:
    """Replay episodes together on one clock (all start at t = 0).

    Writes ``.gif`` (Pillow) or ``.mp4`` (ffmpeg) by extension and returns the
    frame count. A finished or failed boat stays at its final pose, marked.
    ``boat_scale`` enlarges the outline for legibility (stated in the corner).
    """
    if not all(tr.exact for tr in trajs):
        raise ValueError(
            "animation needs exact replays; pose-only trajectories are too coarse"
        )
    colors = colors or DEFAULT_COLORS
    times = frame_times(trajs, fps, speed, tail_s)
    fig, ax = plt.subplots(figsize=figsize, dpi=dpi)
    _setup(ax, trajs, task, title)
    clock = ax.text(
        0.02,
        0.02,
        "",
        transform=ax.transAxes,
        fontsize=9,
        family="monospace",
        zorder=10,
    )
    ax.text(
        0.98, 0.02, f"{speed:g}× · boat ×{boat_scale:g}", transform=ax.transAxes,
        fontsize=7, ha="right", color="0.4",
    )  # fmt: skip
    artists = []
    for i, tr in enumerate(trajs):
        c = colors[i % len(colors)]
        lc = LineCollection([], colors=[c], linewidths=1.6, zorder=5)
        ax.add_collection(lc)
        hull = Polygon(np.zeros((3, 2)), closed=True, fc=c, ec="0.15", lw=0.6, zorder=6)
        ax.add_patch(hull)
        (boom,) = ax.plot([], [], color="0.1", lw=1.2, zorder=7)
        (end,) = ax.plot([], [], marker="", color=c, ms=8, mew=2, ls="", zorder=8)
        ax.plot([], [], color=c, lw=2, label=label_of(tr))
        artists.append((tr, lc, hull, boom, end))
    _legend(ax, len(trajs))
    fig.tight_layout()

    def update(fi: int):
        t = times[fi]
        out = [clock]
        clock.set_text(f"t = {t:5.1f} s")
        for tr, lc, hull, boom, end in artists:
            k = min(int(np.searchsorted(tr.t, t, side="right")) - 1, len(tr.t) - 1)
            k0 = 0 if trail_s is None else int(np.searchsorted(tr.t, t - trail_s))
            xy = tr.states[k0 : k + 1][:, [RS["x"], RS["y"]]]
            lc.set_segments([xy] if len(xy) > 1 else [])
            s = tr.states[k]
            hull.set_xy(hull_world(s[RS["x"]], s[RS["y"]], s[RS["psi"]], boat_scale))
            bm = boom_world(
                s[RS["x"]], s[RS["y"]], s[RS["psi"]], s[RS["beta"]], boat_scale
            )
            boom.set_data(bm[:, 0], bm[:, 1])
            if k == len(tr.t) - 1:
                end.set_marker("o" if tr.meta["outcome"] == "finished" else "x")
                end.set_data([s[RS["x"]]], [s[RS["y"]]])
            out += [lc, hull, boom, end]
        return out

    anim = animation.FuncAnimation(fig, update, frames=len(times), blit=True)
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.suffix == ".gif":
        writer = animation.PillowWriter(fps=fps)
    elif path.suffix == ".mp4":
        writer = animation.FFMpegWriter(fps=fps, bitrate=1800)
    else:
        raise ValueError(f"unsupported video type {path.suffix}")
    anim.save(path, writer=writer)
    plt.close(fig)
    return len(times)
