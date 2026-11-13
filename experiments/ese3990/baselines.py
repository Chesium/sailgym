"""Reference controllers: smooth random control and the bound RuleSailor.

Both act through :class:`sailing.TaskEnv`, so the shared release restriction
applies to them exactly as it does to the learners and the planner.
"""

from __future__ import annotations

import numpy as np
from sailing import TaskEnv

import sailgym_core

RANDOM_VERSION = 1


class SmoothRandom:
    """Versioned smooth-random rates: uniform in [-1, 1]^2, each held ``hold``
    decisions. Used for calibration data and as the environment sanity check.
    """

    version = RANDOM_VERSION

    def __init__(self, seed: int, hold: int = 10) -> None:
        self.seed = seed
        self.hold = hold
        self.rng = np.random.default_rng(seed)
        self._k = 0
        self._cmd = np.zeros(2)

    def reset(self, env: TaskEnv) -> None:
        self._k = 0

    def act(self, env: TaskEnv) -> np.ndarray:
        if self._k % self.hold == 0:
            self._cmd = self.rng.uniform(-1.0, 1.0, size=2)
        self._k += 1
        return self._cmd

    def identity(self) -> dict:
        return {
            "controller": "smooth_random",
            "version": self.version,
            "hold": self.hold,
        }


class RuleSailorController:
    """The Rust RuleSailor, fed the full-precision observation.

    Its maneuver memory lives in the Rust object and is reset with the
    episode. The release flag it emits is recorded (``release_requests``) and
    then overwritten to -1 by the shared adapter.
    """

    def __init__(self) -> None:
        self.inner: sailgym_core.RuleSailor | None = None
        self._out = np.zeros(3)
        self.release_requests = 0

    def reset(self, env: TaskEnv) -> None:
        self.inner = sailgym_core.RuleSailor(env.spec)
        self.inner.reset(env.spec.obs_names)
        if self.inner.missing_fields:
            raise ValueError(
                f"RuleSailor is missing columns {self.inner.missing_fields}"
            )
        self.release_requests = 0

    def act(self, env: TaskEnv) -> np.ndarray:
        assert self.inner is not None
        self.inner.decide(env.observation64(), self._out)
        if self._out[2] > 0.0:
            self.release_requests += 1
        return self._out

    def identity(self) -> dict:
        v = self.inner.version if self.inner is not None else None
        return {"controller": "rule_sailor", "version": v, "release": "forced off"}
