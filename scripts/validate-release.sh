#!/bin/bash
# Validate scolta-core is ready for release.
set -e
cd "$(dirname "$0")/.."

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
echo "Version: $VERSION"

FAIL=0

if [[ "$VERSION" == *-dev ]]; then
    echo "FAIL: Version ends in -dev"
    FAIL=1
fi

if git tag -l "v$VERSION" | grep -q .; then
    echo "FAIL: Tag v$VERSION already exists"
    FAIL=1
fi

if [ $FAIL -eq 0 ]; then
    echo "PASS: Ready to release $VERSION"
else
    exit 1
fi
