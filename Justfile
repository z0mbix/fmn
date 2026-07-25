set shell := ["bash", "-euo", "pipefail", "-c"]

fmt:
    cargo fmt --all --check

lint:
    cargo clippy --all-targets --all-features -- -D warnings

test:
    cargo test --all-targets --all-features

check: fmt lint test

coverage:
    rustup run stable cargo llvm-cov --all-features --lib --tests --fail-under-lines 90

bench:
    cargo bench --bench history

stress:
    cargo test --test concurrency -- --nocapture

fuzz target="import_parse":
    cargo +nightly fuzz run {{target}}

mutants:
    cargo mutants --workspace
