"""The exact prefix gate allows the long run to complete its last 4M block."""

import json

import pytest
from m5_sac_budget_evaluate import check_prefix, heldout_freeze


def write_run(path, checkpoints):
    path.mkdir()
    (path / "ckpt").mkdir()
    curve = []
    for d, params in checkpoints.items():
        (path / "ckpt" / f"{d:07d}.npz").write_bytes(params)
        curve.append(json.dumps({"decisions": d, "val_finished": 0}))
    (path / "curve.jsonl").write_text("\n".join(curve))


def test_shared_prefix_and_clipped_budget(tmp_path):
    old, new = tmp_path / "old", tmp_path / "new"
    write_run(old, {100: b"same", 200: b"clipped"})
    write_run(new, {100: b"same", 203: b"full block", 400: b"final"})
    result = check_prefix(new, old)
    assert result["matched_decisions"] == [100]
    assert result["extension_crossing"] == 203
    (new / "ckpt/0000100.npz").write_bytes(b"changed")
    with pytest.raises(ValueError, match="diverged"):
        check_prefix(new, old)


def test_heldout_roles_seal_before_evaluation(tmp_path):
    checkpoint = tmp_path / "ckpt" / "0000200"
    checkpoint.parent.mkdir()
    cfg = {"m5": {"hold": 10}}
    meta = dict(learner="sac", training_seed=4, config=cfg, decisions=200)
    checkpoint.with_suffix(".json").write_text(json.dumps(meta))
    checkpoint.with_suffix(".npz").write_bytes(b"parameters")
    run = dict(
        summary={"decisions": 200},
        selected={"decisions": 200},
        seed=4,
        run=str(tmp_path),
        run_id="extended_sac_B_s4",
    )
    sealed = heldout_freeze([run], cfg, {}, "test")
    assert len(sealed["groups"]) == 1
    assert sealed["groups"][0]["roles"] == ["selected", "final"]
    assert len(sealed["input_hashes"]) == 2
