# Copies and overhead

Ordinary exports borrow compatible memory and copy when alignment, layout
or ownership requires it. `#[goose::export(zero_copy)]` disables those copy
paths for the generated bridge. Neither mode guarantees zero overhead.

## Strict zero-copy exports

```rust
#[goose::export(zero_copy)]
pub fn sum(xs: &[f64]) -> Result<f64, String> {
    Ok(xs.iter().sum())
}
```

The shim validates `xs` and borrows its storage. Misalignment prevents the
body from running: a `Result` reports failure, an `Option` returns `None`,
and a non-fallible export aborts. No aligned temporary or write-back copy
is created. Null nonempty slices, invalid lengths and mutable aliasing
abort under the existing bridge rules.

The guarantee covers argument and successful-result payloads handled by
the bridge. User code, callees, error messages and panic diagnostics may
allocate or copy. UTF-8, bool, char, enum-tag and mutable-overlap checks
remain enabled. Mutable aligned enums also need tag fixups after Rust
writes them.

| Rust type or operation | Strict behavior |
|---|---|
| Primitive scalars and scalar aliases | Pass by value |
| `&str`, slices, references to flat values or arrays | Borrow in place after validation |
| Optional borrowed inputs | Preserve the borrow when present |
| Borrowed or owned handles | Pass a token; retain generation, thread and lock checks |
| `&mut GooseBuf`, `&mut GooseVec<T>` | Write to the Goose builder |
| Result borrowed from an input slice | Return offset and length into that input |
| Iterator over views into an input slice | Write span records directly to the builder |
| Owned `String` or `Vec` inputs, including optional inputs | Compile error; use a borrow |
| Owned payload results, collected or lazy iterators, or views borrowed from a handle | Compile error; use an input view or builder |
| Flat structs, enums or arrays passed or returned by value | Compile error; borrow inputs and use builders for outputs |
| Coroutine closure callbacks | Compile error; use direct re-entry |

The attribute supports `instantiate`, `main_thread`, `throws`, optional
arguments and direct re-entry. Trait checks cover named types, aliases and
generic instantiations. Incompatible flat layouts are rejected even for
empty inputs.

## Binding requirements

Strict bindings require `--emit-interface`. The binding generator checks
Rust metadata against the compiler's sizes, strides, field offsets and
sizes, alignment declarations, enum tags and payload layouts.
`--no-verify` cannot bypass this check. Packed structs may still be borrowed
when their actual address meets Rust's alignment; the shim checks it.

Without `emit-interface`, strict functions are omitted with a comment in
the generated module. Typed builders and span results also require
`typed-builders`; they have no length/copy fallback. Ordinary exports
retain their stock-compiler fallbacks.

Packages exposing strict functions currently support the host target only.
Host metadata cannot verify another target's layout or conditional fields,
so the packager rejects foreign targets before building them. Portable
package modules omit strict forms requiring typed builders, aligned enums
or re-entry.

The policy uses metadata format 2, which older binding tools reject.
Current tools also accept format 1 libraries as ordinary exports. The
bridge and public native value ABIs remain at version 1; the annotation
requires no additional compiler patch.

## Direct output

```rust
#[goose::export(zero_copy)]
pub fn squares(n: i64, out: &mut goose::GooseVec<i64>) {
    for i in 0..n {
        out.push(i * i);
    }
}
```

This writes each result to Goose storage. Returning `Vec<i64>` would first
build a Rust vector, then copy it into Goose. Builder stores and metadata
updates still cost work, and a user function can still copy data itself.

A borrowed result transfers an offset and length. A span list writes
records while keeping the referenced payload in place. Creating a handle
may allocate; borrowing one takes a lock. These are separate costs from
payload copying.

## Evidence

- [Compile-fail fixtures](../crates/goose/tests/ui) cover owned payloads,
  aggregate value passing, converted layouts, coroutine callbacks and
  copied views of handles.
- [Shim tests](../conformance/rust/tests/zero_copy.rs) check pointer identity,
  failure before body entry, mutation, value validation and direct output.
  After panic-catcher/TLS initialization, the measured borrowed-mutation,
  span-output and builder calls made no heap allocations.
- [Binding tests](../conformance/rust/tests/bindings.rs) reject enums with
  matching total sizes but wrong tags or payload offsets.
  [CLI tests](../crates/cargo-goose/src/main.rs) check that `--no-verify`
  cannot disable strict verification.
- [Goose tests](../conformance/rust/goose-tests) cover alias rejection,
  optional-reference identity and nested calls during direct re-entry.

[Latency results](../bench/RESULTS.md) predate the strict attribute. They
measure ordinary calls on one machine; they do not measure re-entry,
coroutines or every conversion. Copy counters, allocation counts and
latency measure different properties.

Interface emission and native providers add build work. Library loading
and hooks add startup/shutdown work. Reentrant calls publish stack
positions and check callback eligibility; successful symbol lookups are
cached. Those costs remain under either copy policy.
