"""M4P probe: noise colour, holds, determinism, exact replay, resume, and
refusal of sources changed since the freeze."""

import json
from dataclasses import replace

import numpy as np
import probe
import pytest
from sailing import NOMINAL, Task
from viz.episode_store import EpisodeStore
from viz.replay import replay


@pytest.mark.parametrize("beta", [0, 1, 2])
def test_noise_has_the_declared_spectral_slope(beta):
    rng, n = np.random.default_rng(0), 4096
    x = [probe.powerlaw_noise(rng, beta, n) for _ in range(200)]
    assert all(abs(v.std() - 1) < 1e-9 for v in x)
    power = np.mean([np.abs(np.fft.rfft(v)) ** 2 for v in x], axis=0)
    band = slice(4, n // 4)
    f = np.fft.rfftfreq(n)[band]
    slope = np.polyfit(np.log(f), np.log(power[band]), 1)[0]
    assert abs(slope + beta) < 0.2


def test_actions_are_held_clipped_and_seeded():
    def actions(seed, hold=10, sigma=0.1):
        c = probe.NoiseProbe(seed, beta=1, hold=hold, sigma=sigma, decisions=100)
        c.reset(None)
        return np.array([c.act(None) for _ in range(100)])

    clipped = actions(7, sigma=1.0)
    assert np.all(np.abs(clipped) <= 1.0) and np.any(np.abs(clipped) == 1.0)
    a = actions(7)  # σ = 0.1: no clipping, so every window's sample is distinct
    blocks = a.reshape(10, 10, 2)
    assert np.all(blocks == blocks[:, :1])  # constant within each hold window
    assert np.all(np.diff(blocks[:, 0], axis=0) != 0)  # a new sample each window
    np.testing.assert_array_equal(a, actions(7))
    assert not np.array_equal(a, actions(8))
    assert len(np.unique(actions(7, hold=1)[:, 0])) > 50  # hold 1: every decision


def test_probe_cell_replays_exactly_and_resumes(tmp_path):
    tiny = replace(Task.load("U"), max_steps=100)  # 10 decisions
    (tmp_path / "freeze.json").write_text("{}")
    cell = next(c for c in probe.cells() if c["id"] == "beta1_hold10_sigma0.6")
    assert probe.run_cell(tmp_path, cell, tiny, [NOMINAL, NOMINAL]) == 2
    assert probe.run_cell(tmp_path, cell, tiny, [NOMINAL, NOMINAL]) == 0  # resumes
    eps = probe.read_jsonl(tmp_path / "episodes" / f"{cell['id']}.jsonl")
    assert [e["episode_index"] for e in eps] == [0, 1]
    assert eps[0]["controller_seed"] != eps[1]["controller_seed"]
    store = EpisodeStore(tmp_path / "store" / cell["id"])
    for log in store.index:
        assert log["controller_identity"]["beta"] == 1
        replay(store.load(log), store.identity, tiny)  # raises unless exact


def test_runs_refuse_sources_changed_since_the_freeze(tmp_path, monkeypatch):
    path = tmp_path / "M4P_freeze.json"
    path.write_text(
        json.dumps({"source_sha256": {"experiments/ese3990/probe.py": "0" * 64}})
    )
    monkeypatch.setattr(probe, "FREEZE", path)
    with pytest.raises(ValueError, match="source changed"):
        probe.prepare()


def _diag(tacks=0, outcome="truncated", max_y=0.0):
    fails = {r: outcome == r for r in ("mark_missed", "out_of_bounds", "capsized")}
    return {
        "outcome": "terminated" if any(fails.values()) else outcome,
        "timeout": outcome == "truncated",
        "final_distance_m": 50.0,
        **fails,
        "tacks": tacks,
        "failed_tacks": 0,
        "gybes": 0,
        "irons_s": 0.0,
        "sternway_s": 0.0,
        "max_y_m": max_y,
    }


def test_cell_rows_apply_the_declared_threshold_and_draw_the_grid(tmp_path):
    cells = probe.cells()
    diags = {c["id"]: [_diag()] * 10 for c in cells}
    white = "beta0_hold01_sigma0.3"
    diags[white] = [_diag(tacks=1, max_y=25.0)] + [_diag()] * 9  # 10%, no failures
    risky = "beta2_hold40_sigma1.0"  # tacks, but 2/10 failures
    diags[risky] = [_diag(tacks=1)] * 2 + [_diag(outcome="out_of_bounds")] * 2
    diags[risky] += [_diag()] * 6
    diags["beta1_hold10_sigma0.6"] = [_diag(outcome="finished")] + [_diag()] * 9
    rows, flags = probe.cell_rows(cells, diags)
    row = {r["id"]: r for r in rows}
    assert row[white]["reaches"] and row[white]["share_max_y_ge_20m"] == 0.1
    assert row[risky]["tack_rate"] == 0.2 and row[risky]["failure_rate"] == 0.2
    assert flags["white_per_decision_reaches"] is True
    assert flags["cells_reaching"] == [white]
    assert flags["cells_with_tacks_but_failures"] == [risky]
    assert flags["finishes"] == {"beta1_hold10_sigma0.6": 1}
    assert flags["episodes_replayed_exactly"] == 10 * len(cells)
    probe.grid_figure(rows, tmp_path / "grid.png")
    assert (tmp_path / "grid.png").stat().st_size > 0
