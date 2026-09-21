"""Session set-up for every Python test in the repository (v2 section 03).

**x64 is enabled here, once, at import, and the effect is asserted.**

JAX defaults to f32 (F16.1 says so in as many words). F9.5 forbids f32
intermediates in physics, and F16.8 makes f32 "a different environment until
measured". A conformance run that silently fell back to f32 would produce
disagreements of order ``1e-7`` against tolerances of order ``1e-15`` — a
number that looks exactly like a tolerance failure and is actually a
configuration failure. That is RV14, and it is a day lost to reading the
wrong end of the problem.

So the switch is thrown before anything else imports ``jax.numpy``, and
:func:`_assert_x64_took_effect` checks that it took, rather than trusting
that the call did what it says. A standalone ``python -c`` process does not
import a pytest ``conftest``, which is why
``test_bundle.py::x64_is_enabled_for_every_test`` asserts the same property
from inside a test: the assertion has to be somewhere the suite actually
runs.
"""

from __future__ import annotations

import os
from pathlib import Path

import pytest

# Belt and braces: the environment variable is read by JAX at import, the
# `config.update` below covers an interpreter where JAX was already imported,
# and the assertion covers both being wrong.
os.environ.setdefault("JAX_ENABLE_X64", "1")

import jax  # noqa: E402  — must follow the environment variable above.
import jax.numpy as jnp  # noqa: E402

jax.config.update("jax_enable_x64", True)


def _assert_x64_took_effect() -> None:
    probe = jnp.asarray(1.0)
    if probe.dtype != jnp.float64:
        raise RuntimeError(
            "jax_enable_x64 did not take effect: jnp.asarray(1.0).dtype is "
            f"{probe.dtype}, not float64. Every conformance number this suite "
            "reports would be meaningless (v2 RV14, F16.8)."
        )


_assert_x64_took_effect()


from sailgym_conformance import Bundle, ExpectedIdentity  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]

# ---------------------------------------------------------------------------
# The identity this suite holds the committed bundle to
# ---------------------------------------------------------------------------

# F16.4: "Verification must reject stale records even if the directory and
# manifest agree with each other. Compare against the selected source/contract
# identity." A bundle that agrees with itself proves nothing — the loader
# needs an expectation supplied from outside it, and this is that expectation.
#
# It lives in the test suite rather than in `sailgym_conformance`, because
# F17.1 forbids wrapper scope from carrying independently defined parameter or
# layout values: `dt` is a number Rust owns. Here it is not a definition, it
# is the statement of which bundle this suite was written against. Section 02
# generated it; `docs/v2/progress/02-handoff.md` §4 records every field.
EXPECTED = ExpectedIdentity(
    digest="2323a34073ebdbd6adfe7e1fbfa22d41ff9874fd92df1fbdcb6f6e9ec9884a63",
    bundle_schema_version=1,
    generator_version=1,
    tolerance_contract_version=1,
    model_version=2,
    integrator="Rk2Midpoint",
    dt=0.005,
)


@pytest.fixture(scope="session")
def repo_root() -> Path:
    return REPO_ROOT


@pytest.fixture(scope="session")
def bundle() -> Bundle:
    """The committed bundle, checked against :data:`EXPECTED` on load."""
    return Bundle.load(REPO_ROOT, expect=EXPECTED)
