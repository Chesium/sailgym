"""Spaces built from what Rust reports, and the refusal when they differ.

v2 section 07 task 7.2; F14.3, F14.5, F16.4, F17.1.

``cross-stack.md`` §1.2 states the rule and the reason in one sentence, and
the section PRD quotes it: spaces are built from what the Rust core reports,
never typed out in Python, *"because Python is where that discipline usually
collapses"*. So there is no integer in this file that is not read from
:class:`sailgym_core.Spec`, and no bound either:

===========================  ==============================================
this file writes             it reads
===========================  ==============================================
``shape=(n,)``               ``spec.obs_len`` / ``spec.action_dim``
``low`` / ``high``           ``spec.obs_low`` / ``spec.obs_high``
the action box               ``spec.action_low`` / ``spec.action_high``
the field-name list          ``spec.obs_names``
the identity it refuses on   ``spec.obs_digest``
===========================  ==============================================

**Why the dtypes are what they are.** The observation buffers are ``float32``
because that is the shape F17.5 asks for — an *output buffer*, exactly like
``sample_wind_grid``'s, and not an ``f32`` intermediate in physics, which F9.5
forbids and which does not exist. Actions are ``float64`` because
``VecEnv::step_all`` takes ``&[f64]``: the funnel denormalises in Rust
(F14.5), so nothing is lost on the way in.

**Why the refusal is a refusal.** RV45: a policy loaded against a different
observation layout produces a plausible-looking number that means nothing.
:func:`check_observation_compatible` raises; it does not warn, and it does not
fall back to comparing lengths. F16.4 makes canonical-record equality the
comparison authority, and section 05's ``obs_digest`` **is** that canonical
record rather than a hash of it, so equality of the string is equality of the
record.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Any

import numpy as np
from gymnasium import spaces

OBS_DTYPE = np.float32
"""The observation buffer's element type.

F17.5: ``obs_out: &mut [f32]`` is an output buffer and not a physical
intermediate, so F9.5 is not in play. The value is a *dtype*, not a number.
"""

ACTION_DTYPE = np.float64
"""The action buffer's element type. ``VecEnv::step_all`` takes ``&[f64]``."""


def observation_space(spec: Any) -> spaces.Box:
    """The observation :class:`~gymnasium.spaces.Box` for ``spec``.

    Every argument comes from Rust. An absent bound arrives as ``±inf``,
    substituted in ``crates/sailgym-py/src/spec.rs`` rather than here, because
    a substitution made on this side would be a value Python invented.
    """
    return spaces.Box(
        low=np.asarray(spec.obs_low, dtype=OBS_DTYPE),
        high=np.asarray(spec.obs_high, dtype=OBS_DTYPE),
        shape=(spec.obs_len,),
        dtype=OBS_DTYPE,
    )


def action_space(spec: Any) -> spaces.Box:
    """The action :class:`~gymnasium.spaces.Box` for ``spec``.

    F14.5: every actuation adapter presents :math:`[-1, 1]^k` and denormalises
    internally. The corners are read from ``spec`` for the same reason the
    observation's are — an adapter that changed its dimension must change the
    space, and a space typed out here would not notice.
    """
    return spaces.Box(
        low=np.asarray(spec.action_low, dtype=ACTION_DTYPE),
        high=np.asarray(spec.action_high, dtype=ACTION_DTYPE),
        shape=(spec.action_dim,),
        dtype=ACTION_DTYPE,
    )


class IncompatibleObservation(ValueError):
    """Raised when two observation contracts are not the same contract."""


@dataclass(frozen=True)
class ObservationContract:
    """Everything needed to say whether two runs saw the same observation.

    F16.4 requires the **full record** to travel beside any digest, and
    section 05's ``obs_digest`` is the canonical record itself rather than a
    hash of it — so :attr:`digest` and :attr:`layout_json` carry the same text
    today and are kept apart because F16.4 permits a compact key later.

    Storage is deliberately not solved here: the PRD defers checkpoint
    storage and asks for an explicit compatibility-check function instead.
    :meth:`to_dict` and :meth:`from_dict` are that function's two ends, and
    whatever writes the JSON is a consumer's business.
    """

    digest: str
    names: tuple[str, ...]
    length: int
    layout_json: str

    @classmethod
    def of(cls, spec: Any) -> ObservationContract:
        """Read the contract off a :class:`sailgym_core.Spec`."""
        return cls(
            digest=spec.obs_digest,
            names=tuple(spec.obs_names),
            length=spec.obs_len,
            layout_json=spec.obs_layout_json,
        )

    def to_dict(self) -> dict[str, Any]:
        return {
            "digest": self.digest,
            "names": list(self.names),
            "length": self.length,
            "layout_json": self.layout_json,
        }

    @classmethod
    def from_dict(cls, data: Mapping[str, Any]) -> ObservationContract:
        missing = [
            k for k in ("digest", "names", "length", "layout_json") if k not in data
        ]
        if missing:
            raise IncompatibleObservation(
                "observation metadata is missing "
                + ", ".join(missing)
                + "; unknown metadata prevents strict comparison (F16.4) and is "
                "not treated as agreement"
            )
        names: Sequence[str] = data["names"]
        return cls(
            digest=str(data["digest"]),
            names=tuple(str(n) for n in names),
            length=int(data["length"]),
            layout_json=str(data["layout_json"]),
        )


def check_observation_compatible(spec: Any, recorded: Any) -> None:
    """Refuse an observation contract that is not this environment's.

    ``recorded`` is an :class:`ObservationContract`, or a mapping
    :meth:`ObservationContract.from_dict` accepts — what a checkpoint would
    have stored beside its weights.

    Raises :class:`IncompatibleObservation` naming *what* differs. RV45: a
    policy that loads cleanly against a different layout reads its input
    wrongly and reports a number that looks fine.
    """
    if not isinstance(recorded, ObservationContract):
        recorded = ObservationContract.from_dict(recorded)
    mine = ObservationContract.of(spec)
    if recorded == mine:
        return

    why: list[str] = []
    if recorded.length != mine.length:
        why.append(f"length {recorded.length} against this environment's {mine.length}")
    if recorded.names != mine.names:
        first = _first_difference(recorded.names, mine.names)
        why.append(f"field names differ first at {first}")
    if recorded.digest != mine.digest:
        why.append("the canonical layout record differs (F16.4)")
    raise IncompatibleObservation(
        "this observation layout is not the one that was recorded: "
        + "; ".join(why)
        + ". A policy trained against the other one would misread its input "
        "(RV45), so this is a refusal and not a warning."
    )


def _first_difference(a: Sequence[str], b: Sequence[str]) -> str:
    """Where two field-name lists first disagree, as a readable phrase."""
    for i, (x, y) in enumerate(zip(a, b, strict=False)):
        if x != y:
            return f"index {i}: {x!r} against {y!r}"
    shorter, longer = (a, b) if len(a) < len(b) else (b, a)
    return f"index {len(shorter)}: nothing against {longer[len(shorter)]!r}"
