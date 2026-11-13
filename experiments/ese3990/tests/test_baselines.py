"""M0 baseline checks: restricted RuleSailor and the condition manifests."""

from __future__ import annotations

import json

from evaluate import evaluate
from make_conditions import TRAIN_RANGE, TrainingSampler, check, held_out_keys
from sailing import load_manifest


def test_manifests_match_generator_and_are_disjoint():
    assert check() == []
    keys = [c.key() for s in ("validation", "test", "shift") for c in load_manifest(s)]
    assert len(keys) == len(set(keys)) == 90


def test_held_out_splits_respect_their_ranges():
    lo_s, hi_s = TRAIN_RANGE["wind_speed"]
    for split in ("validation", "test"):
        for c in load_manifest(split):
            assert lo_s <= c.wind_speed <= hi_s
            assert abs(c.wind_bearing_deg) <= 5.0
            assert abs(c.heading_deg - 90.0) <= 15.0
    for c in load_manifest("shift"):
        assert c.wind_speed in (4.0, 6.0)
        assert 15.0 <= abs(c.heading_deg - 90.0) <= 30.0


def test_training_sampler_is_deterministic_and_excludes_held_out():
    a = [TrainingSampler(0).next().key() for _ in range(1)]
    s0, s0b, s1 = TrainingSampler(0), TrainingSampler(0), TrainingSampler(1)
    xs = [s0.next().key() for _ in range(500)]
    assert xs == [s0b.next().key() for _ in range(500)]
    assert xs[0] == a[0]
    assert xs != [s1.next().key() for _ in range(500)]
    assert not set(xs) & held_out_keys()


def test_restricted_rule_sailor_finishes_nominal_tasks():
    for task in ("R", "U"):
        rep = evaluate(task, "rule_sailor", "nominal")
        s = rep["summary"]
        assert s["finished"] == 1, (task, s)
        assert rep["meta"]["controller"]["release"] == "forced off"
        json.dumps(rep)  # the report is serialisable as written


def test_restricted_rule_sailor_finishes_every_validation_condition_on_r():
    s = evaluate("R", "rule_sailor", "validation")["summary"]
    assert s["finished"] == s["episodes"] == 20
