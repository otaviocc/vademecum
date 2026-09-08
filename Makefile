.PHONY: build run test clean fmt lint completions install uninstall

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

completions:
	mkdir -p target/dist/completions
	cargo run --release --quiet -- --man > target/dist/vademecum.1
	for shell in bash zsh fish powershell elvish; do \
		cargo run --release --quiet -- --completions $$shell > target/dist/completions/vademecum.$$shell; \
	done

install:
	cargo install --path . --locked --force

uncompletions:
	mkdir -p target/dist/completions
	cargo run --release --quiet -- --man > target/dist/vademecum.1
	for shell in bash zsh fish powershell elvish; do \
		cargo run --release --quiet -- --completions $$shell > target/dist/completions/vademecum.$$shell; \
	done

install:
	cargo uninstall vademecum
