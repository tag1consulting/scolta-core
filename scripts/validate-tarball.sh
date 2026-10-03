#!/usr/bin/env bash
#
# validate-tarball.sh — fail-closed content sweep of the release tarball.
#
# scolta-core ships two pre-built artifacts, packed by scripts/package-release.sh:
# scolta-core-${VERSION}.tar.gz (full, from `pkg/`) and
# scolta-core-slim-${VERSION}.tar.gz (slim, from `pkg-slim/`), both built from
# wasm-pack output (the crate is not on crates.io and the
# compiled WASM is not committed). `pkg/` is wasm-pack's, not ours: a wasm-pack
# upgrade can silently start dropping new files there (package.json, README.md,
# LICENSE, .gitignore, snippets/ ...). The release tar selects members by name,
# so today's artifact is clean — but nothing GUARDS that selection, and nothing
# guards the size of what we do ship.
#
# This script is the guard. It extracts the tarball and asserts:
#   1. Every member is on the explicit allowlist for its artifact (fail-closed:
#      an unknown member fails the build, with a wasm-pack-version hint). The
#      artifact is identified by its .wasm member; a tarball holding neither
#      fails.
#   2. The .wasm payload stays under a hard runaway cap. The real raw, gzip
#      and Brotli budgets live in size-budgets.json and are enforced by
#      `npm run measure:size`, which ci.yml and release.yml run on the same
#      tarballs.
# It is the single source of truth, called from BOTH ci.yml (every PR) and
# release.yml (at tag time). Edit the allowlist HERE when the artifact's file
# set legitimately changes.
#
# Usage: scripts/validate-tarball.sh <path-to-tarball.tar.gz>  (either artifact)

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
# FAIL-CLOSED ALLOWLISTS — the exact set of files each artifact may ship.
#
# Both are the four files `wasm-pack build --target web --release` writes for
# the module, named after its --out-name (scolta_core for the full artifact,
# scolta_core_slim for the slim one). Checked against wasm-pack 0.15.0.
# wasm-pack also drops package.json, README.md, LICENSE, and a .gitignore
# (containing `*`) into its output directory: NONE of those belong in a
# published artifact and they are deliberately NOT listed here. .d.ts files
# (both the JS bindings .d.ts and the _bg.wasm.d.ts) ARE legitimate and shipped.
#
# If a future wasm-pack version emits a different set, this list must be reviewed
# and updated deliberately — do not just append the new file.
#
# The slim artifact adds three files scripts/build.sh writes after wasm-pack:
# the module pre-gzipped (${stem}_bg.wasm.gz, checked below to inflate to the
# module byte for byte) and the loader that fetches it, with its .d.ts. The
# full artifact never gains a member: downstream updaters reject any extra.
# ---------------------------------------------------------------------------
allowed_for() {
    local stem="$1"
    printf '%s\n' "${stem}_bg.wasm" "${stem}.js" "${stem}.d.ts" "${stem}_bg.wasm.d.ts"
    if [ "$stem" = "scolta_core_slim" ]; then
        printf '%s\n' "${stem}_bg.wasm.gz" "${stem}_load.js" "${stem}_load.d.ts"
    fi
}

# ---------------------------------------------------------------------------
# RUNAWAY CAPS: about 2x each artifact's measured .wasm, to catch a debug or
# unstripped build, an accidentally vendored blob or a lost opt-level. They
# are not the size budgets: size-budgets.json holds those, in raw, gzip and
# Brotli bytes with small headroom, and `npm run measure:size` enforces them.
#   full: 1,228,907 bytes measured 2026-10-03 (rustc 1.98.1, wasm-pack 0.15.0)
#   slim:   305,660 bytes measured 2026-10-03 (same toolchain)
# ---------------------------------------------------------------------------
max_wasm_bytes_for() {
    case "$1" in
        scolta_core) echo 2500000 ;;
        scolta_core_slim) echo 650000 ;;
    esac
}

WASM_HINT="wasm-pack version drift or a debug/unstripped build leaking into release"

WORKDIR="$(mktemp -d)"
cleanup() { rm -rf "$WORKDIR"; }
trap cleanup EXIT

echo "Validating release tarball: $TARBALL"
tar -xzf "$TARBALL" -C "$WORKDIR"

# Collect the member list (files only; reject any unexpected directories too).
MEMBERS="$(tar -tzf "$TARBALL")"

FAIL=0

# Which artifact this is, from its module. Exactly one must be present.
STEM=""
for candidate in scolta_core scolta_core_slim; do
    if grep -qx "\(\./\)\{0,1\}${candidate}_bg.wasm" <<< "$MEMBERS"; then
        if [ -n "$STEM" ]; then
            echo "FAIL: tarball holds more than one artifact's module." >&2
            FAIL=1
        fi
        STEM="$candidate"
    fi
done
if [ -z "$STEM" ]; then
    echo "FAIL: tarball holds neither scolta_core_bg.wasm nor scolta_core_slim_bg.wasm." >&2
    echo "      ($WASM_HINT, or the wrong directory was packed.)" >&2
    exit 1
fi
echo "Artifact: $STEM"
ALLOWED=()
while IFS= read -r f; do ALLOWED+=("$f"); done < <(allowed_for "$STEM")
WASM_MAX_BYTES="$(max_wasm_bytes_for "$STEM")"

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
        echo "      It is NOT on the $STEM allowlist in scripts/validate-tarball.sh." >&2
        echo "      Likely cause: $WASM_HINT." >&2
        echo "      If this file is legitimate, add it to allowed_for in" >&2
        echo "      scripts/validate-tarball.sh; otherwise exclude it in" >&2
        echo "      scripts/package-release.sh." >&2
        FAIL=1
    fi
done <<< "$MEMBERS"

# 2. Every allowlisted file must actually be present (artifact completeness).
for allowed in "${ALLOWED[@]}"; do
    if [ ! -f "$WORKDIR/$allowed" ]; then
        echo "FAIL: required member missing from tarball: '$allowed'" >&2
        echo "      The wasm-pack build did not produce it, or" >&2
        echo "      scripts/package-release.sh dropped it. ($WASM_HINT.)" >&2
        FAIL=1
    fi
done

# 3. Runaway cap on the WASM payload.
WASM_PATH="$WORKDIR/${STEM}_bg.wasm"
if [ -f "$WASM_PATH" ]; then
    WASM_SIZE="$(wc -c < "$WASM_PATH" | tr -d '[:space:]')"
    echo "${STEM}_bg.wasm: $WASM_SIZE bytes (runaway cap $WASM_MAX_BYTES)"
    if [ "$WASM_SIZE" -gt "$WASM_MAX_BYTES" ]; then
        echo "FAIL: ${STEM}_bg.wasm is $WASM_SIZE bytes, over its" >&2
        echo "      $WASM_MAX_BYTES-byte runaway cap (about 2x its expected size)." >&2
        echo "      Likely cause: $WASM_HINT, an unstripped/debug build, or a" >&2
        echo "      large new dependency. Cap lives in scripts/validate-tarball.sh." >&2
        FAIL=1
    fi
fi

# 4. A pre-gzipped module must be the module: a stale or corrupt .gz would
#    load a different build than the one beside it, or none.
GZ_PATH="$WORKDIR/${STEM}_bg.wasm.gz"
if [ -f "$GZ_PATH" ] && [ -f "$WASM_PATH" ]; then
    if gzip -dc "$GZ_PATH" | cmp -s - "$WASM_PATH"; then
        echo "${STEM}_bg.wasm.gz: $(wc -c < "$GZ_PATH" | tr -d '[:space:]') bytes, inflates to ${STEM}_bg.wasm"
    else
        echo "FAIL: ${STEM}_bg.wasm.gz does not inflate to ${STEM}_bg.wasm." >&2
        echo "      scripts/build.sh writes it after wasm-pack; rebuild both together." >&2
        FAIL=1
    fi
fi

if [ "$FAIL" -ne 0 ]; then
    echo "" >&2
    echo "Tarball validation FAILED. See messages above." >&2
    exit 1
fi

echo "PASS: tarball contains exactly the allowlisted members and is within the size cap."
