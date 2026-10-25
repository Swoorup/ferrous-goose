#!/usr/bin/env bash
# Everything, against the stock and the patched compiler: the Rust tests,
# the conformance suite in both directions, the examples and the packages'
# examples (with no Rust on PATH). scripts/toolchain.sh builds the compilers.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
stock="$root/.toolchain/goose-stock/build/goose"
patched="$root/.toolchain/goose/build/goose"
export GOOSE_PROGRESS=plain

step() { printf '\n== %s\n' "$*"; }

step "Rust tests"
cargo test -q --workspace 2>&1 | grep -E 'test result|FAILED|panicked' | sort | uniq -c

cargo build -q --release -p cargo-goose
cg="$root/target/release/cargo-goose"

for g in "$stock" "$patched"; do
    name=$([[ "$g" == "$stock" ]] && echo stock || echo patched)
    step "Goose using Rust ($name)"
    "$cg" test --manifest-path conformance/rust/Cargo.toml --goose "$g" conformance/rust/goose-tests | tail -1
    step "Rust using Goose ($name)"
    GOOSE="$g" cargo run -q -p goose-embed-conformance
    step "examples ($name)"
    "$cg" run examples/regex/goose/find_emails.goose --manifest-path examples/regex/rust/Cargo.toml --goose "$g" | tail -1
    "$cg" run examples/shapes/goose/main.goose --manifest-path examples/shapes/rust/Cargo.toml --goose "$g" | tail -1
    GOOSE="$g" cargo run -q -p goose-embed-example | tail -1
done

step "cargo_crate (patched)"
GOOSE_CARGO_GOOSE="$cg" "$patched" run examples/regex/linked/find_emails.goose | tail -1
GOOSE_CARGO_GOOSE="$cg" "$patched" build examples/regex/linked/find_emails.goose >/dev/null
examples/regex/linked/find_emails | tail -1
step "two crates in one program (patched)"
(cd examples/multi && GOOSE_CARGO_GOOSE="$cg" "$patched" run multi.goose | tail -1)
(cd examples/multi && GOOSE_CARGO_GOOSE="$cg" "$patched" build multi.goose >/dev/null && ./multi | tail -1)

if [[ -n "${SKIP_PACKAGES:-}" ]]; then
    step "all checks passed (packages skipped)"
    exit 0
fi
step "packages, with no Rust on PATH (patched)"
if env -i PATH=/usr/bin:/bin sh -c 'command -v cargo || command -v rustc'; then
    echo "Rust is on /usr/bin:/bin, so this would not show running without it"
    exit 1
fi
for ex in packages/*/dist/examples/*.goose; do
    [[ -e "$ex" ]] || { echo "no packages built: cargo goose package packages/<name>/dist"; break; }
    d=$(dirname "$ex")
    if (cd "$d" && env -i HOME="$HOME" PATH=/usr/bin:/bin "$patched" run "$(basename "$ex")" 2>/dev/null) | diff -q - "${ex%.goose}.expected" >/dev/null; then
        echo "ok   $ex"
    else
        echo "FAIL $ex"
        exit 1
    fi
done
step "all checks passed"
