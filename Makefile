CARGO ?= cargo
RUSTUP ?= rustup

.PHONY: check test fmt build build-macos check-linux build-linux

fmt:
	$(CARGO) fmt --all -- --check

test:
	$(CARGO) test --locked

check: fmt
	$(CARGO) clippy --locked --all-targets -- -D warnings
	$(CARGO) test --locked

build:
	$(CARGO) build --locked --release
	mkdir -p bin
	cp $${CARGO_TARGET_DIR:-target}/release/bxdl bin/bxdl

build-macos:
	$(CARGO) build --locked --release --target aarch64-apple-darwin
	mkdir -p dist
	cp $${CARGO_TARGET_DIR:-target}/aarch64-apple-darwin/release/bxdl dist/bxdl-darwin-arm64

# Requires installed Rust target; this performs type checking, not a Linux link/run.
check-linux:
	$(CARGO) check --locked --target x86_64-unknown-linux-gnu

# Run on a Linux amd64 builder or with an explicitly configured cross-linker.
build-linux:
	$(CARGO) build --locked --release --target x86_64-unknown-linux-gnu
	mkdir -p dist
	cp $${CARGO_TARGET_DIR:-target}/x86_64-unknown-linux-gnu/release/bxdl dist/bxdl-linux-amd64
