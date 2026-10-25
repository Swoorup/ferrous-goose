# Goose compiler patches

The eight patches in [goose-patches/](../goose-patches/) apply in order to
Goose commit `e76d5c9f7f0a17d83c12a405fbb2513094d03985`.
`scripts/toolchain.sh` builds the result at `.toolchain/goose/build/goose`;
`--stock` builds the pinned commit without patches.

They extend the native interface for C, Rust and other languages. Cargo
commands and crate metadata stay in the Rust build provider.

## Capabilities and ABI

```text
goose runtime <runtime-hash>
features: typed-builders export-results emit-interface runtime-hooks native-build package-dylibs align reentrant
native-abi: 1
```

`features:` lists supported operations. `native-abi: 1` versions the public
slice, builder and stack-control layouts. Runtime hooks have their own
version. Feature support alone does not establish ABI compatibility.

`cargo goose` and `goose_build` read the feature list. Stock-compiler
fallbacks include a length/copy protocol for vector results. Functions
requiring unavailable features are omitted with a comment. Rebuild older
experimental compilers that report only `ferrous:`.

| Patch | Capability | Behavior |
|---|---|---|
| 0001 | `typed-builders` | Native functions append flat, fixed-size elements to `T[>..]&`; `gs_bld_push` counts elements. Adds capability and ABI reporting. |
| 0002 | `export-results` | Multiple results use a return value and output pointers. Exported builder parameters use runtime scratch storage. |
| 0003 | `emit-interface` | JSON describes exports and reachable layouts for binding generation and verification. |
| 0004 | `runtime-hooks` | Libraries receive Goose's abort behavior and end-of-program callbacks. |
| 0005 | `native-build` | An external command supplies modules, an archive, linker arguments and optional initialization/combination commands. |
| 0006 | `package-dylibs` | Packages resolve prebuilt libraries from their own directory, with platform and architecture overrides. |
| 0007 | `align` | Structs and enums stay aligned in variables, arrays and data stacks; invalid layouts and containers are rejected. |
| 0008 | `reentrant` | Native code calls back into exports after callers publish their stack tops. Runtime lookup checks callback eligibility. |

## External native builders

A sidecar selects a provider with an argv list:

```goose
native_build = ["cargo-goose", "native", "--manifest-path", "../rust/Cargo.toml"];
```

The command runs from the package directory, without shell expansion.
Goose appends:

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
`const gs_rt_hooks *`. A native build links the archive; the JIT builds a
shared library from it. The language server reads generated modules but
does not run providers.

Providers that need to combine packages can also report `combine` (an argv
list) and `combine_input` (an opaque string). Packages sharing a combiner
command are grouped. Goose invokes that command with `--out-dir <cache>`
and one `--input <string>` per package; it must return the same versioned
library report. The Rust provider uses an umbrella crate to avoid duplicate
Rust runtime symbols.

Install `cargo-goose` on PATH or use an absolute executable path. Cargo
features are command arguments, for example `"--features", "unicode"`.
These replace the former `cargo_crate` and `cargo_features` keys. A
[plain C provider test](../scripts/check-native-build.py) exercises hooks,
typed builders and JIT/native builds without Rust on PATH.

## Layout and lifetimes

Alignment and layout compatibility are separate checks. An aligned enum's
tag uses an alignment-sized prefix before its payload. Rust bindings match
that representation with explicit padding and fix the upper tag bytes
after mutation.

Exported scratch-builder contents remain valid until the next exported
call. Rust wrappers enforce this with a mutable runtime borrow; other
hosts must enforce the lifetime themselves.

Re-entry requires the program thread. Callback exports cannot access mutable
`var` globals, including through callees. Closure callbacks use a separate
coroutine protocol that works with stock Goose.

Strict `#[goose::export(zero_copy)]` uses these capabilities without adding
a compiler feature. Its [copy contract](performance.md) covers validation,
failure behavior and packaging limits.

## Maintaining the series

Use an isolated clone of the pinned compiler with one commit per patch.
Apply the series in order and regenerate patches against the pinned commit.
After changing `src/runtime/`, run `build/goose --gen-runtime-header` and
rebuild so `runtime_inline.h` matches the runtime sources.
