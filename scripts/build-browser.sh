#!/usr/bin/env bash
set -euo pipefail

# Build the browser-targeted WASM module using wasm-pack.
# Output goes to pkg/ in wasm-pack's default structure:
#   pkg/scolta_core_bg.wasm  — the WASM binary
#   pkg/scolta_core.js       — JS glue (ESM)
#   pkg/scolta_core.d.ts     — TypeScript declarations
#   pkg/package.json         — npm package metadata

cd "$(dirname "$0")/.."

# Install wasm-pack if not present
if ! command -v wasm-pack &> /dev/null; then
    echo "Installing wasm-pack..."
    curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
fi

# Build with browser feature, targeting web (not node or bundler)
wasm-pack build \
    --target web \
    --release \
    --no-default-features \
    --features browser \
    --out-dir pkg

# Strip the auto-generated .gitignore from pkg/ — we want to commit these files
rm -f pkg/.gitignore

echo "Browser WASM built successfully:"
ls -la pkg/scolta_core_bg.wasm pkg/scolta_core.js
echo "Size: $(wc -c < pkg/scolta_core_bg.wasm) bytes"
