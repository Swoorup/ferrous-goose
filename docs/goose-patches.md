# The Goose patch series

Ferrous Goose pins Goose at
[`e76d5c9f`](https://github.com/aardappel/goose/commit/e76d5c9f7f0a17d83c12a405fbb2513094d03985)
and carries the compiler changes the proposal's later phases call for as
patches against it, in [goose-patches/](../goose-patches/).
`scripts/toolchain.sh` applies them in order and builds the compiler into
`.toolchain/goose/build/goose`; `--stock` builds the pinned commit alone into
`.toolchain/goose-stock/`.

A patched compiler lists what it carries:

```
$ goose version
goose runtime 43500621d6a0
ferrous: typed-builders export-results emit-interface runtime-hooks cargo-crate package-dylibs align reentrant
```

`cargo goose` and `goose_build` read that line and use what is there, so
the same crate works with either compiler: where a feature is missing the
bindings fall back (a typed builder becomes a count and a copy) or leave out
what cannot be expressed (an aligned enum), saying so in the module.

Each patch updates the spec (`docs/goose_spec.md`, or `docs/packages.md`)
with its rule. Goose's own test suite passes with the series applied
(`python3 test/run_tests.py --profile baseline --cc native`: every section,
about 4,750 checks).

| Patch | Feature | Phase | What it changes |
|---|---|---|---|
| 0001 | `typed-builders` | 2 | An extern fn may take `T[>..]&` of any flat fixed-size `T`: C appends whole elements at the stack's top and counts them in the header (`gs_bld_push` in `goose.h`). Rust's `Vec<T>` results and span lists use it. Also adds the `ferrous:` line to `goose version`. |
| 0002 | `export-results` | 2 | An `export fn` may have several results (the first is C's return value, the others out-pointers) and builder parameters: the runtime keeps a scratch builder per parameter, whose elements and count the wrapper hands back, valid until the next exported call. |
| 0003 | `emit-interface` | 2 | `--emit-interface out.json` (and `goose check --emit-interface json`): each export's parameters and results, and the layout of every struct and enum they reach, as the backend lays them out. `goose_build` builds its Rust from it; `cargo goose` checks generated modules against it. |
| 0004 | `runtime-hooks` | 2 | A library an extern block opens that exports `goose_runtime_hooks` is handed the runtime's abort (which ends a contained worker alone, as any Goose abort does) and `at_end` (a function called when the program ends; the Rust side prints its leak report there in debug builds). `gs_fail` names a place with no line alone. |
| 0005 | `cargo-crate` | 2 | `cargo_crate = "dir";` and `cargo_features` in a sidecar: the compiler runs `cargo goose native` as the sidecar loads, which builds the crate and writes its module; `goose build` links the static library, `goose run` builds the JIT library from it, and the program hands the crate the runtime's hooks. Several crates in one program link as one static library, an umbrella crate `cargo goose combine` builds; a package whose module its crate writes needs only its sidecar. |
| 0006 | `package-dylibs` | 4 | A `./` spelling in a package's `dylibs` entry is a file from the package's own root, so a package can ship a prebuilt library per platform; `dylibs_<platform>_x86_64` and `dylibs_<platform>_arm64` override `dylibs_<platform>` where the compiler runs on that architecture. |
| 0007 | `align` | 2, 3 | `align n` among a struct's fields or an enum's variants: every value of the type sits at a multiple of n (its C type is aligned, and a growable array of it starts at an aligned stack top). An aligned enum's tag takes n bytes, so its payload starts at n: Rust's `#[repr(C, u8)]` layout. The rules that keep elements aligned (size a multiple of n, aligned fields in aligned containers, no limited or variable arrays of it) are compile errors. |
| 0008 | `reentrant` | 3 | `reentrant` before an extern's `fn`: its C may call the program's export fns back. Its callers publish every cached stack top first, so a call back starts above all live data. The program registers its export fns as it starts; the runtime's hooks (version 2) add `reentry_lookup(name, sig, &why)`, which finds one by C symbol and signature and refuses one touching a `var` global, and `on_program_thread()`. |

## Callbacks, two ways

0008 is the change the proposal lists for Phase 3: nested calls that start
above every live data stack top, so Rust can call Goose export fns while
Goose waits on it (`&goose::Reentry`, `goose::callbacks!`). A Rust function
taking a closure needs no patch: it runs as a coroutine (corosensei) on its
own stack, on the Goose thread, each call of the closure handing its
arguments to the caller's block. See [design.md](design.md#callbacks).

## Working on the series

`.toolchain/goose-dev` is a clone with one commit per patch:

```sh
git -C .toolchain/goose-dev format-patch -o ../../goose-patches e76d5c9f7f0a17d83c12a405fbb2513094d03985
```

After editing `src/runtime/`, regenerate the embedded copy with
`build/goose --gen-runtime-header` and rebuild before committing.
