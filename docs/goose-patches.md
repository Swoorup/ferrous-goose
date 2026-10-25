# The Goose patch series

The compiler changes in [goose-patches/](../goose-patches/) apply in order to
Goose commit `e76d5c9f7f0a17d83c12a405fbb2513094d03985`.
`scripts/toolchain.sh` builds them into `.toolchain/goose/build/goose`;
`--stock` builds the pinned compiler without the patches.

These patches describe a native interface usable from C, Rust or another
language. The compiler contains no Cargo commands, Rust crate metadata or
binding-project capability registry. Cargo integration remains in the Rust
tooling, which implements the generic native-build provider protocol.

A compiler advertises capabilities separately from its native layout ABI:

```
goose runtime <runtime-hash>
features: typed-builders export-results emit-interface runtime-hooks native-build package-dylibs align reentrant
native-abi: 1
```

`features:` describes supported operations. `native-abi: 1` identifies the
public native value layouts (slices, builder headers/references and stack
control blocks); it does not promise that a compiler supports every optional
operation. Runtime-hook structs carry their own version. A feature list by
itself is not evidence of ABI compatibility.

`cargo goose` and `goose_build` read the neutral feature line. Stock compilers
still work through the existing fallbacks: vector results use a length/copy
protocol; declarations requiring an unavailable feature are omitted with a
comment. Older experimental compilers with only a `ferrous:` line should be
rebuilt with `scripts/toolchain.sh`.

| Patch | Capability | General native-interface purpose |
|---|---|---|
| 0001 | `typed-builders` | Allow an extern to append whole flat, fixed-size elements to a `T[>..]&`; `gs_bld_push` counts elements. Also report compiler capabilities and native ABI. |
| 0002 | `export-results` | Export multiple results using a return value and out-pointers. Export builders use runtime scratch storage borrowed until the next exported call. |
| 0003 | `emit-interface` | Describe exports and reachable type layouts as JSON for binding generators and ABI checks. |
| 0004 | `runtime-hooks` | Offer native libraries the program's abort behavior and end-of-program callbacks. |
| 0005 | `native-build` | Run an external provider which supplies generated modules, a static library, linker arguments and optional hook/combiner metadata. |
| 0006 | `package-dylibs` | Resolve package-relative prebuilt libraries, with platform and architecture overrides. |
| 0007 | `align` | Keep declared structs/enums aligned in variables, arrays and data stacks. Enforce size, field-placement and supported-container constraints. |
| 0008 | `reentrant` | Permit native code to call program exports while a native call is running, publishing stack tops and enforcing callback eligibility. |

## External native builders

For this repository's Rust provider, a sidecar is:

```goose
native_build = ["cargo-goose", "native", "--manifest-path", "../rust/Cargo.toml"];
```

The command is an argv list, run from the package root with no shell
expansion. Goose appends these provider arguments:

- `--out-dir`: the absolute package root, where generated modules belong.
- `--cache-dir`: a directory available for cached build products.
- `--compiler`: the absolute Goose executable used for this compilation.
- `--include-dir`: a directory containing the compiler's public `goose.h`.

A provider prints a final report line after any build logs:

```
goose-native: {"version":1,"staticlib":"/abs/libnative.a","native_libs":["-lm"],"runtime_hooks":"native_init"}
```

`staticlib` is required and must exist; relative paths are resolved from the
provider's working directory. `native_libs` optionally contains linker
arguments. `runtime_hooks` optionally names an initializer taking
`const gs_rt_hooks *`. Goose links the archive for a native build, or builds
a library around it for the JIT. The language server never runs providers.

A provider may also report `combine` (an argv command) and `combine_input`
(an opaque string). When several packages name the same combiner, Goose
invokes it with `--out-dir <cache>` and repeated `--input <opaque-string>`
arguments. It returns the same versioned library report. The Rust provider
uses this to build one umbrella crate, avoiding duplicate Rust runtime
symbols; none of that interpretation lives in Goose.

`cargo_features` and `cargo_crate` are replaced by explicit arguments to the
provider command. For example, append `"--features", "unicode"` to enable
Cargo features. Put `cargo-goose` on PATH (`cargo install --path
crates/cargo-goose`); a sidecar can instead name any provider executable by
absolute path. See `scripts/check-native-build.py` for a plain C provider
which exercises builders and hooks with no Rust on PATH.

## Layout and lifetime policies

Alignment makes borrowing possible; it does not prove identical layouts or
remove validity checks. In the current aligned-enum representation the tag
occupies the alignment-sized prefix. This policy is retained for existing
bindings; independently choosing tag width and payload alignment is a
separate language-layout design, not implied by removing project coupling.

Export scratch builders remain valid until the next exported call. Rust
bindings enforce that lifetime through a mutable runtime borrow. Other
hosts must enforce it themselves.

Reentrant exports must run on the program thread and may not access a
mutable `var` global, including through callees. Rust closure-style callbacks
use a separate coroutine implementation that works with stock Goose.

The bridge's strict `#[goose::export(zero_copy)]` contract and current
performance guarantees are explained in [performance.md](performance.md).
The annotation uses these existing generic capabilities; it adds no
project-specific compiler feature.

## Maintaining the series

Reconstruct the pinned compiler, apply each patch in order and keep one
commit per feature in an isolated development clone. Regenerate patches
against the pinned commit after changes. After editing `src/runtime/`, run
`build/goose --gen-runtime-header` and rebuild so `runtime_inline.h` matches
the source runtime.
