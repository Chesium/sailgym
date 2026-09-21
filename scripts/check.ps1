#!/usr/bin/env pwsh
<#
.SYNOPSIS
    The sailgym gate. Runs the F12 chain, in order, failing fast.

.DESCRIPTION
    Defined once here (and mirrored in scripts/check.sh); every task's
    acceptance criteria call this script rather than an ad-hoc command line
    (docs/v1/00-foundations.md F12, as amended by v2 F12').

.PARAMETER Step
    Run a single step, 1-11, instead of the whole chain. Each step is
    independently runnable; without this switch a failing step aborts the rest.

    The [ValidateRange] below moves with the chain. Forgetting it would make
    `scripts/check.ps1 -Step 10` fail with a parameter error rather than
    running the step, which is a confusing way to discover a typo (v2 RV17).

.PARAMETER Fast
    The pre-commit subset: steps 1-9, with step 9 restricted to Chromium and to
    the specs not tagged `@slow` — the browser performance measurement and the
    three brief section 46 demonstrations, which between them are about half
    the browser suite's wall time and none of which can be made quick without
    making it mean less. Step 8, the vitest run, is not restricted: it takes
    well under a second.

    The Python steps 10 and 11 are deliberately outside the subset (v2 section
    03, tracked as a debt in its PRD): the subset was not re-derived when they
    landed. Selecting a step explicitly always runs it, -Fast or not.

    **-Fast is not the gate.** It is what to run while working; the full chain
    is what has to be green before a section is finished (F13.7) and is what CI
    runs. Section 10's measured times for both are in
    docs/v1/progress/10-handoff.md and docs/v1/acceptance.md.

.EXAMPLE
    pwsh scripts/check.ps1
    pwsh scripts/check.ps1 -Step 3
    pwsh scripts/check.ps1 -Step 11
    pwsh scripts/check.ps1 -Fast
#>
[CmdletBinding()]
param(
    [ValidateRange(1, 11)]
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
    # v2 section 11 added `-p sailgym-task` (v2 F12'). Step 3 is what proves
    # the pure Rust is correct **and builds on the host with no WASM
    # toolchain**, and the practice evaluator is pure Rust with the same
    # property, so it belongs in this step rather than in a step of its own.
    # `-p sailgym-task` must survive every later revision of this step: a chain
    # rewritten for the Python steps that dropped it would take the whole
    # practice evaluator out of the gate silently (v2 F12').
    @{ Name = 'cargo test -p sailgym-physics -p sailgym-task'; Action = { cargo test -p sailgym-physics -p sailgym-task } }
    # Every audit target in one invocation: the brief section 35 invariants,
    # the prohibited-shortcut greps (section 07), the convergence study and the
    # rotation/mirror sweep (section 10 tasks 10.2 and 10.3), the F7 provenance
    # audit (task 10.7), and - added 2026-09-21 by human approval (v2 normative
    # delta D1; see docs/v2/prds/02-conformance-bundle.md) - the
    # conformance-bundle freshness check of v2 section 02. A stale bundle is a
    # property of the physics crate, so it belongs in the step that already
    # asks whether the physics crate is still what it says it is (v2 F12').
    # `--test conformance` must stay here for the same reason
    # `-p sailgym-task` must stay in step 3.
    @{ Name = 'cargo test -p sailgym-physics --test invariants --test no_shortcuts --test convergence --test symmetry --test provenance --test conformance'; Action = {
            cargo test -p sailgym-physics `
                --test invariants --test no_shortcuts `
                --test convergence --test symmetry --test provenance `
                --test conformance
        } }
    @{ Name = 'cargo test -p sailgym-physics --test regression'; Action = { cargo test -p sailgym-physics --test regression } }
    @{ Name = 'wasm-pack build crates/sailgym-wasm --target web --out-dir ../../web/src/wasm'; Action = { & (Join-Path $PSScriptRoot 'build-wasm.ps1') } }
    @{ Name = 'pnpm --dir web typecheck';                   Action = { pnpm --dir web typecheck } }
    # Added 2026-09-20 by human approval (normative delta D1; see
    # docs/v2/prds/01-boat-3d-svg.md). The vitest suite was cited by every
    # section's acceptance criteria from M1 onwards and run by none of them;
    # the Playwright step moved from 8 to 9 to make room.
    @{ Name = 'pnpm --dir web test:unit';                   Action = { pnpm --dir web test:unit } }
    $E2E
    # Steps 10 and 11 added 2026-09-21 by human approval (v2 normative delta
    # D1; see docs/v2/prds/03-jax-wind.md and v2 F12'). Step 10 is the Python
    # lint and format check, the counterpart of steps 1 and 2. Step 11
    # **delegates to a script**, exactly as step 6 delegates to build-wasm.ps1:
    # section 07 puts `maturin develop` in front of `pytest` inside
    # py-test.ps1 without amending F12 again. The chain is now eleven steps,
    # and [ValidateRange(1, 11)] above moved with it (v2 RV17).
    @{ Name = 'uv run ruff check python && uv run ruff format --check python'; Action = {
            uv run ruff check python
            if ($LASTEXITCODE -ne 0) { return }
            uv run ruff format --check python
        } }
    @{ Name = 'scripts/py-test.sh'; Action = { & (Join-Path $PSScriptRoot 'py-test.ps1') } }
)

$Total = $Steps.Count
# The last step -Fast covers. See the .PARAMETER note above.
$FastTotal = 9

$Selected = if ($Step -ne 0) { @($Step) }
            elseif ($Fast)   { 1..$FastTotal }
            else             { 1..$Total }

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
        Write-Host ('check: steps 1-{0} passed (-Fast subset; 10 and 11 not run) in {1:n0}s' -f $FastTotal, $elapsed) -ForegroundColor Green
    } else {
        Write-Host ('check: all steps passed in {0:n0}s' -f $elapsed) -ForegroundColor Green
    }
}
finally {
    Pop-Location
}
