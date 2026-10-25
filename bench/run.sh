#!/usr/bin/env bash
# The overhead budget: builds the bench crate and the Goose program that
# times it (goose build, -O2), runs it, and writes bench/RESULTS.md.
#
#   bench/run.sh [--goose PATH]
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
goose="${GOOSE:-$root/.toolchain/goose/build/goose}"
if [[ "${1:-}" == "--goose" ]]; then goose="$2"; fi
cd "$root"
cargo build -q --release -p cargo-goose -p goose-bench
"$root/target/release/cargo-goose" bindings --manifest-path bench/rust/Cargo.toml --goose "$goose" -o bench/goose/bench.goose
lib="$root/target/release"
case "$(uname)" in
    Darwin) libfile="$lib/libgoose_bench.dylib"; platform=macos ;;
    Linux) libfile="$lib/libgoose_bench.so"; platform=linux ;;
    *) libfile="$lib/goose_bench.dll"; platform=windows ;;
esac
printf 'dylibs_%s = ["goose_bench=%s"];\n' "$platform" "$libfile" > bench/goose/overhead.build
GOOSE_PROGRESS=plain "$goose" build bench/goose/overhead.goose -o bench/goose/overhead >/dev/null
table=$(bench/goose/overhead)
{
    echo "# The overhead budget"
    echo
    echo "What Ferrous Goose's boundary checks cost per call, measured by"
    echo "\`bench/run.sh\`: each \`#[goose::export]\` function against a bare"
    echo "\`extern \"C\"\` function doing the same work, both called from a Goose"
    echo "program built with \`goose build\` (-O2). \"checks\" is the difference."
    echo
    echo "- machine: $(uname -sm), $(sysctl -n machdep.cpu.brand_string 2>/dev/null || grep -m1 'model name' /proc/cpuinfo 2>/dev/null | cut -d: -f2 || echo unknown)"
    echo "- rustc: $(rustc --version)"
    echo "- goose: $("$goose" version | tr '\n' ' ')"
    echo "- date: $(date -u +%Y-%m-%d)"
    echo
    echo "$table"
    echo
    echo "A scalar call pays for the panic catcher; a slice adds an alignment"
    echo "test (and an aligned copy when Goose's packed storage leaves it"
    echo "unaligned, which \`align n\` rules out); \`&str\` adds UTF-8 validation,"
    echo "linear in the length (take \`&[u8]\` to skip it); a \`&mut\` argument"
    echo "adds the overlap check; a handle adds the generation check and its lock."
} > bench/RESULTS.md
cat bench/RESULTS.md
