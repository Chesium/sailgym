#!/usr/bin/env bash
# The sailgym gate. Runs the F12 chain, in order, failing fast.
#
# Linux/CI equivalent of scripts/check.ps1; keep the two in step.
# Usage:
#   scripts/check.sh          run the whole chain
#   scripts/check.sh 3        run step 3 only

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script_dir="$repo_root/scripts"

step_names=(
    'cargo fmt --check'
    'cargo clippy --all-targets -- -D warnings'
    'cargo test -p sailgym-physics'
    'cargo test -p sailgym-physics --test invariants'
    'cargo test -p sailgym-physics --test regression'
    'wasm-pack build crates/sailgym-wasm --target web --out-dir ../../web/src/wasm'
    'pnpm --dir web typecheck'
    'pnpm --dir web test:e2e'
)
total=${#step_names[@]}

run_step() {
    case "$1" in
        1) cargo fmt --check ;;
        2) cargo clippy --all-targets -- -D warnings ;;
        3) cargo test -p sailgym-physics ;;
        4) cargo test -p sailgym-physics --test invariants ;;
        5) cargo test -p sailgym-physics --test regression ;;
        6) "$script_dir/build-wasm.sh" ;;
        7) pnpm --dir web typecheck ;;
        8) pnpm --dir web test:e2e ;;
        *) echo "unknown step: $1" >&2; return 2 ;;
    esac
}

cd "$repo_root"

if [[ $# -gt 0 ]]; then
    selected=("$1")
else
    selected=($(seq 1 "$total"))
fi

for i in "${selected[@]}"; do
    echo
    echo "[$i/$total] ${step_names[$((i - 1))]}"
    if ! run_step "$i"; then
        status=$?
        echo "[$i/$total] FAILED (${step_names[$((i - 1))]})" >&2
        exit "$status"
    fi
done

echo
echo 'check: all steps passed'
