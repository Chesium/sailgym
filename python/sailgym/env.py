"""One episode, as a :class:`gymnasium.Env` (v2 section 07 task 7.3).

F17.3 and F17.4, in one class.

**Seeding.** ``reset(seed=k)`` maps *straight* onto the Rust ``u64``. There is
no hashing, no offset and no second generator: the wind field, the agent's
stream and the next episode's seed all derive from that one number inside
``sailgym-env``. Gymnasium's :attr:`np_random` is seeded by the base class
because the API says it is, and is then **never read** — not for wind, not for
sensor noise, not for scenario sampling. ``python/tests/test_env.py`` draws
from it a different number of times under the same ``k`` and asserts the
trajectory is bit-identical, which is the only test that would notice a leak
(RV43).

**Episode boundaries.** ``Outcome`` is decided in Rust and arrives here as two
separate booleans; their union is never computed (RV34). Gymnasium's
step-budget wrapper is **not** used: F17.4 is explicit that the vectorised
path cannot use Python wrappers, so a wrapper-supplied truncation would make
the two paths disagree about episode boundaries — silently, in the returns. A
step budget is ``Spec(max_steps=…)`` and it is enforced by the runner, and
``python/tests/test_autoreset.py`` greps the whole of ``python/`` for the
wrapper's three usable spellings (RV44).

**Rewards.** This package ships none. ``sailgym-env`` supplies ``ZeroReward``
and a reward is an experiment parameter, not an environment constant. An
experiment passes ``reward_fn``, which is called on the observation the
decision period **ended on** — the one quantity both autoreset conventions
agree about, which is what makes 7.4's returns identity exact rather than
approximate.
"""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

import gymnasium
import numpy as np

import sailgym_core

from .spaces import (
    ACTION_DTYPE,
    OBS_DTYPE,
    ObservationContract,
    action_space,
    observation_space,
)

SINGLE_ENV_AUTORESET = "Disabled"
"""What a lone :class:`gymnasium.Env` steps under, unless told otherwise.

Gymnasium's single-environment API has no autoreset: a terminated env is reset
by its caller. ``"Disabled"`` is one of the three names ``sailgym-env`` takes
verbatim from ``gymnasium.vector.AutoresetMode``, and
:func:`sailgym_core.autoreset_modes` is checked against this string at import
so a rename in either place is a loud failure rather than a silent default.
"""

if SINGLE_ENV_AUTORESET not in sailgym_core.autoreset_modes():
    raise RuntimeError(
        f"this build of sailgym_core spells its autoreset modes "
        f"{sailgym_core.autoreset_modes()}, which does not include "
        f"{SINGLE_ENV_AUTORESET!r}"
    )


def as_u64(seed: int) -> int:
    """F17.3: the Gymnasium seed **is** the Rust ``u64``.

    Refused rather than wrapped, hashed or masked. A seed that silently became
    a different seed would break the one property this environment sells.
    """
    value = int(seed)
    if value < 0 or value.bit_length() > 64:
        raise ValueError(
            f"seed {seed} is not a u64; F17.3 maps the Gymnasium seed straight "
            "onto the Rust one, so it is refused rather than reduced"
        )
    return value


class SailgymEnv(gymnasium.Env):
    """One sailing episode.

    Either hand it a :class:`sailgym_core.Spec`, or give it the keyword
    arguments to build one::

        SailgymEnv(scenario="tack", max_steps=4000, cadence=10)

    Attributes that carry the observation identity — :attr:`obs_digest`,
    :attr:`obs_layout_json` and :attr:`observation_contract` — are what a
    checkpoint stores and :func:`sailgym.spaces.check_observation_compatible`
    refuses on (RV45).
    """

    metadata: dict[str, Any] = {"render_modes": []}

    def __init__(
        self,
        spec: Any = None,
        *,
        seed: int = 0,
        reward_fn: Callable[[np.ndarray], float] | None = None,
        **spec_kwargs: Any,
    ) -> None:
        if isinstance(spec, str):
            # A bare scenario id is the common case; `Spec` is the escape
            # hatch for everything else.
            spec_kwargs["scenario"] = spec
            spec = None
        if spec is None:
            spec_kwargs.setdefault("autoreset", SINGLE_ENV_AUTORESET)
            spec = sailgym_core.Spec(**spec_kwargs)
        elif spec_kwargs:
            raise ValueError(
                "pass a Spec or the arguments to build one, not both: "
                + ", ".join(sorted(spec_kwargs))
            )
        self.spec_core = spec
        self.render_mode = None
        self._reward_fn = reward_fn
        self._root_seed = as_u64(seed)
        self._raw = sailgym_core.RawEnv(spec, self._root_seed)

        self.observation_space = observation_space(spec)
        self.action_space = action_space(spec)

        # Two reusable buffers. The *public* return is a copy, because a
        # single Gymnasium env is read by code that keeps its observations;
        # the batch API reuses its buffers and says so (F17.5), and
        # `python/tests/test_performance.py` measures the difference.
        self._obs = np.zeros(spec.obs_len, dtype=OBS_DTYPE)
        self._final_obs = np.zeros(spec.obs_len, dtype=OBS_DTYPE)
        self._final_obs_valid = False

    # -- identity ---------------------------------------------------------

    @property
    def obs_digest(self) -> str:
        """The canonical observation record (F16.4), as Rust reports it."""
        return self.spec_core.obs_digest

    @property
    def obs_layout_json(self) -> str:
        """The same record, named as the record rather than as the key."""
        return self.spec_core.obs_layout_json

    @property
    def observation_contract(self) -> ObservationContract:
        """What a checkpoint stores, and what a mismatched one is refused
        against."""
        return ObservationContract.of(self.spec_core)

    @property
    def obs_names(self) -> list[str]:
        return list(self.spec_core.obs_names)

    @property
    def dt(self) -> float:
        """s, the fixed physics timestep (F7). Read from Rust; F17.1 forbids
        this file from knowing it."""
        return self.spec_core.dt

    @property
    def autoreset_mode(self) -> str:
        return self.spec_core.autoreset_mode

    @property
    def root_seed(self) -> int:
        return self._root_seed

    @property
    def episode_steps(self) -> int:
        """Physics steps executed in the **current** episode.

        What a caller needs to key a scripted action off the episode's own
        decision counter rather than off a global call counter (section 06
        §3.1): under ``NextStep`` one call per boundary is spent on a reset,
        and a globally indexed script would feed the two conventions
        different actions.
        """
        return int(self._raw.steps)

    # -- the API ----------------------------------------------------------

    def reset(
        self,
        *,
        seed: int | None = None,
        options: dict[str, Any] | None = None,
    ) -> tuple[np.ndarray, dict[str, Any]]:
        # The u64 check comes **first**, so that a seed this environment
        # cannot honour is refused by F17.3's rule and not by Gymnasium's
        # narrower one: the message has to say which contract was broken.
        if seed is not None:
            self._root_seed = as_u64(seed)
        # Seeds `np_random`, because the API says `reset` does. Nothing in
        # this file, in `sailgym_core` or in `sailgym-env` ever reads it
        # (F17.3).
        super().reset(seed=seed)
        # A reset with no seed repeats the current root seed rather than
        # inventing a new one: this environment has exactly one source of
        # randomness and it is the caller's `u64`. Variety within a chain
        # comes from autoreset, which draws from `STREAM_SCENARIO`.
        self._raw.reset(self._root_seed)
        self._raw.observe(self._obs)
        self._final_obs_valid = False
        return self._obs.copy(), self._info(steps=0, autoreset=False)

    def step(
        self, action: np.ndarray
    ) -> tuple[np.ndarray, float, bool, bool, dict[str, Any]]:
        values = np.ascontiguousarray(action, dtype=ACTION_DTYPE).reshape(-1)
        reward, terminated, truncated, autoreset, steps, final_valid = self._raw.step(
            values, self._obs, self._final_obs
        )
        self._final_obs_valid = bool(final_valid)
        if self._reward_fn is not None:
            # `steps == 0` is the neutral record `AutoresetMode::NextStep`
            # inserts between two episodes: it executed no physics, so it
            # earns no reward. `sailgym-env` applies exactly this rule to its
            # own `Reward`, and `tests/returns.rs` is what proves the two
            # conventions agree because of it.
            reward = 0.0 if steps == 0 else float(self._reward_fn(self.end_observation))
        info = self._info(steps=steps, autoreset=autoreset)
        if final_valid:
            info["final_obs"] = self._final_obs.copy()
        return self._obs.copy(), float(reward), bool(terminated), bool(truncated), info

    @property
    def end_observation(self) -> np.ndarray:
        """The observation the last decision period **ended** on.

        Under ``NextStep`` the returned observation already is it. Under
        ``SameStep`` the returned observation is the *reset* one and this is
        carried separately. A reward that is a function of this quantity is a
        reward the two conventions agree about, which is the whole of 7.4.

        The returned array is a live buffer; copy it if you keep it.
        """
        return self._final_obs if self._final_obs_valid else self._obs

    def close(self) -> None:
        return None

    # -- internals --------------------------------------------------------

    def _info(self, *, steps: int, autoreset: bool) -> dict[str, Any]:
        return {
            "outcome": self._raw.outcome,
            "termination_reason": self._raw.termination_reason,
            "physics_steps": int(steps),
            "episode_steps": int(self._raw.steps),
            "episode_index": int(self._raw.episode_index),
            "seed": int(self._raw.seed),
            "autoreset": bool(autoreset),
            "t": float(self._raw.t),
        }
