"""M4U planner study and held-out driver: R unchanged, U settings/selection,
freeze-before-outcomes, refusal of changed inputs, resume and parallel workers."""

import json
import shutil
from dataclasses import asdict, replace
from math import ceil

import jax
import m4u_compare
import m4u_report
import mpc_figures
import mpc_study
import numpy as np
import ppo
import pytest
import sac
import u_diagnostics
from m4u_compare import prepare, run
from mpc import CEMConfig
from mpc_study import SETTINGS, STUDIES, load_episodes, report, select
from ppo import PPOPolicy
from sac import SACPolicy
from sailing import (
    FEATURE_NAMES,
    HERE,
    NOMINAL,
    REPO,
    Task,
    contract_identity,
    load_manifest,
    run_metadata,
)
from viz.episode_store import EpisodeStore, EpisodeWriter, build_identity

M3 = REPO / "runs/ese3990/mpc/M3"


def test_r_study_keeps_its_settings_freeze_path_and_selection(tmp_path):
    assert STUDIES["R"]["settings"] is SETTINGS and len(SETTINGS) == 5
    assert STUDIES["R"]["freeze"] == "M3_freeze.json"
    if not (M3 / "episodes.jsonl").exists():
        pytest.skip("the recorded M3 sweep is not on this machine")
    for name in ("study.json", "episodes.jsonl"):
        shutil.copy(M3 / name, tmp_path / name)
    result = report(tmp_path)
    assert result == json.loads((M3 / "summary.json").read_text())
    chosen = select(result["settings"])
    frozen = json.loads((HERE / "configs/M3_freeze.json").read_text())
    assert chosen["setting"] == frozen["setting"]
    assert chosen["config"] == frozen["config"]


def test_u_study_settings_selection_and_parallel_layout(tmp_path):
    u = STUDIES["U"]
    assert u["settings"] == {
        "H40_K32": CEMConfig(candidates=32),
        "H160_K32": CEMConfig(horizon=160, candidates=32),
    }
    m3 = json.loads((HERE / "configs/M3_freeze.json").read_text())["config"]
    for config in u["settings"].values():
        assert {k: v for k, v in asdict(config).items() if k != "horizon"} == {
            k: v for k, v in m3.items() if k != "horizon"
        }
        assert max(2, ceil(config.candidates / 8)) == 4  # elites, as in mpc.py
    assert u["labels"]["H160_K32"] == "offline, non-real-time"

    def row(setting, finished, p50):
        return {"setting": setting, "finished": finished, "latency_ms": {"p50": p50}}

    # More finishes wins even at four times the latency; ties go to lower p50.
    assert select([row("H40_K32", 50, 20), row("H160_K32", 55, 80)])["setting"] == (
        "H160_K32"
    )
    assert select([row("H40_K32", 55, 20), row("H160_K32", 55, 80)])["setting"] == (
        "H40_K32"
    )
    # Serial and per-process group files are read together on resume.
    (tmp_path / "groups/H160_K32_s0").mkdir(parents=True)
    (tmp_path / "episodes.jsonl").write_text('{"a": 1}\n')
    (tmp_path / "groups/H160_K32_s0/episodes.jsonl").write_text('{"b": 2}\n')
    assert load_episodes(tmp_path) == [{"a": 1}, {"b": 2}]


def test_u_study_groups_resume_then_freeze_probes_and_figures(tmp_path, monkeypatch):
    tiny = replace(Task.load("U"), max_steps=50)
    for module in (mpc_study, mpc_figures):
        monkeypatch.setattr(module, "study_task", lambda meta: tiny)
    monkeypatch.setattr(mpc_study, "load_manifest", lambda n: load_manifest(n)[:2])
    for name, horizon in (("H40_K32", 10), ("H160_K32", 20)):
        config = CEMConfig(horizon=horizon, candidates=2, iterations=1)
        monkeypatch.setitem(STUDIES["U"]["settings"], name, config)
    (tmp_path / "study.json").write_text(json.dumps({"task": "U"}))
    for name in STUDIES["U"]["settings"]:
        for seed in mpc_study.SEEDS:
            dest = tmp_path / "groups" / f"{name}_s{seed}"
            mpc_study.run_group(tmp_path, dest, name, seed)
            mpc_study.run_group(tmp_path, dest, name, seed)  # resume: no repeats
            assert len(EpisodeStore(dest / "episodes").index) == 2
    keys = [
        (e["setting"], e["search_seed"], e["condition"]["id"])
        for e in load_episodes(tmp_path)
    ]
    assert len(keys) == len(set(keys)) == 2 * 3 * 2
    mpc_study.probes(tmp_path)
    frozen = json.loads((tmp_path / "M4U_mpc_freeze.json").read_text())
    assert frozen["task"] == "U" and frozen["contract_version"] == 2
    assert frozen["label"] == STUDIES["U"]["labels"][frozen["setting"]]
    assert len(EpisodeStore(tmp_path / "probes").index) == 3  # nominal + 2
    mpc_figures.figures(tmp_path, tmp_path / "figs")
    assert (tmp_path / "figs/M4U_mpc_prediction.png").exists()


@pytest.fixture
def heldout(tmp_path, monkeypatch):
    """Two tiny synthetic learner runs (one expert-data arm per policy class),
    a tiny MPC freeze, a 10-decision U task and four conditions."""
    monkeypatch.setattr(m4u_compare, "ARMS", {"ppo_bc": "ppo", "sac_demo": "sac"})
    monkeypatch.setattr(m4u_compare, "SEEDS", (0,))
    task = Task.load("U")
    tiny = replace(task, max_steps=100)
    val = load_manifest("validation")
    conditions = {"nominal": [NOMINAL], "test": val[:2], "shift": val[2:3]}
    obs = len(FEATURE_NAMES)
    cfg = {
        "decisions": 30,
        "checkpoints": [0, 30],
        "calibration_decisions": 10,
        "hidden": 8,
        "log_std": -0.5,
        "norm_clip": 5.0,
    }
    for arm, learner in m4u_compare.ARMS.items():
        run_dir = tmp_path / "main" / f"{arm}_U_s0"
        run_dir.mkdir(parents=True)
        config = run_metadata(
            task, learner=learner, training_seed=0, config={**cfg, "arm": arm}
        )
        (run_dir / "config.json").write_text(json.dumps(config))
        (run_dir / "summary.json").write_text(json.dumps({"decisions": 30}))
        EpisodeWriter(run_dir / "store").close()  # run.json: this build
        lib, policy = (ppo, PPOPolicy) if learner == "ppo" else (sac, SACPolicy)
        for i, decisions in enumerate((20, 30)):
            path = run_dir / "ckpt" / f"{decisions:07d}"
            params = lib.init(jax.random.PRNGKey(decisions), obs, 2, 8)
            meta = {
                **config,
                "decisions": decisions,
                "obs_dim": obs,
                "normalization": {"mean": [0.0] * obs, "std": [1.0] * obs},
                "wall_s": float(decisions),
            }
            ppo.save(path, params, meta)
            nominal = m4u_compare.run_episode(tiny, NOMINAL, policy.load(path))
            with (run_dir / "curve.jsonl").open("a") as f:
                row = {
                    "decisions": decisions,
                    "val_finished": 0,
                    "val_mean_return": float(i),
                    "nominal_return": nominal["return"],
                }
                f.write(json.dumps(row) + "\n")
    paths = {
        "mpc": tmp_path / "mpc_freeze.json",
        "train": tmp_path / "train_freeze.json",
        "thresholds": tmp_path / "thresholds.json",
    }
    paths["mpc"].write_text(
        json.dumps(
            {
                "task": "U",
                **contract_identity(task),
                "identity": build_identity(),
                "setting": "tiny",
                "config": asdict(CEMConfig(horizon=10, candidates=4, iterations=1)),
                "label": "test fixture",
                "search_seeds": [0, 1],
            }
        )
    )
    configs = {arm: {**cfg, "arm": arm} for arm in m4u_compare.ARMS}
    paths["train"].write_text(json.dumps({"configs": configs}))
    paths["thresholds"].write_text('{"irons_s": 5}')

    def frozen():
        return prepare(
            paths["mpc"], paths["train"], paths["thresholds"], tmp_path / "main"
        )

    return {
        "frozen": frozen,
        "task": tiny,
        "conditions": conditions,
        "paths": paths,
        "out": tmp_path / "heldout",
    }


def test_freeze_precedes_episodes_and_changed_inputs_are_refused(heldout, monkeypatch):
    out, frozen = heldout["out"], heldout["frozen"]()
    roles = {g["id"]: g["roles"] for g in frozen["groups"]}
    assert roles["ppo_bc_s0_d0000020"] == ["curve", "bc_only"]
    assert roles["ppo_bc_s0_d0000030"] == ["curve", "selected", "final"]
    assert roles["sac_demo_s0_d0000020"] == ["curve"]
    assert [g["id"] for g in frozen["groups"][:3]] == [
        "mpc_search0",
        "mpc_search1",
        "rule_sailor",
    ]
    calls = []
    original = m4u_compare.run_episode

    def spy(*args, **kwargs):
        assert json.loads((out / "freeze.json").read_text()) == frozen
        calls.append(kwargs["log_meta"]["group"])
        return original(*args, **kwargs)

    monkeypatch.setattr(m4u_compare, "run_episode", spy)
    run(out, frozen, 1, heldout["task"], heldout["conditions"])
    assert len(calls) == 7 * 4
    heldout["paths"]["thresholds"].write_text('{"irons_s": 6}')
    with pytest.raises(ValueError, match="frozen inputs changed"):
        run(out, heldout["frozen"](), 1, heldout["task"], heldout["conditions"])
    assert len(calls) == 7 * 4
    assert json.loads((out / "freeze.json").read_text()) == frozen


def test_restart_runs_only_missing_episodes(heldout):
    out, frozen, conditions = heldout["out"], heldout["frozen"](), heldout["conditions"]
    run(out, frozen, 1, heldout["task"], {"nominal": conditions["nominal"]})
    run(out, frozen, 1, heldout["task"], conditions)
    run(out, frozen, 1, heldout["task"], conditions)
    for group in frozen["groups"]:
        reported = m4u_compare.read_jsonl(out / "episodes" / f"{group['id']}.jsonl")
        keys = [(e["manifest"], e["condition"]["id"]) for e in reported]
        assert len(keys) == len(set(keys)) == 4
        assert len(EpisodeStore(out / "store" / group["id"]).index) == 4
    assert json.loads((out / "complete.json").read_text())["episodes"] == 7 * 4


def test_parallel_workers_reproduce_the_serial_run(heldout, tmp_path, monkeypatch):
    frozen, task, conditions = (
        heldout["frozen"](),
        heldout["task"],
        heldout["conditions"],
    )
    serial, parallel = tmp_path / "serial", tmp_path / "parallel"
    run(serial, frozen, 1, task, conditions)
    run(parallel, frozen, 3, task, conditions)
    timing = ("wall_s", "decision_latency_s")
    for group in frozen["groups"]:
        a, b = (
            [
                {k: v for k, v in e.items() if k not in timing}
                for e in m4u_compare.read_jsonl(d / "episodes" / f"{group['id']}.jsonl")
            ]
            for d in (serial, parallel)
        )
        assert a == b
        stores = [EpisodeStore(d / "store" / group["id"]) for d in (serial, parallel)]
        for x, y in zip(stores[0].index, stores[1].index, strict=True):
            assert x["final_checksum"] == y["final_checksum"]
            np.testing.assert_array_equal(
                stores[0].load(x).actions, stores[1].load(y).actions
            )

    # The report verifies every frozen input and the nominal reloads.
    monkeypatch.setattr(m4u_report, "load_manifest", lambda m: conditions[m])
    loaded, episodes = m4u_report.load(parallel)
    checks = m4u_report.verify(parallel, loaded, episodes)
    assert checks["nominal_checkpoint_reloads_verified"] == 4
    assert checks["unreported_store_entries"] == 0
    summary = m4u_report.summarize_rows(loaded, episodes)
    assert {(r["arm"], r["role"]) for r in summary} >= {("ppo_bc", "bc_only")}
    specs, _ = m4u_report.video_specs(parallel, loaded, episodes)
    assert specs[0]["id"] == "controllers_nominal"
    assert "BC only (expert data)" in m4u_report.outcome_table(summary)
    m4u_report.learning_figure(loaded, episodes, summary, tmp_path / "learning.png")
    # Diagnostics replay every stored episode exactly, in-process at jobs=1 so
    # the tiny task applies; one summary row per outcome row.
    real = (HERE / "configs/M4U_diagnostics.json").read_text()
    heldout["paths"]["thresholds"].write_text(real)
    monkeypatch.setattr(u_diagnostics, "_task", lambda name, version: task)
    diag, per = m4u_report.diagnostics(parallel, loaded, episodes, 1)
    assert len(per) == len(episodes)
    assert [(r["arm"], r["role"], r["manifest"]) for r in diag["rows"]] == [
        (r["arm"], r["role"], r["manifest"]) for r in summary
    ]
    assert all(
        r["episodes"] == x["episodes"]
        for r, x in zip(diag["rows"], summary, strict=True)
    )
    assert "BC only (expert data)" in m4u_report.diagnostics_table(diag["rows"])
    heldout["paths"]["thresholds"].write_text("{}")
    with pytest.raises(ValueError, match="frozen input changed"):
        m4u_report.verify(parallel, loaded, episodes)


def test_runs_must_match_the_training_freeze(heldout):
    train = heldout["paths"]["train"]
    doc = json.loads(train.read_text())
    doc["configs"]["sac_demo"]["hidden"] = 9
    train.write_text(json.dumps(doc))
    with pytest.raises(ValueError, match="training freeze"):
        heldout["frozen"]()
