from m6 import FINAL, paired, panel
from m6_train import Curriculum


def test_probe_geometry_and_training_split():
    rows = panel(0, 100001, 10)
    assert sum(r["xy"][0] > 0 for r in rows) == 10
    assert all(37 <= r["xy"][1] <= 39 for r in rows)
    assert all(r["xy"] == [0, 0] for r in panel(FINAL, 100002, 0))
    tracks = {"-1": {"angle": -46}, "1": {"angle": 46}}
    pairs = paired(1, tracks, 10)
    assert len(pairs) == 20
    assert sum(r["board"] == 1 for r in pairs) == 10
    for direct, switch in zip(pairs[::2], pairs[1::2], strict=True):
        assert direct["condition"] == switch["condition"]
        assert direct["xy"][1] == switch["xy"][1]
        assert direct["seed"] == switch["seed"]
        assert abs(direct["projected_gate_x"]) <= 4
        assert switch["board"] * switch["projected_gate_x"] >= 6
        assert switch["board"] * switch["opposite_gate_x"] <= 4


def test_promotion_requires_two_consecutive_probes_and_stage_one_success(tmp_path):
    run = Curriculum(
        tmp_path,
        tmp_path,
        {
            "widths": [10] * 6,
            "controls": [{"pink": {"attempts": 100, "finishes": 5}}] * 6,
            "paired": [[]] * 6,
        },
        100,
        "ppo",
        None,
        None,
        {"config": {"calibration_decisions": 10000}},
    )
    wins = iter([6, 5, 6, 6, 0, 1])
    run.probe = lambda kind, *_: {"finishes": next(wins) if kind == "promotion" else 0}
    for _ in range(99):
        run.ended(0, 10)
    assert run.stage == 0
    run.ended(0, 10)
    assert run.stage == 1
    for _ in range(25):
        run.ended(0, 10)
        run.ended(1, 10)
    assert not run.qualified
    for _ in range(25):
        run.ended(1, 10)
    assert run.qualified
