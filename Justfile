set shell := ["bash", "-euo", "pipefail", "-c"]

default: help

help:
    just --list --list-prefix '  » '

fmt:
    cargo fmt --all --check

lint:
    cargo clippy --all-targets --all-features -- -D warnings

build:
    cargo build --locked --bin fmn

test:
    cargo test --all-targets --all-features

check: fmt lint test

shellcheck:
    cargo run --quiet -- init bash | shellcheck -s bash -

coverage:
    rustup run stable cargo llvm-cov --all-features --lib --tests --fail-under-lines 90

bench:
    cargo bench --bench history

stress:
    cargo test --test concurrency -- --nocapture

fuzz target="import_parse":
    cargo +nightly fuzz run {{ target }}

mutants:
    cargo mutants --workspace

deny:
    cargo deny check

release-check: check shellcheck coverage
    cargo check --manifest-path fuzz/Cargo.toml --all-targets
    dist generate --check
    dist plan
