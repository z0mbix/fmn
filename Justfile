set shell := ["bash", "-euo", "pipefail", "-c"]
export CGO_ENABLED := "0"

default: help

help:
    just --list --list-prefix '  » '

fmt:
    files="$(gofmt -l cmd internal)"; if [[ -n "$files" ]]; then printf '%s\n' "$files"; exit 1; fi

lint:
    go vet -mod=readonly ./...

build:
    go build -mod=readonly -trimpath -o target/debug/fmn ./cmd/fmn

test: build
    go test -mod=readonly ./...
    FMN_TEST_BIN="$PWD/target/debug/fmn" python3 -m unittest discover -s tests -p 'test_*.py'

test-release:
    python3 -m unittest discover -s tests -p 'test_release*.py'
    python3 -m unittest discover -s tests -p 'test_homebrew_formula.py'

shellcheck: build
    target/debug/fmn init bash | shellcheck -s bash -
    shellcheck -s sh scripts/install.sh

check: fmt lint test shellcheck

coverage:
    mkdir -p target/coverage
    rm -rf target/coverage/processes
    mkdir target/coverage/processes
    go test -mod=readonly -covermode=atomic -coverpkg=./... -coverprofile=target/coverage/unit.out ./...
    go build -mod=readonly -cover -covermode=atomic -coverpkg=./... -o target/coverage/fmn ./cmd/fmn
    GOCOVERDIR="$PWD/target/coverage/processes" FMN_TEST_BIN="$PWD/target/coverage/fmn" python3 -m unittest discover -s tests -p 'test_*.py'
    go tool covdata textfmt -i=target/coverage/processes -o=target/coverage/integration.out
    (cat target/coverage/unit.out; tail -n +2 target/coverage/integration.out) > target/coverage/total.out
    go tool cover -func=target/coverage/total.out | tee target/coverage/report.txt
    awk '$1 == "total:" { coverage = $3 + 0 } END { if (coverage < 80) { print "Statement coverage must be at least 80%"; exit 1 } }' target/coverage/report.txt

stress:
    go test -mod=readonly -count=20 -run TestConcurrent ./internal/history

fuzz package="./internal/importer" target="FuzzNativeHistory" duration="30s":
    go test -mod=readonly {{quote(package)}} -run '^$' -fuzz '^'{{quote(target)}}'$' -fuzztime {{quote(duration)}}

dist version="go-dev":
    python3 scripts/release.py build --version {{quote(version)}}

release-check: check dist
