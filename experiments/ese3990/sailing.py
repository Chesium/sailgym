"""The shared ESE3990 task contract: tasks, conditions, actions, features,
reward and explicit-reset collection.

Every controller in the study (random, RuleSailor, PPO, SAC, CEM-MPC) goes
through this module, so they share one action restriction, one transition
reward and one notion of an episode boundary. Physics, gate passage, capsize,
bounds and time limits stay in Rust (``sailgym_core``); nothing here models the
boat.

Plan reference: ``docs/ese3990/ESE3990_SailGym_Project_Plan.md`` §3.
"""

from __future__ import annotations

import copy
import hashlib
import json
import math
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import numpy as np

import sailgym_core

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
TASKS_PATH = HERE / "configs" / "tasks.json"
# Contract amendments, by version. tasks.json is contract 1 and never changes,
# because its hash is replay identity for every existing store (M4U).
AMENDMENT_PATHS = {2: HERE / "configs" / "tasks_v2.json"}
CONDITIONS_DIR = HERE / "configs" / "conditions"

FEATURE_VERSION = 1
REWARD_VERSION = 1

# Outcome names reported by `RawEnv.outcome`.
RUNNING, FINISHED, TERMINATED, TRUNCATED = (
    "running",
    "finished",
    "terminated",
    "truncated",
)


# ---------------------------------------------------------------------------
# Tasks and conditions
# ---------------------------------------------------------------------------


def load_tasks(version: int = 1) -> dict[str, Any]:
    """``tasks.json`` with amendments 2..``version`` applied.

    An amendment may only override a task's reward constants; each task's
    ``reward`` is the global block merged with its own overrides.
    """
    cfg = json.loads(TASKS_PATH.read_text())
    for t in cfg["tasks"].values():
        t["reward"] = dict(cfg["reward"])
    for v in range(2, version + 1):
        amend = json.loads(AMENDMENT_PATHS[v].read_text())
        assert amend["contract_version"] == v, AMENDMENT_PATHS[v]
        for name, a in amend["tasks"].items():
            cfg["tasks"][name]["reward"].update(a["reward"])
    cfg["contract_version"] = version
    return cfg


def current_contract(task_name: str) -> int:
    """The latest contract version that amends ``task_name`` (1 if none).

    R stays at 1, so its stores, checkpoints and reports are unchanged.
    """
    return max(
        [1]
        + [
            v
            for v, p in AMENDMENT_PATHS.items()
            if task_name in json.loads(p.read_text())["tasks"]
        ]
    )


@dataclass(frozen=True)
class Condition:
    """One explicit initial/environmental condition (a scenario document)."""

    id: str
    wind_speed: float  # m/s
    wind_bearing_deg: float  # FROM, compass degrees (0 = northerly)
    heading_deg: float  # compass degrees (90 = east)

    def key(self) -> tuple[float, float, float]:
        return (self.wind_speed, self.wind_bearing_deg, self.heading_deg)

    def to_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "wind_speed": self.wind_speed,
            "wind_bearing_deg": self.wind_bearing_deg,
            "heading_deg": self.heading_deg,
        }

    @staticmethod
    def from_dict(d: dict[str, Any]) -> Condition:
        return Condition(
            d["id"],
            float(d["wind_speed"]),
            float(d["wind_bearing_deg"]),
            float(d["heading_deg"]),
        )


def template_scenario() -> dict[str, Any]:
    tasks = load_tasks()
    return json.loads((REPO / tasks["scenario_template"]).read_text())


NOMINAL = Condition("nominal", 5.0, 0.0, 90.0)
"""`free_sail` unchanged: 5 m/s from north, heading east."""


def scenario_json(cond: Condition, template: dict[str, Any] | None = None) -> str:
    """The scenario document for ``cond``: `free_sail` with wind and heading set.

    Only the three condition fields change; the wind stays ``uniform``.
    """
    doc = copy.deepcopy(template if template is not None else template_scenario())
    assert doc["wind"]["mode"] == "uniform", "the core study is uniform-wind only"
    doc["name"] = f"ese3990-{cond.id}"
    doc["description"] = "ESE3990 condition document generated from free_sail."
    doc["wind"]["speed"] = cond.wind_speed
    doc["wind"]["bearing_deg"] = cond.wind_bearing_deg
    doc["initial_state"]["heading_deg"] = cond.heading_deg
    return json.dumps(doc, sort_keys=True)


def load_manifest(name: str) -> list[Condition]:
    data = json.loads((CONDITIONS_DIR / f"{name}.json").read_text())
    return [Condition.from_dict(c) for c in data["conditions"]]


@dataclass
class Task:
    """A frozen single-gate task: geometry, budget and derived step counts."""

    name: str
    start: tuple[float, float]
    target: tuple[float, float]
    half_width: float
    budget_s: float
    bounds: tuple[float, float, float, float]
    cadence: int
    route_json: str
    dt: float = field(default=0.0)
    max_steps: int = field(default=0)
    contract_version: int = 1
    reward: dict[str, Any] = field(default_factory=dict)

    @staticmethod
    def load(name: str, contract_version: int | None = None) -> Task:
        """``contract_version`` defaults to the task's current contract; replay
        passes the version an episode recorded."""
        version = (
            current_contract(name) if contract_version is None else contract_version
        )
        cfg = load_tasks(version)
        t = cfg["tasks"][name]
        b = cfg["bounds"]
        (target,) = (tuple(p) for p in t["waypoints"])
        route = sailgym_core.route_waypoints(
            tuple(t["start"]), [tuple(p) for p in t["waypoints"]], t["half_width"]
        )
        task = Task(
            name=name,
            start=tuple(t["start"]),
            target=target,
            half_width=t["half_width"],
            budget_s=t["budget_s"],
            bounds=(b["min_x"], b["min_y"], b["max_x"], b["max_y"]),
            cadence=cfg["cadence_physics_steps"],
            route_json=route,
            contract_version=version,
            reward=t["reward"],
        )
        # dt from Rust, never typed here; the budget must be a whole number of
        # decisions.
        probe = sailgym_core.Spec("free_sail", cadence=task.cadence)
        task.dt = probe.dt
        steps = task.budget_s / task.dt
        task.max_steps = round(steps)
        if abs(steps - task.max_steps) > 1e-6 or task.max_steps % task.cadence:
            raise ValueError(
                f"budget {task.budget_s} s is not a whole number of "
                f"{task.cadence}-step decisions at dt = {task.dt}"
            )
        return task

    @property
    def decision_dt(self) -> float:
        return self.dt * self.cadence

    @property
    def max_decisions(self) -> int:
        return self.max_steps // self.cadence

    def spec(self, cond: Condition) -> sailgym_core.Spec:
        return sailgym_core.Spec(
            scenario_json(cond),
            max_steps=self.max_steps,
            autoreset="Disabled",
            cadence=self.cadence,
            bounds=self.bounds,
            route_json=self.route_json,
            log_decisions=False,
        )


# ---------------------------------------------------------------------------
# Actions
# ---------------------------------------------------------------------------


class ActionAdapter:
    """The one release restriction, applied to every controller.

    Learners emit two rates in [-1, 1] (rudder, sheet). Controllers that emit
    the native three-scalar action (RuleSailor) have their release flag
    overwritten. Either way the flag sent to Rust is ``release_value`` (-1).
    """

    def __init__(self, action_dim: int, release_value: float | None = None) -> None:
        if action_dim != 3:
            raise ValueError(f"expected the 3-scalar Rate adapter, got {action_dim}")
        if release_value is None:
            release_value = load_tasks()["action"]["release_value"]
        self.release_value = float(release_value)
        self._buf = np.empty(3, dtype=np.float64)

    def __call__(self, action: np.ndarray) -> np.ndarray:
        a = np.asarray(action, dtype=np.float64)
        if a.shape not in ((2,), (3,)):
            raise ValueError(f"action shape {a.shape}")
        self._buf[0] = float(np.clip(a[0], -1.0, 1.0))
        self._buf[1] = float(np.clip(a[1], -1.0, 1.0))
        self._buf[2] = self.release_value
        return self._buf


# ---------------------------------------------------------------------------
# Research state and actor features
# ---------------------------------------------------------------------------

RS_NAMES: list[str] = sailgym_core.research_state_names()
RS_UNITS: list[str] = sailgym_core.research_state_units()
RS_VERSION: int = sailgym_core.research_state_version()
RS = {n: i for i, n in enumerate(RS_NAMES)}

FEATURE_NAMES = [
    "dx_target",
    "dy_target",
    "cos_psi",
    "sin_psi",
    "phi",
    "u",
    "v",
    "r",
    "p",
    "cos_beta",
    "sin_beta",
    "beta_dot",
    "delta_r",
    "l_sheet",
    "capsize_pending",
    "wind_x",
    "wind_y",
]


def features(rs: np.ndarray, target: tuple[float, float]) -> np.ndarray:
    """The actor feature vector shared by PPO and SAC (plan §3.3).

    Wrapped angles (yaw, boom) become sin/cos; heel stays unwrapped; absolute
    time is excluded. Works on a single state ``(n,)`` or a batch ``(..., n)``.
    """
    rs = np.asarray(rs, dtype=np.float64)
    g = rs[..., RS["psi"]]
    b = rs[..., RS["beta"]]
    cols = [
        target[0] - rs[..., RS["x"]],
        target[1] - rs[..., RS["y"]],
        np.cos(g),
        np.sin(g),
        rs[..., RS["phi"]],
        rs[..., RS["u"]],
        rs[..., RS["v"]],
        rs[..., RS["r"]],
        rs[..., RS["p"]],
        np.cos(b),
        np.sin(b),
        rs[..., RS["beta_dot"]],
        rs[..., RS["delta_r"]],
        rs[..., RS["l_sheet"]],
        rs[..., RS["capsize_pending"]],
        rs[..., RS["wind_x"]],
        rs[..., RS["wind_y"]],
    ]
    return np.stack(cols, axis=-1)


# ---------------------------------------------------------------------------
# Reward
# ---------------------------------------------------------------------------

OUTCOME_CODE = {RUNNING: 0, FINISHED: 1, TERMINATED: 2, TRUNCATED: 3}


class TransitionReward:
    """r = -Δt/T + B·1[finished] - P·1[failed] + γΦ(s') - Φ(s), Φ = -d/D.

    Φ of a genuine terminal state (finished or terminated) is 0; a truncated
    state keeps its potential. ``failed`` is any Rust termination (capsize,
    out of bounds, mark missed). The scalar and batch paths share one formula.
    """

    def __init__(self, task: Task, params: dict[str, Any] | None = None) -> None:
        p = params if params is not None else task.reward
        self.time_scale = float(p["time_scale_s"])
        self.dist_scale = float(p["distance_scale_m"])
        self.finish_bonus = float(p["finish_bonus"])
        self.failure_penalty = float(p["failure_penalty"])
        self.gamma = float(p["gamma"])
        self.target = np.asarray(task.target, dtype=np.float64)
        self.dt = task.dt

    def potential(self, rs: np.ndarray) -> np.ndarray:
        rs = np.asarray(rs, dtype=np.float64)
        d = np.hypot(
            rs[..., RS["x"]] - self.target[0], rs[..., RS["y"]] - self.target[1]
        )
        return -d / self.dist_scale

    def batch(
        self,
        prev_rs: np.ndarray,
        next_rs: np.ndarray,
        outcome_code: np.ndarray,
        steps: np.ndarray,
    ) -> np.ndarray:
        code = np.asarray(outcome_code)
        genuine_terminal = (code == OUTCOME_CODE[FINISHED]) | (
            code == OUTCOME_CODE[TERMINATED]
        )
        phi_next = np.where(genuine_terminal, 0.0, self.potential(next_rs))
        phi_prev = self.potential(prev_rs)
        elapsed = np.asarray(steps, dtype=np.float64) * self.dt
        return (
            -elapsed / self.time_scale
            + self.finish_bonus * (code == OUTCOME_CODE[FINISHED])
            - self.failure_penalty * (code == OUTCOME_CODE[TERMINATED])
            + self.gamma * phi_next
            - phi_prev
        )

    def __call__(
        self, prev_rs: np.ndarray, next_rs: np.ndarray, outcome: str, steps: int
    ) -> float:
        return float(
            self.batch(
                prev_rs, next_rs, np.asarray(OUTCOME_CODE[outcome]), np.asarray(steps)
            )
        )

    def identity(self) -> dict[str, Any]:
        return {
            "version": REWARD_VERSION,
            "time_scale_s": self.time_scale,
            "distance_scale_m": self.dist_scale,
            "finish_bonus": self.finish_bonus,
            "failure_penalty": self.failure_penalty,
            "gamma": self.gamma,
            "target": self.target.tolist(),
        }


def masks(outcome: str) -> tuple[float, float]:
    """``(bootstrap, continue_trajectory)`` for a transition ending in ``outcome``.

    * running: bootstrap, continue.
    * truncated: bootstrap from the actual final state; do not continue GAE
      into whatever episode follows.
    * finished / terminated: neither.
    """
    if outcome == RUNNING:
        return 1.0, 1.0
    if outcome == TRUNCATED:
        return 1.0, 0.0
    if outcome in (FINISHED, TERMINATED):
        return 0.0, 0.0
    raise ValueError(outcome)


# ---------------------------------------------------------------------------
# One environment with explicit resets
# ---------------------------------------------------------------------------


@dataclass
class StepRecord:
    """One real decision: everything a learner or a report needs."""

    rs: np.ndarray  # research state at the decision
    action: np.ndarray  # the 3-scalar action actually sent
    reward: float
    next_rs: np.ndarray  # research state the period ended on (never a reset state)
    outcome: str
    reason: str | None
    steps: int  # physics steps executed (shorter on a terminal decision)
    bootstrap: float
    cont: float


class TaskEnv:
    """One condition of one task, with ``Disabled`` autoreset and explicit resets.

    Wraps ``sailgym_core.RawEnv`` directly because the research readout and
    the planning branch are not part of the Gymnasium surface. Episode
    boundaries are Rust's.
    """

    def __init__(self, task: Task, cond: Condition, seed: int = 0) -> None:
        self.task = task
        self.cond = cond
        self.spec = task.spec(cond)
        self.raw = sailgym_core.RawEnv(self.spec, seed)
        self.adapter = ActionAdapter(self.spec.action_dim)
        self.reward_fn = TransitionReward(task)
        self._obs32 = np.empty(self.spec.obs_len, dtype=np.float32)
        self._final32 = np.empty(self.spec.obs_len, dtype=np.float32)
        self._obs64 = np.empty(self.spec.obs_len, dtype=np.float64)
        self._seed = seed

    def reset(self, seed: int | None = None) -> np.ndarray:
        self.raw.reset(self._seed if seed is None else seed)
        return self.research_state()

    def research_state(self) -> np.ndarray:
        out = np.empty(len(RS_NAMES), dtype=np.float64)
        self.raw.research_state(out)
        return out

    def observation64(self) -> np.ndarray:
        self.raw.observe_f64(self._obs64)
        return self._obs64.copy()

    @property
    def outcome(self) -> str:
        """``running``/``finished``/``terminated``/``truncated``.

        ``RawEnv.outcome`` names the termination *reason* (``capsized``,
        ``out_of_bounds``, ``mark_missed``) for a termination; it is
        normalised here and the reason is kept in ``termination_reason``.
        """
        if self.raw.termination_reason is not None:
            return TERMINATED
        o = self.raw.outcome
        assert o in (RUNNING, FINISHED, TRUNCATED), o
        return o

    def step(self, action: np.ndarray) -> StepRecord:
        rs = self.research_state()
        a = self.adapter(action).copy()
        _, terminated, truncated, autoreset, steps, _ = self.raw.step(
            a, self._obs32, self._final32
        )
        assert not autoreset, "TaskEnv runs with Disabled autoreset"
        nrs = self.research_state()
        outcome = self.outcome
        assert terminated == (outcome in (FINISHED, TERMINATED))
        assert truncated == (outcome == TRUNCATED)
        r = self.reward_fn(rs, nrs, outcome, steps)
        b, c = masks(outcome)
        return StepRecord(
            rs, a, r, nrs, outcome, self.raw.termination_reason, steps, b, c
        )

    def fork(self) -> sailgym_core.RawEnv:
        return self.raw.fork()

    def rollout(
        self, rates: np.ndarray
    ) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
        """K candidate rate sequences ``(K, H, 2)`` from this exact point, stepped
        in Rust (``RawEnv.rollout``); the live episode is untouched.

        The vector form of ``ActionAdapter``: rates are clipped to [-1, 1] and
        release is forced to ``release_value``. Returns ``(next_rs (K,H,n),
        outcome_code (K,H), steps (K,H), reward (K,H))``. ``steps == 0`` marks
        a decision after the candidate's terminal one; its reward is 0.
        """
        r = np.asarray(rates, dtype=np.float64)
        if r.ndim != 3 or r.shape[2] != 2 or min(r.shape[:2]) < 1:
            raise ValueError("rates must have shape (K>0, H>0, 2)")
        if not np.isfinite(r).all():
            raise ValueError("candidate rates must be finite")
        k, h, _ = r.shape
        acts = np.empty((k, h, 3))
        acts[..., :2] = np.clip(r[..., :2], -1.0, 1.0)
        acts[..., 2] = self.adapter.release_value
        nrs = np.empty((k, h, len(RS_NAMES)))
        code = np.empty((k, h), np.uint8)
        steps = np.empty((k, h), np.uint32)
        self.raw.rollout(acts, nrs, code, steps)
        prev = np.concatenate(
            [np.broadcast_to(self.research_state(), (k, 1, nrs.shape[2])), nrs[:, :-1]],
            axis=1,
        )
        rew = np.where(steps > 0, self.reward_fn.batch(prev, nrs, code, steps), 0.0)
        return nrs, code, steps, rew


# ---------------------------------------------------------------------------
# Provenance
# ---------------------------------------------------------------------------


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()


def run_metadata(task: Task, **extra: Any) -> dict[str, Any]:
    """The record every checkpoint, report and figure carries (plan §5)."""
    tasks_text = TASKS_PATH.read_text()
    spec = task.spec(NOMINAL)
    return {
        "task": task.name,
        "tasks_json_sha256": sha256_text(tasks_text),
        "feature_version": FEATURE_VERSION,
        "feature_names": FEATURE_NAMES,
        "reward": TransitionReward(task).identity(),
        "research_state_version": RS_VERSION,
        "research_state_names": RS_NAMES,
        "obs_digest_sha256": sha256_text(spec.obs_digest),
        "action_identity": json.loads(spec.action_identity_json),
        "dt": task.dt,
        "cadence": task.cadence,
        "max_steps": task.max_steps,
        "bounds": list(task.bounds),
        "route": json.loads(task.route_json),
        **contract_identity(task),
        **extra,
    }


def contract_identity(task: Task) -> dict[str, Any]:
    """Contract fields for records of an amended task; empty for contract 1,
    so every R record keeps its exact M0–M4 form."""
    if task.contract_version == 1:
        return {}
    return {
        "contract_version": task.contract_version,
        "contract_amendment_sha256": {
            str(v): sha256_text(AMENDMENT_PATHS[v].read_text())
            for v in range(2, task.contract_version + 1)
        },
    }


def wrap_deg(a: float) -> float:
    return (a + 180.0) % 360.0 - 180.0


def distance_to_target(rs: np.ndarray, task: Task) -> float:
    return math.hypot(rs[RS["x"]] - task.target[0], rs[RS["y"]] - task.target[1])
