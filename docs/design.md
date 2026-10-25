# Architecture

Ferrous Goose generates bindings for Goose calling Rust and Rust calling
Goose. The macros produce Rust functions with a C ABI; the Goose compiler
produces C for native builds and the JIT. Users do not write that glue.

## Goose calling Rust

```text
Rust crate → export macros → C-compatible functions + metadata
metadata   → cargo goose   → Goose module
module     → Goose program → native calls
```

`#[goose::export]` generates a function and registers its signature through
`inventory`. When `cargo goose` loads the crate's shared library,
`goose_metadata` builds the records using sizes from `size_of` and field
offsets from `offset_of!`. The macros do not write files. Traits
(`Arg`, `Out`, `Value`) distinguish flat values from handles and supply
layout details that macro syntax alone cannot determine.

Before invoking Rust, a generated function checks argument overlap,
validates values, and borrows or converts the arguments. Holders live in
the call frame; a mutable copy is written back when its holder drops.
Results cross by value, through an output pointer, as an offset into an
input slice, or through a builder. The [type map](type-map.md) lists each
supported form.

`cargo goose` reads the library's records and writes declarations, explicit
padding, native function declarations and Goose wrappers. It compares the
recorded C signature with its own lowering. With `--emit-interface`, it
also compares generated layouts with the compiler's layouts.

The namespace comes from `[package.metadata.goose] namespace`. Native
symbols default to `ns_name`, and Rust doc comments become Goose comments.

### Loading and linking

| Method | Build behavior |
|---|---|
| `extern "lib"` | `cargo goose run` sets the library search path; `cargo goose build` maps the built library in the program's sidecar. Both support stock Goose. |
| `native_build` provider | Goose invokes the configured command to build an archive and generate modules. A native build links the archive; a JIT run builds a shared library from it. This path needs a C toolchain. |
| Prebuilt package | Package-relative library mappings load a library shipped with the module. Users need neither Rust nor a C toolchain for `goose run`. |

The Rust provider is `cargo-goose native`. When several Rust crates need
to link together, it offers `cargo-goose combine`, which builds one umbrella
crate. Each dependency needs `rlib` in its crate types. Combining avoids
duplicate Rust runtime symbols. Cargo metadata and crate construction stay
in the provider; Goose implements the [native-build protocol](goose-patches.md#external-native-builders).

## Validation and ownership

| Requirement | Enforcement |
|---|---|
| Exclusive mutable access | Compare reference byte ranges and handle identities. Overlap involving a mutable argument aborts. |
| Rust alignment and valid values | Check alignment, UTF-8, bools, chars and enum tags. Ordinary exports copy incompatible storage; strict exports fail instead. |
| Call-scoped borrows | Generated Rust lifetimes keep borrowed arguments within the call. `'static` reference parameters are rejected. |
| Append-only builders | `GooseBuf` and `GooseVec` expose append operations. |
| Live handles | Check slot and generation on every use. |
| Thread restrictions | Embedding wrappers require a `Runtime` that is neither `Send` nor `Sync`; pinned handles and re-entry also check their thread. |
| Panic containment | Catch Rust panics before they leave the generated function. |

`#[goose::export(zero_copy)]` rejects signatures that require payload copies
and disables fallback copies for borrowed inputs. Compiler layout checks
are mandatory for these exports. See [copies and overhead](performance.md)
for the contract, supported types and costs.

### Handles

`#[goose::handle]` stores an object in a slot table. Goose receives an
8-byte `{ slot: u32, generation: u32 }` token. Each object is held in an
`Arc<RwLock<T>>`: shared arguments take a read lock, mutable arguments take
a write lock, and owned arguments remove the object after other calls
release it. Releasing a handle changes its generation so stale tokens are
rejected.

A handle type must be `Send + Sync`, or use `#[goose::handle(pinned)]` to
restrict each object to its creating thread. The bridge tracks active
borrows on each thread; a nested call that would wait on an object already
held by that thread aborts instead of deadlocking.

Generated modules provide `release`, `live_count`, `live_count_<type>` and
`leak_report`. With runtime hooks, debug builds and builds run with
`GOOSE_RUST_LEAKS` set report live handles at program exit. Constructor
helpers named `with_<constructor>` release the handle when their block
finishes normally.

### Errors and threads

`Result<T, E>` becomes a value and a success bool. An overload accepts a
byte builder for `E`'s text. `#[goose::export(throws)]` adds a wrapper using
Goose's `exception` module; that wrapper is generic so its throw is checked
where it is called inside a `try`.

Caught panics produce failure for `Result` and `Option` exports. Other
exports abort. Recoverable argument errors, such as invalid UTF-8, follow
the same failure path. Broken borrowing or handle rules abort even for
fallible exports.

Runtime hooks route aborts through Goose. In a contained worker, only the
worker ends and `thread_wait` receives its message. The generated function
leaves no pending Rust drops in frames that the abort jumps over.
`#[goose::export(main_thread)]` separately rejects calls from other threads.
An abort inside a Goose export called by Rust still ends the process.

## Types and iterators

Flat structs use explicit padding to match Rust's field offsets. Matching
layouts can be borrowed when aligned. Default data-carrying enums use
Goose's packed representation and convert at the boundary; the conformance
`Shape` occupies 17 bytes in Goose and 24 in Rust. Mutable converted slices
are written back after the call.

`#[goose(align)]` on a `#[repr(C, u8)]` enum uses Goose's `align n` layout.
Its tag occupies an alignment-sized prefix and the payload uses Rust's
padding. This permits borrowing in place; after mutation, the bridge
zeros upper tag bytes that Rust treats as padding.

Generic functions use `instantiate(T = f32, f64)` to generate concrete
entry points and Goose overloads. Optional parameters generate one entry
point per presence combination, with up to three optional parameters.

An iterator over views into an input slice returns span records and an
`each_<name>` helper that passes each view to a Goose block. An owned
iterator uses a pinned handle with `next`; the `each_` helper releases it
at the end. An iterator that borrows inputs but produces owned values is
collected.

## Callbacks

### Direct re-entry

A Rust function taking `&goose::Reentry` can call Goose exports while Goose
waits on the native call:

```rust
goose::callbacks! { fn on_node(depth: i64, label: &str) -> i64; }

#[goose::export]
pub fn walk(depth: i64, goose: &goose::Reentry) -> Result<i64, goose::ReentryError> {
    let mut total = 0;
    for d in 0..depth {
        total += on_node(goose, d, "node")?;
    }
    Ok(total)
}
```

For a crate using namespace `cf`, Goose can supply the callback and call
`walk`:

```goose
import cf;

export fn on_node(depth: i64, label: const u8[:]) -> i64 {
    return depth + label.len;
}

fn main() {
    let total, ok = cf::walk(4);
    assert(ok);
    print(total);
}
```

The binding declares the native function `reentrant`. Before calling it,
Goose callers publish their cached data-stack tops. A callback starts above
live data and restores the stacks on return, including the position where
an output builder next appends.

The program registers exports at startup. Version 2 runtime hooks provide
`reentry_lookup(name, sig, &why)` and `on_program_thread()`. Callback lookup
checks the C symbol and Goose signature, such as
`i64,const u8[:]->i64`, and caches a successful result. It returns
`ReentryError` for a missing export, signature mismatch, or export that
accesses a mutable `var` global through any of its callees. That restriction
protects references held by the interrupted Goose code.

Creating a re-entry token requires runtime hooks and the program thread;
an invalid call aborts. Generated functions lend the token for the duration of the call. Functions taking
it cannot also take builder or closure parameters. Callback arguments are
scalars, strings, slices or references to flat types with matching layouts;
results are a scalar or nothing. Without the compiler feature, the binding
generator omits the function and writes a comment explaining why.

### Closure callbacks

Functions taking `impl FnMut` or `&mut dyn FnMut` run in a `corosensei`
coroutine on the Goose thread. This works with stock Goose:

1. `start` copies arguments and creates the coroutine on its own stack.
2. `next` resumes Rust until it invokes the closure.
3. Goose runs the supplied block; `reply` sends its result to Rust.
4. `finish` retrieves the function result and releases the session.

Arguments are copied because the Goose block can change their source
storage between steps. Handle arguments keep their locks for the call;
a conflicting nested borrow aborts. This route does not re-enter Goose
from a running native call and is rejected by strict `zero_copy` exports.

## Rust calling Goose

```text
Goose source → compiler → C + interface description
C            → cc crate → linked code
interface    → goose_build → Rust module → goose::include!
```

`goose_build::Program::compile()` runs from `build.rs`. It requests C with
`--standalone` and `--header`, plus layout JSON when `emit-interface` is
available. Otherwise it reads the sources for the forms supported by stock
exports. The `cc` crate compiles the C; generated Rust mirrors the packed
or aligned types and asserts their layouts.

Every wrapper borrows `Runtime`, which keeps calls on the thread that ran
`goose_init`. Multiple export results become a tuple. Output builders use
runtime scratch storage borrowed through `&mut Runtime`; the borrow
prevents another exported call while their contents are still in use.

## Compatibility

The records use metadata format 2 and bridge ABI 1. `cargo goose` also reads
format 1 libraries as ordinary exports and rejects unsupported versions.
Generated modules call `goose_abi_1` at startup, so a library missing that
ABI symbol fails during loading.

The compiler's native value ABI is versioned separately. The tools accept
native ABI 1 or a known runtime hash; `--allow-runtime <hash>` permits an
explicit override. Compiler layout verification is available through
`--emit-interface`, and cannot be skipped for strict exports.

Supported and rejected Rust types are listed in the [type map](type-map.md).
Measured costs and their limits are recorded in [copies and overhead](performance.md)
and [benchmark results](../bench/RESULTS.md).
