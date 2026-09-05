.PHONY: build run test clean fmt lint

build:
	cargo build --release

run:
	cargo run --release -- README.md

test:
	cargo test

clean:
	cargo clean

fmt:
	cargo fmt

lint:
	cargo clippy --all-targets -- -D warnings
