#!/usr/bin/env pwsh
# Run the Python test suite. This is F12' step 11.
#
# A script rather than a command line, for the reason v2 F12' states: later
# sections put `maturin develop` in front of `pytest` here, **without amending
# F12 again** - exactly as step 6 has delegated to build-wasm.ps1 since v1.
# Section 07 is that later section, and this is that line. F12 is not amended
# (PRD 07 D3): the chain is still eleven steps and step 11 is still
# scripts/py-test.ps1.
#
# Primary script for the Windows host. scripts/py-test.sh is the equivalent for
# CI/Linux; keep the two in step.
#
# SAILGYM_WRITE_CONFORMANCE=1 makes python/tests/test_wind_tier0.py rewrite the
# two-arm divergence section of docs/v2/conformance.md (v2 F16.6). It is off by
# default: the gate must not rewrite a committed artifact while checking it.
#
# SAILGYM_SKIP_MATURIN=1 skips the extension build, for a quick re-run of the
# suite against an extension that is already current. The gate never sets it.

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$RepoRoot = Split-Path -Parent $PSScriptRoot

# RV18: a missing tool must not look like a test failure. build-wasm.ps1 says
# the same thing about wasm-pack, in the same place, for the same reason.
if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
    Write-Error 'uv not found on PATH. Install it with: irm https://astral.sh/uv/install.ps1 | iex'
    exit 1
}

# Section 07: the suite now imports a compiled extension, so a missing Rust
# toolchain is a second way for step 11 to fail with someone else's message.
# maturin itself is in the locked dev group and needs no check.
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error 'cargo not found on PATH. Install it from https://rustup.rs/'
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

    # The environment first, from the lock and only from the lock. `uv run`
    # would do this implicitly, but it would also do it *between* the two
    # commands below and prune whatever `maturin develop` had just installed.
    Write-Host '[py-test] uv sync --frozen'
    uv sync --frozen
    if ($LASTEXITCODE -ne 0) {
        Write-Error "uv sync failed with exit code $LASTEXITCODE"
        exit $LASTEXITCODE
    }

    $skip = [Environment]::GetEnvironmentVariable('SAILGYM_SKIP_MATURIN')
    if ($skip -ne '1') {
        # --release, deliberately: python/tests/test_performance.py measures a
        # GIL overlap ratio and a per-step allocation bound, and a debug build
        # would have it measuring the debug build's own overhead.
        Write-Host '[py-test] uv run maturin develop --release --manifest-path crates/sailgym-py/Cargo.toml'
        uv run --frozen --no-sync maturin develop --uv --release --manifest-path crates/sailgym-py/Cargo.toml
        if ($LASTEXITCODE -ne 0) {
            Write-Error "maturin develop failed with exit code $LASTEXITCODE"
            exit $LASTEXITCODE
        }
    }

    Write-Host '[py-test] uv run --frozen pytest python'
    uv run --frozen --no-sync pytest python @args
    if ($LASTEXITCODE -ne 0) {
        Write-Error "pytest failed with exit code $LASTEXITCODE"
        exit $LASTEXITCODE
    }
    Write-Host '[py-test] ok'
}
finally {
    Pop-Location
}
