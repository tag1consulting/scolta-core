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

### Version management and -dev workflow

The version in the repo is always either a tagged release (`0.2.0`) or a dev pre-release (`0.3.0-dev`). See VERSIONING.md "Development Versions" for the full workflow.

**When committing code:**

- If the current version already has `-dev` (e.g., `0.2.0-dev`), **do not change it**. Multiple commits accumulate on the same `-dev` version.
- If the current version is a bare release (e.g., `0.1.0`) and you are making the first change after that release, **bump to the next target with `-dev`**:
  - Bug fix only → `0.1.1-dev`
  - New feature or deprecation → `0.2.0-dev`
  - Breaking change → `1.0.0-dev` (coordinated across all packages)
- Update `Cargo.toml` `version` field.

**WARNING:** Never commit a bare version bump (e.g., `0.2.0`) without tagging it as a release. A bare version in the repo without a corresponding git tag means the release process was not completed.

## Testing

- Run tests with: `cargo test` (requires temporary crate-type switch for native tests).
- WASM compilation check: `cargo check --target wasm32-wasip1`.
- All new functions MUST have corresponding tests in `tests/integration.rs`.
- Pagefind integration tests require `npx pagefind` to be available.

## Architecture

- `#[plugin_fn]` functions are thin Extism wrappers — keep logic in `inner::` module.
- `inner::` functions are the testable API surface.
- `describe()` is the single source of truth for the function manifest.

## Documentation Rules

Documentation follows code. When a PR changes behavior, the same PR must update the relevant docs.

- **CHANGELOG.md**: Every PR that changes code (not docs-only) MUST add an entry under `## [Unreleased]`. CI enforces this.
- **README.md**: Update if the change affects installation, usage examples, or the function list.
- **describe()**: New or changed functions MUST update their `describe()` entry — this is the runtime documentation.
- **VERSIONING.md**: Only update when the versioning policy itself changes.
- Do not create separate documentation files unless explicitly requested. Keep docs in README.md and code comments.
