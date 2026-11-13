"""Protect final-evaluation freezes from silently changing on resume."""

import json

import pytest
from compare import write_freeze


def test_resume_requires_identical_frozen_inputs(tmp_path):
    frozen = {"checkpoint_hashes": {"actor.npz": "original"}, "groups": ["seed0"]}
    write_freeze(tmp_path, frozen)
    write_freeze(tmp_path, frozen)
    changed = {**frozen, "checkpoint_hashes": {"actor.npz": "changed"}}
    with pytest.raises(ValueError, match="frozen inputs changed"):
        write_freeze(tmp_path, changed)
    assert json.loads((tmp_path / "freeze.json").read_text()) == frozen
