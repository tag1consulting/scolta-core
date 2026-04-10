#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v wasm-pack &> /dev/null; then
    echo "Installing wasm-pack..."
    curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
fi

wasm-pack build \
    --target web \
    --release

rm -f pkg/.gitignore

echo "WASM built successfully:"
ls -la pkg/scolta_core_bg.wasm pkg/scolta_core.js
echo "Size: $(wc -c < pkg/scolta_core_bg.wasm) bytes"
