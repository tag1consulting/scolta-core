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
wasm-pack build --target web --release

# Verify output files
test -f pkg/scolta_core_bg.wasm
test -f pkg/scolta_core.js
test -f pkg/scolta_core.d.ts
```

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
