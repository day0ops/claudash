.PHONY: build release test lint fmt check clean install uninstall

# Default target
all: check build

# Debug build
build:
	cargo build

# Optimised release build (LTO + symbol stripping)
release:
	cargo build --release

# Run unit tests
test:
	cargo test

# Run clippy lints
lint:
	cargo clippy -- -D warnings

# Check formatting
fmt:
	cargo fmt --check

# Format code in place
fmt-fix:
	cargo fmt

# Full CI check: format + lint + test + release build
check: fmt lint test

# Remove build artifacts
clean:
	cargo clean

# Install to /usr/local/bin (release build)
install: release
	cp target/release/claudash /usr/local/bin/claudash

# Remove from /usr/local/bin
uninstall:
	rm -f /usr/local/bin/claudash
