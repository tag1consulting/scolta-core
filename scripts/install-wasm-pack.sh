#!/usr/bin/env bash
#
# install-wasm-pack.sh [bin-dir]
#
# Install the wasm-pack release CI builds with, verified by checksum. The size
# budgets in size-budgets.json are measured against this version: a floating
# install would move them under every PR. Linux x86_64 only, as CI runs.

set -euo pipefail

VERSION="0.15.0"
SHA256="c09f971ecaed9a2efc80fdcea7a00ef6b53c7fadc8c57d1f61b53a6aa66b668a"
TARGET="x86_64-unknown-linux-musl"
BIN_DIR="${1:-$HOME/.cargo/bin}"

NAME="wasm-pack-v${VERSION}-${TARGET}"
WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

curl -fsSL "https://github.com/wasm-bindgen/wasm-pack/releases/download/v${VERSION}/${NAME}.tar.gz" \
    -o "$WORKDIR/wasm-pack.tar.gz"
echo "${SHA256}  $WORKDIR/wasm-pack.tar.gz" | sha256sum -c -
tar -xzf "$WORKDIR/wasm-pack.tar.gz" -C "$WORKDIR"
mkdir -p "$BIN_DIR"
install -m 0755 "$WORKDIR/$NAME/wasm-pack" "$BIN_DIR/wasm-pack"
"$BIN_DIR/wasm-pack" --version
