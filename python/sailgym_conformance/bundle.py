"""Read the committed conformance bundle (v2 F16.4, section 03 task 3.2).

Shared by every stack's runner, now and later, so it belongs to no stack and
holds no equation (F17.1). What it does hold is three refusals, and each one
exists because the alternative is a test that passes for the wrong reason:

1. **A digest mismatch is an error, never a warning** (F16.4.2). The
   manifest's ``digest`` must equal its own directory name *and* the identity
   the caller states it expects, and every ``.npy``'s payload must still hash
   to the ``data_key`` the manifest recorded for it. F16.4 is explicit that
   verification "must reject stale records even if the directory and manifest
   agree with each other" — a bundle that agrees with itself has said nothing,
   which is why :class:`ExpectedIdentity` comes from outside the loader.

2. **Columns are addressed by name, never by index.** The manifest's column
   names are the contract. A port that reads column 3 is one insertion away
   from comparing the wrong quantity and never finding out; an unknown name
   raises, and the message lists the names that do exist.

3. **A tolerance travels with its justification.** :meth:`Bundle.tolerance`
   returns both, and the comparison helper prints the justification on
   failure. A number without its derivation is how a tolerance comes to be
   widened by someone who does not know what it meant (F16.2: "A tolerance
   change requires a new derivation, never merely a green port test").

Arrays come back as ``float64`` explicitly, and a bundle whose stored dtype is
anything else is refused. That is RV14 from the data side: an f32 array would
turn a configuration mistake into what looks like a port disagreement.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np

__all__ = [
    "Bundle",
    "BundleError",
    "ExpectedIdentity",
    "Table",
    "Tolerance",
    "WindKernelBounds",
    "WindModes",
]

BUNDLE_DIRNAME = "conformance"
"""The directory, relative to the repository root, that holds the bundles."""

MANIFEST_NAME = "manifest.json"
"""The bundle's own record of what it is (F16.4)."""

WIND_MODES_NAME = "wind_modes.json"
"""The precomputed spectral modes, which is why no stack but Rust has PCG32."""


class BundleError(Exception):
    """A bundle was missing, malformed, or not the bundle that was expected."""


# ---------------------------------------------------------------------------
# The records the manifest carries
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class ExpectedIdentity:
    """The contract identity a caller states **before** opening a bundle.

    F16.4: comparison is against "the selected source/contract identity", not
    against whatever the artifact says about itself. Supplying this is how a
    runner declares which bundle it was written against; a newer bundle is a
    refusal, not an upgrade.
    """

    digest: str
    bundle_schema_version: int
    generator_version: int
    tolerance_contract_version: int
    model_version: int
    integrator: str
    dt: float


@dataclass(frozen=True)
class Tolerance:
    """One column's bound, exactly as the manifest records it.

    ``absolute`` and ``relative`` are combined as the manifest's tier rule
    states: a value passes when
    ``|port - recorded| <= max(absolute, relative * |recorded|)``. ``absolute``
    is the near-zero floor — it is the bound that means anything where
    cancellation has eaten the significant digits, which is why F16.2 refuses
    to let a relative bound stand alone.
    """

    fixture: str
    quantity: str
    unit: str
    measured_scale: float
    absolute: float
    relative: float
    ulp: int | None
    measured: dict[str, float] | None

    def bound(self, recorded: np.ndarray) -> np.ndarray:
        """The per-row bound this tolerance allows against ``recorded``."""
        return np.maximum(self.absolute, self.relative * np.abs(recorded))


@dataclass(frozen=True)
class WindKernelBounds:
    """F16.6's two measured numbers, as section 02 wrote them.

    ``kernel_vs_libm_absolute`` is what an arm calling the host library's
    cosine is held to; the transcribed arm is held to the tier-0 bound for its
    column instead. ``summation_vs_compensated_absolute`` is the other half:
    what a port that reassociates the fixed two-slot mode reduction is held
    to.

    F16.6's own prose says the kernel agrees with ``f64::cos`` "to about
    1e-14". That was a bound, not a measurement; section 02 measured it two
    decades tighter and wrote the measurement here. **Consume this, not the
    clause** — which is what the clause itself asks for.
    """

    domain: str
    kernel_vs_libm_absolute: float
    kernel_vs_libm_at: float
    summation_vs_compensated_absolute: float
    summation_vs_compensated_relative: float
    derivation: str


@dataclass(frozen=True)
class WindModes:
    """One wind field's mode table, read as data and never regenerated.

    F16.7: ``ProceduralWind::new`` draws the table once at construction and
    ``sample`` is a pure branch-free sum over the result, so the table travels
    in the bundle. **No stack other than Rust implements PCG32.** A port that
    reproduces a twelve-row table instead of reading it has spent days
    reproducing a kilobyte, and is wrong in a way no tolerance catches.

    ``index`` is the value the ``field`` input column of ``tier0_wind_sample``
    carries: the fields are in a fixed order and that order is the key.
    """

    index: int
    name: str
    config: dict[str, Any]
    k_x: np.ndarray
    k_y: np.ndarray
    amp_x: np.ndarray
    amp_y: np.ndarray
    omega: np.ndarray
    phase: np.ndarray

    @property
    def count(self) -> int:
        """``K``. Zero for a uniform field, whose sample is exactly ``base``."""
        return int(self.k_x.shape[0])


class Table:
    """One ``.npy`` fixture, addressed by column name.

    The array itself is available as :attr:`data`, but reaching into it by
    index is the mistake this class exists to prevent, so the columns are the
    interface.
    """

    def __init__(
        self,
        name: str,
        data: np.ndarray,
        input_columns: list[str],
        output_columns: list[str],
        tier: int,
        samplers: list[str],
    ) -> None:
        self.name = name
        self.data = data
        self.input_columns = tuple(input_columns)
        self.output_columns = tuple(output_columns)
        self.tier = tier
        self.samplers = tuple(samplers)
        ordered = list(input_columns) + list(output_columns)
        self._at = {c: i for i, c in enumerate(ordered)}

    @property
    def rows(self) -> int:
        return int(self.data.shape[0])

    @property
    def columns(self) -> tuple[str, ...]:
        return self.input_columns + self.output_columns

    def column(self, name: str) -> np.ndarray:
        """One column, by name. An unknown name raises and lists the real ones."""
        try:
            at = self._at[name]
        except KeyError:
            raise BundleError(
                f"{self.name}: no column named {name!r}. "
                f"Available columns: {', '.join(self.columns)}"
            ) from None
        return self.data[:, at]

    def __repr__(self) -> str:  # pragma: no cover - diagnostics only
        return f"<Table {self.name} rows={self.rows} columns={list(self.columns)}>"


# ---------------------------------------------------------------------------
# The loader
# ---------------------------------------------------------------------------


class Bundle:
    """A loaded, verified conformance bundle."""

    def __init__(self, root: Path, manifest: dict[str, Any]) -> None:
        self.root = root
        self.manifest = manifest
        self._tables: dict[str, Table] = {}
        self._modes: tuple[WindModes, ...] = ()

    # -- identity ----------------------------------------------------------

    @property
    def digest(self) -> str:
        return str(self.manifest["digest"])

    @property
    def identity(self) -> dict[str, Any]:
        """The full canonical record. F16.4 keeps it whole beside the digest."""
        return self.manifest["identity"]

    @property
    def toolchain(self) -> dict[str, Any]:
        return self.manifest["toolchain"]

    @property
    def parameters(self) -> dict[str, Any]:
        """The F7 catalogue the bundle was generated from, verbatim."""
        return self.identity["parameters"]

    # -- data --------------------------------------------------------------

    def table(self, name: str) -> Table:
        try:
            return self._tables[name]
        except KeyError:
            raise BundleError(
                f"no fixture named {name!r}. "
                f"Available fixtures: {', '.join(sorted(self._tables))}"
            ) from None

    def fixtures(self) -> tuple[str, ...]:
        return tuple(sorted(self._tables))

    def modes(self, which: int | str | None = None) -> Any:
        """The wind mode table.

        With no argument, every field in bundle order — the order the ``field``
        input column indexes. With an index or a name, that one field.
        """
        if which is None:
            return self._modes
        if isinstance(which, int):
            try:
                return self._modes[which]
            except IndexError:
                raise BundleError(
                    f"no wind field with index {which}; the bundle has "
                    f"{len(self._modes)}"
                ) from None
        for m in self._modes:
            if m.name == which:
                return m
        raise BundleError(
            f"no wind field named {which!r}. "
            f"Available fields: {', '.join(m.name for m in self._modes)}"
        )

    # -- tolerances --------------------------------------------------------

    def tolerance(self, fixture: str, quantity: str) -> tuple[Tolerance, str]:
        """``(tolerance, justification)`` for one column of one fixture.

        The justification is the manifest's derivation for that tier, stated
        once per tier exactly as F16.2 asks. It is returned rather than
        looked up separately so that no caller can report a bound without it.
        """
        for tier in self.manifest["tolerances"]["tiers"]:
            for q in tier["quantities"]:
                if q["fixture"] == fixture and q["quantity"] == quantity:
                    return (
                        Tolerance(
                            fixture=fixture,
                            quantity=quantity,
                            unit=q["unit"],
                            measured_scale=float(q["measured_scale"]),
                            absolute=float(q["absolute"]),
                            relative=float(q["relative"]),
                            ulp=None if q["ulp"] is None else int(q["ulp"]),
                            measured=q["measured"],
                        ),
                        str(tier["rule"]),
                    )
        known = sorted(
            f"{q['fixture']}.{q['quantity']}"
            for tier in self.manifest["tolerances"]["tiers"]
            for q in tier["quantities"]
            if q["fixture"] == fixture
        )
        raise BundleError(
            f"no tolerance for {fixture}.{quantity}. "
            f"Available for that fixture: {', '.join(known) or '(none)'}"
        )

    def wind_kernel(self) -> WindKernelBounds:
        """F16.6's measured bounds (section 02 §4.1)."""
        w = self.manifest["tolerances"]["wind_kernel"]
        return WindKernelBounds(
            domain=str(w["domain"]),
            kernel_vs_libm_absolute=float(w["kernel_vs_libm_absolute"]),
            kernel_vs_libm_at=float(w["kernel_vs_libm_at"]),
            summation_vs_compensated_absolute=float(
                w["summation_vs_compensated_absolute"]
            ),
            summation_vs_compensated_relative=float(
                w["summation_vs_compensated_relative"]
            ),
            derivation=str(w["derivation"]),
        )

    def notes(self) -> tuple[str, ...]:
        return tuple(self.manifest["tolerances"]["notes"])

    # -- construction ------------------------------------------------------

    @classmethod
    def load(cls, root: str | Path, *, expect: ExpectedIdentity) -> Bundle:
        """Discover, read and verify ``<root>/conformance/<digest>/``.

        ``expect`` is not optional and has no default. A loader that guessed
        would be a loader that silently accepted a regenerated bundle, which
        is the exact failure F16.4.2 is written against.
        """
        root = Path(root)
        holder = root / BUNDLE_DIRNAME
        if not holder.is_dir():
            raise BundleError(f"no bundle directory at {holder}")

        directory = holder / expect.digest
        if not directory.is_dir():
            present = sorted(p.name for p in holder.iterdir() if p.is_dir())
            raise BundleError(
                f"no bundle {expect.digest} under {holder}. "
                f"Present: {', '.join(present) or '(none)'}"
            )

        manifest_path = directory / MANIFEST_NAME
        if not manifest_path.is_file():
            raise BundleError(f"{directory.name}: {MANIFEST_NAME} is missing")
        try:
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as e:
            raise BundleError(
                f"{directory.name}: {MANIFEST_NAME} is not JSON: {e}"
            ) from e

        bundle = cls(directory, manifest)
        bundle._check_identity(expect)
        bundle._load_fixtures()
        bundle._load_modes()
        return bundle

    # -- verification ------------------------------------------------------

    def _check_identity(self, expect: ExpectedIdentity) -> None:
        got = self.manifest.get("digest")
        if got != self.root.name:
            raise BundleError(
                f"digest mismatch: {MANIFEST_NAME} says {got!r} but the directory "
                f"is named {self.root.name!r}. A bundle that does not know its own "
                f"name is stale or hand-edited (F16.4.2)."
            )
        if got != expect.digest:
            raise BundleError(
                f"digest mismatch: this bundle is {got!r}, the runner expects "
                f"{expect.digest!r}. Regenerating the bundle is a decision, not an "
                f"upgrade (F16.4.2)."
            )

        identity = self.manifest["identity"]
        mismatches = []
        for field, actual in (
            ("bundle_schema_version", identity["bundle_schema_version"]),
            ("generator_version", identity["generator_version"]),
            ("tolerance_contract_version", identity["tolerance_contract_version"]),
            ("model_version", identity["model"]["model_version"]),
            ("integrator", identity["integrator"]),
            ("dt", identity["dt"]),
        ):
            wanted = getattr(expect, field)
            if actual != wanted:
                mismatches.append(f"{field}: bundle {actual!r}, expected {wanted!r}")
        if mismatches:
            raise BundleError(
                "the bundle does not describe the expected contract:\n  "
                + "\n  ".join(mismatches)
            )

    def _load_fixtures(self) -> None:
        for fx in self.manifest["identity"]["fixtures"]:
            name = fx["name"]
            path = self.root / f"{name}.npy"
            if not path.is_file():
                raise BundleError(f"{name}: {path.name} is missing from the bundle")
            data = np.load(path, allow_pickle=False)

            if data.dtype != np.float64:
                raise BundleError(
                    f"{name}: dtype is {data.dtype}, not float64. Conformance runs in "
                    f"f64 (F16.8); an f32 fixture would make a configuration mistake "
                    f"read as a port disagreement (RV14)."
                )
            if data.ndim != 2:
                raise BundleError(f"{name}: rank is {data.ndim}, not 2")

            columns = list(fx["input_columns"]) + list(fx["output_columns"])
            if len(set(columns)) != len(columns):
                duplicates = sorted({c for c in columns if columns.count(c) > 1})
                raise BundleError(
                    f"{name}: duplicate column names {duplicates}. The column names "
                    f"are the contract; a contract with a duplicate key is not one."
                )
            if data.shape != (fx["rows"], len(columns)):
                raise BundleError(
                    f"{name}: shape is {data.shape}, but the manifest records "
                    f"{fx['rows']} rows and {len(columns)} columns"
                )

            payload = np.ascontiguousarray(data, dtype="<f8").tobytes()
            data_key = hashlib.sha256(payload).hexdigest()
            if data_key != fx["data_key"]:
                raise BundleError(
                    f"{name}: the stored numbers hash to {data_key}, but the manifest "
                    f"records {fx['data_key']}. The file and the manifest disagree, so "
                    f"one of them has been edited (F16.4)."
                )

            self._tables[name] = Table(
                name=name,
                data=data,
                input_columns=list(fx["input_columns"]),
                output_columns=list(fx["output_columns"]),
                tier=int(fx["tier"]),
                samplers=list(fx["samplers"]),
            )

    def _load_modes(self) -> None:
        path = self.root / WIND_MODES_NAME
        if not path.is_file():
            raise BundleError(f"{WIND_MODES_NAME} is missing from the bundle")
        entries = json.loads(path.read_text(encoding="utf-8"))
        recorded = self.manifest["identity"]["wind"]
        if len(entries) != len(recorded):
            raise BundleError(
                f"{WIND_MODES_NAME} holds {len(entries)} fields but the manifest "
                f"records {len(recorded)}"
            )

        built = []
        for index, (entry, ident) in enumerate(zip(entries, recorded, strict=True)):
            if entry["name"] != ident["name"] or entry["seed"] != ident["seed"]:
                raise BundleError(
                    f"wind field {index}: {WIND_MODES_NAME} says "
                    f"{entry['name']!r}/{entry['seed']}, the manifest says "
                    f"{ident['name']!r}/{ident['seed']}"
                )
            modes = entry["modes"]
            if len(modes) != ident["mode_count"]:
                raise BundleError(
                    f"{entry['name']}: {len(modes)} modes in {WIND_MODES_NAME}, "
                    f"{ident['mode_count']} in the manifest"
                )

            def col(*path_: str, rows: list[Any] = modes) -> np.ndarray:
                out = np.empty(len(rows), dtype=np.float64)
                for i, m in enumerate(rows):
                    v: Any = m
                    for p in path_:
                        v = v[p]
                    out[i] = v
                return out

            built.append(
                WindModes(
                    index=index,
                    name=entry["name"],
                    config=entry["config"],
                    k_x=col("k", "x"),
                    k_y=col("k", "y"),
                    amp_x=col("amp", "x"),
                    amp_y=col("amp", "y"),
                    omega=col("omega"),
                    phase=col("phase"),
                )
            )
        self._modes = tuple(built)
