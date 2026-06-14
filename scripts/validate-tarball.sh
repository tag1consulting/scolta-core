#!/usr/bin/env bash
#
# validate-tarball.sh — fail-closed content sweep of the release tarball.
#
# scolta-core ships exactly ONE pre-built artifact: scolta-core-${VERSION}.tar.gz,
# built from the wasm-pack `pkg/` output (the crate is not on crates.io and the
# compiled WASM is not committed). `pkg/` is wasm-pack's, not ours: a wasm-pack
# upgrade can silently start dropping new files there (package.json, README.md,
# LICENSE, .gitignore, snippets/ ...). The release tar selects members by name,
# so today's artifact is clean — but nothing GUARDS that selection, and nothing
# guards the size of what we do ship.
#
# This script is the guard. It extracts the tarball and asserts:
#   1. Every member is on the explicit allowlist below (fail-closed: an unknown
#      member fails the build, with a wasm-pack-version hint).
#   2. The .wasm payload stays under a hard size cap (~2x the current build).
# It is the single source of truth, called from BOTH ci.yml (every PR) and
# release.yml (at tag time). Edit the allowlist HERE when the artifact's file
# set legitimately changes.
#
# Usage: scripts/validate-tarball.sh <path-to-tarball.tar.gz>

set -euo pipefail

if [ "$#" -ne 1 ]; then
    echo "usage: $0 <path-to-tarball.tar.gz>" >&2
    exit 2
fi

TARBALL="$1"

if [ ! -f "$TARBALL" ]; then
    echo "FAIL: tarball not found: $TARBALL" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# FAIL-CLOSED ALLOWLIST — the exact set of files the artifact is allowed to ship.
#
# Determined by inspecting `wasm-pack build --target web --release` output with
# wasm-pack 0.13.1. wasm-pack also drops package.json, README.md, LICENSE, and a
# .gitignore (containing `*`) into pkg/ — NONE of those belong in the published
# WASM artifact and they are deliberately NOT listed here. .d.ts files (both the
# JS bindings .d.ts and the _bg.wasm.d.ts) ARE legitimate and shipped.
#
# If a future wasm-pack version emits a different set, this list must be reviewed
# and updated deliberately — do not just append the new file.
# ---------------------------------------------------------------------------
ALLOWED=(
    "scolta_core_bg.wasm"
    "scolta_core.js"
    "scolta_core.d.ts"
    "scolta_core_bg.wasm.d.ts"
)

# ---------------------------------------------------------------------------
# SIZE CAP — measured against the wasm-pack 0.13.1 --release build on 2026-06-14:
#   scolta_core_bg.wasm = 1,244,431 bytes (~1.19 MiB).
# Cap set at ~2x the measured payload to catch a runaway (debug build leaking
# into release, an accidentally-vendored blob, opt-level regression) while
# leaving normal growth headroom. Bump deliberately if the WASM legitimately
# grows past this.
# ---------------------------------------------------------------------------
WASM_MAX_BYTES=2500000   # ~2x of measured 1,244,431

WASM_HINT="wasm-pack version drift or a debug/unstripped build leaking into release"

WORKDIR="$(mktemp -d)"
cleanup() { rm -rf "$WORKDIR"; }
trap cleanup EXIT

echo "Validating release tarball: $TARBALL"
tar -xzf "$TARBALL" -C "$WORKDIR"

# Collect the member list (files only; reject any unexpected directories too).
MEMBERS="$(tar -tzf "$TARBALL")"

FAIL=0

# 1. Fail-closed allowlist sweep.
while IFS= read -r member; do
    [ -z "$member" ] && continue
    # Normalise a possible leading "./" that some tar implementations emit.
    name="${member#./}"
    # A trailing slash means a directory entry — never expected in this artifact.
    if [[ "$name" == */ ]]; then
        echo "FAIL: tarball contains an unexpected directory entry: '$name'" >&2
        echo "      The artifact must be a flat set of files. ($WASM_HINT.)" >&2
        echo "      Allowlist lives in scripts/validate-tarball.sh." >&2
        FAIL=1
        continue
    fi
    ok=0
    for allowed in "${ALLOWED[@]}"; do
        if [ "$name" = "$allowed" ]; then
            ok=1
            break
        fi
    done
    if [ "$ok" -ne 1 ]; then
        echo "FAIL: tarball ships an unexpected file: '$name'" >&2
        echo "      It is NOT on the allowlist in scripts/validate-tarball.sh." >&2
        echo "      Likely cause: $WASM_HINT." >&2
        echo "      If this file is legitimate, add it to the ALLOWED list in" >&2
        echo "      scripts/validate-tarball.sh; otherwise exclude it from the tar" >&2
        echo "      step in release.yml / ci.yml." >&2
        FAIL=1
    fi
done <<< "$MEMBERS"

# 2. Every allowlisted file must actually be present (artifact completeness).
for allowed in "${ALLOWED[@]}"; do
    if [ ! -f "$WORKDIR/$allowed" ]; then
        echo "FAIL: required member missing from tarball: '$allowed'" >&2
        echo "      The wasm-pack build did not produce it, or the tar step in" >&2
        echo "      release.yml/ci.yml dropped it. ($WASM_HINT.)" >&2
        FAIL=1
    fi
done

# 3. Size cap on the WASM payload.
WASM_PATH="$WORKDIR/scolta_core_bg.wasm"
if [ -f "$WASM_PATH" ]; then
    WASM_SIZE="$(wc -c < "$WASM_PATH" | tr -d '[:space:]')"
    echo "scolta_core_bg.wasm: $WASM_SIZE bytes (cap $WASM_MAX_BYTES)"
    if [ "$WASM_SIZE" -gt "$WASM_MAX_BYTES" ]; then
        echo "FAIL: scolta_core_bg.wasm is $WASM_SIZE bytes, over the" >&2
        echo "      $WASM_MAX_BYTES-byte cap (~2x the expected ~1.19 MiB release build)." >&2
        echo "      Likely cause: $WASM_HINT, an unstripped/debug build, or a" >&2
        echo "      large new dependency. Cap lives in scripts/validate-tarball.sh." >&2
        FAIL=1
    fi
fi

if [ "$FAIL" -ne 0 ]; then
    echo "" >&2
    echo "Tarball validation FAILED. See messages above." >&2
    exit 1
fi

echo "PASS: tarball contains exactly the allowlisted members and is within the size cap."
