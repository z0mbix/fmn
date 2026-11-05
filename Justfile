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
    go build -mod=readonly -trimpath -o bin/fmn ./cmd/fmn

# Remove local builds, release archives, coverage, and Python caches.
clean:
    rm -f bin/fmn
    rm -rf bin/dist bin/coverage
    find bin tests -type d -name __pycache__ -prune -exec rm -rf {} +

test: build
    go test -mod=readonly ./...
    FMN_TEST_BIN="$PWD/bin/fmn" python3 -m unittest discover -s tests -p 'test_*.py'

test-release:
    python3 -m unittest discover -s tests -p 'test_release*.py'
    python3 -m unittest discover -s tests -p 'test_homebrew_formula.py'

shellcheck: build
    bin/fmn init bash | shellcheck -s bash -
    shellcheck -s sh bin/install.sh

check: fmt lint test shellcheck

coverage:
    mkdir -p bin/coverage
    rm -rf bin/coverage/processes
    mkdir bin/coverage/processes
    go test -mod=readonly -covermode=atomic -coverpkg=./... -coverprofile=bin/coverage/unit.out ./...
    go build -mod=readonly -cover -covermode=atomic -coverpkg=./... -o bin/coverage/fmn ./cmd/fmn
    GOCOVERDIR="$PWD/bin/coverage/processes" FMN_TEST_BIN="$PWD/bin/coverage/fmn" python3 -m unittest discover -s tests -p 'test_*.py'
    go tool covdata textfmt -i=bin/coverage/processes -o=bin/coverage/integration.out
    (cat bin/coverage/unit.out; tail -n +2 bin/coverage/integration.out) > bin/coverage/total.out
    go tool cover -func=bin/coverage/total.out | tee bin/coverage/report.txt
    awk '$1 == "total:" { coverage = $3 + 0 } END { if (coverage < 80) { print "Statement coverage must be at least 80%"; exit 1 } }' bin/coverage/report.txt

stress:
    go test -mod=readonly -count=20 -run TestConcurrent ./internal/history

fuzz package="./internal/importer" target="FuzzNativeHistory" duration="30s":
    go test -mod=readonly {{quote(package)}} -run '^$' -fuzz '^'{{quote(target)}}'$' -fuzztime {{quote(duration)}}

dist version="go-dev":
    python3 bin/release.py build --version {{quote(version)}}

release-check: check dist
