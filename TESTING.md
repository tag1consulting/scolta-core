# Scolta Core: Build and Test Instructions

## Prerequisites

- Rust toolchain (stable): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- WASM target: `rustup target add wasm32-wasip1`
- Extism CLI (optional, for testing): see https://extism.org/docs/install
- PHP 8.1+ with Extism extension (for PHP integration tests)
- Composer (for PHP dependency management)

## Build the WASM Module

```bash
cd packages/scolta-php

# Debug build (faster compilation, larger binary)
cargo build --target wasm32-wasip1

# Release build (optimized, for production)
cargo build --release --target wasm32-wasip1

# The binary is at:
# target/wasm32-wasip1/debug/scolta_core.wasm (debug)
# target/wasm32-wasip1/release/scolta_core.wasm (release)
```

## Deploy WASM to PHP Package

```bash
# Copy the release binary to the PHP package
cp target/wasm32-wasip1/release/scolta_core.wasm \
   ../scolta-php/wasm/scolta_core.wasm
```

## Run Rust Tests

```bash
# Unit tests (native, not WASM)
cargo test

# Format check
cargo fmt --check

# Lint
cargo clippy --target wasm32-wasip1 -- -D warnings
```

## Test with Extism CLI

```bash
# Test prompt resolution
echo '{"template":"expand_query","site_name":"Acme Corp","site_description":"corporate website"}' | \
  extism call target/wasm32-wasip1/release/scolta_core.wasm resolve_prompt --wasi --input=-

# Test HTML cleaning
echo '{"html":"<nav>skip</nav><main id=\"main-content\"><p>Hello world</p></main><footer>skip</footer>","title":"Test"}' | \
  extism call target/wasm32-wasip1/release/scolta_core.wasm clean_html --wasi --input=-

# Test version
extism call target/wasm32-wasip1/release/scolta_core.wasm version --wasi --input=""

# Test debug call
echo '{"function":"version","input":""}' | \
  extism call target/wasm32-wasip1/release/scolta_core.wasm debug_call --wasi --input=-
```

## PHP Integration Test

```bash
cd packages/scolta-php

# Install dependencies (including extism/extism)
composer install

# Run tests
./vendor/bin/phpunit

# Or test manually in PHP:
php -r "
require 'vendor/autoload.php';
use Tag1\Scolta\Wasm\ScoltaWasm;
echo 'WASM version: ' . ScoltaWasm::version() . PHP_EOL;
echo 'Prompt: ' . substr(ScoltaWasm::getPrompt('expand_query'), 0, 100) . '...' . PHP_EOL;

ScoltaWasm::enableDebug();
\$clean = ScoltaWasm::cleanHtml('<p>Hello <b>world</b></p>', 'Test');
echo 'Clean HTML: ' . \$clean . PHP_EOL;
print_r(ScoltaWasm::getDebugLog());
"
```

## Platform Adapter Testing

### Drupal
```bash
cd packages/scolta-drupal
composer install
# Ensure scolta-core has the WASM binary in wasm/
drush scolta:build  # Should work identically to before
```

### WordPress
```bash
cd packages/scolta-wp
composer install
# Activate plugin, run: wp scolta build
```

### Laravel
```bash
cd packages/scolta-laravel
composer install
php artisan scolta:build  # Should work identically to before
```

## Verifying Consistency

The WASM module guarantees identical behavior across platforms. To verify:

1. Run the same HTML through clean_html in Rust tests, PHP, and the Extism CLI
2. Compare output byte-for-byte
3. Run the same scoring inputs through score_results in all three
4. Verify JSON output matches exactly

## Debug Mode

Enable debug logging in PHP to see all WASM calls:

```php
use Tag1\Scolta\Wasm\ScoltaWasm;

ScoltaWasm::enableDebug();

// ... run your operations ...

// Dump the log
foreach (ScoltaWasm::getDebugLog() as $entry) {
    printf("[%s] %dB in, %dB out, %.2fms\n",
        $entry['function'],
        $entry['input_size'],
        $entry['output_size'],
        $entry['time_ms']
    );
}
```

## Troubleshooting

### "Scolta WASM module not found"
- Build the WASM module and copy to `packages/scolta-php/wasm/scolta_core.wasm`
- Or set a custom path: `ScoltaWasm::setWasmPath('/path/to/scolta_core.wasm')`

### "Class Extism\Plugin not found"
- Install the Extism PHP SDK: `composer require extism/extism`
- Install the Extism runtime: see https://extism.org/docs/install

### Scoring differences from previous PHP version
- The WASM module IS the canonical implementation now
- Previous PHP code may have had subtle floating-point differences
- Use debug mode to compare inputs/outputs
