#!/usr/bin/env bash
# The sailgym gate. Runs the F12 chain, in order, failing fast.
#
# Linux/CI equivalent of scripts/check.ps1; keep the two in step.
# Usage:
#   scripts/check.sh          run the whole chain (F12/F12', eleven steps)
#   scripts/check.sh 3        run step 3 only
#   scripts/check.sh --fast   the pre-commit subset; see below
#
# `--fast` runs steps 1-9 with step 9 restricted to Chromium and to the specs
# that are not tagged `@slow` — the browser performance measurement and the
# three brief §46 demonstrations, which between them account for about half of
# the browser suite's wall time and none of which can be made quick without
# making it mean less. Step 8, the vitest run, is not restricted: it takes well
# under a second. **The Python steps 10 and 11 are deliberately outside the
# `--fast` subset** (v2 section 03, recorded as a tracked debt in its PRD): the
# subset was not re-derived when they landed, and the debt is repaid whenever
# the Python suite gets slow enough that skipping it matters. Selecting a step
# explicitly always runs it, `--fast` or not.
#
# **`--fast` is not the gate.** It is what to run while working; the full chain
# is what has to be green before a section is finished (F13.7), and it is what
# CI runs. Section 10's measured times for both are in
# `docs/v1/progress/10-handoff.md` and `docs/v1/acceptance.md`.

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script_dir="$repo_root/scripts"

fast=0
if [[ ${1:-} == "--fast" ]]; then
    fast=1
    shift
fi

step_names=(
    'cargo fmt --check'
    'cargo clippy --all-targets -- -D warnings'
    'cargo test -p sailgym-physics -p sailgym-task -p sailgym-course -p sailgym-agent -p sailgym-env'
    'cargo test -p sailgym-physics --test invariants --test no_shortcuts --test convergence --test symmetry --test provenance --test conformance'
    'cargo test -p sailgym-physics --test regression'
    'wasm-pack build crates/sailgym-wasm --target web --out-dir ../../web/src/wasm'
    'pnpm --dir web typecheck'
    'pnpm --dir web test:unit'
    'pnpm --dir web test:e2e'
    'uv run ruff check python && uv run ruff format --check python'
    'scripts/py-test.sh'
)
if [[ $fast -eq 1 ]]; then
    step_names[8]='pnpm --dir web test:e2e --project=chromium --grep-invert @slow'
fi
total=${#step_names[@]}
# The last step `--fast` covers. See the note at the top of the file.
fast_total=9

run_step() {
    case "$1" in
        1) cargo fmt --check ;;
        2) cargo clippy --all-targets -- -D warnings ;;
        # v2 section 11 added `-p sailgym-task`, section 04
        # `-p sailgym-course`, section 05 `-p sailgym-agent` and section 06
        # `-p sailgym-env` (v2 F12′). Step 3 is what proves the pure Rust is
        # correct **and builds on the host with no WASM toolchain**, and all
        # four crates are pure Rust with the same property — so they belong
        # in this step rather than each in a step of its own. None may be
        # dropped by a later revision of this step: a chain rewritten for
        # some new tool that lost one would take the whole practice
        # evaluator, the whole course layer, the whole agent interface or the
        # whole episode runner out of the gate silently (F12′). A later
        # section extends this same list rather than adding a step.
        3) cargo test -p sailgym-physics -p sailgym-task -p sailgym-course -p sailgym-agent -p sailgym-env ;;
        # Every audit target in one invocation: the brief §35 invariants, the
        # prohibited-shortcut greps (section 07), the convergence study and the
        # rotation/mirror sweep (section 10 tasks 10.2 and 10.3), the F7
        # provenance audit (task 10.7), and — added 2026-09-21 by human
        # approval (v2 normative delta D1; see
        # docs/v2/prds/02-conformance-bundle.md) — the conformance-bundle
        # freshness check of v2 section 02. A stale bundle is a property of
        # the physics crate, so it belongs in the step that already asks
        # whether the physics crate is still what it says it is (v2 F12').
        # `--test conformance` must stay here for the same reason
        # `-p sailgym-task` must stay in step 3.
        4) cargo test -p sailgym-physics \
               --test invariants --test no_shortcuts \
               --test convergence --test symmetry --test provenance \
               --test conformance ;;
        5) cargo test -p sailgym-physics --test regression ;;
        6) "$script_dir/build-wasm.sh" ;;
        7) pnpm --dir web typecheck ;;
        # Added 2026-09-20 by human approval (normative delta D1; see
        # docs/v2/prds/01-boat-3d-svg.md). The vitest suite was cited by every
        # section's acceptance criteria from M1 onwards and run by none of
        # them; the Playwright step moved from 8 to 9 to make room.
        8) pnpm --dir web test:unit ;;
        9)
            if [[ $fast -eq 1 ]]; then
                pnpm --dir web test:e2e --project=chromium --grep-invert @slow
            else
                pnpm --dir web test:e2e
            fi
            ;;
        # Steps 10 and 11 added 2026-09-21 by human approval (v2 normative
        # delta D1; see docs/v2/prds/03-jax-wind.md and v2 F12'). Step 10 is
        # the Python lint and format check, the counterpart of steps 1 and 2.
        # Step 11 **delegates to a script**, exactly as step 6 delegates to
        # build-wasm.sh: section 07 puts `maturin develop` in front of
        # `pytest` inside py-test.sh without amending F12 again. The chain is
        # now **eleven** steps.
        10) uv run ruff check python && uv run ruff format --check python ;;
        11) "$script_dir/py-test.sh" ;;
        *) echo "unknown step: $1" >&2; return 2 ;;
    esac
}

cd "$repo_root"

if [[ $# -gt 0 ]]; then
    selected=("$1")
elif [[ $fast -eq 1 ]]; then
    selected=($(seq 1 "$fast_total"))
else
    selected=($(seq 1 "$total"))
fi

started=$SECONDS
for i in "${selected[@]}"; do
    echo
    echo "[$i/$total] ${step_names[$((i - 1))]}"
    # `status` must be captured from the command itself. Inside `if ! cmd`,
    # `$?` is the status of the *negated* compound and is therefore always 0,
    # which made every failure exit 0 — reported as defect A in the section 05
    # and 06 handoffs, and fixed in section 07 because an acceptance criterion
    # reads this script's exit code.
    step_started=$SECONDS
    run_step "$i"
    status=$?
    if [[ $status -ne 0 ]]; then
        echo "[$i/$total] FAILED (${step_names[$((i - 1))]})" >&2
        exit "$status"
    fi
    echo "[$i/$total] ok in $((SECONDS - step_started))s"
done

echo
if [[ $fast -eq 1 ]]; then
    echo "check: steps 1-$fast_total passed (--fast subset; 10 and 11 not run) in $((SECONDS - started))s"
else
    echo "check: all steps passed in $((SECONDS - started))s"
fi
