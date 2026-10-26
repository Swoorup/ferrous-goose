# Implementation and validation

This file records test coverage and platform runs. Supported API types are
listed in the [type map](type-map.md).

## Latest validation: 2026-10-27

The six patches now apply to upstream `master` commit
[`a108677d300cb0e8bbb842c35a353abe67a20c6c`](https://github.com/aardappel/goose/commit/a108677d300cb0e8bbb842c35a353abe67a20c6c),
17 commits after the previous pin. Stock and patched compilers were rebuilt
on macOS arm64. The update preserves upstream's formatter, help/completion,
version reporting and runtime additions.

| Check | Result |
|---|---|
| Each patch stage | All six pass C++ syntax checks and embedded runtime-header regeneration checks |
| Upstream compiler baseline | 4,802 checks passed with native Clang and TinyCC; optional graphics/audio/physics/UI cases were skipped |
| Workspace tests | Passed, including old/new compiler version parsing, strict borrowing/allocation tests and mandatory layout verification |
| Goose calling Rust | Stock: 23 passed, 8 skipped for unavailable features; patched: 31 passed |
| Rust calling Goose | Stock: 14 basic checks; patched: 24 full checks |
| Plain C native provider | JIT/native builds, runtime hooks, typed builders and rejection cases passed |
| Rust native provider | One-crate and combined two-crate programs passed under `goose run` and `goose build` |
| New upstream tooling | Formatter cases, source dumps retaining `reentrant`/`align`, JSON stdout and quiet interface-file output passed |
| Ordinary Goose code generation | Identical stock/patched caller and callee bodies at `-O0`; identical inlined caller bodies at `-O2` for the typed-builder example |

The local bridge checks used
`CARGO_TERM_COLOR=always SKIP_PACKAGES=1 scripts/check.sh`; the compiler
checks used `python3 test/run_tests.py --profile baseline --cc native --jobs 4`.
The full cross-platform package build runs in
[GitHub Actions](https://github.com/Swoorup/ferrous-goose/actions/workflows/ci.yml).

The stock runtime hash is now `fba599d313fe` and the patched hash is
`0d7c0dc8e74b`. The public slice, builder and stack-control layouts are
unchanged, so native ABI 1 and the eight capability names are retained.
The CLI also continues to recognize the previously validated stock runtime
`a5dbd11b6309` and the legacy version-report format.

The earlier consolidation from eight patches to six preserved compiler
behavior at the old pin. Its nine documentation examples passed locally:
five Goose callers, two Rust hosts, a native provider and a prebuilt package
without Rust on PATH. The upstream update also fixes source-dump and quiet
output integration. It does not add conditional runtime-hook emission;
unused startup/shutdown machinery is described in the performance guide.

Before these changes, [CI run 40214715127](https://github.com/Swoorup/ferrous-goose/actions/runs/40214715127)
at `044c7a95` passed all seven jobs: Linux, macOS and Windows tests, the
cross-platform package build, and no-Rust package checks on all three
platforms. That completed run covers the old upstream pin; current update
results are reported by the Actions workflow linked above.

Strict exports support host-target packaging only. Foreign targets are
rejected before building because host metadata cannot verify their layouts.

## Coverage by feature

| Feature | Implementation and tests |
|---|---|
| Arguments, builders, handle tables and panic handling | [Runtime tests](../crates/goose/tests/runtime.rs), [shim tests](../conformance/rust/tests/shims.rs) |
| Export, handle and flat-type macros | [Macros](../crates/goose-macros/src), [rejection fixtures](../crates/goose/tests/ui) |
| Metadata, generated layouts and version checks | [Binding tests](../conformance/rust/tests/bindings.rs) |
| Native-build providers and crate combination | [C provider test](../scripts/check-native-build.py), [linked regex](../examples/regex/linked), [multi-crate example](../examples/multi) |
| Rust embedding Goose | [Embedding conformance](../conformance/embed), [example](../examples/embed) |
| Typed builders, alignment, enums, optional parameters, generics and iterators | [Goose conformance programs](../conformance/rust/goose-tests) |
| Direct re-entry and coroutine callbacks | `reentry.goose`, `callbacks.goose` and `zero_copy_reentry.goose` in the conformance suite |
| Strict zero-copy exports | [Shim/allocation tests](../conformance/rust/tests/zero_copy.rs), compile-fail fixtures, binding checks and `zero_copy.goose` |
| Package generation | [Package sources](../packages), `cargo goose package`, package-relative library mappings |
| Scaffolding and test runner | `cargo goose new`, `cargo goose test`, [CLI tests](../crates/cargo-goose/tests) |
| Boundary costs | [Recorded benchmark results](../bench/RESULTS.md); these predate strict exports |

## Where it has run

| Platform | Recorded coverage |
|---|---|
| macOS arm64 | Latest upstream pin passes the local checks above; the previous pin also passed CI and no-Rust package examples. |
| Linux x86_64 | Previous series passed full CI tests, examples and no-Rust package checks at `044c7a95`. |
| Windows x86_64 | Previous series passed full CI tests, JIT/native examples and no-Rust package checks at `044c7a95`. |
| Linux arm64, Ubuntu 24.04 | Earlier compiler builds, conformance, examples and package examples passed. |
| macOS x86_64 | Universal package libraries contain this architecture; no execution results recorded. |
| CI | The [workflow](../.github/workflows/ci.yml) builds the pinned stock/patched compilers on all three runner platforms, packages the libraries, and runs their examples without Rust. |
