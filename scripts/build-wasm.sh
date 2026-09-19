#!/usr/bin/env bash
# Build the WASM package consumed by the web app.
#
# This is F12 step 6, verbatim:
#   wasm-pack build crates/sailgym-wasm --target web --out-dir ../../web/src/wasm
# (`--out-dir` is resolved relative to the crate directory, so it lands in
# web/src/wasm/ at the repository root.)
#
# CI/Linux equivalent of scripts/build-wasm.ps1; keep the two in step.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v wasm-pack >/dev/null 2>&1; then
    echo 'wasm-pack not found on PATH. Install it with: cargo install wasm-pack' >&2
    exit 1
fi

cd "$repo_root"
echo '[build-wasm] wasm-pack build crates/sailgym-wasm --target web --out-dir ../../web/src/wasm'
wasm-pack build crates/sailgym-wasm --target web --out-dir ../../web/src/wasm
echo '[build-wasm] ok -> web/src/wasm/'
