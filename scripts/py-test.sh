#!/usr/bin/env bash
# Run the Python test suite. This is F12' step 11.
#
# A script rather than a command line, for the reason v2 F12' states: later
# sections put `maturin develop` in front of `pytest` here, **without amending
# F12 again** — exactly as step 6 has delegated to build-wasm.sh since v1.
# Section 07 is that later section, and this is that line. **F12 is not
# amended** (PRD 07 D3): the chain is still eleven steps and step 11 is still
# `scripts/py-test.sh`.
#
# CI/Linux equivalent of scripts/py-test.ps1; keep the two in step.
#
# Usage:
#   scripts/py-test.sh                 the whole suite
#   scripts/py-test.sh python/tests/test_bundle.py -s      arguments pass through
#
# SAILGYM_WRITE_CONFORMANCE=1 makes python/tests/test_wind_tier0.py rewrite the
# two-arm divergence section of docs/v2/conformance.md (v2 F16.6). It is off by
# default: the gate must not rewrite a committed artifact while checking it.
#
# SAILGYM_SKIP_MATURIN=1 skips the extension build, for a quick re-run of the
# suite against an extension that is already current. The gate never sets it.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# RV18: a missing tool must not look like a test failure. build-wasm.sh says
# the same thing about wasm-pack, in the same place, for the same reason.
if ! command -v uv >/dev/null 2>&1; then
    echo 'uv not found on PATH. Install it with: curl -LsSf https://astral.sh/uv/install.sh | sh' >&2
    echo '(or see https://docs.astral.sh/uv/getting-started/installation/)' >&2
    exit 1
fi

# Section 07: the suite now imports a compiled extension, so a missing Rust
# toolchain is a second way for step 11 to fail with someone else's message.
# `maturin` itself is in the locked dev group and needs no check.
if ! command -v cargo >/dev/null 2>&1; then
    echo 'cargo not found on PATH. Install it with: curl --proto =https --tlsv1.2 -sSf https://sh.rustup.rs | sh' >&2
    echo '(or see https://rustup.rs/)' >&2
    exit 1
fi

cd "$repo_root"

# A PYTHONPATH inherited from an unrelated toolchain on the developer's machine
# puts foreign `site-packages` directories ahead of the project's virtual
# environment, and pytest then autoloads whatever plugins it finds there. That
# fails inside a third party's import, which is RV18's failure mode wearing a
# different hat: a gate failure that names someone else's traceback. The suite
# needs nothing from outside the locked environment, so it is run without one.
export PYTHONNOUSERSITE=1
unset PYTHONPATH PYTHONHOME || true

# The environment first, from the lock and only from the lock. `uv run` would
# do this implicitly, but it would also do it *between* the two commands below
# and prune whatever `maturin develop` had just installed.
echo '[py-test] uv sync --frozen'
uv sync --frozen

if [ "${SAILGYM_SKIP_MATURIN:-0}" != "1" ]; then
    # `--release`, deliberately. python/tests/test_performance.py measures a
    # GIL overlap ratio and a per-step allocation bound; a debug build would
    # have it measuring the debug build's own overhead, and the rest of the
    # suite would take minutes rather than seconds. The compile is cached
    # after the first run.
    echo '[py-test] uv run maturin develop --release --manifest-path crates/sailgym-py/Cargo.toml'
    uv run --frozen --no-sync maturin develop --uv --release \
        --manifest-path crates/sailgym-py/Cargo.toml
fi

echo '[py-test] uv run --frozen pytest python'
uv run --frozen --no-sync pytest python "$@"
echo '[py-test] ok'
