.PHONY: build-server build-browser build-all test fmt lint clean

# Server-side WASM (Extism/WASI) — used by PHP for build-time content processing
build-server:
	cargo build --release --target wasm32-wasip1
	cp target/wasm32-wasip1/release/scolta_core.wasm ../scolta-php/wasm/scolta_core.wasm

# Browser-side WASM (wasm-bindgen) — used by scolta.js for client-side scoring
build-browser:
	./scripts/build-browser.sh

# Build both targets
build-all: build-server build-browser

# Run tests (default features = extism)
test:
	cargo test --lib

# Format code
fmt:
	cargo fmt

# Lint
lint:
	cargo clippy --target wasm32-wasip1 -- -D warnings

# Remove build artifacts
clean:
	cargo clean
	rm -rf pkg/
