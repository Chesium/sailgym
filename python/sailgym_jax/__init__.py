"""The JAX verification implementation (v2 F16.6, F17.1).

This package is the **explicit exception** F17.1 carves out of the F8 rule: it
is an independent implementation of part of the model, written to be compared
against the Rust one, so it necessarily contains the arithmetic it tests. The
exception is narrow and does not extend to any Python wrapper over Rust.

Two consequences follow, and both are enforced by
``python/tests/test_no_stray_constants.py``:

* every kernel constant is a named module constant whose doc string cites the
  line of the Rust source it was transcribed from; and
* the physical parameters and the wind mode table are **read from the
  identified bundle**, never written here.

Section 03 ports exactly one function, ``environment::wind::sample``, and
ships it in the two arms F16.6 requires. See :mod:`sailgym_jax.wind`.
"""

from . import wind

__all__ = ["wind"]
