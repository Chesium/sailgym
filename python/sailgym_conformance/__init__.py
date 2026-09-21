"""Stack-neutral access to the committed conformance bundle (v2 F16).

This package belongs to **no stack**. Every runner — the JAX one this section
ships, the Warp one a later section may — reads the same bundle through the
same loader, because two loaders are two chances to disagree about what a
column means.

F17.1 governs what may live here: this is wrapper scope, so it contains **no
physical equation and no independently defined parameter or layout value**.
Rust exports those contracts and the bundle carries them as data. The
equations live one directory across, in :mod:`sailgym_jax`, which is an
independent verification implementation and is audited under the other half
of F17.1.
"""

from .bundle import (
    Bundle,
    BundleError,
    ExpectedIdentity,
    Table,
    Tolerance,
    WindKernelBounds,
    WindModes,
)

__all__ = [
    "Bundle",
    "BundleError",
    "ExpectedIdentity",
    "Table",
    "Tolerance",
    "WindKernelBounds",
    "WindModes",
]
