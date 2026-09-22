"""The spaces are Rust's, and an incompatible layout is refused.

v2 section 07 task 7.2; F14.3, F14.5, F16.4, RV41, RV45.

The point of this file is not that the numbers are *right* — it is that there
are no numbers. Every shape, bound and name asserted here is compared against
what :class:`sailgym_core.Spec` reports, so a shape typed into
``python/sailgym/spaces.py`` would have to be typed into this file too, and
``test_no_stray_constants.py`` would catch it in the package regardless.
"""

from __future__ import annotations

import numpy as np
import pytest
from gymnasium import spaces

import sailgym
import sailgym_core
from sailgym.spaces import (
    IncompatibleObservation,
    ObservationContract,
    action_space,
    check_observation_compatible,
    observation_space,
)

SCENARIO = "free_sail"


def spec(**kwargs):
    return sailgym_core.Spec(SCENARIO, **kwargs)


def test_the_observation_space_is_the_rust_layout():
    """Shape, dtype and bounds, all read back from the layout Rust reports."""
    s = spec()
    box = observation_space(s)
    assert isinstance(box, spaces.Box)
    assert box.shape == (s.obs_len,)
    assert box.dtype == np.float32
    # The layout is runtime data (F14.3): its length is the sum of the
    # configured sensors' widths, and this asserts the space agrees with it
    # rather than with a remembered number.
    assert s.obs_len == len(s.obs_names)
    assert s.obs_len == len(s.obs_low) == len(s.obs_high)
    np.testing.assert_array_equal(box.low, np.asarray(s.obs_low, dtype=np.float32))
    np.testing.assert_array_equal(box.high, np.asarray(s.obs_high, dtype=np.float32))
    # Not vacuous: the tier-0 suite is not empty and not a single column.
    assert s.obs_len > 1


def test_the_field_names_match_rust_exactly_order_included():
    s = spec()
    env = sailgym.SailgymEnv(s)
    assert env.obs_names == list(s.obs_names)
    # Each name is `<sensor>.<field>`, and the sensors appear in the order
    # the configuration lists them — the ordered concatenation of F14.3.
    prefixes = [n.split(".", 1)[0] for n in s.obs_names]
    first_seen = []
    for p in prefixes:
        if p not in first_seen:
            first_seen.append(p)
    assert first_seen == list(s.sensors)
    env.close()


def test_a_configured_subset_of_sensors_shortens_the_layout():
    """RV41's real test: the length is **not** a constant.

    A layout that had been typed out in Python would be wrong here and only
    here, because every other assertion compares Rust against Rust.
    """
    full = spec()
    ids = list(sailgym_core.tier0_sensors())
    part = spec(sensors=ids[:2])
    assert part.obs_len < full.obs_len
    assert list(part.obs_names) == list(full.obs_names)[: part.obs_len]
    assert observation_space(part).shape == (part.obs_len,)


def test_the_action_space_is_the_unit_box_rust_reports():
    s = spec()
    box = action_space(s)
    assert box.shape == (s.action_dim,)
    assert box.dtype == np.float64
    np.testing.assert_array_equal(box.low, np.asarray(s.action_low))
    np.testing.assert_array_equal(box.high, np.asarray(s.action_high))
    # F14.5: every adapter presents [-1, 1]^k. Asserted against the values
    # Rust reported, not against a pair of literals written here.
    assert set(np.unique(box.low)) == {-1.0}
    assert set(np.unique(box.high)) == {1.0}
    assert s.action_dim >= 1


def test_the_observation_contract_round_trips():
    s = spec()
    contract = ObservationContract.of(s)
    assert contract == ObservationContract.from_dict(contract.to_dict())
    check_observation_compatible(s, contract)
    check_observation_compatible(s, contract.to_dict())


def test_incompatible_observation_metadata_raises():
    """RV45. A policy loaded against another layout must not load cleanly."""
    s = spec()
    mine = ObservationContract.of(s)

    shorter = ObservationContract(
        digest=mine.digest,
        names=mine.names[:-1],
        length=mine.length - 1,
        layout_json=mine.layout_json,
    )
    with pytest.raises(IncompatibleObservation) as why:
        check_observation_compatible(s, shorter)
    assert "length" in str(why.value)

    renamed = ObservationContract(
        digest=mine.digest,
        names=("imu.not_a_field",) + mine.names[1:],
        length=mine.length,
        layout_json=mine.layout_json,
    )
    with pytest.raises(IncompatibleObservation) as why:
        check_observation_compatible(s, renamed)
    assert "field names differ first at index 0" in str(why.value)

    restamped = ObservationContract(
        digest=mine.digest.replace('"sensor_version":1', '"sensor_version":2', 1),
        names=mine.names,
        length=mine.length,
        layout_json=mine.layout_json,
    )
    assert restamped.digest != mine.digest, "the substitution must have bitten"
    with pytest.raises(IncompatibleObservation) as why:
        check_observation_compatible(s, restamped)
    assert "canonical layout record" in str(why.value)


def test_metadata_that_is_merely_missing_is_not_agreement():
    """F16.4: unknown metadata prevents strict comparison. It is not a pass."""
    s = spec()
    partial = ObservationContract.of(s).to_dict()
    del partial["digest"]
    with pytest.raises(IncompatibleObservation) as why:
        check_observation_compatible(s, partial)
    assert "missing digest" in str(why.value)


def test_a_real_subset_layout_is_refused_against_the_full_one():
    """The refusal that would actually happen, not a hand-built one."""
    full = spec()
    part = spec(sensors=list(sailgym_core.tier0_sensors())[:2])
    with pytest.raises(IncompatibleObservation):
        check_observation_compatible(full, ObservationContract.of(part))
    with pytest.raises(IncompatibleObservation):
        check_observation_compatible(part, ObservationContract.of(full))


def test_the_digest_is_the_canonical_record_and_not_a_hash():
    """Section 05 chose the record over a second SHA-256 (F16.4).

    Asserted so that a later change to a compact key is a deliberate one:
    the digest must still *be* the layout, and the layout must still carry
    every column's name, unit, bounds, normalisation, noise and privilege.
    """
    s = spec()
    assert s.obs_digest == s.obs_layout_json
    for field in ("sensor", "sensor_version", "unit", "normalisation", "privileged"):
        assert field in s.obs_digest
    for name in s.obs_names:
        assert name.split(".", 1)[1] in s.obs_digest


def test_the_env_exposes_its_identity_as_attributes():
    """The PRD: *`obs_digest` and its complete canonical layout record are
    exposed as env attributes.*

    The vector env's half of the same assertion is in
    ``test_autoreset.py``, with the rest of task 7.4.
    """
    s = spec()
    env = sailgym.SailgymEnv(s)
    assert env.obs_digest == s.obs_digest
    assert env.obs_layout_json == s.obs_layout_json
    assert env.observation_contract == ObservationContract.of(s)
    assert env.obs_names == list(s.obs_names)
    assert env.dt == s.dt
    env.close()
