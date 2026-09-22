"""``sailgym``: the Gymnasium environment over ``sailgym-env`` (v2 section 07).

**Wrapper scope.** F17.1: this package contains no physical equation, no
parameter value and no layout. Every shape, bound, field name, digest and
timestep comes out of :mod:`sailgym_core`, the compiled binding, which reads
them from Rust. ``python/tests/test_no_stray_constants.py`` audits this
directory with the same three tiers section 03 built for
``sailgym_conformance``, and RV41 is the risk it exists for.

The package is deliberately thin:

=========================  ================================================
:mod:`sailgym.spaces`      spaces and the observation contract, built from
                           what Rust reports and refused when it differs
:mod:`sailgym.env`         :class:`~sailgym.env.SailgymEnv`, one episode
:mod:`sailgym.vector`      :class:`~sailgym.vector.SailgymVectorEnv`, N of
                           them, with the GIL released around the batch
=========================  ================================================

What is **not** here, and is not an oversight: no reward (``sailgym-env``
ships ``ZeroReward`` and a reward is an experiment parameter), no training
loop, no policy, no ``TimeLimit`` wrapper (F17.4 — the episode boundary is
Rust's), and no PettingZoo ``ParallelEnv`` (when multi-boat arrives the
single-agent env is its N = 1 specialisation).
"""

from __future__ import annotations

import sailgym_core

from .env import SailgymEnv
from .spaces import (
    IncompatibleObservation,
    ObservationContract,
    action_space,
    check_observation_compatible,
    observation_space,
)
from .vector import SailgymVectorEnv, pinned_autoreset_mode

Spec = sailgym_core.Spec
"""The episode contract. Constructed in Rust; see :mod:`sailgym.spaces`."""

__all__ = [
    "IncompatibleObservation",
    "ObservationContract",
    "SailgymEnv",
    "SailgymVectorEnv",
    "Spec",
    "action_space",
    "check_observation_compatible",
    "observation_space",
    "pinned_autoreset_mode",
]
