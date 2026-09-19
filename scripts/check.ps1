#!/usr/bin/env pwsh
<#
.SYNOPSIS
    The sailgym gate. Runs the F12 chain, in order, failing fast.

.DESCRIPTION
    Defined once here (and mirrored in scripts/check.sh); every task's
    acceptance criteria call this script rather than an ad-hoc command line
    (docs/00-foundations.md F12).

.PARAMETER Step
    Run a single step, 1-8, instead of the whole chain. Each step is
    independently runnable; without this switch a failing step aborts the rest.

.EXAMPLE
    pwsh scripts/check.ps1
    pwsh scripts/check.ps1 -Step 3
#>
[CmdletBinding()]
param(
    [ValidateRange(1, 8)]
    [int] $Step = 0
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$RepoRoot = Split-Path -Parent $PSScriptRoot

# name = what the step proves; action = how it is proved.
$Steps = @(
    @{ Name = 'cargo fmt --check';                          Action = { cargo fmt --check } }
    @{ Name = 'cargo clippy --all-targets -- -D warnings';  Action = { cargo clippy --all-targets -- -D warnings } }
    @{ Name = 'cargo test -p sailgym-physics';              Action = { cargo test -p sailgym-physics } }
    @{ Name = 'cargo test -p sailgym-physics --test invariants --test no_shortcuts'; Action = {
            cargo test -p sailgym-physics --test invariants
            if ($LASTEXITCODE -ne 0) { return }
            cargo test -p sailgym-physics --test no_shortcuts
        } }
    @{ Name = 'cargo test -p sailgym-physics --test regression'; Action = { cargo test -p sailgym-physics --test regression } }
    @{ Name = 'wasm-pack build crates/sailgym-wasm --target web --out-dir ../../web/src/wasm'; Action = { & (Join-Path $PSScriptRoot 'build-wasm.ps1') } }
    @{ Name = 'pnpm --dir web typecheck';                   Action = { pnpm --dir web typecheck } }
    @{ Name = 'pnpm --dir web test:e2e';                    Action = { pnpm --dir web test:e2e } }
)

$Total = $Steps.Count
$Selected = if ($Step -eq 0) { 1..$Total } else { @($Step) }

Push-Location $RepoRoot
try {
    foreach ($i in $Selected) {
        $s = $Steps[$i - 1]
        Write-Host ''
        Write-Host ('[{0}/{1}] {2}' -f $i, $Total, $s.Name) -ForegroundColor Cyan
        & $s.Action
        if ($LASTEXITCODE -ne 0) {
            Write-Host ('[{0}/{1}] FAILED ({2})' -f $i, $Total, $s.Name) -ForegroundColor Red
            exit $LASTEXITCODE
        }
    }
    Write-Host ''
    Write-Host 'check: all steps passed' -ForegroundColor Green
}
finally {
    Pop-Location
}
