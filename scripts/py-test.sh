#!/usr/bin/env bash
# Run the Python test suite. This is F12' step 11.
#
# A script rather than a command line, for the reason v2 F12' states: later
# sections put `maturin develop` in front of `pytest` here, **without amending
# F12 again** — exactly as step 6 has delegated to build-wasm.sh since v1.
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

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# RV18: a missing tool must not look like a test failure. build-wasm.sh says
# the same thing about wasm-pack, in the same place, for the same reason.
if ! command -v uv >/dev/null 2>&1; then
    echo 'uv not found on PATH. Install it with: curl -LsSf https://astral.sh/uv/install.sh | sh' >&2
    echo '(or see https://docs.astral.sh/uv/getting-started/installation/)' >&2
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

echo '[py-test] uv run --frozen pytest python'
uv run --frozen pytest python "$@"
echo '[py-test] ok'
