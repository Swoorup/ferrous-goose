# Ferrous Goose: the design as built

Goose programs call Rust crates, and Rust programs call Goose, with no C, no
headers and no unsafe Rust in user code. This is the proposal's design as
this repository implements it; where the implementation differs from the
proposal it says so.

## Goals

- No C anywhere: no shims, no headers, no hand-written unsafe on either side.
- Borrow compatible slices and flat structs in place; direct builders can
  produce output in Goose storage. Owned results and conversions still copy.
- Both languages' rules hold, enforced by generated code.
- Both build paths: `goose run` (no C toolchain) and `goose build`.
- Both directions.

Out of scope: Rust keeping Goose memory past a call, Rust closures kept as
values, async.

## Goose using Rust

```
Rust crate  --#[goose::export]-->  shims + records   (cargo build: cdylib, staticlib)
   records  --cargo goose bindings-->  ns.goose       (read from the built library)
 ns.goose   --import ns;-->  your Goose program       (goose run / goose build)
```

**The macros** (`crates/goose-macros`) turn each marked function into an
`extern "C"` shim in the shapes of spec §7.10 and a record of its
signature. They never write files: a record is a function registered with
`inventory`, run when `cargo goose` asks the library for its records
(`goose_metadata`, exported by the `goose` crate), so the records hold what
only rustc knows, field offsets from `offset_of!` and sizes from `size_of`.
Which named types are structs and which are handles is left to traits
(`goose::Arg`, `goose::Out`, `goose::Value`) that `#[derive(Flat)]` and
`#[goose::handle]` implement, so the macro decides shapes from syntax and
the trait system fills in the rest.

**A shim** holds each argument first (validates it, borrows it in place or
copies it to aligned memory), checks that no `&mut` argument aliases another,
runs the function under the panic catcher (`goose::call`), and sends the
result out: by value, through an out-pointer, as an offset and length into
an argument, or appended to a builder. The holders live in the shim's frame,
so nothing the function is lent outlives the call; a `&mut` copy is written
back when its holder drops.

**cargo goose** (`crates/cargo-goose`) builds the crate, opens the cdylib,
reads the records (checking their format and ABI versions), and writes the
Goose module: struct and enum declarations with rustc's layout, the extern
declarations, and wrappers that give each Rust shape its Goose shape. Its
lowering mirrors the macro's, and every function's record carries the C
shapes the macro lowered it to, which cargo goose compares with its own. With
a compiler that has `--emit-interface`, it also compiles a probe of the
module and compares the compiler's layout of every type with rustc's.

Names: everything lands in the crate's namespace (`[package.metadata.goose]
namespace`), so a wrapper never collides with the standard library; C
symbols are `ns_name`. Rust doc comments are copied as Goose comments.

**How the program reaches the library.** Three ways, all tested:

1. An `extern "lib" { ... }` block (spec §7.10) the program opens when it
   starts: `cargo goose run` points the loader at the build, `cargo goose
   build` maps the library in the program's sidecar. Works with the stock
   compiler, under `goose run` and `goose build`.
2. `native_build = ["cargo-goose", "native", "--manifest-path", "rust/Cargo.toml"];`
   in the sidecar (goose-patches/0005): the compiler invokes an external
   provider using a versioned language-neutral protocol. The Rust provider
   builds the crate and writes the module, reports the static archive and
   linker arguments, and offers a combiner for several crates. Goose links
   the archive for a native build or makes the JIT library from it. Cargo
   metadata and umbrella-crate construction stay in `cargo goose`.
3. A package (goose-patches/0006): the module and a prebuilt library per
   platform, mapped with `./` paths in the package's sidecar. No Rust needed.

## Who enforces what

| Rule | Could be broken by | The bridge's answer |
|---|---|---|
| A `&mut` has no alias (Rust) | Goose, which allows aliasing | the shim compares the byte ranges of every reference argument, and handles by slot, when any is mutable; an overlap aborts |
| References are aligned (Rust) | Goose's packed storage | borrowed in place when aligned, through an aligned copy otherwise; `align n` (0007) makes Goose keep shared structs aligned so the copy never happens |
| Borrowed arguments are not kept (Goose) | Rust | the shim lends call-scoped lifetimes; `'static` on a parameter is a macro error |
| Builders are append-only (Goose) | Rust | `GooseBuf` and `GooseVec` have no other operation |
| Panics do not cross into C (Rust) | Rust | `catch_unwind` in every shim: a `Result` or `Option` function returns a failure, anything else aborts as Goose does |
| Exported calls stay on the init thread (Goose) | Rust | `goose::Runtime` is neither Send nor Sync and every wrapper takes it |
| Nothing grows a stack under a live builder (Goose) | Rust calling back into Goose | a `reentrant` extern's callers publish every cached stack top first, so a call back starts above all live data (0008); closures run as coroutines and never re-enter (below) |
| Code an extern interrupts keeps its references into globals (Goose) | a call back to an export fn that changes a `var` global | the compiler marks such export fns, itself or through its callees, and the lookup refuses them |
| Export wrappers use the main thread's stacks (Goose) | a call back from another thread | `Reentry::enter` checks the thread; a call back from elsewhere is an `Err` |
| `&str` is UTF-8, `char` a scalar value, `bool` 0 or 1, tags in range (Rust) | Goose bytes | validated on entry |
| A handle names a live object | Goose | slot and generation, checked on every use; a released handle is a clean abort |
| Objects shared between threads | Goose workers passing handles | a handle type is Send + Sync, or `#[goose::handle(pinned)]` and checked against the thread that made each object |
| Two calls do not deadlock on one object | a callback in progress | each thread records the objects its running calls hold; a borrow that would wait on one of them aborts instead |

## Handles

A `#[goose::handle]` type lives in a slot table (`goose::HandleTable`);
Goose holds an 8-byte `{ slot, generation }`. Each object sits in an
`Arc<RwLock<T>>`: `&T` takes a shared lock, `&mut T` an exclusive one, a
by-value parameter takes the object out (waiting for calls on other threads
to finish with it). Releasing bumps the generation, so the old handle is
stale forever. Every table counts its live entries: the module gets
`live_count()`, `live_count_<type>()` and `leak_report()`, and with the
runtime hooks (0004) a debug build, or any build with `GOOSE_RUST_LEAKS`
set, prints the live handles of each type when the program ends. Each
constructor gets a scoped helper, `with_<constructor><F>(..)`, which
releases the handle when the block finishes normally.

## Errors, panics and threads

- `Result<T, E>` becomes `T, bool`, with an overload taking an error builder
  for `E`'s text. `#[goose::export(throws)]` adds a wrapper that throws the
  text through Goose's `exception` module; it is generic, so it is checked
  only where it is called, inside a `try`, as a throw must be.
- A panic in a `Result` or `Option` function is its failure (`panic: msg
  (file:line)`); anywhere else it ends the program as a Goose abort does:
  `goose runtime error: <msg> (rust ns::name)`. With the runtime hooks
  (0004) that abort is the runtime's own, so inside a contained worker
  (`thread_spawn_contained`) it ends only the worker, and `thread_wait`
  hands the program the message. The shim leaves nothing to drop in the
  frames that abort jumps over.
- `#[goose::export(main_thread)]` aborts when called from another thread.

## Phase 3 shapes

**Enums with data** convert at the boundary: Goose stores a fixed-mode enum
as a tag and its largest payload, packed (17 bytes for the proposal's
`Shape`, against Rust's 24). A slice of them converts in one pass, and a
`&mut` slice is written back. `#[goose(align)]` on a `#[repr(C, u8)]` enum
declares it in Goose with `align n` and rustc's pads instead (0007): the
layouts match and it crosses in place, with the tag's upper bytes zeroed
after Rust writes one.

**Generics**: `#[goose::export(instantiate(T = f32, f64))]` makes one shim
per type, which Goose sees as overloads, bound to their own symbols.

**Iterators**: an iterator over parts of a slice parameter becomes a span
list filled in one call, and `each_<name><F>` hands each part to the
caller's block as a slice of the argument. An iterator making values becomes
a pinned handle with `next`, and `each_<name><F>` releases it at the end. An
iterator borrowing its arguments but making values is collected.

**Optional parameters**: one entry point per combination of the `Option`
parameters (up to three), and a wrapper that narrows the Goose `T?`s.

### Callbacks

Two ways, both tested.

**Calling the program back** is the proposal's runtime change, goose-patches/0008.
A function taking `goose: &goose::Reentry` may call the program's export
fns while Goose waits on it; `goose::callbacks!` declares, in Rust, the ones
it expects:

```rust
goose::callbacks! {
    fn on_node(depth: i64, label: &str) -> i64;
}

#[goose::export]
pub fn walk(depth: i64, goose: &goose::Reentry) -> Result<i64, goose::ReentryError> {
    let mut total = 0;
    for d in 0..depth {
        total += on_node(goose, d, &format!("node {d}"))?;
    }
    Ok(total)
}
```

```goose
export fn on_node(depth: i64, label: const u8[:]) -> i64 { ... }

fn main() {
    var keep: i64[>..] = [7, 8, 9];      // live while Rust calls back
    let total, ok = cf::walk(4);
}
```

The binding declares `walk` a `reentrant fn`. Its callers, and theirs,
publish every cached data stack top before the call, so a call back starts
above all live data, the elements of a builder the extern was handed
included, and leaves the stacks as it found them. The program registers a
table of its export fns as it starts, and the runtime's hooks (version 2)
give the crate `reentry_lookup(name, sig, &why)` and `on_program_thread()`.
Each callback finds its export fn once, by C symbol and by its signature as
Goose writes it (`i64,const u8[:]->i64`), and keeps it. A call back is an
`Err(ReentryError)` naming the reason, never undefined behaviour, when:

- the program has no such export fn, or declares it otherwise;
- the export fn touches a `var` global, itself or through its callees: the
  code the call back interrupts assumed the extern left the globals alone,
  and may hold references into them;
- the call comes from a thread other than the program's main thread;
- the program was built by a compiler without the patch (`cargo goose` then
  leaves the function out of the module, saying so there).

`&Reentry` borrows the call's frame and cannot outlive it. A reentrant
function may not also take `GooseBuf`, `GooseVec` or a closure. Callback
arguments are scalars, `&str`, slices and references to `Flat` types laid
out as Goose lays them out (asserted at compile time); results are a scalar
or nothing. A handle the outer call holds mutably, borrowed again by Rust
called from the call back, aborts rather than deadlocks.

**Closures** need no patch and work on the stock compiler. A function taking
`impl FnMut(A..) -> R` (or `&mut dyn FnMut`) runs as a coroutine on a stack
of its own, on the Goose thread:

```
geo::keep_area(shapes) { s => is_round(s) }
  -> start: the arguments copied, the coroutine made (a pinned handle)
  -> next:  runs until the closure is called; its arguments come back
  -> the block runs in Goose, its result goes back with reply
  -> next again ... until the function returns: finish sends its result
```

Goose never re-enters, so its stack rules hold untouched. Arguments are
copied before the coroutine starts, since Goose code (the block) runs
between its steps and may change what was passed; a handle argument keeps
its shared lock for the whole call, and a `&mut` borrow of the same object
from the block aborts rather than deadlocks.

## Rust using Goose

```
kernels.goose --goose -o kernels.c --standalone --header (--emit-interface)--> C + interface
        C     --cc crate-->  linked into the crate
interface     --goose_build-->  $OUT_DIR/kernels.rs  --goose::include!(kernels)-->  safe module
```

`goose_build::Program::new(root).compile()` runs from build.rs. With a
compiler that has `--emit-interface` (0003) the compiler describes the
exports and layouts; without it, `goose_build` reads the sources for the
shapes a stock export takes. The Rust module mirrors each struct (packed,
or aligned for `align n`, with the layout asserted at compile time),
declares the C functions and wraps each in a safe function taking
`&Runtime`. `Runtime::init()` runs `goose_init` once, on the calling thread,
through a registration the generated module makes.

With 0002, an export may return several values (a tuple in Rust) and take
builders: the runtime's scratch builder holds the elements, and the Rust
wrapper lends them as `&'a [T]` for as long as it holds `&'a mut Runtime`,
so the next call, which needs the runtime mutably again, cannot run while
they are read.

## Versioning

- Records carry a format version and the bridge's calling-convention (ABI)
  version; `cargo goose` refuses others, naming both.
- Every library exports `goose_abi_1`, which every generated module calls as
  the program starts: a library built with another ABI fails there, naming
  the symbol, rather than misbehave.
- `cargo goose` compares the compiler's runtime with the ones the bridge was
  checked against (including compilers advertising native ABI 1) and refuses
  others unless told `--allow-runtime`.
- With `--emit-interface`, every generated module's layouts are checked
  against the compiler's.

## The overhead budget

`bench/run.sh` times each kind of call against a bare `extern "C"` function
doing the same work, from a Goose program built with `goose build`, and
writes [bench/RESULTS.md](../bench/RESULTS.md). On an M1 Max, a scalar call
pays about 2 ns for the panic catcher, a slice argument about 3 to 6 ns for
its checks, `&str` adds UTF-8 validation (linear in the length; take
`&[u8]` to skip it), and a handle about 20 ns for its lock.

## Open questions, as built

- **Several crates, one program.** Under extern blocks each crate is its own
  library, so a program may use any number. Linked (`native_build`), two Rust
  static libraries would define Rust's own symbols twice. The Rust provider
  offers `cargo goose combine` to build one: an umbrella crate depending on every
  crate the program names (each needs `rlib` among its crate types).
  `examples/multi` uses two, under `goose run` and `goose build`.
- **Packages per architecture.** A package's sidecar may map a library per
  architecture (`dylibs_linux_arm64`, `dylibs_windows_x86_64`, 0006), over
  the per-platform default; `cargo goose package` lays them out as
  `lib/<os>-<arch>/`, and macOS gets one universal library.
- **Catching Goose aborts from Rust.** Unchanged: an abort inside a Goose
  export ends the process.
- **Check opt-outs.** None: validation remains part of safe exports. Its
  relative cost depends on the workload (see the budget). Compatible
  layouts with `align n` avoid the alignment copy fallback.

The current copy paths, measured overhead and a proposed strict zero-copy
export contract are described in [performance.md](performance.md). That
contract is a proposal; current exports retain safe copy fallbacks.
