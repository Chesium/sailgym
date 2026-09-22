"""N independent episodes, as a :class:`gymnasium.vector.VectorEnv`.

v2 section 07 task 7.4; F16.5, F17.4, F17.5, F19.1.

**The convention is read, not inherited.** Section 06 implemented both live
autoreset conventions in Rust and proved numerically that they produce the
same discounted returns — and it could not pin Gymnasium, because the binding
that needs it is this section's. F19.1 therefore instructs section 07 to
*re-check* the convention against whatever it pins and **select** the mode
that version declares, rather than inherit ``NextStep`` because section 06
defaulted to it. :func:`pinned_autoreset_mode` is that re-check: it reads
``gymnasium.vector.AutoresetMode`` and ``SyncVectorEnv``'s own declared
default out of the installed release, and refuses anything the Rust side does
not spell identically.

**Caller-owned buffers, reused.** F17.5. :meth:`SailgymVectorEnv.step` writes
into the same five arrays every call and returns them; a caller that keeps an
observation copies it. That is the trade the PRD names in as many words — the
low-level buffers are reusable and the Gymnasium tuple and ``info`` dictionary
above them are not, and their costs are measured separately in
``python/tests/test_performance.py``.

**The GIL is released** for the duration of the batch, inside
``crates/sailgym-py/src/vector.rs``. It is measured, not asserted in a
comment: the same test file runs a Python thread alongside a batch and holds
the overlap to a stated bound (RV42).

**Two masks, never their union** (RV34). ``terminations`` and ``truncations``
are separate arrays because Rust reports them separately, and nothing here
computes ``done``.
"""

from __future__ import annotations

import inspect
from collections.abc import Callable, Sequence
from typing import Any

import gymnasium
import numpy as np
from gymnasium.vector import AutoresetMode, VectorEnv
from gymnasium.vector.utils import batch_space

import sailgym_core

from .env import as_u64
from .spaces import (
    ACTION_DTYPE,
    OBS_DTYPE,
    ObservationContract,
    action_space,
    observation_space,
)


def gymnasium_autoreset_modes() -> list[str]:
    """The names the **installed** Gymnasium spells its conventions with."""
    return [m.value for m in AutoresetMode]


def pinned_autoreset_mode() -> str:
    """The convention the installed Gymnasium declares as its default.

    F19.1: *"Section 07 must re-check them against whatever it pins and select
    the mode its version declares in ``metadata['autoreset_mode']``."* The
    declaration a vector env's metadata carries comes from
    ``SyncVectorEnv``'s own signature default, so that is what is read — from
    the release, by :mod:`inspect`, and never from this document.

    Raises :class:`RuntimeError` if the installed Gymnasium and this build of
    ``sailgym_core`` do not spell the conventions identically, which is RV44's
    early warning rather than a silent mismatch in the returns.
    """
    ours = sailgym_core.autoreset_modes()
    theirs = gymnasium_autoreset_modes()
    if sorted(theirs) != sorted(ours):
        raise RuntimeError(
            f"gymnasium {gymnasium.__version__} spells its autoreset modes "
            f"{theirs} and this build of sailgym_core spells them {ours}. "
            "F19.1 requires the two vocabularies to be the same words; a "
            "translation table here would be a second definition of the "
            "convention."
        )
    parameters = inspect.signature(gymnasium.vector.SyncVectorEnv.__init__).parameters
    if "autoreset_mode" not in parameters:
        raise RuntimeError(
            f"gymnasium {gymnasium.__version__} does not declare an "
            "`autoreset_mode` on SyncVectorEnv, so its convention cannot be "
            "read; F17.4 forbids assuming one"
        )
    default = parameters["autoreset_mode"].default
    name = default.value if isinstance(default, AutoresetMode) else str(default)
    if name not in ours:
        raise RuntimeError(
            f"gymnasium {gymnasium.__version__} defaults to autoreset mode "
            f"{name!r}, which this build of sailgym_core does not implement"
        )
    return name


class SailgymVectorEnv(VectorEnv):
    """``num_envs`` independent episodes, stepped together.

    Independent, and nothing more: there is no boat-to-boat interaction of any
    kind — no collisions, no right-of-way, no wind shadow (``brief.md`` §3).
    Two slots with the same seed are equal, not coupled.

    ``parallel`` selects rayon inside ``sailgym-env``. Section 06 asserts the
    serial and parallel arms are ``to_bits()``-identical at
    N ∈ {1, 8, 64, 512} × six thread counts, so it is a throughput switch and
    never a semantic one (F16.5).
    """

    metadata: dict[str, Any] = {}

    def __init__(
        self,
        spec: Any = None,
        *,
        num_envs: int | None = None,
        seeds: Sequence[int] | None = None,
        parallel: bool = True,
        reward_fn: Callable[[np.ndarray], np.ndarray] | None = None,
        **spec_kwargs: Any,
    ) -> None:
        if isinstance(spec, str):
            # A bare scenario id is the common case; `Spec` is the escape
            # hatch for everything else.
            spec_kwargs["scenario"] = spec
            spec = None
        if spec is None:
            spec_kwargs.setdefault("autoreset", pinned_autoreset_mode())
            spec = sailgym_core.Spec(**spec_kwargs)
        elif spec_kwargs:
            raise ValueError(
                "pass a Spec or the arguments to build one, not both: "
                + ", ".join(sorted(spec_kwargs))
            )
        seeds = self._seed_list(seeds, num_envs)

        self.spec_core = spec
        self.render_mode = None
        self._reward_fn = reward_fn
        self._root_seeds = list(seeds)
        self._raw = sailgym_core.RawVecEnv(spec, self._root_seeds, parallel)

        self.num_envs = len(self._root_seeds)
        self.single_observation_space = observation_space(spec)
        self.single_action_space = action_space(spec)
        self.observation_space = batch_space(
            self.single_observation_space, self.num_envs
        )
        self.action_space = batch_space(self.single_action_space, self.num_envs)
        self.metadata = dict(self.metadata)
        self.metadata["autoreset_mode"] = AutoresetMode(spec.autoreset_mode)

        n, width = self.num_envs, spec.obs_len
        self._obs = np.zeros((n, width), dtype=OBS_DTYPE)
        self._final_obs = np.zeros((n, width), dtype=OBS_DTYPE)
        self._end_obs = np.zeros((n, width), dtype=OBS_DTYPE)
        self._rewards = np.zeros(n, dtype=np.float64)
        self._terminated = np.zeros(n, dtype=np.uint8)
        self._truncated = np.zeros(n, dtype=np.uint8)
        self._final_valid = np.zeros(n, dtype=np.uint8)
        # Preallocated views of the two masks as booleans. `astype` would
        # allocate two arrays per step, which is exactly the churn F17.5 asks
        # this layer not to have.
        self._terminations = np.zeros(n, dtype=bool)
        self._truncations = np.zeros(n, dtype=bool)
        self._actions = np.zeros((n, spec.action_dim), dtype=ACTION_DTYPE)

    # -- identity ---------------------------------------------------------

    @property
    def obs_digest(self) -> str:
        return self.spec_core.obs_digest

    @property
    def obs_layout_json(self) -> str:
        return self.spec_core.obs_layout_json

    @property
    def observation_contract(self) -> ObservationContract:
        return ObservationContract.of(self.spec_core)

    @property
    def autoreset_mode(self) -> str:
        return self.spec_core.autoreset_mode

    @property
    def root_seeds(self) -> list[int]:
        return list(self._root_seeds)

    @property
    def is_parallel(self) -> bool:
        return self._raw.is_parallel

    @property
    def episode_steps(self) -> list[int]:
        """Physics steps executed in each slot's current episode.

        See :attr:`sailgym.env.SailgymEnv.episode_steps`.
        """
        return [int(s) for s in self._raw.episode_steps]

    # -- the API ----------------------------------------------------------

    def reset(
        self,
        *,
        seed: int | Sequence[int] | None = None,
        options: dict[str, Any] | None = None,
    ) -> tuple[np.ndarray, dict[str, Any]]:
        if seed is not None:
            self._root_seeds = self._seed_list(seed, self.num_envs)
        self._raw.reset_all(self._root_seeds)
        self._raw.observe_all(self._obs)
        self._final_valid[:] = 0
        return self._obs, self._infos(autoreset=False)

    def step(
        self, actions: np.ndarray
    ) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray, dict[str, Any]]:
        np.copyto(self._actions, np.asarray(actions, dtype=ACTION_DTYPE))
        self._raw.step_all(
            self._actions,
            self._obs,
            self._rewards,
            self._terminated,
            self._truncated,
        )
        self._raw.final_obs_valid(self._final_valid)
        any_final = bool(self._final_valid.any())
        if any_final:
            self._raw.final_obs(self._final_obs)
        np.not_equal(self._terminated, 0, out=self._terminations)
        np.not_equal(self._truncated, 0, out=self._truncations)
        if self._reward_fn is not None:
            np.copyto(self._end_obs, self._obs)
            if any_final:
                rows = self._final_valid != 0
                self._end_obs[rows] = self._final_obs[rows]
            np.copyto(self._rewards, np.asarray(self._reward_fn(self._end_obs)))
        infos = self._infos(autoreset=any_final)
        if any_final:
            infos["final_obs"] = self._final_obs
            infos["_final_obs"] = self._final_valid != 0
        return (
            self._obs,
            self._rewards,
            self._terminations,
            self._truncations,
            infos,
        )

    def close(self, **kwargs: Any) -> None:
        self.closed = True

    # -- internals --------------------------------------------------------

    def _infos(self, *, autoreset: bool) -> dict[str, Any]:
        return {
            "outcome": self._raw.outcomes,
            "termination_reason": self._raw.termination_reasons,
            "autoreset": autoreset,
        }

    @staticmethod
    def _seed_list(
        seeds: int | Sequence[int] | None, num_envs: int | None
    ) -> list[int]:
        """The per-slot root seeds.

        An explicit sequence is used verbatim — F17.3, one ``u64`` per slot.
        A single integer follows Gymnasium's own convention, ``seed + i``,
        which is what a user of the standard expects; section 06's
        ``one_seed_reproduces_the_episode_and_the_chain_after_it`` asserts
        that two adjacent root seeds produce non-overlapping chains, so the
        convention is safe here rather than merely conventional.
        """
        if seeds is None:
            raise ValueError("a vector env needs `seeds=` or `seed=`")
        if isinstance(seeds, int):
            if num_envs is None:
                raise ValueError("`num_envs=` is required with a single seed")
            base = as_u64(seeds)
            return [as_u64(base + i) for i in range(num_envs)]
        out = [as_u64(s) for s in seeds]
        if num_envs is not None and len(out) != num_envs:
            raise ValueError(f"{len(out)} seeds for {num_envs} environments")
        return out
