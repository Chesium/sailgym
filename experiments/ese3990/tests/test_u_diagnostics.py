"""U diagnostics (plan M4U): synthetic manoeuvres and RuleSailor's real U tacks."""

from __future__ import annotations

import math

import numpy as np
import pytest
from baselines import RuleSailorController
from evaluate import run_episode
from mode_check import mode_check, rule_modes
from sailing import NOMINAL, RS, RS_NAMES, Task, load_manifest
from u_diagnostics import diagnose, diagnose_store, load_config, true_wind_angle
from viz.episode_store import EpisodeStore, EpisodeWriter
from viz.replay import ReplayMismatch, Trajectory, replay

DT = 0.05  # s per decision


def synthetic(
    twa_deg: np.ndarray, u: np.ndarray, outcome: str = "truncated"
) -> Trajectory:
    """A northerly (wind toward -y) at 5 m/s, so psi = TWA + 90 deg."""
    n = len(twa_deg)
    s = np.zeros((n, len(RS_NAMES)))
    s[:, RS["t"]] = np.arange(n) * DT
    s[:, RS["psi"]] = np.radians(np.asarray(twa_deg) + 90.0)
    s[:, RS["u"]] = u
    s[:, RS["wind_y"]] = -5.0
    meta = {"episode_id": "syn", "condition": {"id": "syn"}, "outcome": outcome}
    meta["reason"] = None
    return Trajectory(meta, s, np.zeros((n - 1, 2)), True)


def seg(a: float, b: float, seconds: float) -> np.ndarray:
    return np.linspace(a, b, round(seconds / DT), endpoint=False)


@pytest.fixture(scope="module")
def task():
    return Task.load("U")


def test_twa_matches_the_rust_convention():
    # sailgym-task `a_wind_from_the_port_bow_is_a_negative_true_wind_angle`.
    s = np.zeros((4, len(RS_NAMES)))
    s[:, [RS["wind_x"], RS["wind_y"]]] = [[0, -5], [0, 5], [-5, 0], [5, 0]]
    got = np.degrees(true_wind_angle(s))
    np.testing.assert_allclose(got[:3], [-90.0, 90.0, 0.0], atol=1e-12)
    assert abs(abs(got[3]) - 180.0) < 1e-12


def test_a_clean_tack(task):
    twa = np.r_[seg(-45, -45, 5), seg(-45, 45, 6), seg(45, 45, 5)]
    u = np.r_[seg(1.5, 1.5, 5), seg(1.5, 0.4, 3), seg(0.4, 1.5, 3), seg(1.5, 1.5, 5)]
    d = diagnose(synthetic(twa, u), task)
    assert (d["tacks"], d["failed_tacks"], d["gybes"], d["turns"]) == (1, 0, 0, 0)
    assert d["irons_s"] == 0.0 and d["sternway_s"] == 0.0
    (ev,) = d["events"]
    # Leaves 30 deg at 5 + 15/90*6 s; settled from 5 + 75/90*6 s (±rounding
    # at exactly 30 deg).
    assert ev["t0"] == pytest.approx(6.0, abs=2 * DT)
    assert ev["t1"] == pytest.approx(10.0, abs=2 * DT)


def test_a_failed_tack_falls_back_and_is_not_counted(task):
    twa = np.r_[seg(-45, -45, 5), seg(-45, 10, 3), seg(10, 10, 4), seg(10, -45, 4),
                seg(-45, -45, 5)]  # fmt: skip
    u = np.r_[seg(1.5, 1.5, 5), seg(1.5, 0.0, 3), seg(-0.5, -0.5, 4),
              seg(0.0, 1.5, 4), seg(1.5, 1.5, 5)]  # fmt: skip
    d = diagnose(synthetic(twa, u), task)
    assert (d["tacks"], d["failed_tacks"]) == (0, 1)
    assert d["sternway_s"] == pytest.approx(4.0, abs=DT)
    assert d["irons_s"] >= 4.0


def test_sitting_in_irons(task):
    twa = np.r_[seg(-45, -45, 3), seg(-10, -10, 20)]
    u = np.r_[seg(1.0, 1.0, 3), seg(-0.3, -0.3, 20)]
    d = diagnose(synthetic(twa, u), task)
    assert (d["tacks"], d["failed_tacks"], d["gybes"]) == (0, 0, 0)
    # The last state holds for no decision, so 20 s of states give 20 s - dt.
    assert d["irons_s"] == pytest.approx(20.0 - DT)
    assert d["irons_longest_s"] == d["irons_s"] == d["sternway_s"]


def test_a_tack_unsettled_at_timeout_is_not_counted_but_a_finish_settles_it(task):
    twa = np.r_[seg(-45, -45, 5), seg(-45, 15, 5)]
    u = np.r_[seg(1.5, 1.5, 5), seg(1.5, 0.4, 5)]
    d = diagnose(synthetic(twa, u, "truncated"), task)
    assert d["tacks"] == 0 and d["tack_unsettled_at_end"] and d["timeout"]
    d = diagnose(synthetic(twa, u, "finished"), task)
    assert d["tacks"] == 1 and not d["tack_unsettled_at_end"]


def test_a_gybe_and_a_full_turn(task):
    gybe = np.r_[seg(-150, -150, 5), seg(-150, -210, 3), seg(150, 150, 5)]
    d = diagnose(synthetic(gybe, np.ones(len(gybe))), task)
    assert (d["tacks"], d["gybes"], d["failed_tacks"]) == (0, 1, 0)
    # Round through head to wind and the stern without settling on the way.
    turn = np.r_[seg(-45, -45, 5), seg(-45, 315, 2), seg(-45, -45, 5)]
    d = diagnose(synthetic(turn, np.ones(len(turn))), task)
    assert (d["tacks"], d["gybes"], d["failed_tacks"], d["turns"]) == (0, 0, 0, 1)


def test_a_swing_shorter_than_the_settle_hold_is_a_failed_tack(task):
    hold = load_config()["settle_s"]
    twa = np.r_[seg(-45, -45, 5), seg(-45, 40, 2), seg(40, 40, hold / 2),
                seg(40, -45, 2), seg(-45, -45, 5)]  # fmt: skip
    d = diagnose(synthetic(twa, np.ones(len(twa))), task)
    assert (d["tacks"], d["failed_tacks"]) == (0, 1)


def test_pose_only_trajectories_are_refused(task):
    tr = synthetic(seg(-45, -45, 1), np.ones(20))
    tr.exact = False
    with pytest.raises(ValueError):
        diagnose(tr, task)


# ---------------------------------------------------------------------------
# Real RuleSailor U episodes, recorded here and replayed exactly
# ---------------------------------------------------------------------------


@pytest.fixture(scope="module")
def store(tmp_path_factory, task):
    d = tmp_path_factory.mktemp("u_store")
    with EpisodeWriter(d, {"purpose": "test"}) as w:
        for cond in (NOMINAL, load_manifest("validation")[4]):
            run_episode(task, cond, RuleSailorController(), writer=w)
    return EpisodeStore(d)


def test_rule_sailor_nominal_by_hand(store, task):
    """Hand check: the boat starts on port, is close-hauled on starboard
    mid-episode and finishes on port, never near dead downwind: two tacks."""
    e = store.query(condition="nominal")[0]
    tr = replay(store.load(e), store.identity, task)
    twa = np.degrees(true_wind_angle(tr.states))
    at = lambda s: twa[np.searchsorted(tr.t, s)]  # noqa: E731
    assert at(0) < -80 and at(10) < -40 and at(35) > 40 and twa[-1] < -30
    assert np.abs(twa[tr.t > 5]).max() < 90
    d = diagnose(tr, task)
    assert e["outcome"] == "finished"
    assert (d["tacks"], d["failed_tacks"], d["gybes"]) == (2, 0, 0)
    assert d["irons_s"] == 0.0 and d["final_distance_m"] < task.half_width
    t1, t2 = d["events"]
    assert 15 < t1["t0"] < t1["t1"] < 35 < t2["t0"] < t2["t1"] <= tr.t[-1]


def test_rule_sailor_validation_004_fails_its_first_tack(store, task):
    """Its first tack crosses head to wind, falls back and drifts in irons."""
    e = store.query(condition="validation-004")[0]
    d = diagnose(replay(store.load(e), store.identity, task), task)
    assert d["timeout"] and d["events"][0]["type"] == "failed_tack"
    assert (d["tacks"], d["failed_tacks"]) == (1, 1)
    assert d["irons_longest_s"] > 15 and d["sternway_s"] > 15
    assert d["final_distance_m"] > 40


def test_diagnose_store_matches_direct_diagnosis(store, task):
    got = diagnose_store(store.dir, jobs=2)
    for e in store.index:
        tr = replay(store.load(e), store.identity, task)
        assert got[e["episode_id"]] == diagnose(tr, task)


def test_mode_trace_regenerates_the_stored_actions(store):
    e = store.query(condition="nominal")[0]
    states, modes = rule_modes(store, e)
    assert len(modes) == len(states) == e["decisions"]
    assert modes.count("tacking") > 0 and "settling" in modes
    with pytest.raises(ReplayMismatch):
        rule_modes(store, dict(e, final_checksum="0" * 16))


def test_mode_check_runs_end_to_end(store):
    res = mode_check([store.dir], test_frac=0.5, steps=20)
    assert len(res["episodes"]["train"]) == len(res["episodes"]["test"]) == 1
    # The majority class (beating) is perfectly recalled and every other is not.
    maj = res["majority"]["balanced_accuracy"]
    assert maj == pytest.approx(1 / len(res["scored_modes"]))
    for k in ("linear", "mlp_2x64"):
        assert 0.0 <= res[k]["test"]["balanced_accuracy"] <= 1.0
    assert math.isclose(sum(res["mode_share_test"].values()), 1.0)
