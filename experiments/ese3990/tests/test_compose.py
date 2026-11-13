"""Saved selectors and reproducible first frames for the M4 compositor."""

import numpy as np
import pytest
from baselines import RuleSailorController
from evaluate import run_episode
from PIL import Image
from sailing import NOMINAL, Task
from viz.compose import compose, trajectories
from viz.episode_store import EpisodeWriter


def test_fixed_spec_has_identical_frame_count_and_first_frame(tmp_path):
    with EpisodeWriter(tmp_path / "store") as writer:
        run_episode(Task.load("R"), NOMINAL, RuleSailorController(), writer=writer)
    spec = {
        "id": "fixed",
        "title": "Replay",
        "fps": 2,
        "speed": 10,
        "episodes": [
            {
                "store": str(tmp_path / "store"),
                "query": {"condition": "nominal"},
                "label": "RuleSailor",
            }
        ],
    }
    a = compose(spec, tmp_path / "a", formats=("gif",))
    b = compose(spec, tmp_path / "b", formats=("gif",))
    assert a["frames"] == b["frames"] and a["frames"]["gif"] > 1
    with (
        Image.open(tmp_path / "a/fixed.gif") as left,
        Image.open(tmp_path / "b/fixed.gif") as right,
    ):
        assert left.n_frames == right.n_frames == a["frames"]["gif"]
        np.testing.assert_array_equal(
            np.asarray(left.convert("RGB")), np.asarray(right.convert("RGB"))
        )
    spec["episodes"][0]["query"] = {"condition": "absent"}
    with pytest.raises(ValueError, match="must match one episode"):
        trajectories(spec)
