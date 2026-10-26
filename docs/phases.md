# Implementation and validation

This file records test coverage and platform runs. Supported API types are
listed in the [type map](type-map.md).

## Latest validation: 2026-10-27

The six-patch consolidation was checked on macOS arm64:

- All six patches apply in order to the pinned upstream commit. Every
  intermediate stage passes C++ syntax checking, and its embedded runtime
  header matches the runtime sources.
- The fully applied compiler tree matches the previous eight-patch result,
  except for removing an obsolete patch-number comment. The runtime hash,
  eight capability names and native ABI version are unchanged.
- The compiler rebuild and `CARGO_TERM_COLOR=always SKIP_PACKAGES=1
  scripts/check.sh` passed.

| Check | Result |
|---|---|
| Workspace tests | Passed, including compile-fail fixtures, strict borrowing/allocation tests and mandatory layout verification |
| Goose calling Rust | Stock: 23 passed, 8 skipped for unavailable features; patched: 31 passed |
| Rust calling Goose | Stock: 14 basic checks; patched: 24 full checks |
| Plain C native provider | JIT/native builds, runtime hooks, typed builders and rejection cases passed |
| Rust native provider | One-crate and combined two-crate programs passed under `goose run` and `goose build` |
| Patch-guide examples | Five Goose callers, two Rust hosts, a native provider and a prebuilt package without Rust on PATH passed |

The original eight-patch compiler passed 4,763 baseline checks. The
consolidation preserves that compiler's behavior; it does not add the
conditional runtime-hook emission discussed in the performance guide.
The ordinary Goose builder example emitted identical stock/patched function
bodies at `-O0` and identical inlined caller bodies at `-O2`. These are
code-generation checks, not latency measurements.

Before consolidation, [CI run 40214715127](https://github.com/Swoorup/ferrous-goose/actions/runs/40214715127)
at `044c7a95` passed all seven jobs: Linux, macOS and Windows tests, the
cross-platform package build, and no-Rust package checks on all three
platforms. The consolidated series has been validated locally; that CI run
covers the equivalent compiler source before the reorganization.

Strict exports support host-target packaging only. Foreign targets are
rejected before building because host metadata cannot verify their layouts.
The full cross-platform package build was not repeated for the consolidation.

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
| macOS arm64 | Latest local checks above; the previous series also passed the full CI job and no-Rust package examples. |
| Linux x86_64 | Previous series passed full CI tests, examples and no-Rust package checks at `044c7a95`. |
| Windows x86_64 | Previous series passed full CI tests, JIT/native examples and no-Rust package checks at `044c7a95`. |
| Linux arm64, Ubuntu 24.04 | Earlier compiler builds, conformance, examples and package examples passed. |
| macOS x86_64 | Universal package libraries contain this architecture; no execution results recorded. |
| CI | The [workflow](../.github/workflows/ci.yml) and successful run linked above cover all three runner platforms. |
