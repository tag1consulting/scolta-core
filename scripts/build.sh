#!/usr/bin/env bash
#
# Build both browser WASM artifacts:
#
#   pkg/       scolta_core       full API: every export, custom PII patterns
#   pkg-slim/  scolta_core_slim  search and built-in PII redaction only; no
#                                regex engine, no AI helper exports
#
# The slim artifact is opt-in. A consumer selects it by loading its files
# instead of the full ones, before initialization, and can confirm what it
# loaded with describe(): `artifact` and `capabilities` say which build it is.
# scripts/package-release.sh packs both; tools/src/artifacts.ts describes them.

set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v wasm-pack &> /dev/null; then
    echo "Installing wasm-pack..."
    curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
fi

wasm-pack build \
    --target web \
    --release \
    --locked

wasm-pack build \
    --target web \
    --release \
    --out-dir pkg-slim \
    --out-name scolta_core_slim \
    --locked \
    --no-default-features

rm -f pkg/.gitignore pkg-slim/.gitignore

echo "WASM built successfully:"
for f in pkg/scolta_core_bg.wasm pkg/scolta_core.js pkg-slim/scolta_core_slim_bg.wasm pkg-slim/scolta_core_slim.js; do
    echo "  $f: $(wc -c < "$f" | tr -d '[:space:]') bytes"
done
