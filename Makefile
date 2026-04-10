.PHONY: build test fmt lint clean

# Build browser WASM module
build:
	./scripts/build.sh

# Run unit tests
test:
	cargo test --lib

# Format code
fmt:
	cargo fmt

# Lint
lint:
	cargo clippy -- -D warnings

# Remove build artifacts
clean:
	cargo clean
	rm -rf pkg/
