# Claude Rules for scolta-core

## Versioning (CRITICAL — read VERSIONING.md)

This project follows strict semantic versioning with synchronized major versions across all Scolta packages. **Violations of these rules are blocking errors.**

### Adding a new public function

- MUST add `since` and `stability` fields to the function's entry in `describe()`.
- MUST add `# Stability` doc comment block with `Status` and `Since` fields.
- New functions MUST start as `experimental` unless explicitly promoted.
- MUST add the function to the integration test `describe_lists_all_functions`.

### Modifying a stable function's signature

- **NEVER** change the input/output format of a stable function within a major version.
- If the behavior must change: deprecate the old function, introduce a new one alongside it.

### Deprecating a function

- MUST set `stability` to `"deprecated"` in `describe()`.
- MUST add `deprecated_in`, `replacement`, and `removal` fields.
- MUST add Rust `#[deprecated]` attribute.
- `removal` version MUST be the next major version (e.g., `"2.0.0"` if current is `1.x`).

### Removing a function

- **NEVER** remove a `stable` function without a deprecation phase.
- Removal MUST only happen in a major version bump.
- The function MUST have been `deprecated` for at least one minor release.
- CI tests in `versioning` module enforce this — they will fail if violated.

### WASM interface version

- `WASM_INTERFACE_VERSION` in `lib.rs` MUST be incremented when function signatures or calling conventions change in a way that breaks binary compatibility with host wrappers.
- Incrementing WASM interface version is a coordinated change — scolta-php, scolta-python, and scolta.js must all be updated.

### Version bumps

- PATCH: bug fixes only, no API changes.
- MINOR: new features, new functions, deprecations. Update `Cargo.toml` version.
- MAJOR: breaking changes, function removals. All packages bump together.

## Testing

- Run tests with: `cargo test` (requires temporary crate-type switch for native tests).
- WASM compilation check: `cargo check --target wasm32-wasip1`.
- All new functions MUST have corresponding tests in `tests/integration.rs`.
- Pagefind integration tests require `npx pagefind` to be available.

## Architecture

- `#[plugin_fn]` functions are thin Extism wrappers — keep logic in `inner::` module.
- `inner::` functions are the testable API surface.
- `describe()` is the single source of truth for the function manifest.
