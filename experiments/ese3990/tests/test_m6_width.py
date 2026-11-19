import numpy as np
from m6_width import FirstBand, fixed_panels


def test_fixed_width_panels_match_and_transfer_headings_fetch():
    tracks = {"-1": {"angle": -53}, "1": {"angle": 53}}
    p = fixed_panels(tracks)
    assert sum(abs(r["xy"][0]) < 5 for r in p["broad"]) == 40
    for i, (b, n, h) in enumerate(
        zip(p["broad"], p["narrow"], p["heading"], strict=True)
    ):
        assert b["seed"] == n["seed"] and b["condition"] == n["condition"]
        assert n["xy"] == [0.3 * b["xy"][0], b["xy"][1]] == h["xy"]
        if i % 2:
            assert b["xy"][0] == -p["broad"][i - 1]["xy"][0]
        c = h["condition"]
        side = (c["heading_deg"] - c["wind_bearing_deg"] + 180) % 360 - 180
        assert np.isclose(abs(side), 45)
        course = np.deg2rad(c["wind_bearing_deg"] + np.sign(side) * 53)
        assert abs(h["xy"][0] + (40 - h["xy"][1]) * np.tan(course)) <= 5


def test_first_band_freezes_first_passing_snapshot(tmp_path):
    run = FirstBand(
        tmp_path,
        tmp_path,
        {
            "arm": "narrow",
            "tracks": {},
            "controls": [{"pink": {"finishes": 30, "attempts": 100}}],
        },
        1,
        "ppo",
        None,
        None,
        {"config": {"calibration_decisions": 10000}},
    )
    wins = iter([7, 6, 7, 7, 8])
    run.probe = lambda *_: {"finishes": next(wins)}
    for _ in range(125):
        run.ended(0, 10)
    import json

    assert json.loads((tmp_path / "qualifying.json").read_text())["decisions"] == 11000
    assert run.qualified and run.stage == 0
