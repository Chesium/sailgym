"""Compact episode logs: actions + condition + seed, replayable exactly (§10.1).

A run directory holds::

    run.json          store version, build identity, contract hashes, run metadata
    index.jsonl       one line per episode (metadata + where its arrays live)
    shard-00000.npz   concatenated actions (f32 and f64) and 5 Hz poses

The simulator is deterministic, so the action sequence plus the condition and
seed *is* the episode on a given build. States are recomputed by
``viz.replay``; the low-rate pose is a build-independent fallback for plots.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import numpy as np
from sailing import (
    FEATURE_VERSION,
    REWARD_VERSION,
    RS,
    RS_VERSION,
    TASKS_PATH,
    Condition,
    StepRecord,
    TaskEnv,
    sha256_text,
)

import sailgym_core

STORE_VERSION = 1
POSE_FIELDS = ("x", "y", "psi", "phi")
POSE_HZ = 5.0
ACTION_DTYPES = {"float32": np.float32, "float64": np.float64}


def state_checksum(rs: np.ndarray) -> str:
    """16 hex chars of SHA-256 over the f64 research-state bytes."""
    return hashlib.sha256(
        np.ascontiguousarray(rs, dtype=np.float64).tobytes()
    ).hexdigest()[:16]


def build_identity() -> dict[str, Any]:
    """Everything a replay must match to be exact."""
    return {
        "model": json.loads(sailgym_core.model_identity()),
        "tasks_json_sha256": sha256_text(TASKS_PATH.read_text()),
        "research_state_version": RS_VERSION,
        "feature_version": FEATURE_VERSION,
        "reward_version": REWARD_VERSION,
    }


@dataclass
class EpisodeLog:
    meta: dict[str, Any]
    actions: np.ndarray  # (decisions, 2): rudder rate, sheet rate
    pose: np.ndarray  # (k, 4): POSE_FIELDS at POSE_HZ, including t=0 and the end


# ---------------------------------------------------------------------------
# Recording one episode
# ---------------------------------------------------------------------------


class EpisodeRecorder:
    """Accumulates one episode while a controller drives a ``TaskEnv``.

    ``action_dtype`` is the controller's native precision: ``float32`` for a
    JAX policy, ``float64`` for RuleSailor. Storing ``float32`` is checked to
    be lossless, so a replay sends Rust exactly the actions it saw.
    """

    def __init__(self, env: TaskEnv, action_dtype: str = "float64") -> None:
        if action_dtype not in ACTION_DTYPES:
            raise ValueError(action_dtype)
        self.env = env
        self.dtype = ACTION_DTYPES[action_dtype]
        self.dtype_name = action_dtype
        self.pose_every = max(1, round(1.0 / (POSE_HZ * env.task.decision_dt)))
        self._actions: list[np.ndarray] = []
        self._pose: list[np.ndarray] = []
        self._ret = 0.0
        self._steps = 0
        self._k = 0

    def start(self, rs0: np.ndarray) -> None:
        self._pose.append(self._pose_of(rs0))

    @staticmethod
    def _pose_of(rs: np.ndarray) -> np.ndarray:
        return np.array([rs[RS[f]] for f in POSE_FIELDS])

    def step(self, rec: StepRecord) -> None:
        a = rec.action[:2]
        stored = a.astype(self.dtype)
        if not np.array_equal(stored.astype(np.float64), a):
            raise ValueError(
                f"action {a} is not representable as {self.dtype_name}; "
                "log it as float64 or round it before it is sent"
            )
        self._actions.append(stored)
        self._ret += rec.reward
        self._steps += rec.steps
        self._k += 1
        terminal = rec.outcome != "running"
        if self._k % self.pose_every == 0 or terminal:
            self._pose.append(self._pose_of(rec.next_rs))

    def finish(self, **meta: Any) -> EpisodeLog:
        env = self.env
        final = env.research_state()
        acts = (
            np.stack(self._actions) if self._actions else np.zeros((0, 2), self.dtype)
        )
        m = {
            "task": env.task.name,
            "condition": env.cond.to_dict(),
            "seed": env._seed,
            "outcome": env.outcome,
            "reason": env.raw.termination_reason,
            "finish_time_s": env.raw.finish_time,
            "decisions": int(len(acts)),
            "physics_steps": int(self._steps),
            "return": float(self._ret),
            "final_checksum": state_checksum(final),
            "action_dtype": self.dtype_name,
            "pose_hz": POSE_HZ,
            "pose_every_decisions": self.pose_every,
            **meta,
        }
        return EpisodeLog(m, acts, np.stack(self._pose))


# ---------------------------------------------------------------------------
# Writing and reading a run
# ---------------------------------------------------------------------------


class EpisodeWriter:
    """Append episodes to a run directory, in compressed shards."""

    def __init__(
        self,
        run_dir: Path | str,
        run_meta: dict[str, Any] | None = None,
        shard_size: int = 256,
    ) -> None:
        self.dir = Path(run_dir)
        self.dir.mkdir(parents=True, exist_ok=True)
        self.shard_size = shard_size
        head = self.dir / "run.json"
        ident = build_identity()
        if head.exists():
            existing = json.loads(head.read_text())
            if existing["identity"] != ident:
                raise ValueError(
                    f"{self.dir} was written by a different build or contract; "
                    "use a new run directory"
                )
        else:
            head.write_text(
                json.dumps(
                    {
                        "store_version": STORE_VERSION,
                        "identity": ident,
                        "run": run_meta or {},
                    },
                    indent=1,
                )
            )
        self._shard = len(list(self.dir.glob("shard-*.npz")))
        self._count = sum(1 for _ in self._index_lines())
        self._pending: list[EpisodeLog] = []

    def _index_lines(self):
        idx = self.dir / "index.jsonl"
        if idx.exists():
            yield from idx.read_text().splitlines()

    def add(self, log: EpisodeLog) -> str:
        eid = log.meta.get("episode_id") or f"ep{self._count + len(self._pending):07d}"
        log.meta["episode_id"] = eid
        self._pending.append(log)
        if len(self._pending) >= self.shard_size:
            self.flush()
        return eid

    def flush(self) -> None:
        if not self._pending:
            return
        name = f"shard-{self._shard:05d}.npz"
        arrays: dict[str, list[np.ndarray]] = {"float32": [], "float64": []}
        offs = {"float32": 0, "float64": 0}
        poses: list[np.ndarray] = []
        p_off = 0
        lines = []
        for log in self._pending:
            dt = log.meta["action_dtype"]
            n = len(log.actions)
            entry = dict(log.meta)
            entry.update(
                shard=name,
                action_offset=offs[dt],
                pose_offset=p_off,
                pose_len=len(log.pose),
            )
            arrays[dt].append(log.actions)
            offs[dt] += n
            poses.append(log.pose.astype(np.float32))
            p_off += len(log.pose)
            lines.append(json.dumps(entry, sort_keys=True))
        np.savez_compressed(
            self.dir / name,
            actions_float32=np.concatenate(
                arrays["float32"] or [np.zeros((0, 2), np.float32)]
            ),
            actions_float64=np.concatenate(
                arrays["float64"] or [np.zeros((0, 2), np.float64)]
            ),
            pose=np.concatenate(poses),
        )
        with open(self.dir / "index.jsonl", "a") as f:
            f.write("\n".join(lines) + "\n")
        self._count += len(self._pending)
        self._shard += 1
        self._pending = []

    def close(self) -> None:
        self.flush()

    def __enter__(self) -> EpisodeWriter:
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()


@dataclass
class EpisodeStore:
    """Read side of a run directory."""

    dir: Path
    head: dict[str, Any] = field(init=False)
    index: list[dict[str, Any]] = field(init=False)
    _shards: dict[str, Any] = field(init=False, default_factory=dict)

    def __post_init__(self) -> None:
        self.dir = Path(self.dir)
        self.head = json.loads((self.dir / "run.json").read_text())
        idx = self.dir / "index.jsonl"
        self.index = (
            [json.loads(line) for line in idx.read_text().splitlines() if line]
            if idx.exists()
            else []
        )

    @property
    def identity(self) -> dict[str, Any]:
        return self.head["identity"]

    def query(self, **filters: Any) -> list[dict[str, Any]]:
        """Entries whose fields equal every filter. ``condition`` matches the
        condition id; a callable filter is applied as a predicate."""

        def ok(e: dict[str, Any]) -> bool:
            for k, v in filters.items():
                got = e["condition"]["id"] if k == "condition" else e.get(k)
                if callable(v):
                    if not v(got):
                        return False
                elif got != v:
                    return False
            return True

        return [e for e in self.index if ok(e)]

    def _shard(self, name: str):
        if name not in self._shards:
            with np.load(self.dir / name) as z:
                self._shards[name] = {k: z[k] for k in z.files}
        return self._shards[name]

    def load(self, entry: dict[str, Any]) -> EpisodeLog:
        z = self._shard(entry["shard"])
        a0, n = entry["action_offset"], entry["decisions"]
        p0, m = entry["pose_offset"], entry["pose_len"]
        acts = z[f"actions_{entry['action_dtype']}"][a0 : a0 + n]
        return EpisodeLog(
            dict(entry), acts.copy(), z["pose"][p0 : p0 + m].astype(np.float64)
        )

    def condition_of(self, entry: dict[str, Any]) -> Condition:
        return Condition.from_dict(entry["condition"])
