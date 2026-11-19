from types import SimpleNamespace

import numpy as np
import pytest
from m6_mirror import attempt, reflect
from sailing import Condition, Task
from viz.episode_store import EpisodeStore, EpisodeWriter


def test_reflection_is_involutive_on_geometry_and_wind():
    c = Condition("src", 5, -7, 288)
    m, xy = reflect(c, [3, 38])
    assert m.heading_deg == 72 and m.wind_bearing_deg == 7 and xy == [-3, 38]
    again, xy = reflect(m, xy)
    assert again.key() == c.key() and xy == [3, 38]


def test_unfinished_reflection_costs_one_step_and_supplies_no_buffer_rows(tmp_path):
    c = Condition("src", 5, 3, 42)
    u = np.array([[0.25, -0.2]], np.float32)
    source = SimpleNamespace(
        actions=np.tanh(u),
        meta=dict(
            kind="training",
            outcome="finished",
            condition=c.to_dict(),
            seed=1,
            m5_start_xy=[2, 38],
            final_checksum="source",
        ),
    )
    with EpisodeWriter(tmp_path / "store") as w:
        assert attempt(Task.load("U"), source, u, {}, w, set()) is None
        source.meta["kind"] = "promotion"
        with pytest.raises(AssertionError):
            attempt(Task.load("U"), source, u, {}, w, set())
    s = EpisodeStore(tmp_path / "store")
    assert len(s.index) == 1
    log = s.load(s.index[0])
    assert log.meta["outcome"] == "running" and log.meta["decisions"] == 1
    np.testing.assert_array_equal(log.actions, source.actions * [-1, 1])
