"""M4X driver: X2's value tail on real checkpoints, the recorded predicted
scores, exact replay, resume, and refusal of sources changed since the freeze."""

import json
from dataclasses import replace

import m4x
import numpy as np
import ppo
import pytest
from sailing import NOMINAL, Task, TaskEnv
from viz.episode_store import EpisodeStore
from viz.replay import replay

pytestmark = pytest.mark.skipif(
    not m4x.M4U_HELDOUT_FREEZE.exists(), reason="needs the M4U held-out freeze"
)


def test_value_tail_is_the_bc_critic_on_normalized_features():
    task = Task.load("U")
    ckpt = m4x.value_checkpoints()[0]
    assert ckpt.name == "0093462"  # 10k calibration + 83,462 demonstration decisions
    params, meta = ppo.load(ckpt)
    rs = np.stack([TaskEnv(task, NOMINAL).reset()] * 3)
    rs[1, 0] += 5.0  # a different position must change V
    obs = ppo.normalize(rs, task.target, meta["normalization"], 5.0)
    v = m4x.ValueTail(ckpt, task)(rs)
    np.testing.assert_allclose(v, np.asarray(ppo.value(params, obs)), rtol=1e-6)
    assert v[0] == v[2] != v[1]


def test_x2_group_records_predictions_replays_and_resumes(tmp_path):
    tiny = replace(Task.load("U"), max_steps=100)  # 10 decisions
    group = {
        "id": "x2_search0",
        "controller": "mpc_tail",
        "search_seed": 0,
        "config": {"horizon": 20, "candidates": 4, "iterations": 1},
        "value_checkpoint": m4x.rel(m4x.value_checkpoints()[0]),
        "roles": ["selected"],
    }
    (tmp_path / "freeze.json").write_text("{}")
    conditions = {"nominal": [NOMINAL]}
    assert m4x.run_group(tmp_path, group, tiny, conditions) == 1
    assert m4x.run_group(tmp_path, group, tiny, conditions) == 0  # resumes
    (ep,) = m4x.m4u_compare.read_jsonl(tmp_path / "episodes/x2_search0.jsonl")
    assert len(ep["predicted"]) == ep["decisions"] == 10 and ep["tail_s"] > 0
    store = EpisodeStore(tmp_path / "store/x2_search0")
    (log,) = store.index
    assert log["controller_identity"]["tail_value"]["checkpoint"].endswith("0093462")
    replay(store.load(log), store.identity, tiny)  # raises unless exact


def test_runs_refuse_sources_changed_since_the_freeze(tmp_path, monkeypatch):
    path = tmp_path / "M4X_freeze.json"
    fz = {"source_sha256": {"experiments/ese3990/m4x.py": "0" * 64}}
    path.write_text(json.dumps(fz))
    monkeypatch.setattr(m4x, "FREEZE", path)
    with pytest.raises(ValueError, match="source changed"):
        m4x.prepare("validation")


def _metrics(tmp_path, rows):
    keys = ("decisions", "alpha", "entropy", "q_mean", "q_abs_max", "q_max", "q_min")
    (tmp_path / "metrics.jsonl").write_text(
        "".join(json.dumps(dict(zip(keys, r, strict=True))) + "\n" for r in rows)
    )
    return tmp_path


def test_entropy_adjusted_rule_admits_lag_and_ignores_the_burn_in(tmp_path):
    """Amendment 1: γ = 0.5 makes o = α·H. Before 400k nothing is tested (a
    start-up offset of +1000 neither fails nor widens [lo, hi]); afterwards
    a critic anywhere between 0 and the offsets seen since 400k is bounded."""
    import m4x_report

    lagging = [
        (100_000, 0.2, 5000.0, 0.0, 1.0, 1.0, -1.0),  # o = +1000, untested
        (400_000, 1.0, -800.0, -100.0, 300.0, 0.0, -300.0),  # o = -800
        (500_000, 1.0, -100.0, -600.0, 700.0, -10.0, -700.0),  # lags at -800 → ok
    ]
    s = m4x_report.critic_scale(_metrics(tmp_path, lagging), 0.5)["summary"]
    assert s["admissible_offsets_since_400k"] == [-800.0, 0.0]
    assert s["bounded_entropy_adjusted"] is True and s["bounded_as_declared"] is False
    above = lagging[:2] + [(500_000, 1.0, -100.0, 25.0, 30.0, 30.0, -5.0)]
    s = m4x_report.critic_scale(_metrics(tmp_path, above), 0.5)["summary"]
    assert s["max_mean_excess"] == 25.0 and s["bounded_entropy_adjusted"] is False
    below = lagging[:2] + [(500_000, 1.0, -100.0, -900.0, 1051.0, 0.0, -1051.0)]
    s = m4x_report.critic_scale(_metrics(tmp_path, below), 0.5)["summary"]
    assert s["bounded_entropy_adjusted"] is False  # 251 below the lowest offset
