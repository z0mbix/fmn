set shell := ["bash", "-euo", "pipefail", "-c"]

fmt:
    cargo fmt --all --check

lint:
    cargo clippy --all-targets --all-features -- -D warnings

test:
    cargo test --all-targets --all-features

check: fmt lint test

coverage:
    cargo llvm-cov --all-features --lib --tests --fail-under-lines 90

mutants:
    cargo mutants --workspace

