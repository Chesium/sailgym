"""Outcome denominators, costs and deterministic representative selection."""

from comparison_report import aggregate, video_specs
from sailing import REPO


def test_summary_keeps_failures_out_of_conditional_completion_time():
    common = {
        "peak_heel_deg": 20.0,
        "rudder_travel_rad_decision_sampled": 1.0,
        "sheet_travel_m_decision_sampled": 2.0,
        "decisions": 1,
        "physics_steps": 3,
        "wall_s": 0.2,
    }
    episodes = [
        {
            **common,
            "outcome": "finished",
            "reason": None,
            "finish_time_s": 10.0,
            "decision_latency_s": [0.05],
            "planning": {"hypothetical_physics_steps": 23},
        },
        {
            **common,
            "outcome": "truncated",
            "reason": None,
            "finish_time_s": None,
            "decision_latency_s": [0.09],
        },
    ]
    row = aggregate(episodes)
    assert row["success_rate"] == 0.5 and row["outcome_counts"] == {
        "finished": 1,
        "truncated": 1,
    }
    assert row["finish_time_s_given_success"]["mean"] == 10.0
    assert row["deadline_misses_50ms"] == 1
    assert row["hypothetical_physics_steps"] == 23 and row["physics_steps"] == 6


def test_failure_specs_use_first_manifest_id_without_reselecting_policy():
    groups = [
        {
            "id": m,
            "controller": m,
            "training_seed": 0,
            "roles": ["selected", "reference"],
        }
        for m in ("ppo", "sac", "mpc", "rule_sailor")
    ]
    episodes = [
        {
            "group": "ppo",
            "manifest": "shift",
            "condition": {"id": c},
            "outcome": "truncated",
            "reason": None,
        }
        for c in ("shift-009", "shift-002")
    ]
    specs, failures = video_specs(REPO / "runs/example", {"groups": groups}, episodes)
    assert failures["ppo"]["condition"]["id"] == "shift-002"
    assert failures["mpc"] is None
    failure = next(s for s in specs if s["id"] == "ppo_first_failure")
    assert failure["episodes"][0]["query"] == {"group": "ppo", "condition": "shift-002"}
