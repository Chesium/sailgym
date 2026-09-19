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

.PARAMETER Fast
    The pre-commit subset: the same eight steps, with step 8 restricted to
    Chromium and to the specs not tagged `@slow` — the browser performance
    measurement and the three brief section 46 demonstrations, which between
    them are about half the browser suite's wall time and none of which can be
    made quick without making it mean less.

    **-Fast is not the gate.** It is what to run while working; the full chain
    is what has to be green before a section is finished (F13.7) and is what CI
    runs. Section 10's measured times for both are in
    docs/progress/10-handoff.md and docs/acceptance.md.

.EXAMPLE
    pwsh scripts/check.ps1
    pwsh scripts/check.ps1 -Step 3
    pwsh scripts/check.ps1 -Fast
#>
[CmdletBinding()]
param(
    [ValidateRange(1, 8)]
    [int] $Step = 0,
    [switch] $Fast
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$RepoRoot = Split-Path -Parent $PSScriptRoot

$E2E = if ($Fast) {
    @{ Name = 'pnpm --dir web test:e2e --project=chromium --grep-invert @slow'
       Action = { pnpm --dir web test:e2e --project=chromium --grep-invert '@slow' } }
} else {
    @{ Name = 'pnpm --dir web test:e2e'; Action = { pnpm --dir web test:e2e } }
}

# name = what the step proves; action = how it is proved.
$Steps = @(
    @{ Name = 'cargo fmt --check';                          Action = { cargo fmt --check } }
    @{ Name = 'cargo clippy --all-targets -- -D warnings';  Action = { cargo clippy --all-targets -- -D warnings } }
    @{ Name = 'cargo test -p sailgym-physics';              Action = { cargo test -p sailgym-physics } }
    # Every audit target in one invocation: the brief section 35 invariants,
    # the prohibited-shortcut greps (section 07), the convergence study and the
    # rotation/mirror sweep (section 10 tasks 10.2 and 10.3), and the F7
    # provenance audit (task 10.7).
    @{ Name = 'cargo test -p sailgym-physics --test invariants --test no_shortcuts --test convergence --test symmetry --test provenance'; Action = {
            cargo test -p sailgym-physics `
                --test invariants --test no_shortcuts `
                --test convergence --test symmetry --test provenance
        } }
    @{ Name = 'cargo test -p sailgym-physics --test regression'; Action = { cargo test -p sailgym-physics --test regression } }
    @{ Name = 'wasm-pack build crates/sailgym-wasm --target web --out-dir ../../web/src/wasm'; Action = { & (Join-Path $PSScriptRoot 'build-wasm.ps1') } }
    @{ Name = 'pnpm --dir web typecheck';                   Action = { pnpm --dir web typecheck } }
    $E2E
)

$Total = $Steps.Count
$Selected = if ($Step -eq 0) { 1..$Total } else { @($Step) }

Push-Location $RepoRoot
$Started = Get-Date
try {
    foreach ($i in $Selected) {
        $s = $Steps[$i - 1]
        Write-Host ''
        Write-Host ('[{0}/{1}] {2}' -f $i, $Total, $s.Name) -ForegroundColor Cyan
        $stepStarted = Get-Date
        & $s.Action
        if ($LASTEXITCODE -ne 0) {
            Write-Host ('[{0}/{1}] FAILED ({2})' -f $i, $Total, $s.Name) -ForegroundColor Red
            exit $LASTEXITCODE
        }
        Write-Host ('[{0}/{1}] ok in {2:n0}s' -f $i, $Total, ((Get-Date) - $stepStarted).TotalSeconds)
    }
    Write-Host ''
    $elapsed = ((Get-Date) - $Started).TotalSeconds
    if ($Fast) {
        Write-Host ('check: all steps passed (-Fast subset) in {0:n0}s' -f $elapsed) -ForegroundColor Green
    } else {
        Write-Host ('check: all steps passed in {0:n0}s' -f $elapsed) -ForegroundColor Green
    }
}
finally {
    Pop-Location
}
