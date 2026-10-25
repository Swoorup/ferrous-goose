# Implementation and validation

This file records test coverage and platform runs. Supported API types are
listed in the [type map](type-map.md).

## Latest validation: 2026-10-26

On macOS arm64, `SKIP_PACKAGES=1 scripts/check.sh` passed:

| Check | Result |
|---|---|
| Workspace tests | Passed, including compile-fail fixtures, strict borrowing/allocation tests and mandatory layout verification |
| Goose calling Rust, stock compiler | 23 passed, 8 skipped for unavailable compiler features |
| Goose calling Rust, patched compiler | 31 passed |
| Rust calling Goose | 14 basic checks on stock; 24 full checks on patched |
| Plain C native provider | JIT/native builds, runtime hooks, typed builders and rejection cases passed |
| Rust native provider | One-crate and combined two-crate programs passed under `goose run` and `goose build` |

A strict host package was also built and used without Rust on PATH.
Packaging strict exports for a foreign target was rejected before the
foreign build, as required by the current layout-verification policy.

The generic compiler patch series passed 4,763 baseline checks before the
strict Rust attribute was added. The strict implementation made no further
compiler changes. Optional graphics, audio, physics and UI layers were not
built for that run.

The full package distribution was not rebuilt during this validation.
Current native-provider and strict-export behavior has not been tested on
Linux or Windows.

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

Earlier platform results apply to the original compiler series and bridge.
They do not validate later native-provider or strict-export changes.

| Platform | Recorded coverage |
|---|---|
| macOS arm64 | Latest validation above. Earlier runs also covered the standard package examples without Rust on PATH. |
| Linux arm64, Ubuntu 24.04 | Earlier compiler builds, conformance, examples and package examples passed. |
| Linux x86_64 | Earlier package libraries built; no execution results recorded. |
| macOS x86_64 | Earlier universal package libraries contain this architecture; no execution results recorded. |
| Windows x86_64 | Earlier package libraries built; no execution results recorded. |
| CI | [Workflow](../.github/workflows/ci.yml) defines Linux, macOS and Windows jobs, package builds and no-Rust checks. No CI results are recorded here. |
