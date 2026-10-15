# Scolta Core: Build and Test Instructions

## Prerequisites

- Rust toolchain (stable): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- wasm-pack (for WASM builds): `cargo install wasm-pack`
- Composer (for PHP dependency management)

## Run Rust Tests

```bash
cd packages/scolta-core

# Unit tests (native, no WASM runtime needed)
cargo test

# Format check
cargo fmt --check

# Lint
cargo clippy -- -D warnings
```

## Build the WASM Module

```bash
cd packages/scolta-core

# Release build (optimized for size)
./scripts/build.sh   # full artifact into pkg/, slim into pkg-slim/

# Verify output files
test -f pkg/scolta_core_bg.wasm
test -f pkg/scolta_core.js
test -f pkg/scolta_core.d.ts
test -f pkg-slim/scolta_core_slim_bg.wasm
test -f pkg-slim/scolta_core_slim.js
```

## Test the Built Artifacts

Needs Node 22 or later. These run on the release tarballs, so a file the packager drops fails here.

```bash
npm ci
npx playwright install chromium firefox webkit   # one-time

# Raw, gzip -9 and Brotli 11 bytes per file and per artifact, as JSON;
# exits non-zero over a budget in size-budgets.json.
npm run measure:size
npm run measure:size -- --tarball dist/scolta-core-ci.tar.gz   # a packed tarball

# Both artifacts in Chromium, Firefox and WebKit, served with a strict CSP:
# the search parity fixture, the sanitizer differential fixture, malformed
# input, custom patterns per capability, and hostile inputs in a worker with
# an external deadline.
npm run test:browser

# Cold init, first call and warm timings (median and p95) for any builds.
npm run bench:browser -- full=pkg slim=pkg-slim
```

`tests/fixtures/sanitize-differential.json` is generated from the `regex` crate reference by
`SCOLTA_WRITE_FIXTURES=1 cargo test --lib sanitize_browser_fixture`, and a unit test fails if it goes stale.
`tests/fixtures/search-parity.json` was captured from scolta-core `main` before the slim artifact existed
(`npm run fixtures:capture -- --dir <pkg> --source <description>`); recapture it only for a deliberate
behavior change. `tests/fixtures/size-baseline.json` records the sizes and timings this change was
measured against.

## Platform Adapter Testing

### PHP
```bash
cd packages/scolta-php
composer install
./vendor/bin/phpunit
```

### Drupal
```bash
cd packages/scolta-drupal
composer install
./vendor/bin/phpunit
```

### WordPress
```bash
cd packages/scolta-wp
composer install
./vendor/bin/phpunit
```

### Laravel
```bash
cd packages/scolta-laravel
composer install
./vendor/bin/phpunit
```

## Verifying Consistency

The WASM module guarantees identical behavior across platforms. To verify:

1. Run the same scoring inputs through `score_results` in Rust tests and in each platform adapter
2. Verify JSON output matches exactly
3. Run `describe()` to confirm the function manifest matches expectations

## Troubleshooting

### "wasm-pack not found"
- Install with: `cargo install wasm-pack`

### "pkg/ directory missing after build"
- Run `wasm-pack build --target web --release` from `packages/scolta-core/`
- The `pkg/` directory is created by wasm-pack

### Scoring differences from expected values
- The WASM module IS the canonical implementation
- Run `cargo test` to verify the inner functions match expected behavior
- Check `ScoringConfig` defaults — the scoring algorithm uses additive boosts, not multiplicative
