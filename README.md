# Ferrous Goose

Goose programs calling Rust crates, and Rust programs calling Goose, with no
C, no headers and no unsafe Rust in user code.

A crate marks what Goose may call:

```rust
#[goose::handle]
pub struct Regex(regex::Regex);

#[goose::export]
pub fn compile(pattern: &str) -> Result<Regex, regex::Error> {
    regex::Regex::new(pattern).map(Regex)
}

#[goose::export]
pub fn find<'t>(re: &Regex, text: &'t str) -> Option<&'t str> {
    re.0.find(text).map(|m| m.as_str())
}
```

and a Goose program imports the module `cargo goose` writes from it:

```goose
import rx;

fn main() {
    let re, ok = rx::compile("[0-9]+");
    let hit, found = rx::find(re, "order 1001 shipped");   // a view of the text, no copy
    if found { print(hit); }
    rx::release(re);
}
```

The other way, a Rust program embeds Goose code:

```rust
// build.rs
fn main() { goose_build::Program::new("goose/kernels.goose").compile().unwrap(); }

// src/main.rs
goose::include!(kernels);
fn main() {
    let rt = goose::Runtime::init().unwrap();      // !Send: Goose calls stay on this thread
    println!("{} words", kernels::count_words(&rt, b"no heap, no gc"));
}
```

This repository implements the design in
[the Ferrous Goose proposal](docs/design.md) through its fourth phase. Goose
itself is pinned at [`e76d5c9f`](https://github.com/aardappel/goose/commit/e76d5c9f7f0a17d83c12a405fbb2513094d03985),
and the compiler changes the later phases need are a patch series,
[goose-patches/](goose-patches/), which `scripts/toolchain.sh` applies and
builds. Everything Phase 1 promises works on the stock compiler.

## What is here

| Path | What it is |
|---|---|
| [crates/goose](crates/goose) | `ferrous-goose`, imported as `goose`: the runtime the shims use (argument holders that check alignment, UTF-8 and aliasing, builders, handle tables, the panic catcher, the records `cargo goose` reads) and, for embedding, `Runtime` and `include!` |
| [crates/goose-macros](crates/goose-macros) | `#[goose::export]`, `#[goose::handle]`, `#[derive(goose::Flat)]` |
| [crates/cargo-goose](crates/cargo-goose) | `cargo goose bindings / build / run / test / new / package / native` |
| [crates/goose-build](crates/goose-build) | `goose_build` for build.rs: compiles a Goose program into a crate and writes safe Rust bindings |
| [goose-patches](goose-patches) | the Goose changes, one patch per feature ([docs/goose-patches.md](docs/goose-patches.md)) |
| [conformance](conformance) | every row of the type map, in both directions, with the negative cases |
| [examples](examples) | the regex crate from Goose (dynamically and linked through a native-build provider), two crates in one program, Goose kernels from Rust, shapes |
| [packages](packages) | Goose packages over regex, serde_json, image and rusqlite, prebuilt per platform |
| [bench](bench) | the overhead budget: what the checks cost per call ([bench/RESULTS.md](bench/RESULTS.md)) |
| [docs](docs) | [the design](docs/design.md), [the type map](docs/type-map.md), [the patches](docs/goose-patches.md), [the phases and their status](docs/phases.md) |

## Getting started

```sh
scripts/toolchain.sh            # the patched Goose, in .toolchain/goose/build/goose
scripts/toolchain.sh --stock    # the pinned Goose without the patches
cargo install --path crates/cargo-goose   # the `cargo goose` subcommand
```

`GOOSE_REPO=~/github/goose scripts/toolchain.sh` clones from a local copy
instead of GitHub. `cargo goose` finds the compiler through `--goose`,
`$GOOSE`, a `.toolchain/goose/build/goose` above the working directory, or
`goose` on PATH.

Goose using Rust, from a crate with `crate-type = ["cdylib"]`:

```sh
cargo goose new my-crate        # a crate with the attributes in place, a Goose example and a test
cd my-crate
cargo goose run goose/main.goose
cargo goose test                # the Goose programs in goose-tests/
cargo goose bindings -o ns.goose   # just the module
```

or, with the patched compiler, a sidecar that builds and links the crate
through an external provider, so `goose run` and `goose build` invoke it automatically:

```goose
// find_emails.build
native_build = ["cargo-goose", "native", "--manifest-path", "../rust/Cargo.toml"];
```

`cargo-goose` must be on PATH; the command runs from the sidecar's directory.
The compiler's provider protocol is language-neutral ([details](docs/goose-patches.md#external-native-builders)).

Rust may call the program back while it waits (the patched compiler):

```rust
goose::callbacks! { fn on_node(depth: i64, label: &str) -> i64; }

#[goose::export]
pub fn walk(depth: i64, goose: &goose::Reentry) -> Result<i64, goose::ReentryError> {
    (0..depth).map(|d| on_node(goose, d, "node")).sum()
}
```

calls the program's `export fn on_node(depth: i64, label: const u8[:]) -> i64`.

A package anyone can use without Rust:

```sh
cargo goose package dist --targets aarch64-apple-darwin,x86_64-apple-darwin,x86_64-unknown-linux-gnu,x86_64-pc-windows-gnu --zig
```

Use `#[goose::export(zero_copy)]` to require borrowing in place: incompatible
layouts are rejected and misaligned inputs fail instead of being copied.
Strict exports require compiler interface verification. Ordinary exports
keep their copy fallback. See [docs/performance.md](docs/performance.md) for
the supported shapes, examples and validation costs.

## Testing

`scripts/check.sh` runs everything below against both compilers. Or, one
at a time:

```sh
cargo test --workspace
cargo goose test --manifest-path conformance/rust/Cargo.toml conformance/rust/goose-tests
cargo run -p goose-embed-conformance            # Rust calling Goose
bench/run.sh                                    # the overhead budget
```

Each runs against the stock compiler too (`--goose .toolchain/goose-stock/build/goose`,
or `GOOSE=...` for the embed conformance); the tests a patch is needed for say
so and are skipped. `scripts/check.sh` has passed on macOS (arm64) and Linux
(arm64, Ubuntu 24.04), the packages run with no Rust on PATH on both.
[.github/workflows/ci.yml](.github/workflows/ci.yml) is written to run all
of it on Windows, macOS and Linux, build the packages for every platform and
run their examples on machines with no Rust; it has not run, and nothing has
run on Windows ([docs/phases.md](docs/phases.md#where-it-has-run)).
