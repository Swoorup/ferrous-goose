# The phases, and where each stands

What each phase of the proposal promised, what implements it, and what
checks it. "Tested" means a test in this repository runs it; the platform
column says where it has run.

## Phase 1: the Rust crates (no Goose changes)

| Promise | Where | Tested |
|---|---|---|
| The `goose` crate: `GooseSlice`, the borrows that check alignment, UTF-8 and overlap with an aligned copy fallback, `GooseBuf`, `Runtime`, generation-checked handle tables with live counts, the panic catcher | `crates/goose` | `crates/goose/tests/runtime.rs` |
| `#[goose::export]`, `#[goose::handle]`, `#[derive(goose::Flat)]` | `crates/goose-macros` | `conformance/rust/tests/shims.rs`, the conformance suite |
| `cargo goose bindings`: struct declarations with pads, the extern block, the wrappers, the crate's namespace, doc comments | `crates/cargo-goose` | `conformance/rust/tests/bindings.rs` |
| The regex example runs with no hand-written glue | `examples/regex` | macOS and Linux (arm64) |
| A conformance suite covering every row of the type map, unaligned and overlapping arguments, panics and stale handles | `conformance/rust/goose-tests` | 23 programs on the stock compiler, 28 on the patched one |

## Phase 2: Cargo and Goose meet halfway

| Promise | Where | Tested |
|---|---|---|
| `cargo_crate` and `cargo_features` sidecar keys: `goose build` links a staticlib, `goose run` builds and loads a library | goose-patches/0005, `cargo goose native` | `examples/regex/linked` (run and build) |
| Several crates in one program, linked as one static library | goose-patches/0005, `cargo goose combine` | `examples/multi` (run and build) |
| Typed builders, `T[>..]&` of flat T | goose-patches/0001, `goose::GooseVec` | `typed_builders.goose`, `vecs.goose` |
| Richer exports: several results, builders with a runtime scratch builder | goose-patches/0002, `goose_build` | `conformance/embed` (full) |
| `goose check --emit-interface json` | goose-patches/0003 | `goose_build`, `cargo goose`'s layout check and its test |
| An exported `gs_abort`: panics end the program, or a contained worker, like a Goose abort | goose-patches/0004 | `contained.goose`, `leaks_at_end.goose` |
| `align n` for structs | goose-patches/0007 | `align.goose` (no copies), `conformance/embed` (aligned mirror) |
| Rust-embeds-Goose: `goose_build`, `goose::include!`, `Runtime` | `crates/goose-build` | `examples/embed`, `conformance/embed` (basic on stock, full patched) |

## Phase 3: richer types

| Promise | Where | Tested |
|---|---|---|
| Enums with data, converted at the boundary; slices in one pass | `#[derive(Flat)]` | `enums.goose`, `shims.rs` |
| `align n` for enums: zero-copy enums with data | goose-patches/0007, `#[goose(align)]` | `align.goose` |
| Generic functions through instantiation lists | `instantiate(..)` | `generics.goose` |
| Iterators: span lists with block-taking wrappers; lazy handle iterators | `impl Iterator` results | `iterators.goose` |
| Optional parameters | `Option<..>` parameters | `optionals.goose` |
| Callbacks: Rust calls the program's export fns while Goose waits on it, through the runtime change (nested calls above every live stack top) and a re-entry token | goose-patches/0008, `&goose::Reentry`, `goose::callbacks!` ([design](design.md#callbacks)) | `reentry.goose`, `bindings.rs` (`calling_back_is_reentrant`) |
| Closures as callbacks, on the stock compiler too | coroutines | `callbacks.goose` |
| Leak reports: live counts for tests, a report at the end in debug builds | handle tables, goose-patches/0004 | `handles.goose`, `leaks_at_end.goose` |

Done when each of these has a conformance test, and the type map rejects
only async functions and Rust closures kept as values (and the few shapes
[type-map.md](type-map.md#still-rejected) lists).

## Phase 4: an ecosystem

| Promise | Where | Tested |
|---|---|---|
| Wrapper packages over regex, serde_json, image and rusqlite, each with its module and a prebuilt library per platform and architecture | `packages/*`, `cargo goose package`, goose-patches/0006 | examples run with no cargo or rustc on PATH on macOS (arm64) and Linux (arm64); libraries built for macOS (universal), Linux (x86_64, arm64) and Windows (x86_64) |
| `cargo goose new` and `cargo goose test` | `crates/cargo-goose` | a scaffolded crate's test passes |
| Versioning: format and ABI versions in records and symbols | `goose::abi`, `goose_abi_1` | `bindings.rs` (`versions_are_checked`) |
| An overhead budget, measured and published | `bench/` | `bench/RESULTS.md` |

Done when someone with no Rust installed imports a crate-backed package and
runs it on Windows, macOS and Linux. Met on macOS and Linux (arm64):
`scripts/check.sh` runs every package's example under `env -i
PATH=/usr/bin:/bin`, after checking that no cargo or rustc is on it. Not met
on Windows: the libraries are built for it, but nothing has run there.

## Where it has run

| Platform | What ran | Result |
|---|---|---|
| macOS (Darwin 25.6), arm64 | `scripts/check.sh`: the Rust tests, the conformance suite both ways and the examples against both compilers, `cargo_crate`, two crates in one program, the packages with no Rust; Goose's own test suite with the series applied | passed |
| Linux arm64, Ubuntu 24.04 (a Lima VM) | `scripts/check.sh` from a clean clone, both compilers built there, the packages (`lib/linux-arm64`) with no Rust | passed |
| Linux x86_64 | nothing; the packages' libraries are built (`lib/linux-x86_64`) | not run |
| macOS x86_64 | nothing; the universal library has the slice | not run |
| Windows x86_64 | nothing; the packages' libraries are built (`lib/windows`) | not run |
| CI ([ci.yml](../.github/workflows/ci.yml)) | written for all three platforms and a no-Rust job; the repository is not on GitHub | never run |
