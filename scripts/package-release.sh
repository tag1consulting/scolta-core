#!/usr/bin/env bash
#
# package-release.sh <version> <out-dir>
#
# Pack the two release tarballs from the build directories scripts/build.sh
# writes:
#
#   scolta-core-<version>.tar.gz       the full artifact, the same four files
#                                      every release has shipped
#   scolta-core-slim-<version>.tar.gz  the slim artifact: wasm-pack's four
#                                      files, the module pre-gzipped, and
#                                      the loader for it with its .d.ts
#
# Members are named explicitly, never globbed, so wasm-pack's package.json,
# README.md and LICENSE never ride along. scripts/validate-tarball.sh is the
# check on what this produces; ci.yml and release.yml both run it on both.

set -euo pipefail

if [ "$#" -ne 2 ]; then
    echo "usage: $0 <version> <out-dir>" >&2
    exit 2
fi

VERSION="$1"
OUT="$2"

cd "$(dirname "$0")/.."
mkdir -p "$OUT"

tar -czf "$OUT/scolta-core-${VERSION}.tar.gz" \
    -C pkg \
    scolta_core_bg.wasm \
    scolta_core.js \
    scolta_core.d.ts \
    scolta_core_bg.wasm.d.ts

tar -czf "$OUT/scolta-core-slim-${VERSION}.tar.gz" \
    -C pkg-slim \
    scolta_core_slim_bg.wasm \
    scolta_core_slim.js \
    scolta_core_slim.d.ts \
    scolta_core_slim_bg.wasm.d.ts \
    scolta_core_slim_bg.wasm.gz \
    scolta_core_slim_load.js \
    scolta_core_slim_load.d.ts

echo "$OUT/scolta-core-${VERSION}.tar.gz"
echo "$OUT/scolta-core-slim-${VERSION}.tar.gz"
