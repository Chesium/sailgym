"""M5 follow-on ordering, freeze-before-heldout, action holds and safe resume."""

import fcntl
import json
from dataclasses import replace
from pathlib import Path

import jax
import m5_compare as comparison
import m5_followup as followup
import numpy as np
import ppo
import pytest
from sailing import FEATURE_NAMES, NOMINAL, Task, load_manifest, run_metadata
from viz.episode_store import EpisodeStore, EpisodeWriter
from viz.replay import replay


@pytest.fixture
def heldout(tmp_path, monkeypatch):
    root = tmp_path / "M5"
    run = root / "main/ppo_B_s0"
    run.mkdir(parents=True)
    task = Task.load("U")
    tiny = replace(task, max_steps=230)
    cfg = dict(
        task="U",
        arm="ppo",
        decisions=30,
        checkpoints=[0, 20, 30],
        hidden=8,
        log_std=-0.5,
        norm_clip=5.0,
        m5={"hold": 10},
    )
    meta = run_metadata(task, learner="ppo", training_seed=0, config=cfg)
    (run / "config.json").write_text(json.dumps(meta))
    (run / "summary.json").write_text(json.dumps({"decisions": 30}))
    (run / "metrics.jsonl").write_text('{"decisions":30,"loss":0.0}\n')
    with EpisodeWriter(run / "store"):
        pass
    (run / "ckpt").mkdir()
    curve = []
    obs_dim = len(FEATURE_NAMES)
    for d, successes, ret in ((10, 1, 2.0), (20, 0, 9.0), (30, 0, 3.0)):
        params = ppo.init(jax.random.PRNGKey(d), obs_dim, 2, 8)
        saved = {
            **meta,
            "decisions": d,
            "obs_dim": obs_dim,
            "normalization": {"mean": [0.0] * obs_dim, "std": [1.0] * obs_dim},
        }
        ppo.save(run / "ckpt" / f"{d:07d}", params, saved)
        curve.append(dict(decisions=d, val_finished=successes, val_mean_return=ret))
    (run / "curve.jsonl").write_text("".join(json.dumps(r) + "\n" for r in curve))
    for name in ("main_audit.json", "extended_audit.json", "followup_plan.json"):
        (root / name).write_text("{}")
    train_freeze = root / "training_freeze.json"
    train_freeze.write_text("{}")
    frozen = dict(configs={"ppo_B": cfg}, reuse=[], q_feasible_band=[-22.3, 15.7])
    spec = dict(
        out=run,
        learner="ppo",
        cell="B",
        seed=0,
        budget=30,
        stage="main",
        id="main_ppo_B_s0",
    )
    monkeypatch.setattr(comparison, "FREEZE", train_freeze)
    monkeypatch.setattr(comparison, "verify_freeze", lambda: frozen)
    monkeypatch.setattr(comparison, "all_runs", lambda r, f: [spec])
    conds = {
        "nominal": [NOMINAL],
        "test": load_manifest("validation")[:1],
        "shift": load_manifest("validation")[1:2],
    }
    return dict(
        root=root,
        run=run,
        task=tiny,
        conditions=conds,
        spec=spec,
        frozen=frozen,
        out=root / "heldout",
    )


def test_only_validation_selected_and_final_are_frozen(heldout):
    frozen = comparison.prepare(heldout["root"])
    assert [g["decisions"] for g in frozen["groups"]] == [10, 30]
    assert [g["roles"] for g in frozen["groups"]] == [["selected"], ["final"]]
    assert all(g["hold"] == 10 for g in frozen["groups"])
    assert not any("0000020.npz" in p for p in frozen["input_hashes"])
    # The intermediate checkpoint's higher return cannot beat a validation finish.
    assert frozen["groups"][0]["checkpoint"].endswith("0000010")


def test_same_selected_final_checkpoint_has_both_roles_once(heldout):
    path = heldout["run"] / "curve.jsonl"
    rows = comparison.read_rows(path)
    rows[-1]["val_finished"] = 2
    path.write_text("".join(json.dumps(r) + "\n" for r in rows))
    frozen = comparison.prepare(heldout["root"])
    assert len(frozen["groups"]) == 1
    assert frozen["groups"][0]["roles"] == ["selected", "final"]


def test_freeze_precedes_loading_test_conditions_and_policy_queries(
    heldout, monkeypatch
):
    out = heldout["out"]
    frozen = comparison.prepare(heldout["root"])
    original = comparison.run_episode

    def manifest(name):
        assert json.loads((out / "freeze.json").read_text()) == frozen
        return heldout["conditions"][name]

    def run(*args, **kwargs):
        assert json.loads((out / "freeze.json").read_text()) == frozen
        return original(*args, **kwargs)

    monkeypatch.setattr(comparison, "load_manifest", manifest)
    monkeypatch.setattr(comparison, "run_episode", run)
    result = comparison.evaluate(out, frozen, 1, heldout["task"])
    assert result["groups"] == 2 and result["episodes"] == 6


def test_held_actions_replay_and_resume_uses_store_after_lost_json(heldout):
    out, task = heldout["out"], heldout["task"]
    frozen = comparison.prepare(heldout["root"])
    comparison.evaluate(out, frozen, 1, task, heldout["conditions"])
    for group in frozen["groups"]:
        store = EpisodeStore(out / "store" / group["id"])
        for entry in store.index:
            log = store.load(entry)
            assert replay(log, store.identity, task).exact
            for lo in range(0, len(log.actions), 10):
                assert np.all(log.actions[lo : lo + 10] == log.actions[lo])
        # Crash recovery: stored episodes remain authoritative if this file was lost.
        (out / "episodes" / f"{group['id']}.jsonl").unlink()
    comparison.evaluate(out, frozen, 1, task, heldout["conditions"])
    assert all(
        len(EpisodeStore(out / "store" / g["id"]).index) == 3 for g in frozen["groups"]
    )
    audit = comparison.audit(out, 1, task)
    assert audit["exact_replays"] == audit["episodes"] == 6
    assert len(audit["rows"]) == 6
    assert all("mean_rudder_travel_rad_decision_sampled" in r for r in audit["rows"])


def test_parallel_heldout_matches_serial(heldout):
    frozen = comparison.prepare(heldout["root"])
    serial = heldout["root"] / "serial"
    parallel = heldout["root"] / "parallel"
    for out, workers in ((serial, 1), (parallel, 2)):
        comparison.evaluate(
            out, frozen, workers, heldout["task"], heldout["conditions"]
        )
    for group in frozen["groups"]:
        a = EpisodeStore(serial / "store" / group["id"])
        b = EpisodeStore(parallel / "store" / group["id"])
        assert [e["final_checksum"] for e in a.index] == [
            e["final_checksum"] for e in b.index
        ]
        for x, y in zip(a.index, b.index, strict=True):
            np.testing.assert_array_equal(a.load(x).actions, b.load(y).actions)


def test_changed_checkpoint_refused_before_evaluation(heldout):
    frozen = comparison.prepare(heldout["root"])
    path = heldout["run"] / "ckpt/0000010.npz"
    path.write_bytes(path.read_bytes() + b"changed")
    with pytest.raises(ValueError, match="held-out input changed"):
        comparison.evaluate(
            heldout["out"], frozen, 1, heldout["task"], heldout["conditions"]
        )
    assert not (heldout["out"] / "store").exists()


def test_all_runs_include_reused_baselines_and_separate_extended_budgets():
    reused = [
        dict(learner=learner, seed=s, run=f"runs/old/{learner}{s}")
        for learner in ("ppo", "sac")
        for s in range(3)
    ]
    specs = comparison.all_runs(Path("/tmp/m5-count-only"), {"reuse": reused})
    assert len(specs) == 50 and len({s["id"] for s in specs}) == 50
    assert sum(s["stage"] == "main" for s in specs) == 40
    assert sum(s["budget"] == 40_000_000 for s in specs) == 10


def test_wait_respects_existing_queue_lock_and_rejects_failed_main(
    tmp_path, monkeypatch
):
    run = tmp_path / "main/one"
    run.mkdir(parents=True)
    monkeypatch.setattr(followup.m5_study, "jobs", lambda s, r: [{"out": run}])
    with (tmp_path / "queue.lock").open("a") as existing:
        fcntl.flock(existing, fcntl.LOCK_EX)
        calls = []

        def finish(_):
            calls.append("waited")
            (run / "summary.json").write_text("{}")
            fcntl.flock(existing, fcntl.LOCK_UN)

        monkeypatch.setattr(followup.time, "sleep", finish)
        followup.wait_for_main(tmp_path)
        assert calls == ["waited"]
    (run / "summary.json").unlink()
    with pytest.raises(RuntimeError, match="incomplete runs"):
        followup.wait_for_main(tmp_path)


@pytest.mark.parametrize("fail", [False, True])
def test_pipeline_order_and_failure_stop(tmp_path, monkeypatch, fail):
    monkeypatch.setattr(
        followup, "protocol", lambda r, w: {"stages": list(followup.STEPS)}
    )
    called = []
    monkeypatch.setattr(followup, "wait_for_main", lambda r: called.append("wait_main"))
    monkeypatch.setattr(
        followup, "commands", lambda r, w: {s: [s] for s in followup.STEPS}
    )

    class Process:
        pid = 999999999

        def __init__(self, cmd, **kwargs):
            called.append(cmd[0])
            self.code = 1 if fail and cmd[0] == "extended" else 0

        def wait(self):
            return self.code

        def poll(self):
            return self.code

    monkeypatch.setattr(followup.subprocess, "Popen", Process)
    if fail:
        with pytest.raises(RuntimeError, match="stage extended failed"):
            followup.run_pipeline(tmp_path, 1)
        assert called == ["wait_main", "extended"]
        assert (
            json.loads((tmp_path / "followup.json").read_text())["status"] == "failed"
        )
    else:
        result = followup.run_pipeline(tmp_path, 1)
        assert called == list(followup.STEPS)
        assert result["status"] == "complete"


def test_audit_cache_invalidates_on_changed_inputs(heldout, monkeypatch):
    monkeypatch.setattr(followup, "validate_run", comparison.validate_run)
    calls = []

    def checked(run):
        calls.append(run)
        return dict(selected={"decisions": 10}, exact_replays=0)

    monkeypatch.setattr(followup.m5_study, "audit_run", checked)
    args = (heldout["root"], heldout["spec"], heldout["frozen"], {"test": "source"})
    followup.audit_cached(args)
    followup.audit_cached(args)
    assert len(calls) == 1
    with (heldout["run"] / "metrics.jsonl").open("a") as f:
        f.write('{"decisions":30,"loss":1.0}\n')
    followup.audit_cached(args)
    assert len(calls) == 2


def test_duplicate_followup_and_changed_protocol_are_refused(tmp_path, monkeypatch):
    monkeypatch.setattr(followup, "protocol", lambda r, w: {"workers": w})
    followup.seal_protocol(tmp_path, {"workers": 1})
    with pytest.raises(ValueError, match="sources/settings changed"):
        followup.seal_protocol(tmp_path, {"workers": 2})
    with (tmp_path / "followup.lock").open("a") as existing:
        fcntl.flock(existing, fcntl.LOCK_EX)
        with pytest.raises(BlockingIOError):
            followup.run_pipeline(tmp_path, 1)
