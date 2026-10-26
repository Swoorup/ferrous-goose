# Goose compiler patches

The six patches in [goose-patches/](../goose-patches/) apply in order to
Goose commit [`a108677d300cb0e8bbb842c35a353abe67a20c6c`](https://github.com/aardappel/goose/commit/a108677d300cb0e8bbb842c35a353abe67a20c6c),
the upstream `master` head fetched on 2026-10-27. Builds remain pinned to
that exact revision so local and CI results are reproducible.
`scripts/toolchain.sh` builds the result at `.toolchain/goose/build/goose`;
`--stock` builds the pinned commit without patches. “Stock” below means
that pinned revision.

These are extensions to our patched Goose compiler and its specification.
They support C, Rust and other languages using the C ABI. Cargo commands,
crate metadata and Rust binding policy stay in `cargo-goose` and the Rust
SDK. They become part of upstream Goose only if accepted there.

An external function is declared with `extern`; an `export fn` makes a
Goose function callable from another language. A namespace such as
`native::` does not make a function external. Ordinary Goose functions
already support typed builders and multiple results.

## Series and compatibility

| Patch | Capabilities | Purpose |
|---|---|---|
| [0001 — Builders and export results](../goose-patches/0001-Builders-and-export-results.patch) | `typed-builders`, `export-results` | Append typed values across the C ABI; expose multiple results and builder output from Goose exports. |
| [0002 — Interface metadata](../goose-patches/0002-Emit-interface.patch) | `emit-interface` | Describe exports and compiler-resolved layouts as JSON. |
| [0003 — Runtime hooks and re-entry](../goose-patches/0003-Runtime-hooks-and-reentry.patch) | `runtime-hooks`, `reentrant` | Share abort/shutdown behavior and support callbacks into a running Goose program. |
| [0004 — Native build providers](../goose-patches/0004-Native-build-providers.patch) | `native-build` | Build and link libraries through an external command and a versioned report. |
| [0005 — Package libraries](../goose-patches/0005-Package-dylibs.patch) | `package-dylibs` | Locate prebuilt libraries relative to their package, with platform/architecture overrides. |
| [0006 — Alignment](../goose-patches/0006-Alignment.patch) | `align` | Preserve explicit struct/enum alignment throughout Goose storage. |

The series was consolidated from eight patches at the previous upstream
pin, `e76d5c9f7f0a17d83c12a405fbb2513094d03985`. That reorganization
preserved compiler behavior. The six patches have since been rebased onto
the current pin above, retaining upstream's formatter, help, completion,
version reporting and runtime changes. The interface flag honors quiet
mode, and source dumps preserve `reentrant` and `align` declarations.
The builder helper in the embedded runtime belongs to the first patch,
alongside the public-header helper.

| Previous patch numbers | Current patch |
|---|---|
| 0001 typed builders + 0002 export results | 0001 |
| 0003 interface metadata | 0002 |
| 0004 runtime hooks + 0008 re-entry | 0003 |
| 0005 native providers | 0004 |
| 0006 package libraries | 0005 |
| 0007 alignment | 0006 |

Builders and export results cover data exchange in both directions. Hooks
and re-entry share a runtime API. Native providers and prebuilt package
libraries remain separate: building a dependency and locating a shipped
library are independently useful operations. Alignment has its own
language and layout rules.

Patch numbers identify files; capability names identify behavior. The
compiler still reports eight capabilities, in the same order as before:

```text
goose <compiler-version> (runtime <runtime-hash>)
features: typed-builders export-results emit-interface runtime-hooks native-build package-dylibs align reentrant
native-abi: 1
```

`native-abi: 1` versions the public slice, builder and stack-control layouts.
The runtime-hook API is version 2. Neither is the Rust metadata format
(currently 2). Feature support alone does not establish ABI compatibility.

`cargo goose` accepts both upstream's current version line and the earlier
`goose runtime <runtime-hash>` form. `cargo goose` and `goose_build` read the
capability lines independently of the compiler version. Ordinary bindings
retain stock-compiler fallbacks. Functions that require unavailable
features are omitted with an explanatory comment. Strict
`#[goose::export(zero_copy)]` uses the existing capabilities; it does not
add a compiler feature. Its [copy contract](performance.md) covers
validation, failure behavior and packaging limits.

## 0001: Builders and export results

### Goose calling Rust

The snippets use this repository's `goose` dependency. Configure library
crate types as in the [regex example](../examples/regex/rust/Cargo.toml),
set its namespace below, and generate the bindings with `cargo goose`:

```toml
[package.metadata.goose]
namespace = "native"
```

Rust can append to the caller's builder directly:

```rust
#[goose::export]
pub fn fill_squares(n: i64, out: &mut goose::GooseVec<i64>) {
    for i in 0..n {
        out.push(i * i);
    }
}
```

```goose
import native;

fn main() {
    var values: i64[>..] = [100];
    native::fill_squares(4, values);
    print(values); // [100, 0, 1, 4, 9]
}
```

**Stock behavior:** byte builders (`u8[>..]&`) can cross an `extern`
boundary, but typed builders such as `i64[>..]&` are rejected. A pre-sized
mutable slice is already supported. For variable-size vector results,
ordinary Rust bindings can use a length/copy protocol instead.

**Generated code and cost:** a builder passes its header and data-stack
pointer. `gs_bld_push` counts elements rather than bytes. The Rust wrapper
writes each element into Goose storage and updates the stack top and
length. This avoids an intermediate Rust vector, but stores, bookkeeping
and any data-stack page commitment still cost work. Elements must be flat,
fixed-size values; ordinary bindings may convert layouts, while strict
exports require compatible layouts.

### Rust calling Goose

Goose exports can expose several results and builder output. For example,
`kernels.goose`:

```goose
export fn squares(n: i64, out: i64[>..]&) -> i64, bool {
    if n < 0 { return 0, false; }
    for i in n { out.push(i * i); }
    return n, true;
}
```

With the embedding build script shown under [0002](#0002-interface-metadata):

```rust
goose::include!(kernels);

fn main() {
    let mut rt = goose::Runtime::init().unwrap();
    let (count, ok, values) = kernels::squares(&mut rt, 4);
    assert!(ok);
    assert_eq!(count, 4);
    assert_eq!(values, &[0, 1, 4, 9]);
}
```

**Stock behavior:** multiple results already work between Goose functions,
but `export fn` accepts at most one result and rejects builder parameters.
A host can instead receive a fixed-size result struct or supply writable
storage through a slice.

**Generated code and cost:** the C wrapper returns the first result and
writes further results through output pointers. Exported builder
parameters use runtime scratch stacks, initialized on first use and reset
for subsequent calls using them. The returned contents are valid until the
next exported call. Rust wrappers enforce this lifetime with `&mut Runtime`;
other hosts must enforce it themselves. Programs without exported builder
parameters reserve no export scratch stacks. Export signatures and the
maximum scratch requirement are collected once during code generation;
this does not change the public C calling convention.

## 0002: Interface metadata

A Goose function can be embedded in Rust:

```goose
// kernels.goose
export fn add(a: i64, b: i64) -> i64 { return a + b; }
```

```rust
// build.rs
fn main() {
    goose_build::Program::new("kernels.goose").compile().unwrap();
}
```

```rust
// src/main.rs
goose::include!(kernels);

fn main() {
    let rt = goose::Runtime::init().unwrap();
    assert_eq!(kernels::add(&rt, 20, 22), 42);
}
```

The build script requests compiler-resolved exports and layouts. To inspect
the same information directly:

```sh
goose check --emit-interface json kernels.goose
```

**Stock behavior:** simple embedding already works. Stock Goose can emit
C headers; `goose_build` falls back to parsing source for the forms it
supports. It cannot request the compiler's layout JSON.

**Generated code and cost:** the flag adds compiler work and JSON output.
The description includes types, sizes, field offsets, padding and alignment.
Binding tools use it to generate wrappers and verify memory compatibility.
It adds no runtime reflection or per-call lookup. Without the flag, no
interface JSON is produced. `-q` suppresses the file-written notice;
explicit JSON output on stdout remains available. Strict Rust exports
require this verification.

## 0003: Runtime hooks and re-entry

### Runtime failures and shutdown

The Rust SDK receives the runtime hooks automatically. A non-fallible Rust
export that panics uses Goose's abort behavior after the shim catches it:

```rust
#[goose::export]
pub fn fail() {
    panic!("native operation failed");
}
```

```goose
import native;

thread_fn worker() { native::fail(); }

fn main() {
    let id = thread_spawn_contained(worker);
    var message: u8[>..] = [];
    let ok = thread_wait(id, message);
    print(ok);      // false
    print(message);
    print("main continues");
}
```

Build a native executable for this worker example with `cargo goose build`.

**Stock behavior:** the bridge's fallback abort exits the process. It
cannot route this failure through Goose's contained-worker mechanism.
The SDK's automatic end-of-program handle diagnostic also needs the hook.

**Generated code and cost:** loaded libraries are probed for
`goose_runtime_hooks`; a library exporting it receives versioned
`abort_msg`, `at_end`, `reentry_lookup` and `on_program_thread` hooks.
Native providers can name a static initializer for the same purpose.
The SDK registers optional handle diagnostics at shutdown. Other native
libraries can register their own cleanup callbacks. Hook delivery costs
startup work; registered callbacks cost shutdown work. The normal-return
hook call is omitted for programs with no extern calls, custom C headers,
or native packages. Other programs retain it conservatively, since their
native code may register a callback. The shared runtime still checks hooks
on explicit exit and abort paths.

### Calling back into Goose

Rust declares the Goose callback and takes a call-scoped re-entry token:

```rust
goose::callbacks! { fn on_node(depth: i64) -> i64; }

#[goose::export]
pub fn walk(n: i64, ctx: &goose::Reentry) -> Result<i64, goose::ReentryError> {
    let mut total = 0;
    for depth in 0..n {
        total += on_node(ctx, depth)?;
    }
    Ok(total)
}
```

```goose
import native;

export fn on_node(depth: i64) -> i64 { return depth * 2; }

fn main() {
    let total, ok = native::walk(4);
    assert(ok);
    print(total); // 12
}
```

**Stock behavior:** direct re-entry is unavailable, and these bindings are
omitted. Closure callbacks using `impl FnMut` already work through the
separate coroutine/resume protocol, which copies arguments and switches
between Goose and Rust. See [callback protocols](design.md#callbacks).

**Generated code and cost:** the generated external declaration is marked
`reentrant`. Affected callers publish cached stack tops so a callback
starts above live data and restores the stacks on return. The program
registers export names, signatures and eligibility at startup. The runtime
checks callback eligibility; Rust caches successful lookups and checks the
program thread when creating a re-entry token. These costs remain even
with strict zero-copy bindings.

The formatter and source dump retain the `reentrant` modifier. Callbacks
require the program thread and cannot access mutable `var` globals,
including through callees. Each export's reachable functions are checked
independently; recursive cycles cannot hide global access or make the
answer depend on declaration order. Safe recursion remains supported.
Rust exports taking `Reentry` cannot
also take explicit builders or closure callbacks. The token is supplied by
the shim, not by a Goose argument.

## 0004: Native build providers

The Rust function and Goose call use ordinary bindings:

```rust
#[goose::export]
pub fn add(a: i64, b: i64) -> i64 { a + b }
```

```goose
// main.goose
import native;
fn main() { print(native::add(20, 22)); }
```

The program's sidecar selects the builder:

```goose
// main.build; the Rust crate is under rust/
native_build = ["cargo-goose", "native", "--manifest-path", "rust/Cargo.toml"];
```

`goose run main.goose` and `goose build main.goose` invoke the provider to
build the library and generate the imported module.

**Stock behavior:** Goose supports C/CMake native builds but has no general
external-provider protocol. Rust users run `cargo goose run/build` or
arrange generation, building and linking themselves.

**Generated code and cost:** provider execution, combination and linking
happen at build time. Native builds link the archive; the JIT makes a shared
library from it. Optional initializers run at startup. The provider protocol
adds no per-call dispatch. No provider runs without a `native_build` entry.
The Linux archive-visibility and MSVC linker/response-file fixes are part of
this patch.

### External native builders

The command is an argv list, run from the package directory without shell
expansion. Goose appends:

| Argument | Value |
|---|---|
| `--out-dir` | Absolute package directory for generated modules |
| `--cache-dir` | Build-cache directory |
| `--compiler` | Absolute path to the active Goose executable |
| `--include-dir` | Directory containing its public `goose.h` |

The provider must exit successfully and print a final report line:

```text
goose-native: {"version":1,"staticlib":"/abs/libnative.a","native_libs":["-lm"],"runtime_hooks":"native_init"}
```

`staticlib` is required and must exist. Relative paths resolve from the
provider's working directory. `native_libs` optionally lists linker
arguments. `runtime_hooks` optionally names a public C initializer taking
`const gs_rt_hooks *`. The language server reads generated modules but does
not run providers.

Providers that need to combine packages can also report `combine` (an argv
list) and `combine_input` (an opaque string). Packages sharing a combiner
command are grouped. Goose invokes that command with `--out-dir <cache>`
and one `--input <string>` per package; it must return the same versioned
library report. The Rust provider uses an umbrella crate to avoid duplicate
Rust runtime symbols. A package selects one of `c_sources`, `cmake_target`
and `native_build`.

Install `cargo-goose` on PATH or use an absolute executable path. Cargo
features are command arguments, for example `"--features", "unicode"`.
These replace the former `cargo_crate` and `cargo_features` keys. A
[plain C provider test](../scripts/check-native-build.py) exercises hooks,
typed builders and JIT/native builds without Rust on PATH.

## 0005: Package libraries

The package author compiles Rust once per supported target:

```rust
#[goose::export]
pub fn twice(x: i64) -> i64 { x * 2 }
```

For a library named `native_lib`, the package sidecar maps its files:

```goose
// native/native.build
dylibs_linux = ["native_lib=./lib/linux/libnative_lib.so"];
dylibs_macos = ["native_lib=./lib/macos/libnative_lib.dylib"];
dylibs_windows = ["native_lib=./lib/windows/native_lib.dll"];
```

Consumers import the generated module and use the prebuilt library:

```goose
import native from "./native";
fn main() { print(native::twice(21)); }
```

**Stock behavior:** loading prebuilt libraries already works, but `./` in
an imported package's mapping is not resolved from that package's root.
Users arrange application resource paths, loader search paths or
application-level mappings. Architecture-specific mapping keys are new.

**Generated code and cost:** package paths are resolved while compiling;
library loading and symbol resolution happen at startup, with no package
lookup on each call. An architecture mapping such as
`dylibs_linux_arm64` overrides the platform mapping. Running a prebuilt
package with `goose run` requires neither Rust nor a C build toolchain.
Native executable builds can still require a C compiler.

## 0006: Alignment

A Rust struct has a layout and alignment that borrowed references require:

```rust
#[derive(goose::Flat, Clone, Copy)]
#[repr(C)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[goose::export]
pub fn translate(p: &mut Point, dx: f64) { p.x += dx; }
```

The generated Goose module declares:

```goose
struct Point { align 8, x: f64, y: f64 }
```

The application uses that type:

```goose
import native;

fn main() {
    var p = native::Point { x: 3.0, y: 4.0 };
    native::translate(p, 2.0);
    print(p.x); // 5.0
}
```

**Stock behavior:** matching field offsets do not guarantee an aligned
address. Ordinary bindings borrow compatible storage when possible and
otherwise use a temporary, including a write-back for mutable arguments.
Strict bindings reject misaligned inputs instead of copying them.

**Generated code and cost:** explicit alignment changes type placement in
variables, fixed/growable arrays and data stacks. The generated C carries
alignment attributes and rounds stack tops where needed. `align` does not
automatically insert field padding; sizes, field offsets and containers
are checked. An aligned enum uses an alignment-sized tag prefix before its
payload, which can increase its size. Rust bindings use matching padding
and fix the upper tag bytes after mutation.

This is a language feature: it also affects pure Goose code that uses
`align`, even without native bindings. Unannotated types retain their
default layouts. Alignment alone does not establish layout compatibility;
interface verification and runtime value checks remain necessary. Formatting
and source dumps preserve the alignment declarations.

## Ordinary Goose code and unused features

This function already works on stock Goose:

```goose
// native.goose
namespace native;
fn fill_squares(n: i64, out: i64[>..]&) {
    for i in n { out.push(i * i); }
}
```

Calling it as `native::fill_squares(4, values)` is an ordinary Goose call.
It introduces no Rust shim, C ABI conversion or re-entry synchronization.
A local comparison of stock and patched compilers found identical emitted
caller and callee bodies at `-O0`, and identical inlined caller bodies at
`-O2`. This checks that example's code generation, not every program's
latency.

Programs without exports emit no callback-registration function or call.
Programs without extern calls, custom C headers or native packages also
omit the normal-return shutdown-hook call. For the all-Goose example
above, the generated initialization and `main` bodies now match stock at
both `-O0` and `-O2`, as do the user-function bodies. This does not depend
on the C optimizer removing empty calls.

Programs with exports retain callback-table metadata even without a
reentrant call. Native programs retain shutdown handling conservatively;
loaded libraries are probed for the optional hook symbol. Runtime
headers/helpers remain present, and the shared runtime's explicit exit
and abort paths still check shutdown hooks. The series therefore does not
promise identical binary size or zero cost on every unused-feature path.
See [compiler costs](performance.md#compiler-and-runtime-costs) for the cost
table and measurement limits.

## Maintaining the series

Use an isolated clone of the pinned compiler with one commit per patch.
Apply all six patches in filename order. Keep each prefix buildable and
keep the `features:` string accurate at each stage; applications should
query capability names rather than patch numbers. Runtime hooks and
re-entry precede native providers because provider initializers use the
hook API. Alignment also updates the interface metadata introduced earlier.

After changing `src/runtime/`, run `build/goose --gen-runtime-header` and
rebuild so `runtime_inline.h` matches the runtime sources. For a scratch
clone, set `GOOSE_RUNTIME=/absolute/path/to/clone/src/runtime` when running
an existing compiler's generator. Regenerate each affected patch against
its predecessor, including embedded runtime-header changes in that patch.

For an organizational change, compare the fully applied source trees
before and after, regenerate/check runtime headers at every patch boundary,
and build the compiler. Then run `scripts/check.sh` for the bridge,
conformance and native-provider paths. Capability names and ABI versions
change only when their contracts change, not when patch files are combined.
