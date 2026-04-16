## Summary

<!-- Brief description of what this PR does and why. -->

## Type of Change

- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to change)
- [ ] Documentation update
- [ ] Refactoring (no functional changes)

## Checklist

- [ ] I have read the [CLAUDE.md](../CLAUDE.md) rules for this package
- [ ] My code follows the project's coding standards
- [ ] I have added tests that prove my fix/feature works
- [ ] All existing tests pass (`cargo test`)
- [ ] WASM build succeeds (`wasm-pack build --target web --release`)
- [ ] I have updated CHANGELOG.md with a summary of my changes
- [ ] New public functions have `since` and `stability` fields in `describe()`
- [ ] No stable function signatures were changed

## Versioning

- [ ] `Cargo.toml` version has `-dev` suffix (or this is a release PR)
- [ ] `WASM_INTERFACE_VERSION` is unchanged (or this PR intentionally changes calling conventions)

## Test Plan

<!-- How did you verify this change works? -->
