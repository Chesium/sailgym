#!/usr/bin/env pwsh
# Run the Python test suite. This is F12' step 11.
#
# A script rather than a command line, for the reason v2 F12' states: later
# sections put `maturin develop` in front of `pytest` here, **without amending
# F12 again** - exactly as step 6 has delegated to build-wasm.ps1 since v1.
#
# Primary script for the Windows host. scripts/py-test.sh is the equivalent for
# CI/Linux; keep the two in step.
#
# SAILGYM_WRITE_CONFORMANCE=1 makes python/tests/test_wind_tier0.py rewrite the
# two-arm divergence section of docs/v2/conformance.md (v2 F16.6). It is off by
# default: the gate must not rewrite a committed artifact while checking it.

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$RepoRoot = Split-Path -Parent $PSScriptRoot

# RV18: a missing tool must not look like a test failure. build-wasm.ps1 says
# the same thing about wasm-pack, in the same place, for the same reason.
if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
    Write-Error 'uv not found on PATH. Install it with: irm https://astral.sh/uv/install.ps1 | iex'
    exit 1
}

Push-Location $RepoRoot
try {
    # See the note in py-test.sh: an inherited PYTHONPATH puts foreign
    # site-packages ahead of the locked environment and pytest autoloads
    # whatever plugins it finds there, which fails inside a third party's
    # import. The suite needs nothing from outside the lock.
    $env:PYTHONNOUSERSITE = '1'
    Remove-Item Env:PYTHONPATH -ErrorAction SilentlyContinue
    Remove-Item Env:PYTHONHOME -ErrorAction SilentlyContinue

    Write-Host '[py-test] uv run --frozen pytest python'
    uv run --frozen pytest python @args
    if ($LASTEXITCODE -ne 0) {
        Write-Error "pytest failed with exit code $LASTEXITCODE"
        exit $LASTEXITCODE
    }
    Write-Host '[py-test] ok'
}
finally {
    Pop-Location
}
