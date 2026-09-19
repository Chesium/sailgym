#!/usr/bin/env pwsh
# Build the WASM package consumed by the web app.
#
# This is F12 step 6, verbatim:
#   wasm-pack build crates/sailgym-wasm --target web --out-dir ../../web/src/wasm
# (`--out-dir` is resolved relative to the crate directory, so it lands in
# web/src/wasm/ at the repository root.)
#
# Primary script for the Windows host. scripts/build-wasm.sh is the equivalent
# for CI/Linux; keep the two in step.

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$RepoRoot = Split-Path -Parent $PSScriptRoot

if (-not (Get-Command wasm-pack -ErrorAction SilentlyContinue)) {
    Write-Error 'wasm-pack not found on PATH. Install it with: cargo install wasm-pack'
    exit 1
}

Push-Location $RepoRoot
try {
    Write-Host '[build-wasm] wasm-pack build crates/sailgym-wasm --target web --out-dir ../../web/src/wasm'
    wasm-pack build crates/sailgym-wasm --target web --out-dir ../../web/src/wasm
    if ($LASTEXITCODE -ne 0) {
        Write-Error "wasm-pack build failed with exit code $LASTEXITCODE"
        exit $LASTEXITCODE
    }
    Write-Host '[build-wasm] ok -> web/src/wasm/'
}
finally {
    Pop-Location
}
