"""M8 general leg follower (post-hoc, exploratory; plan §6 M8).

The leg frame: every leg runs along +y from (0, 0) to a 5 m half-width gate
at (0, D). Its bearing to the wind is set by rotating the wind, not the
course. The x bounds are the corridor (±W; W = None is open water, ±80 m,
y from -80 m to D + 60 m). The reward has U's form and contract-2 constants,
with the potential measured to (0, D); the time limit is 30 s + 3 s/m × D in
whole 20 Hz decisions. Physics, gate passage, capsize, bounds and truncation
stay in Rust; every decision is the same raw step and clip as
``TaskEnv.step``.

The actor sees leg-relative features only: ``FEATURES8`` (no absolute
position). The research state is carried with two extra columns, D and W,
so features and the potential can be computed row by row.

  m8_leg.py manifests                        write configs/M8_legs.json (frozen)
  m8_leg.py train NAME --seed S [...]        PPO/SAC from scratch
  m8_leg.py heldout OUT.json RUN [--splits validation test shift]
  m8_leg.py rule OUT.json                    RuleSailor on every split
  m8_leg.py report [PREFIX ...]              panel curves per run
"""

from __future__ import annotations

import argparse
import collections
import hashlib
import json
import math
import sys
import time
from dataclasses import asdict, dataclass, fields, replace
from pathlib import Path

import jax
import jax.numpy as jnp
import m6_tune
import numpy as np
import optax
import ppo
import sac
from m6_tune import (
    CODE,
    FIXED,
    gae,
    kl_update,
    nstep_rows,
    summarize,
    to_rates,
    upwind_angle,
)
from sailing import (
    HERE,
    REPO,
    RS,
    Condition,
    Task,
    TaskEnv,
    TransitionReward,
    features,
    scenario_json,
    wrap_deg,
)

import sailgym_core

U = Task.load("U")
NRS = len(RS)
COL_D, COL_W = NRS, NRS + 1  # leg length and corridor half-width (inf = open)
NA = NRS + 2
OPEN_HALF = 80.0
EDGE_CLIP = 50.0
GATE_HALF = 5.0
BINS = (
    ("upwind", 0.0, 15.0),
    ("close", 15.0, 60.0),
    ("reach", 60.0, 120.0),
    ("broad", 120.0, 165.0),
    ("run", 165.0, 180.0),
)
TRAIN_WIDTHS = (None, 30.0, 15.0, 10.0)
EVAL_WIDTHS = (None, 15.0, 10.0)
EVAL_D = (30.0, 80.0, 150.0)
SEEDS = {"validation": 8001, "test": 8002, "shift": 8003, "panel": 8090}
# Fresh held-out set (2026-11-25), generated after all tuning and scored once.
FRESH_SEEDS, FRESH_PER_CELL = {"fresh": 8009, "fresh2": 8010}, 10
TRAIN_SEED = 8004
PER_CELL = {"validation": 4, "test": 10, "shift": 4}
MANIFEST = HERE / "configs" / "M8_legs.json"
OUT = REPO / "runs/ese3990/M8"
OBS = 19
FEATURES8 = ["dx_target", "dy_target", "edge_left", "edge_right"] + [
    "cos_psi", "sin_psi", "phi", "u", "v", "r", "p", "cos_beta", "sin_beta",
    "beta_dot", "delta_r", "l_sheet", "capsize_pending", "wind_x", "wind_y",
]  # fmt: skip
# M6 fixed scales; dy, the corridor edges and the (now rotated) wind rescaled.
MEAN8 = np.array([0, 40, 25, 25, *FIXED["mean"][2:15], 0, 0], float)
STD8 = np.array([20, 40, 25, 25, *FIXED["std"][2:15], 3.5, 3.5], float)
CLIP = 10.0


# ---------------------------------------------------------------------------
# Legs, tasks and the generator
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Leg:
    id: str
    wind_speed: float  # m/s
    wind_from_deg: float  # compass FROM, relative to the leg (the leg points north)
    D: float  # gate distance, m
    W: float | None  # corridor half-width, m; None = open water
    heading_deg: float  # initial compass heading
    speed: float  # initial surge speed, m/s

    @property
    def beta(self) -> float:
        """Bearing to the wind, |β| in [0, 180]: 0 = dead upwind."""
        return abs(wrap_deg(self.wind_from_deg))

    @property
    def bin(self) -> str:
        return next(n for n, _, hi in BINS if self.beta < hi or n == "run")

    @property
    def cell(self) -> str:
        return f"{self.bin}|{'open' if self.W is None else int(self.W)}"


@dataclass
class LegTask(Task):
    leg: Leg | None = None

    def spec(self, cond=None) -> sailgym_core.Spec:
        L = self.leg
        doc = json.loads(
            scenario_json(Condition(L.id, L.wind_speed, L.wind_from_deg, L.heading_deg))
        )
        doc["initial_state"]["speed"] = L.speed
        return sailgym_core.Spec(
            json.dumps(doc, sort_keys=True),
            max_steps=self.max_steps,
            autoreset="Disabled",
            cadence=self.cadence,
            bounds=self.bounds,
            route_json=self.route_json,
            log_decisions=False,
        )


def budget_decisions(D: float) -> int:
    return round((30.0 + 3.0 * D) / U.decision_dt)


def leg_task(leg: Leg, bounds=None, decisions=None) -> LegTask:
    n = decisions or budget_decisions(leg.D)
    h = OPEN_HALF if leg.W is None else leg.W
    t = replace(
        U,
        name="M8leg",
        target=(0.0, leg.D),
        budget_s=n * U.decision_dt,
        max_steps=n * U.cadence,
        bounds=bounds or (-h, -80.0, h, leg.D + 60.0),
        route_json=sailgym_core.route_waypoints((0.0, 0.0), [(0.0, leg.D)], GATE_HALF),
    )
    return LegTask(**{f.name: getattr(t, f.name) for f in fields(Task)}, leg=leg)


def _start(rng, shift=False):
    """25% at rest heading across the leg (as U), else under way."""
    if rng.random() < 0.25:
        off = (
            rng.uniform(15, 30) * rng.choice([-1, 1]) if shift else rng.uniform(-15, 15)
        )
        return (rng.choice([90.0, 270.0]) + off) % 360, 0.0
    return rng.uniform(0, 360), rng.uniform(1.5, 2.5) if shift else rng.uniform(0, 1.5)


def _leg(i, ws, b, D, W, start):
    hd, sp = start
    return Leg(i, round(ws, 2), round(b, 1), round(D, 1), W, round(hd, 1), round(sp, 2))


def training_leg(
    rng,
    i,
    widths=TRAIN_WIDTHS,
    beta=None,
    p_up=0.0,
    d_max=150.0,
    p_narrow=0.0,
    p_hard=0.0,
    hard_widths=(15.0, 10.0),
    hard_dmin=None,
    hard_beta=(0.0, 15.0),
) -> Leg:
    ws = rng.uniform(4.5, 5.5)
    if p_up and rng.random() < p_up:  # tuning: oversample beats (no draw when off)
        beta = (0.0, 60.0)
    b = (
        rng.uniform(-180, 180)
        if beta is None
        else rng.uniform(*beta) * rng.choice([-1, 1])
    )
    D = rng.uniform(20, d_max)
    W = widths[rng.integers(len(widths))]
    if (
        p_narrow and rng.random() < p_narrow
    ):  # tuning: oversample 10 m; no draw when off
        W = 10.0
    if p_hard and rng.random() < p_hard:  # tuning: dead upwind in a 15 or 10 m corridor
        b = rng.uniform(*hard_beta) * rng.choice([-1, 1])
        W = hard_widths[rng.integers(len(hard_widths))]
        if hard_dmin is not None:  # long beats; no draw when unset
            D = rng.uniform(hard_dmin, 150.0)
    return _leg(i, ws, b, D, W, _start(rng))


class LegGen:
    """Training legs for one seed, in episode order; ``widths`` may change."""

    def __init__(self, seed, widths=TRAIN_WIDTHS, beta=None, p_up=0.0):
        self.rng = np.random.default_rng([TRAIN_SEED, seed])
        self.seed, self.widths, self.beta, self.k = seed, list(widths), beta, 0
        self.p_up, self.d_max, self.p_narrow, self.p_hard = p_up, 150.0, 0.0, 0.0
        self.hard_widths, self.hard_dmin = (15.0, 10.0), None
        self.hard_beta = (0.0, 15.0)

    def __call__(self) -> Leg:
        self.k += 1
        return training_leg(
            self.rng,
            f"train-s{self.seed}-{self.k:07d}",
            self.widths,
            self.beta,
            self.p_up,
            self.d_max,
            self.p_narrow,
            self.p_hard,
            self.hard_widths,
            self.hard_dmin,
            self.hard_beta,
        )


def split_legs(name: str) -> list[Leg]:
    """Stratified: every bearing bin × corridor cell; D and start cycle."""
    rng = np.random.default_rng(SEEDS[name])
    out = []
    widths = TRAIN_WIDTHS if name == "panel" else EVAL_WIDTHS
    per = 2 if name == "panel" else PER_CELL[name]
    for bi, (_, lo, hi) in enumerate(BINS):
        for wi, W in enumerate(widths):
            for j in range(per):
                i = f"{name}-{len(out):03d}"
                if name == "panel":
                    ws, D = rng.uniform(4.5, 5.5), rng.uniform(20, 150)
                    start = _start(rng)
                else:
                    shift = name == "shift"
                    ws = (4.0, 6.0)[j % 2] if shift else rng.uniform(4.5, 5.5)
                    D = EVAL_D[(j + bi + wi) % 3]
                    start = _start(rng, shift)
                b = rng.uniform(lo, hi) * rng.choice([-1, 1])
                out.append(_leg(i, ws, b, D, W, start))
    return out


def random_route(rng, n=5):
    p, th, pts = np.zeros(2), rng.uniform(-np.pi, np.pi), []
    for _ in range(n):
        p = p + rng.uniform(30, 100) * np.array([np.cos(th), np.sin(th)])
        pts.append(p.round(1).tolist())
        th += np.radians(rng.uniform(-150, 150))
    return pts


def routes() -> dict:
    def course(name):
        c = json.loads((REPO / "courses" / f"{name}.json").read_text())
        return {"waypoints": [[w["x"], w["y"]] for w in c["waypoints"]], "W": None}

    rng = np.random.default_rng(8005)
    return {
        "windward_leeward": course("windward_leeward"),
        "triangle": course("triangle"),
        "channel80": {"waypoints": [[0.0, 80.0]], "W": 10.0},
        "channel160": {"waypoints": [[0.0, 160.0]], "W": 10.0},
        **{
            f"random5_{k}": {"waypoints": random_route(rng), "W": None}
            for k in range(3)
        },
    }


def write_manifests():
    if MANIFEST.exists():
        raise SystemExit(f"{MANIFEST} is frozen; delete it deliberately to regenerate")
    doc = {
        "generator_version": 1,
        "created": "2026-11-20",
        "label": "M8 leg frame (post-hoc, exploratory)",
        "bins_deg": BINS,
        "train_widths": TRAIN_WIDTHS,
        "eval_widths": EVAL_WIDTHS,
        "eval_D": EVAL_D,
        "seeds": SEEDS,
        "training_generator_seed": TRAIN_SEED,
        "route_conditions": "the first 20 U test conditions"
        " (configs/conditions/test.json)",
        "splits": {s: [asdict(x) for x in split_legs(s)] for s in (*PER_CELL, "panel")},
        "routes": routes(),
    }
    MANIFEST.write_text(json.dumps(doc, indent=1))
    print(MANIFEST, {s: len(v) for s, v in doc["splits"].items()})


def load_split(name: str) -> list[Leg]:
    path = (
        HERE / "configs" / f"M8_legs_{name}.json" if name in FRESH_SEEDS else MANIFEST
    )
    return [Leg(**d) for d in json.loads(path.read_text())["splits"][name]]


def write_fresh(name="fresh"):
    """A fresh held-out split: test's stratification, a new seed; frozen once."""
    path = HERE / "configs" / f"M8_legs_{name}.json"
    if path.exists():
        raise SystemExit(f"{path} is frozen")
    SEEDS[name], PER_CELL[name] = FRESH_SEEDS[name], FRESH_PER_CELL
    legs = split_legs(name)
    del SEEDS[name], PER_CELL[name]
    doc = {
        "label": "M8 fresh held-out legs: generated 2026-11-25 after all tuning,"
        " scored once; test's stratification with a new seed",
        "seed": FRESH_SEEDS[name],
        "per_cell": FRESH_PER_CELL,
        "splits": {name: [asdict(g) for g in legs]},
    }
    path.write_text(json.dumps(doc, indent=1))
    print(path, len(legs))


# ---------------------------------------------------------------------------
# Features, reward and vectorized legs
# ---------------------------------------------------------------------------


def leg_features(rsa: np.ndarray) -> np.ndarray:
    """FEATURES8 from augmented research state rows ``(..., NA)``."""
    rsa = np.asarray(rsa, dtype=np.float64)
    x, D, W = rsa[..., RS["x"]], rsa[..., COL_D], rsa[..., COL_W]
    f = features(rsa[..., :NRS], (0.0, 0.0))
    f[..., 1] += D
    left = np.minimum(W + x, EDGE_CLIP)[..., None]  # inf -> 50
    right = np.minimum(W - x, EDGE_CLIP)[..., None]
    return np.concatenate([f[..., :2], left, right, f[..., 2:]], -1)


def leg_obs(rsa, twa=False, geo=False, dy_clip=None):
    """Scaled FEATURES8. Tuning options: ``twa`` appends cos/sin of the signed
    angle between the heading and the wind's FROM direction; ``geo`` also
    appends cos/sin of the angle from the heading to the gate centre;
    ``dy_clip`` caps the along-leg distance the actor and critic see."""
    rsa = np.asarray(rsa, dtype=np.float64)
    f = leg_features(rsa)
    if dy_clip is not None:
        f[..., 1] = np.minimum(f[..., 1], dy_clip)
    z = np.clip((f - MEAN8) / STD8, -CLIP, CLIP)
    angs = []
    if twa or geo:
        angs.append(upwind_angle(rsa))
    if geo:
        g = np.arctan2(rsa[..., COL_D] - rsa[..., RS["y"]], -rsa[..., RS["x"]])
        angs.append(g - rsa[..., RS["psi"]])
    for a in angs:
        z = np.concatenate([z, np.cos(a)[..., None], np.sin(a)[..., None]], -1)
    return z.astype(np.float32)


class LegReward(TransitionReward):
    """U's transition reward with Φ = -d/30 measured to (0, D) row by row."""

    def potential(self, rs):
        rs = np.asarray(rs, dtype=np.float64)
        d = np.hypot(rs[..., RS["x"]], rs[..., RS["y"]] - rs[..., COL_D])
        return -d / self.dist_scale


class LegVec(m6_tune.Vec):
    """n leg episodes with explicit resets (``m6_tune.Vec.block`` reused).

    ``legs`` fixes one leg per env (evaluation, no resets); otherwise each
    reset draws ``next_leg()``. ``record`` keeps the actions sent and a hash
    of every research state, for exact replay."""

    def __init__(self, n, next_leg=None, legs=None, record=False, task_fn=leg_task):
        self.n, self.next_leg, self.legs, self.task_fn = n, next_leg, legs, task_fn
        self.count, self.beta = 0, 0.0
        self.envs, self.leg = [None] * n, [None] * n
        self.rs = np.zeros((n, NA))
        self.orig = np.ones(n, bool)
        z = lambda dt=float: np.zeros(n, dt)  # noqa: E731
        self.maxy, self.ang, self.maxax, self.sumax = z(), z(), z(), z()
        self.tacks, self.gybes, self.xrev, self.k = z(int), z(int), z(int), z(int)
        self.xdir, self.xext = z(), z()
        self.reward = LegReward(U)
        self.a3 = np.array([0.0, 0.0, -1.0])
        self.o32 = self.f32 = None
        self.buf = np.zeros(NRS)
        self.record_on = record
        self.rec = [[] for _ in range(n)]
        self.sha = [None] * n
        for i in range(n):
            self.reset(i)

    def reset(self, i):
        leg = self.legs[i] if self.legs is not None else self.next_leg()
        self.leg[i] = leg
        self.envs[i] = TaskEnv(self.task_fn(leg), None, self.count)
        self.count += 1
        self.rs[i, :NRS] = self.envs[i].reset()
        self.rs[i, COL_D] = leg.D
        self.rs[i, COL_W] = np.inf if leg.W is None else leg.W
        if self.o32 is None:
            self.o32 = np.empty(self.envs[i].spec.obs_len, np.float32)
            self.f32 = np.empty_like(self.o32)
        x = self.rs[i, RS["x"]]
        self.maxy[i], self.tacks[i], self.gybes[i], self.xrev[i] = 0.0, 0, 0, 0
        self.maxax[i], self.sumax[i], self.k[i] = 0.0, 0.0, 0
        self.xdir[i], self.xext[i] = 0.0, x
        self.ang[i] = upwind_angle(self.rs[i])
        if self.record_on:
            self.rec[i] = []
            self.sha[i] = hashlib.sha256(self.rs[i, :NRS].tobytes())

    def step(self, rates, alive):
        idx = np.flatnonzero(alive)
        prev = self.rs[idx].copy()
        steps = np.zeros(len(idx))
        code = np.zeros(len(idx), int)
        a3 = self.a3
        for j, i in enumerate(idx):
            env = self.envs[i]
            a3[0] = min(max(rates[i, 0], -1.0), 1.0)
            a3[1] = min(max(rates[i, 1], -1.0), 1.0)
            _, term, trunc, _, st, _ = env.raw.step(a3, self.o32, self.f32)
            env.raw.research_state(self.buf)
            self._put(i, self.buf)
            if self.record_on:
                self.rec[i].append(a3[:2].copy())
                self.sha[i].update(self.buf.tobytes())
            steps[j] = st
            if term or trunc:
                code[j] = CODE[env.outcome]
        nxt = self.rs[idx]
        rew = self.reward.batch(prev, nxt, code, steps)
        x = nxt[:, RS["x"]]
        self.maxy[idx] = np.maximum(self.maxy[idx], nxt[:, RS["y"]])
        self.maxax[idx] = np.maximum(self.maxax[idx], np.abs(x))
        self.sumax[idx] += np.abs(x)
        self.k[idx] += 1
        ang, old = upwind_angle(nxt), self.ang[idx]
        flip = ang * old < 0
        self.tacks[idx] += (np.abs(ang) < 1.4) & (np.abs(old) < 1.4) & flip
        lee = np.pi - 1.4
        self.gybes[idx] += (np.abs(ang) > lee) & (np.abs(old) > lee) & flip
        self.ang[idx] = ang
        # x-reversals (boards) with 2 m hysteresis
        d, e = self.xdir[idx], self.xext[idx]
        start = (d == 0) & (np.abs(x - e) >= 2.0)
        fwd = ((d == 1) & (x > e)) | ((d == -1) & (x < e))
        rev = ((d == 1) & (e - x >= 2.0)) | ((d == -1) & (x - e >= 2.0))
        self.xrev[idx] += rev
        self.xdir[idx] = np.where(start, np.sign(x - e), np.where(rev, -d, d))
        self.xext[idx] = np.where(start | fwd | rev, x, e)
        return idx, rew, code

    def _put(self, i, buf):
        self.rs[i, :NRS] = buf

    def record(self, i, code):
        env, leg, rs = self.envs[i], self.leg[i], self.rs[i]
        r = dict(
            orig=True,
            id=leg.id,
            cell=leg.cell,
            bin=leg.bin,
            W=leg.W,
            D=leg.D,
            at_rest=leg.speed == 0.0,
            outcome=env.outcome,
            reason=env.raw.termination_reason,
            maxy=float(self.maxy[i]),
            tacks=int(self.tacks[i]),
            gybes=int(self.gybes[i]),
            xrev=int(self.xrev[i]),
            max_abs_x=float(self.maxax[i]),
            mean_abs_x=float(self.sumax[i] / max(self.k[i], 1)),
            t=float(rs[RS["t"]]),
            dist=math.hypot(rs[RS["x"]], rs[RS["y"]] - leg.D),
        )
        if self.record_on:
            r["env_seed"] = self.envs[i]._seed
            r["state_sha256"] = self.sha[i].hexdigest()
        return r


@dataclass(frozen=True)
class RouteEp:
    """A chained route in world coordinates, sailed leg by leg."""

    id: str
    waypoints: tuple  # ((x, y), ...) after the start (0, 0)
    W: float | None  # corridor for every leg (channels); None = open box
    wind_speed: float
    wind_from_deg: float  # world compass FROM
    heading_deg: float
    speed: float = 0.0

    @property
    def points(self):
        return [(0.0, 0.0), *[tuple(map(float, w)) for w in self.waypoints]]

    @property
    def D(self):
        (ax, ay), (bx, by) = self.points[:2]
        return math.hypot(bx - ax, by - ay)

    bin = cell = property(lambda self: self.id.split("@")[0])


def route_task(r: RouteEp, bounds=None, decisions=None) -> LegTask:
    """Rust tracks every gate in order; one budget for the whole route."""
    pts = np.asarray(r.points)
    lens = np.hypot(*np.diff(pts, axis=0).T)
    n = decisions or sum(budget_decisions(d) for d in lens)
    if bounds is None and r.W is not None:  # a straight channel along +y
        bounds = (-r.W, -80.0, r.W, pts[:, 1].max() + 60.0)
    lo, hi = pts.min(0) - 80.0, pts.max(0) + 80.0
    leg = Leg(r.id, r.wind_speed, r.wind_from_deg, r.D, r.W, r.heading_deg, r.speed)
    t = replace(
        U,
        name="M8route",
        target=tuple(pts[1]),
        budget_s=n * U.decision_dt,
        max_steps=n * U.cadence,
        bounds=bounds or (lo[0], lo[1], hi[0], hi[1]),
        route_json=sailgym_core.route_waypoints(
            tuple(pts[0].tolist()), [tuple(q) for q in pts[1:].tolist()], GATE_HALF
        ),
    )
    return LegTask(**{f.name: getattr(t, f.name) for f in fields(Task)}, leg=leg)


def to_leg_frame(buf, a, b):
    """World research state -> the frame of the leg a -> b (leg along +y)."""
    th = math.atan2(b[1] - a[1], b[0] - a[0])
    c, s = math.cos(math.pi / 2 - th), math.sin(math.pi / 2 - th)
    out = buf.copy()
    dx, dy = buf[RS["x"]] - a[0], buf[RS["y"]] - a[1]
    out[RS["x"]], out[RS["y"]] = c * dx - s * dy, s * dx + c * dy
    out[RS["psi"]] = buf[RS["psi"]] + (math.pi / 2 - th)
    wx, wy = buf[RS["wind_x"]], buf[RS["wind_y"]]
    out[RS["wind_x"]], out[RS["wind_y"]] = c * wx - s * wy, s * wx + c * wy
    return out


class RouteVec(LegVec):
    """The leg-frame wrapper: ``legs`` are RouteEps; the policy sees the
    current leg, which advances when the boat crosses that leg's gate line
    while Rust still reports the episode running."""

    def __init__(self, n, legs, record=False):
        self.leg_k = np.zeros(n, int)
        super().__init__(n, legs=legs, record=record, task_fn=route_task)

    def reset(self, i):
        self.leg_k[i] = 0
        super().reset(i)
        self._put(i, self.envs[i].research_state())
        self.xext[i] = self.rs[i, RS["x"]]
        self.ang[i] = upwind_angle(self.rs[i])

    def _put(self, i, buf):
        pts = self.leg[i].points
        k = self.leg_k[i]
        a, b = pts[k], pts[k + 1]
        row = to_leg_frame(buf, a, b)
        D = math.hypot(b[0] - a[0], b[1] - a[1])
        if row[RS["y"]] >= D and k + 2 < len(pts) and self.envs[i].outcome == "running":
            self.leg_k[i] = k = k + 1
            a, b = pts[k], pts[k + 1]
            row, D = to_leg_frame(buf, a, b), math.hypot(b[0] - a[0], b[1] - a[1])
        self.rs[i, :NRS], self.rs[i, COL_D] = row, D
        W = self.leg[i].W
        self.rs[i, COL_W] = np.inf if W is None else W

    def record(self, i, code):
        r = super().record(i, code)
        r["legs_passed"] = int(self.leg_k[i]) + (r["outcome"] == "finished")
        r["legs"] = len(self.leg[i].waypoints)
        return r


def run_legs(legs, act, repeat=10, head="target", record=False, task_fn=leg_task):
    """Deterministic ``act`` on fixed legs, one env per leg; records in order."""
    if task_fn is route_task:
        vec = RouteVec(len(legs), legs=legs, record=record)
    else:
        vec = LegVec(len(legs), legs=legs, record=record, task_fn=task_fn)
    alive = np.ones(len(legs), bool)
    out = [None] * len(legs)
    a = np.zeros((len(legs), 2))
    step = 0
    while alive.any():
        if step % repeat == 0:
            a = np.asarray(act(vec.rs))
        rates = to_rates(a, vec.rs, U.decision_dt) if head == "target" else a
        idx, _, code = vec.step(rates, alive)
        for j in np.flatnonzero(code):
            alive[idx[j]] = False
            out[idx[j]] = vec.record(idx[j], code[j])
        step += 1
    return out, vec


def leg_summary(recs, eval_widths=None):
    """Totals, per bearing bin and per bearing × corridor cell; ``general``
    is the frozen M8 rule (>= 90% overall and >= 70% in every cell)."""
    s = summarize(recs)
    cells = collections.defaultdict(lambda: [0, 0])
    bins = collections.defaultdict(lambda: [0, 0])
    for r in recs:
        for tab, k in ((cells, r["cell"]), (bins, r["bin"])):
            tab[k][0] += r["outcome"] == "finished"
            tab[k][1] += 1
    s["cells"], s["bins"] = dict(cells), dict(bins)
    s["general"] = bool(
        s["finished"] >= 0.9 * s["n"] and all(f >= 0.7 * n for f, n in cells.values())
    )
    return s


# ---------------------------------------------------------------------------
# Networks and checkpoints
# ---------------------------------------------------------------------------


def ppo_init(key, hidden, depth=2, log_std=-0.5, OBS=OBS):
    if depth == 2:
        return ppo.init(key, OBS, 2, hidden, log_std)
    ka, kc = jax.random.split(key)
    actor = ppo.mlp_init(ka, [OBS, *[hidden] * depth, 2])
    actor[-1]["w"] = actor[-1]["w"] * 0.01
    return {
        "actor": actor,
        "log_std": jnp.full(2, log_std),
        "critic": ppo.mlp_init(kc, [OBS, *[hidden] * depth, 1]),
    }


def load_policy(run: Path, decisions: int):
    a = json.loads((run / "args.json").read_text())
    twa, geo = a.get("twa", False), a.get("geo", False)
    od = OBS + 2 * (twa or geo) + 2 * geo
    if a["learner"] == "ppo":
        tmpl = ppo_init(jax.random.PRNGKey(0), a["hidden"], a["depth"], OBS=od)
        mean = ppo.mean_action
    else:
        tmpl, mean = (
            sac.init(jax.random.PRNGKey(0), od, 2, a["hidden"]),
            sac.mean_action,
        )
    z = np.load(run / f"ckpt_{decisions:09d}.npz")
    leaves = [jnp.asarray(z[f"arr_{i}"]) for i in range(len(z.files))]
    params = jax.tree.unflatten(jax.tree.structure(tmpl), leaves)
    f = jax.jit(mean)
    dyc = a.get("dy_clip")
    return (lambda rs: f(params, leg_obs(rs, twa, geo, dyc))), a


def select(run: Path):
    """The checkpoint with most full-panel finishes; ties go to the later one."""
    rows = [json.loads(x) for x in (run / "log.jsonl").open()]
    best = max(rows, key=lambda r: (r["eval"].get("finished", 0), r["decisions"]))
    return best["decisions"], best["eval"].get("finished", 0), rows[-1]["decisions"]


# ---------------------------------------------------------------------------
# Training (m6_tune.main with the leg frame and a corridor curriculum)
# ---------------------------------------------------------------------------


def train(argv=None):
    p = argparse.ArgumentParser()
    p.add_argument("name")
    p.add_argument("--seed", type=int, default=0)
    p.add_argument("--learner", choices=["ppo", "sac"], default="ppo")
    p.add_argument("--head", choices=["rate", "target"], default="target")
    p.add_argument("--repeat", type=int, default=10)
    p.add_argument("--decisions", type=float, default=10e6)
    p.add_argument("--n-envs", type=int, default=16)
    p.add_argument("--rollout", type=int, default=256)
    p.add_argument("--epochs", type=int, default=4)
    p.add_argument("--minibatch", type=int, default=512)
    p.add_argument("--lam", type=float, default=0.95)
    p.add_argument("--ent", type=float, default=0.001)
    p.add_argument("--log-std", type=float, default=-0.5)
    p.add_argument("--hidden", type=int, default=256)
    p.add_argument("--depth", type=int, default=2)
    p.add_argument("--lr", type=float, default=3e-4)
    p.add_argument("--vf", type=float, default=0.5)
    p.add_argument("--anneal", type=float, default=1.0)
    p.add_argument("--target-kl", type=float, default=None)
    p.add_argument("--eval-every", type=float, default=1e6)
    p.add_argument("--replay", type=int, default=400_000)
    p.add_argument("--utd", type=float, default=1.0)
    p.add_argument("--warmup", type=int, default=5000)
    p.add_argument("--target-entropy", type=float, default=-2.0)
    p.add_argument("--alpha0", type=float, default=0.2)
    p.add_argument("--nstep", type=int, default=1)
    p.add_argument(
        "--curriculum",
        type=int,
        metavar="NEED",
        help="open water first; add the next narrower corridor of TRAIN_WIDTHS"
        " once the panel at the newest width finishes >= NEED/40",
    )
    p.add_argument(
        "--backoff",
        type=int,
        help="drop the newest width after this many consecutive newest-width"
        " panels below NEED/2",
    )
    p.add_argument("--beta", type=float, nargs=2, help="specialist: |β| range, deg")
    p.add_argument(
        "--p-narrow", type=float, default=0.0,
        help="share of training legs redrawn into the 10 m corridor",
    )  # fmt: skip
    p.add_argument(
        "--p-hard", type=float, default=0.0,
        help="share of training legs redrawn as dead upwind in a 15 or 10 m corridor",
    )  # fmt: skip
    p.add_argument(
        "--hard-width", type=float, nargs="+", default=[15.0, 10.0],
        help="corridor half-widths for --p-hard legs",
    )  # fmt: skip
    p.add_argument(
        "--hard-dmin", type=float, default=None,
        help="--p-hard legs draw D uniform in [HARD_DMIN, 150] m",
    )  # fmt: skip
    p.add_argument(
        "--hard-beta", type=float, nargs=2, default=[0.0, 15.0],
        help="|beta| range in degrees for --p-hard legs (default: dead upwind)",
    )  # fmt: skip
    p.add_argument(
        "--dy-clip", type=float, default=None,
        help="cap the along-leg distance feature at DY_CLIP m (actor and critic)",
    )  # fmt: skip
    p.add_argument(
        "--twa", action="store_true", help="append cos/sin of the true wind angle"
    )
    p.add_argument(
        "--geo",
        action="store_true",
        help="--twa plus cos/sin of the bearing to the gate",
    )
    p.add_argument(
        "--init", metavar="RUN/ckpt_N.npz",
        help="continue from saved PPO params (fresh optimizer; leg stream and noise"
        " reseeded so they do not replay the original run)",
    )  # fmt: skip
    p.add_argument(
        "--d-curriculum", type=float, nargs=2, metavar=("D0", "STEP"),
        help="training D in [20, Dmax], Dmax from D0, +STEP (to 150) whenever the"
        " training episodes since the last checkpoint finish >= 50%%",
    )  # fmt: skip
    p.add_argument(
        "--p-upwind", type=float, default=0.0,
        help="share of training legs redrawn with |β| in [0, 60) (beats oversampled)",
    )  # fmt: skip
    args = p.parse_args(argv)
    assert args.depth == 2 or args.learner == "ppo"
    od = OBS + 2 * (args.twa or args.geo) + 2 * args.geo
    obs = lambda rs: leg_obs(rs, args.twa, args.geo, args.dy_clip)  # noqa: E731

    out = OUT / "train" / f"{args.name}_s{args.seed}"
    out.mkdir(parents=True, exist_ok=False)
    (out / "args.json").write_text(json.dumps(vars(args), indent=1))
    gamma, dec_dt, k = U.reward["gamma"], U.decision_dt, args.repeat
    stage = 0 if args.curriculum else len(TRAIN_WIDTHS) - 1
    gen = LegGen(
        args.seed + 1000 * bool(args.init),
        TRAIN_WIDTHS[: stage + 1],
        args.beta,
        args.p_upwind,
    )
    panel = load_split("panel")
    if args.d_curriculum:
        gen.d_max = args.d_curriculum[0]
    gen.p_narrow = args.p_narrow
    gen.p_hard = args.p_hard
    gen.hard_widths = tuple(args.hard_width)
    gen.hard_dmin = args.hard_dmin
    gen.hard_beta = tuple(args.hard_beta)
    if args.beta:  # a specialist is selected on its own bearing range
        lo, hi = args.beta
        panel = [g for g in panel if lo <= g.beta < hi or (hi >= 180 and g.beta >= lo)]
    key = jax.random.PRNGKey(args.seed)
    rng = np.random.default_rng([args.seed, 2 if args.init else 1])
    vec = LegVec(args.n_envs, gen)
    t0 = time.monotonic()
    episodes, decisions, next_eval, zeros = [], 0, 0.0, 0

    if args.learner == "ppo":
        params = ppo_init(key, args.hidden, args.depth, args.log_std, od)
        if args.init:
            z = np.load(args.init)
            params = jax.tree.unflatten(
                jax.tree.structure(params),
                [jnp.asarray(z[f"arr_{i}"]) for i in range(len(z.files))],
            )
        cfg = dict(
            clip=0.2,
            vf_coef=args.vf,
            ent_coef=args.ent,
            lr=args.lr,
            max_grad_norm=1.0,
            minibatch=args.minibatch,
            epochs=args.epochs,
        )
        if args.anneal < 1:
            per = args.epochs * (args.rollout * args.n_envs // args.minibatch)
            total = int(args.decisions / (args.rollout * args.n_envs * k) + 1) * per
            cfg["lr"] = optax.linear_schedule(args.lr, args.lr * args.anneal, total)
        opt, update = (
            ppo.make_update(cfg)
            if args.target_kl is None
            else kl_update(cfg, args.target_kl)
        )
        opt_state = opt.init(params)
        actor = jax.jit(lambda p_, o: ppo.mlp(p_["actor"], o))
        value, logp_fn = jax.jit(ppo.value), jax.jit(ppo.log_prob)
        mean_act = jax.jit(ppo.mean_action)
    else:
        params = sac.init(key, od, 2, args.hidden, args.alpha0)
        if args.init:  # actor, critics and alpha; empty replay; targets = critics
            z = np.load(args.init)
            params = jax.tree.unflatten(
                jax.tree.structure(params),
                [jnp.asarray(z[f"arr_{i}"]) for i in range(len(z.files))],
            )
        scfg = dict(
            lr=args.lr, max_grad_norm=1.0, tau=0.005, target_entropy=args.target_entropy
        )
        init_opt, sac_update = sac.make_update(scfg, 1.0)
        scan_update = jax.jit(lambda st, b: jax.lax.scan(sac_update, st, b))
        state = dict(
            params=params,
            target_q=params["q"],
            opt=init_opt(params),
            key=jax.random.fold_in(key, 1),
        )
        replay = sac.Replay(args.replay, od)
        dist = jax.jit(sac.actor_dist)
        mean_act = jax.jit(sac.mean_action)
        policy_steps, update_debt = 0, 0.0
        nq = [collections.deque() for _ in range(args.n_envs)]

    def act_det(rs):
        return mean_act(params, obs(rs))

    updates, last_stats = 0, {}
    while True:
        if decisions >= next_eval:
            ts = time.monotonic()
            recs, _ = run_legs(panel, act_det, k, args.head)
            ev = leg_summary(recs)
            row = dict(decisions=int(decisions), updates=updates, eval=ev)
            if args.curriculum:
                newest = TRAIN_WIDTHS[stage]
                cur, _ = run_legs(
                    [replace(g, W=newest) for g in panel], act_det, k, args.head
                )
                kf = sum(r["outcome"] == "finished" for r in cur)
                row["curriculum"] = dict(stage=stage, W=newest, finished=kf)
                need = args.curriculum * len(panel) / 40  # specialists: smaller panel
                zeros = zeros + 1 if stage > 0 and kf < need / 2 else 0
                if kf >= need and stage < len(TRAIN_WIDTHS) - 1:
                    stage += 1
                elif args.backoff and zeros >= args.backoff:
                    stage, zeros = stage - 1, 0
                gen.widths = list(TRAIN_WIDTHS[: stage + 1])
            tr = summarize(episodes)
            if args.d_curriculum:
                if tr and tr["finished"] >= 0.5 * tr["n"]:
                    gen.d_max = min(150.0, gen.d_max + args.d_curriculum[1])
                row["d_max"] = gen.d_max
            row.update(
                wall=round(time.monotonic() - t0, 1),
                eval_s=round(time.monotonic() - ts, 1),
                train=summarize(episodes),
            )
            if args.learner == "ppo":
                row["log_std"] = np.asarray(params["log_std"]).round(3).tolist()
            else:
                row.update({k_: float(v_) for k_, v_ in last_stats.items()})
            with open(out / "log.jsonl", "a") as log:
                log.write(json.dumps(row) + "\n")
            print(json.dumps({x: row[x] for x in row if x != "eval"}),
                  ev.get("finished"), ev["bins"], flush=True)  # fmt: skip
            np.savez(
                out / f"ckpt_{int(decisions):09d}.npz",
                *[np.asarray(x) for x in jax.tree.leaves(params)],
            )
            episodes = []
            next_eval += args.eval_every
        if decisions >= args.decisions:
            break
        if args.learner == "ppo":
            std_ = np.exp(np.asarray(params["log_std"]))
            T, N = args.rollout, args.n_envs
            buf = {x: np.zeros((T, N)) for x in ("ret", "disc", "gb", "cont")}
            OB = np.zeros((T, N, od), np.float32)
            ON = np.zeros((T, N, od), np.float32)
            Uu = np.zeros((T, N, 2), np.float32)
            for t in range(T):
                o = obs(vec.rs)
                u = (
                    np.asarray(actor(params, o)) + std_ * rng.standard_normal((N, 2))
                ).astype(np.float32)
                ret, disc, boot, cont, nrs, ended, ex = vec.block(
                    np.tanh(u), k, args.head, gamma, dec_dt
                )
                decisions += ex
                episodes += ended
                OB[t], ON[t], Uu[t] = o, obs(nrs), u
                buf["ret"][t], buf["disc"][t] = ret, disc
                buf["gb"][t], buf["cont"][t] = disc * boot, cont
            flat = OB.reshape(-1, od)
            v = np.asarray(value(params, flat)).reshape(T, N)
            vn = np.asarray(value(params, ON.reshape(-1, od))).reshape(T, N)
            adv = gae(buf["ret"], v, vn, buf["gb"], buf["disc"], buf["cont"], args.lam)
            batch = dict(
                obs=flat,
                u=Uu.reshape(-1, 2),
                logp=np.asarray(logp_fn(params, flat, Uu.reshape(-1, 2))),
                adv=adv.reshape(-1).astype(np.float32),
                ret=(adv + v).reshape(-1).astype(np.float32),
            )
            params, opt_state, _ = update(params, opt_state, batch, rng)
            updates += 1
        else:
            N = args.n_envs
            o = obs(vec.rs)
            if policy_steps * N < args.warmup and not args.init:
                a = rng.uniform(-1, 1, (N, 2))
            else:
                mu, ls = map(np.asarray, dist(params["actor"], o))
                a = np.tanh(mu + np.exp(ls) * rng.standard_normal((N, 2)))
            ret, disc, boot, cont, nrs, ended, ex = vec.block(
                a, k, args.head, gamma, dec_dt
            )
            decisions += ex
            episodes += ended
            policy_steps += 1
            nobs = obs(nrs)
            rows = []
            for i in range(N):
                nq[i].append((o[i], a[i], ret[i], disc[i], boot[i], nobs[i]))
                rows += nstep_rows(nq[i], args.nstep, cont[i] == 0)
            if rows:
                o_, a_, r_, b_, n_ = map(np.asarray, zip(*rows, strict=True))
                replay.add(
                    obs=o_,
                    act=a_.astype(np.float32),
                    reward=r_,
                    next_obs=n_,
                    bootstrap=b_,
                    cont=np.ones(len(r_)),
                )
            if policy_steps * N >= args.warmup:
                update_debt += N * args.utd
                u_n = int(update_debt)
                if u_n:
                    idx = rng.integers(0, replay.size, (u_n, 256))
                    batches = {k_: v_[idx] for k_, v_ in replay.data.items()}
                    state, st = scan_update(state, batches)
                    last_stats = {k_: v_[-1] for k_, v_ in st.items()}
                    update_debt -= u_n
                    updates += u_n
                params = state["params"]


# ---------------------------------------------------------------------------
# Held-out evaluation, exact replay and references
# ---------------------------------------------------------------------------


def replay_leg(leg: Leg, actions: np.ndarray, env_seed: int, task_fn=leg_task):
    """Re-step the recorded actions in a fresh env; (state sha256, outcome, t)."""
    env = TaskEnv(task_fn(leg), None, env_seed)
    rs = env.reset()
    h = hashlib.sha256(rs.tobytes())
    a3, o32 = np.array([0.0, 0.0, -1.0]), np.empty(env.spec.obs_len, np.float32)
    f32, buf = np.empty_like(o32), np.zeros(NRS)
    for a in actions:
        a3[:2] = a
        env.raw.step(a3, o32, f32)
        env.raw.research_state(buf)
        h.update(buf.tobytes())
    return h.hexdigest(), env.outcome, float(buf[RS["t"]])


def heldout(argv):
    p = argparse.ArgumentParser()
    p.add_argument("out")
    p.add_argument("run")
    p.add_argument("--splits", nargs="+", default=["validation", "test", "shift"])
    p.add_argument("--ckpt", type=int, help="default: the panel-selected checkpoint")
    a = p.parse_args(argv)
    run = Path(a.run)
    sel, panel_fin, final = select(run)
    ck = a.ckpt if a.ckpt is not None else sel
    act, args = load_policy(run, ck)
    res = dict(
        run=run.name, ckpt=ck, selected=sel, panel=panel_fin, final=final, splits={}
    )
    store = Path(a.out).with_suffix(".npz")
    acts = {}
    for s in a.splits:
        legs = load_split(s)
        t0 = time.monotonic()
        recs, vec = run_legs(legs, act, args["repeat"], args["head"], record=True)
        wall = time.monotonic() - t0
        exact = 0
        for i, (leg, r) in enumerate(zip(legs, recs, strict=True)):
            acts[f"{s}/{leg.id}"] = np.asarray(vec.rec[i])
            h, oc, t = replay_leg(leg, acts[f"{s}/{leg.id}"], r["env_seed"])
            r["replay_exact"] = (
                h == r["state_sha256"] and oc == r["outcome"] and t == r["t"]
            )
            exact += r["replay_exact"]
        res["splits"][s] = dict(
            summary=leg_summary(recs), episodes=recs, replay_exact=exact, wall_s=wall
        )
        sm = res["splits"][s]["summary"]
        gen = "general" if sm["general"] else ""
        print(
            run.name,
            s,
            f"{sm['finished']}/{sm['n']}",
            gen,
            f"exact {exact}/{len(legs)}",
        )
    np.savez_compressed(store, **acts)
    Path(a.out).write_text(json.dumps(res, indent=1))


def rule_run(vec):
    """RuleSailor (the Rust controller) in every env of ``vec``; the release
    flag it emits is counted, then forced to -1 by the shared step."""
    from baselines import RuleSailorController

    legs = vec.legs
    cs = [RuleSailorController() for _ in legs]
    for c, env in zip(cs, vec.envs, strict=True):
        c.reset(env)
    alive, recs, rates = (
        np.ones(len(legs), bool),
        [None] * len(legs),
        np.zeros((len(legs), 2)),
    )
    while alive.any():
        for i in np.flatnonzero(alive):
            rates[i] = cs[i].act(vec.envs[i])[:2]
        idx, _, code = vec.step(rates, alive)
        for j in np.flatnonzero(code):
            alive[idx[j]] = False
            recs[idx[j]] = vec.record(idx[j], code[j])
            recs[idx[j]]["release_requests"] = cs[idx[j]].release_requests
    return recs


def route_eps(n=20):
    from sailing import load_manifest

    rts = json.loads(MANIFEST.read_text())["routes"]
    conds = load_manifest("test")[:n]
    return [
        RouteEp(
            f"{name}@{c.id}",
            tuple(map(tuple, r["waypoints"])),
            r["W"],
            c.wind_speed,
            c.wind_bearing_deg,
            c.heading_deg,
        )  # fmt: skip
        for name, r in rts.items()
        for c in conds
    ]


def route_summary(recs):
    out = {}
    for name in dict.fromkeys(r["bin"] for r in recs):
        rr = [r for r in recs if r["bin"] == name]
        fin = [r for r in rr if r["outcome"] == "finished"]
        out[name] = dict(
            n=len(rr),
            finished=len(fin),
            legs=rr[0]["legs"],
            mean_legs_passed=float(np.mean([r["legs_passed"] for r in rr])),
            reasons=dict(collections.Counter(r["reason"] or r["outcome"] for r in rr)),
            **({"t_finish": float(np.mean([r["t"] for r in fin]))} if fin else {}),
            tacks=float(np.mean([r["tacks"] for r in rr])),
            xrev=float(np.mean([r["xrev"] for r in rr])),
        )
    return out


def routes_eval(argv):
    """Chained routes (test only) through the leg-frame wrapper; RUN may be
    ``rule`` for RuleSailor."""
    p = argparse.ArgumentParser()
    p.add_argument("out")
    p.add_argument("run")
    p.add_argument("--n", type=int, default=20)
    a = p.parse_args(argv)
    eps = route_eps(a.n)
    if a.run == "rule":
        recs, res = rule_run(RouteVec(len(eps), legs=eps)), dict(run="rule_sailor")
    else:
        run = Path(a.run)
        sel, panel_fin, _ = select(run)
        act, args = load_policy(run, sel)
        recs, vec = run_legs(eps, act, args["repeat"], args["head"], True, route_task)
        for i, (ep, r) in enumerate(zip(eps, recs, strict=True)):
            h, oc, t = replay_leg(ep, np.asarray(vec.rec[i]), r["env_seed"], route_task)
            r["replay_exact"] = (
                h == r["state_sha256"] and oc == r["outcome"] and t == r["t"]
            )
        res = dict(run=run.name, ckpt=sel, panel=panel_fin,
                   replay_exact=sum(r["replay_exact"] for r in recs))  # fmt: skip
    res.update(summary=route_summary(recs), episodes=recs)
    print(
        res["run"],
        {k: (v["finished"], v["n"]) for k, v in res["summary"].items()},
        flush=True,
    )
    Path(a.out).write_text(json.dumps(res, indent=1))


def rule_reference(argv):
    """RuleSailor on every single-leg split."""
    out = Path(argv[0])
    res = {}
    for s in [*PER_CELL, "panel"]:
        legs = load_split(s)
        recs = rule_run(LegVec(len(legs), legs=legs))
        res[s] = dict(summary=leg_summary(recs), episodes=recs)
        print(
            "rule",
            s,
            res[s]["summary"]["finished"],
            res[s]["summary"]["cells"],
            flush=True,
        )
    out.write_text(json.dumps(res, indent=1))


REASONS = ("out_of_bounds", "capsized", "mark_missed", "truncated")


def results(argv):
    """Aggregate held-out JSONs: per run and per arm (the name before _s)."""
    out, files = Path(argv[0]), [Path(f) for f in argv[1:]]
    runs, arms = {}, collections.defaultdict(list)
    for f in files:
        d = json.loads(f.read_text())
        row = dict(
            ckpt=d["ckpt"], selected=d["selected"], panel=d["panel"], final=d["final"]
        )
        for s, v in d["splits"].items():
            sm = v["summary"]
            row[s] = dict(
                finished=sm["finished"],
                n=sm["n"],
                general=sm["general"],
                cells=sm["cells"],
                replay_exact=v["replay_exact"],
                t_finish=sm.get("t_finish"),
                reasons={k: sm[k] for k in REASONS if k in sm},
            )
        runs[d["run"]] = row
        arms[d["run"].rsplit("_s", 1)[0]].append(d["run"])
    summary = {}
    for arm, names in arms.items():
        rows = [runs[n] for n in names]
        a = dict(seeds=len(rows))
        if all("validation" in r for r in rows):
            a["general_val"] = sum(r["validation"]["general"] for r in rows)
        for s in ("validation", "test", "shift", *FRESH_SEEDS):
            if all(s in r for r in rows):
                a[s] = [
                    sum(r[s]["finished"] for r in rows),
                    sum(r[s]["n"] for r in rows),
                ]
                a[f"{s}_general"] = sum(r[s]["general"] for r in rows)
                a[f"{s}_exact"] = sum(r[s]["replay_exact"] for r in rows)
        summary[arm] = a
        print(arm, a)
    label = (
        "M8 general leg follower (post-hoc, exploratory); general = >= 90% of a"
        " split's single legs and >= 70% in every bearing x corridor cell"
    )
    out.write_text(json.dumps(dict(label=label, arms=summary, runs=runs), indent=1))


def final(argv):
    """docs/ese3990/M8_results.json from the saved pilot, held-out, route and
    reference files under runs/ese3990/M8."""
    M = OUT
    arms = {}
    for f in sorted((M / "heldout").glob("results_*.json")):
        d = json.loads(f.read_text())
        arms.update(
            {k: {**v, "file": str(f.relative_to(REPO))} for k, v in d["arms"].items()}
        )
    routes = collections.defaultdict(lambda: collections.defaultdict(lambda: [0, 0]))
    exact = collections.Counter()
    for f in sorted((M / "routes").glob("*.json")):
        d = json.loads(f.read_text())
        arm = (
            "rule_sailor" if d["run"] == "rule_sailor" else d["run"].rsplit("_s", 1)[0]
        )
        for name, v in d["summary"].items():
            routes[arm][name][0] += v["finished"]
            routes[arm][name][1] += v["n"]
        exact[arm] += d.get("replay_exact", 0)
    rule = json.loads((M / "rule_sailor.json").read_text())
    doc = dict(
        label="M8 general leg follower (post-hoc, exploratory). Held-out rows use"
        " panel-selected checkpoints; general = >= 90% of a split's single legs and"
        " >= 70% in every bearing x corridor cell.",
        freeze="experiments/ese3990/configs/M8_freeze.json",
        manifest="experiments/ese3990/configs/M8_legs.json",
        width_rule=json.loads((M / "width_rule.json").read_text()),
        heldout=arms,
        routes={a: dict(v) for a, v in routes.items()},
        routes_replay_exact=dict(exact),
        rule_sailor={
            s: {k: v["summary"][k] for k in ("finished", "n", "cells")}
            for s, v in rule.items()
        },
        fresh_checks={
            f.parent.name + "/" + f.stem: json.loads(f.read_text())["arms"]
            for f in sorted((M / "heldout").glob("fresh*/results_*.json"))
        },
    )
    out = REPO / "docs/ese3990/M8_results.json"
    out.write_text(json.dumps(doc, indent=1))
    print(out)


def report(prefixes):
    for run in sorted((OUT / "train").glob("*_s*")):
        if prefixes and not any(run.name.startswith(x) for x in prefixes):
            continue
        rows = (
            [json.loads(x) for x in (run / "log.jsonl").open()]
            if (run / "log.jsonl").exists()
            else []
        )
        if not rows:
            continue
        fin = " ".join(f"{r['eval']['finished']:2d}" for r in rows)
        cur = rows[-1].get("curriculum", {})
        b = rows[-1]["eval"]["bins"]
        print(f"{run.name:28} {rows[-1]['decisions'] / 1e6:5.1f}M  [{fin}]  "
              f"st{cur.get('stage', '-')}:{cur.get('finished', '-')}  "
              + " ".join(f"{k[:2]}{v[0]}/{v[1]}" for k, v in b.items()))  # fmt: skip


def main():
    cmd, rest = sys.argv[1], sys.argv[2:]
    if cmd in FRESH_SEEDS:
        write_fresh(cmd)
    elif cmd == "manifests":
        write_manifests()
    elif cmd == "train":
        train(rest)
    elif cmd == "heldout":
        heldout(rest)
    elif cmd == "rule":
        rule_reference(rest)
    elif cmd == "final":
        final(rest)
    elif cmd == "results":
        results(rest)
    elif cmd == "routes":
        routes_eval(rest)
    elif cmd == "report":
        report(rest)
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
