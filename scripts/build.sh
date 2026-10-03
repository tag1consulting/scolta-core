#!/usr/bin/env bash
#
# Build both browser WASM artifacts:
#
#   pkg/       scolta_core       full API: every export, custom PII patterns
#   pkg-slim/  scolta_core_slim  search and built-in PII redaction only; no
#                                regex engine, no AI helper exports. Also
#                                its module pre-gzipped and a loader for it.
#
# The slim artifact is opt-in. A consumer selects it by loading its files
# instead of the full ones, before initialization, and can confirm what it
# loaded with describe(): `artifact` and `capabilities` say which build it is.
# scripts/package-release.sh packs both; tools/src/artifacts.ts describes them.
#
# The pre-gzipped module and the loader are built with the pinned dev tools
# in package.json, so run `npm ci` once before this script.

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

# The slim module, pre-gzipped, for servers that send application/wasm
# uncompressed, and the loader that inflates it in the browser.
if [ ! -d node_modules ]; then
    echo "node_modules is missing: run npm ci first (the loader and the .gz need the pinned dev tools)." >&2
    exit 1
fi
npm run -s build:tools
npx tsc -p loader/tsconfig.json
node tools/dist/gzip-module.js pkg-slim/scolta_core_slim_bg.wasm

echo "WASM built successfully:"
for f in pkg/scolta_core_bg.wasm pkg/scolta_core.js pkg-slim/scolta_core_slim_bg.wasm pkg-slim/scolta_core_slim.js \
    pkg-slim/scolta_core_slim_bg.wasm.gz pkg-slim/scolta_core_slim_load.js; do
    echo "  $f: $(wc -c < "$f" | tr -d '[:space:]') bytes"
done
