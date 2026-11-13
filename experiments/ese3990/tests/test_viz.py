"""Episode store, replay and rendering (plan §10.4)."""

from __future__ import annotations

import json

import numpy as np
import pytest
from baselines import RuleSailorController, SmoothRandom
from evaluate import run_episode
from sailing import NOMINAL, RS, Task, load_manifest
from viz.episode_store import EpisodeStore, EpisodeWriter
from viz.render import animate, frame_times, plot_static
from viz.replay import ReplayMismatch, pose_only, replay


class F32Random(SmoothRandom):
    """A float32-native controller, standing in for a JAX policy."""

    action_dtype = "float32"

    def act(self, env):
        return super().act(env).astype(np.float32)


@pytest.fixture(scope="module")
def store(tmp_path_factory):
    d = tmp_path_factory.mktemp("store")
    task = Task.load("R")
    conds = [NOMINAL, *load_manifest("validation")[:2]]
    reports = []
    with EpisodeWriter(d, {"purpose": "test"}, shard_size=2) as w:
        for c in conds:
            reports.append(
                run_episode(
                    task, c, RuleSailorController(), writer=w, log_meta={"kind": "t"}
                )
            )
        reports.append(
            run_episode(
                task, NOMINAL, F32Random(3), seed=3, writer=w, log_meta={"kind": "t"}
            )
        )
    return d, reports


def test_index_and_shards(store):
    d, reports = store
    s = EpisodeStore(d)
    assert len(s.index) == 4
    assert len(list(d.glob("shard-*.npz"))) == 2  # shard_size = 2
    assert [e["episode_id"] for e in s.index] == [r["episode_id"] for r in reports]
    assert len(s.query(controller="rule_sailor")) == 3
    assert len(s.query(condition="nominal")) == 2
    assert s.query(action_dtype="float32")[0]["controller"] == "smooth_random"
    assert s.identity["model"]["model_version"] >= 1


def test_every_stored_episode_replays_exactly(store):
    d, reports = store
    s = EpisodeStore(d)
    for e, r in zip(s.index, reports, strict=True):
        tr = replay(s.load(e), s.identity)
        assert tr.exact
        assert tr.meta["outcome"] == r["outcome"]
        assert tr.meta["return"] == r["return"]
        assert len(tr.states) == r["decisions"] + 1
        assert tr.meta["final_checksum"] == e["final_checksum"]


def test_f32_actions_are_stored_as_f32_and_lossless(store):
    d, _ = store
    s = EpisodeStore(d)
    (e,) = s.query(action_dtype="float32")
    log = s.load(e)
    assert log.actions.dtype == np.float32
    replay(log, s.identity)  # exact, or it raises


def test_float64_actions_cannot_be_logged_as_float32():
    class Bad(SmoothRandom):
        action_dtype = "float32"

    with EpisodeWriter_tmp() as w, pytest.raises(ValueError, match="not representable"):
        run_episode(Task.load("R"), NOMINAL, Bad(0), writer=w)


class EpisodeWriter_tmp:  # noqa: N801 - a tiny context helper
    def __enter__(self):
        import tempfile

        self._d = tempfile.TemporaryDirectory()
        return EpisodeWriter(self._d.name)

    def __exit__(self, *exc):
        self._d.cleanup()


def test_replay_refuses_another_build_or_contract(store):
    d, _ = store
    s = EpisodeStore(d)
    log = s.load(s.index[0])
    for key, value in [
        ("model", {"model_version": 0, "source": {"tree": "x", "state": "clean"}}),
        ("tasks_json_sha256", "0" * 64),
    ]:
        ident = dict(s.identity, **{key: value})
        with pytest.raises(ReplayMismatch, match=key):
            replay(log, ident)


def test_replay_refuses_a_tampered_log(store):
    d, _ = store
    s = EpisodeStore(d)
    log = s.load(s.index[0])
    log.actions = log.actions.copy()
    log.actions[5, 0] = -log.actions[5, 0] if log.actions[5, 0] else 0.5
    with pytest.raises(ReplayMismatch, match="diverged|ended before"):
        replay(log, s.identity)


def test_a_writer_refuses_to_mix_builds(store, tmp_path):
    d, _ = store
    head = json.loads((d / "run.json").read_text())
    other = tmp_path / "other"
    other.mkdir()
    head["identity"]["model"]["source"]["tree"] = "f" * 40
    (other / "run.json").write_text(json.dumps(head))
    with pytest.raises(ValueError, match="different build"):
        EpisodeWriter(other)


def test_pose_fallback_matches_the_exact_replay_at_its_samples(store):
    d, _ = store
    s = EpisodeStore(d)
    log = s.load(s.index[0])
    exact = replay(log, s.identity)
    coarse = pose_only(log)
    assert not coarse.exact
    every = log.meta["pose_every_decisions"]
    for j in range(len(coarse.t) - 1):
        k = j * every
        assert coarse.col("x")[j] == pytest.approx(exact.col("x")[k], abs=1e-4)
        assert coarse.t[j] == pytest.approx(exact.t[k])
    assert coarse.t[-1] == pytest.approx(exact.t[-1])
    assert coarse.col("y")[-1] == pytest.approx(exact.col("y")[-1], abs=1e-4)


def test_render_static_and_animation_are_deterministic(store, tmp_path):
    d, _ = store
    s = EpisodeStore(d)
    task = Task.load("R")
    trs = [replay(s.load(e), s.identity) for e in s.index[:2]]
    plot_static(trs, task, tmp_path / "static.png")
    assert (tmp_path / "static.png").stat().st_size > 10_000
    n = animate(trs, task, tmp_path / "a.gif", fps=5, speed=4.0)
    assert n == len(frame_times(trs, 5, 4.0, 1.0))
    from PIL import Image

    frames = []
    for name in ("a.gif", "b.gif"):
        if name == "b.gif":
            animate(trs, task, tmp_path / name, fps=5, speed=4.0)
        with Image.open(tmp_path / name) as im:
            assert im.n_frames == n
            im.seek(0)
            frames.append(np.asarray(im.convert("RGB")))
    np.testing.assert_array_equal(frames[0], frames[1])
    with pytest.raises(ValueError, match="exact"):
        animate([pose_only(s.load(s.index[0]))], task, tmp_path / "c.gif")
    assert trs[0].col("x")[-1] == trs[0].states[-1, RS["x"]]
