"""The bundle loader, and the refusals it exists for (section 03, task 3.2).

Three behaviours are the point, and each has a test that makes the loader
prove it rather than a doc string that claims it:

1. a digest mismatch **raises**, and so does a payload that no longer hashes
   to its recorded ``data_key`` — F16.4's "reject stale records even if the
   directory and manifest agree with each other";
2. a column is addressed by name, and an unknown name raises with the names
   that do exist in the message;
3. a tolerance arrives with its justification attached.

It also carries the x64 assertion for task 3.1. ``conftest.py`` enables and
checks x64 at import, but a standalone ``python -c`` process never imports a
pytest ``conftest`` — so the property has to be asserted somewhere the suite
actually runs, and this is that place.
"""

from __future__ import annotations

import json
import shutil
from pathlib import Path

import jax.numpy as jnp
import numpy as np
import pytest
from conftest import EXPECTED

from sailgym_conformance import Bundle, BundleError, ExpectedIdentity


def test_x64_is_enabled_for_every_test():
    """RV14: f64 is the environment conformance is defined in (F16.8)."""
    assert jnp.asarray(1.0).dtype == jnp.float64
    assert jnp.zeros(3).dtype == jnp.float64
    # The failure this guards against is silent, so check a computed value
    # too: a promotion rule can differ from a literal's dtype.
    assert (jnp.asarray(1.0) / jnp.asarray(3.0)).dtype == jnp.float64


def test_the_committed_bundle_loads(bundle: Bundle):
    assert bundle.digest == EXPECTED.digest
    assert bundle.root.name == bundle.digest
    assert bundle.identity["model"]["model_version"] == EXPECTED.model_version


def test_numpy_load_opens_every_npy_in_the_bundle(bundle: Bundle, repo_root: Path):
    """Section 02 task 2.4's contract, asserted here, on the other side.

    The generator chose ``.npy`` v1.0 over ``.npz`` because this workspace's
    dependency graph is ``serde`` and ``serde_json``; whether that choice
    actually works is a statement about ``numpy``, and only this side can
    make it.
    """
    files = sorted((repo_root / "conformance" / bundle.digest).glob("*.npy"))
    assert len(files) == 11, f"expected 11 fixtures, found {[f.name for f in files]}"
    for path in files:
        data = np.load(path, allow_pickle=False)
        assert data.dtype == np.float64, f"{path.name}: {data.dtype}"
        assert data.ndim == 2, f"{path.name}: rank {data.ndim}"
        assert data.size > 0, f"{path.name} is empty"
    # …and every one of them is reachable through the loader, by name.
    assert {p.stem for p in files} == set(bundle.fixtures())


def test_columns_are_addressed_by_name(bundle: Bundle):
    table = bundle.table("tier0_wind_sample")
    assert table.input_columns == ("field", "x", "y", "t")
    assert table.output_columns == ("wx", "wy")
    assert table.column("wx").shape == (table.rows,)
    # The column is the contract, not the position: reading by index is one
    # insertion away from comparing the wrong quantity.
    assert np.array_equal(table.column("wx"), table.data[:, 4])


def test_an_unknown_column_name_raises_and_lists_the_real_ones(bundle: Bundle):
    with pytest.raises(BundleError) as e:
        bundle.table("tier0_wind_sample").column("w_x")
    message = str(e.value)
    assert "w_x" in message
    for name in ("field", "x", "y", "t", "wx", "wy"):
        assert name in message


def test_an_unknown_fixture_raises_and_lists_the_real_ones(bundle: Bundle):
    with pytest.raises(BundleError) as e:
        bundle.table("tier0_wind")
    assert "tier0_wind_sample" in str(e.value)


def test_a_tolerance_arrives_with_its_justification(bundle: Bundle):
    tol, justification = bundle.tolerance("tier0_wave", "wave")
    assert tol.unit == "dimensionless"
    assert tol.absolute > 0.0
    assert tol.relative > 0.0
    assert tol.ulp == 32
    # A number without its derivation is how a tolerance gets widened by
    # someone who does not know what it meant (F16.2).
    assert "F16.2" in justification or "ULP" in justification
    assert len(justification) > 200


def test_an_unknown_quantity_raises_with_the_available_ones(bundle: Bundle):
    with pytest.raises(BundleError) as e:
        bundle.tolerance("tier0_wind_sample", "w_x")
    assert "tier0_wind_sample.wx" in str(e.value)


def test_the_wind_modes_are_data(bundle: Bundle):
    """F16.7: the table travels in the bundle; no stack but Rust draws it."""
    fields = bundle.modes()
    assert len(fields) == 9
    assert [m.index for m in fields] == list(range(9))
    for m in fields:
        for column in (m.k_x, m.k_y, m.amp_x, m.amp_y, m.omega, m.phase):
            assert column.dtype == np.float64
            assert column.shape == (m.count,)
    # The stored amplitude is perpendicular to the wavenumber — the property
    # that makes the field divergence-free by construction (F6.1).
    for m in fields:
        if m.count == 0:
            continue
        dot = m.k_x * m.amp_x + m.k_y * m.amp_y
        scale = np.hypot(m.k_x, m.k_y) * np.hypot(m.amp_x, m.amp_y)
        assert np.max(np.abs(dot) / scale) < 1e-12, m.name
    assert bundle.modes("synthetic_gust").index == 2
    assert bundle.modes(0).name == "synthetic_uniform"


def test_an_unknown_wind_field_raises(bundle: Bundle):
    with pytest.raises(BundleError) as e:
        bundle.modes("synthetic_storm")
    assert "synthetic_gust" in str(e.value)


# ---------------------------------------------------------------------------
# The refusals
# ---------------------------------------------------------------------------


def _copy_bundle(repo_root: Path, tmp_path: Path) -> Path:
    """A writable copy of the committed bundle, for corruption tests."""
    root = tmp_path / "repo"
    (root / "conformance").mkdir(parents=True)
    shutil.copytree(
        repo_root / "conformance" / EXPECTED.digest,
        root / "conformance" / EXPECTED.digest,
    )
    return root


def test_a_hand_corrupted_manifest_digest_raises(repo_root: Path, tmp_path: Path):
    root = _copy_bundle(repo_root, tmp_path)
    manifest_path = root / "conformance" / EXPECTED.digest / "manifest.json"
    manifest = json.loads(manifest_path.read_text())
    manifest["digest"] = "0" * 64
    manifest_path.write_text(json.dumps(manifest))

    with pytest.raises(BundleError) as e:
        Bundle.load(root, expect=EXPECTED)
    assert "digest" in str(e.value)
    assert EXPECTED.digest in str(e.value)


def test_a_bundle_that_is_not_the_expected_one_raises(repo_root: Path):
    """F16.4.2: a newer bundle is a refusal, not an upgrade."""
    other = ExpectedIdentity(
        digest="f" * 64,
        bundle_schema_version=EXPECTED.bundle_schema_version,
        generator_version=EXPECTED.generator_version,
        tolerance_contract_version=EXPECTED.tolerance_contract_version,
        model_version=EXPECTED.model_version,
        integrator=EXPECTED.integrator,
        dt=EXPECTED.dt,
    )
    with pytest.raises(BundleError) as e:
        Bundle.load(repo_root, expect=other)
    assert EXPECTED.digest in str(e.value)


def test_a_contract_field_that_moved_raises(repo_root: Path):
    """The digest can agree while the contract does not — check both."""
    wrong_model = ExpectedIdentity(
        digest=EXPECTED.digest,
        bundle_schema_version=EXPECTED.bundle_schema_version,
        generator_version=EXPECTED.generator_version,
        tolerance_contract_version=EXPECTED.tolerance_contract_version,
        model_version=EXPECTED.model_version + 1,
        integrator=EXPECTED.integrator,
        dt=EXPECTED.dt,
    )
    with pytest.raises(BundleError) as e:
        Bundle.load(repo_root, expect=wrong_model)
    assert "model_version" in str(e.value)


def test_an_edited_payload_raises_even_though_the_manifest_agrees(
    repo_root: Path, tmp_path: Path
):
    """F16.4: "reject stale records even if the directory and manifest agree".

    One byte of one ``.npy`` is flipped — the same shape of injection section
    02 used for its third demonstration — and the manifest is left untouched.
    Nothing in the directory contradicts anything else in it; only the
    recorded ``data_key`` knows.
    """
    root = _copy_bundle(repo_root, tmp_path)
    victim = root / "conformance" / EXPECTED.digest / "tier0_wave.npy"
    blob = bytearray(victim.read_bytes())
    blob[-1] ^= 0x01
    victim.write_bytes(bytes(blob))

    with pytest.raises(BundleError) as e:
        Bundle.load(root, expect=EXPECTED)
    assert "tier0_wave" in str(e.value)
    assert "hash" in str(e.value)


def test_a_missing_bundle_raises(tmp_path: Path):
    with pytest.raises(BundleError) as e:
        Bundle.load(tmp_path, expect=EXPECTED)
    assert "conformance" in str(e.value)
