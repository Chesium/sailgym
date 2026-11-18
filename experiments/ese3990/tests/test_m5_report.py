"""M5 results summary: Q band, frozen threshold and declared readings."""

import json

import m5_report as report

BAND = (-22.3, 15.7)
THRESHOLD = dict(seeds_finishing=3, seeds=5, val_finishes_per_seed=1, val_episodes=20)
READINGS = [
    ["B finishes, S does not", "interval"],
    ["S finishes, B does not", "discovery"],
    ["Only B + S finishes", "both"],
    ["No cell finishes", "second stage"],
    ["A PPO cell finishes only at 40M", "update-bound"],
    ["One learner finishes where the other does not", "report"],
]


def test_q_band_uses_min_max_or_abs_max():
    full = dict(q_min={"min": -11.0}, q_max={"max": 10.9}, q_abs_max={"max": 11.0})
    assert report.in_band(full, *BAND) is True
    assert report.in_band({**full, "q_max": {"max": 16.0}}, *BAND) is False
    assert report.in_band({"q_abs_max": {"max": 15.0}}, *BAND) is True
    assert report.in_band({"q_abs_max": {"max": 20.0}}, *BAND) is None
    assert report.in_band({"q_abs_max": {"max": 4166.0}}, *BAND) is False


def seed(finished):
    view = dict(
        episodes=20,
        finished=finished,
        tack_episodes=0,
        failures=0,
        mean_max_y_m=0.0,
        mean_rudder_travel_rad=1.0,
    )
    heldout = {m: dict(view) for m in ("nominal", "test", "shift")}
    return dict(
        learner="ppo",
        wall_s=60.0,
        selected_val_mean_return=-5.0,
        selected_is_calibration=False,
        training=dict(episodes=1, finished=0),
        validation={"selected": view},
        heldout={"selected": heldout},
    )


def test_cell_finishes_needs_three_of_five_seeds():
    assert report.cell_summary([seed(1)] * 3 + [seed(0)] * 2, THRESHOLD)["finishes"]
    two = report.cell_summary([seed(5)] * 2 + [seed(0)] * 3, THRESHOLD)
    assert two["seeds_finishing"] == 2 and not two["finishes"]


def test_readings():
    cells = {
        f"{stage}/{learner}/{cell}": {"finishes": False}
        for stage in ("main", "extended")
        for learner in ("ppo", "sac")
        for cell in ("neither", "B", "S", "BS")
    }
    none = {
        r["observation"]: r["applies"]
        for r in report.readings(cells, dict(readings=READINGS))
    }
    assert none["No cell finishes"] == {"ppo": True, "sac": True}
    assert none["A PPO cell finishes only at 40M"] is False
    assert none["One learner finishes where the other does not"] is False

    cells["main/sac/B"]["finishes"] = True
    cells["extended/ppo/BS"]["finishes"] = True
    got = {
        r["observation"]: r["applies"]
        for r in report.readings(cells, dict(readings=READINGS))
    }
    assert got["B finishes, S does not"] == {"ppo": False, "sac": True}
    assert got["No cell finishes"] == {"ppo": True, "sac": False}
    assert got["A PPO cell finishes only at 40M"] is True
    assert got["One learner finishes where the other does not"] is True


def test_training_outcomes_split_random_starts(tmp_path):
    rows = [
        dict(kind="calibration", outcome="finished"),
        dict(kind="training", outcome="finished", m5_start_xy=[0.0, 0.0]),
        dict(kind="training", outcome="finished", m5_start_xy=[-4.5, 37.8]),
        dict(kind="training", outcome="truncated", m5_start_xy=[1.0, 30.0]),
        dict(kind="training", outcome="terminated"),
    ]
    (tmp_path / "store").mkdir()
    (tmp_path / "store/index.jsonl").write_text(
        "".join(json.dumps(r) + "\n" for r in rows)
    )
    assert report.training_outcomes(tmp_path) == dict(
        episodes=4, random_starts=2, finished=2, finished_random_start=1
    )
