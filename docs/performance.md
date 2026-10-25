# Copy and overhead contracts

`#[goose::export(zero_copy)]` is an opt-in contract: the generated bridge
never duplicates an argument or successful result's bulk payload. It
borrows compatible storage or reports failure. Ordinary exports retain
safe copy fallbacks. Neither mode promises zero overhead.

## Strict zero-copy exports

```rust
#[goose::export(zero_copy)]
pub fn sum(xs: &[f64]) -> Result<f64, String> {
    Ok(xs.iter().sum())
}
```

The generated shim validates the argument and borrows its original storage.
If the address is misaligned, the function body is not called and the
`Result` wrapper reports failure, with a diagnostic when an error builder
is supplied. An `Option` reports `None`; a non-fallible function aborts as
other bridge failures do. No aligned temporary or mutable write-back copy
is made. Null/invalid slices and mutable aliasing remain broken-language-rule
errors, which abort according to the existing bridge rules.

The annotation composes with `instantiate`, `main_thread`, `throws`,
optional arguments and direct re-entry. Its scope is the generated bridge,
not allocations or copies written by the Rust function or its callees.
Error and panic diagnostics may allocate, format and copy text; they are
outside the successful-data-path guarantee.

| Rust shape | Strict contract |
|---|---|
| Primitive scalars and scalar aliases | Ordinary scalar passing |
| `&str`, `&[T]`, `&mut [T]`, references to flat values/arrays | Borrow in place; layout and address alignment must match |
| Optional borrowed inputs | Preserve the borrow when present; Goose optionals are nullable references |
| Handles, borrowed or owned | Pass a handle token; retain generation/thread/locking checks |
| `&mut GooseBuf`, `&mut GooseVec<T>` | Write output directly to the final Goose builder |
| A result borrowed from an input slice | Return offset and length into the original payload |
| An iterator over views into an input slice | Stream span descriptors into the final builder, without an intermediate vector |
| `String`/`Vec` inputs, including optional inputs | Compile error; take a borrowed input instead |
| `String`, `Vec`, `Box`, `Cow`, collected/lazy iterators, or views borrowed from a handle as results | Compile error; use an input-rooted view or direct builder |
| Flat structs/enums/arrays passed or returned by value | Compile error; borrow inputs and use builders for outputs |
| Coroutine closure callbacks | Compile error; use direct `&goose::Reentry` callbacks |

Named types are checked through their Rust traits, including aliases and
generic instantiations. Incompatible flat layouts are compile errors even
when an input would be empty. Mutable aligned enums retain their tag fixups.
Safety checks for UTF-8, bools, chars, enum tags and mutable overlap remain.

### Binding generation

The Rust metadata records each function's `zero_copy` policy. Binding
generation checks its types and uses the compiler's `--emit-interface` JSON
to verify sizes/array strides, field offsets and sizes, declared alignment,
and enum payload/tag/variant layouts. Packed struct declarations can still
borrow an actually aligned address; the runtime always checks that address.
`--no-verify` cannot disable verification for a library with strict exports.

A compiler without `emit-interface` gets no strict declarations, with an
explanation in the generated module. Strict typed builders and span results
also require `typed-builders`; they never use the length/copy fallback.
Existing ordinary exports continue to work with stock compilers. Packaging
an emitted strict API currently supports the host target only: host metadata
cannot certify a foreign target's Rust layout or conditional fields. The
packager rejects that combination before building the foreign targets.
Portable package modules omit strict forms needing typed builders, aligned
enums or re-entry, just as they omit those compiler-dependent forms today.

Metadata format 2 carries the policy and prevents older binding tools from
silently ignoring it. Current tools also read format 1 libraries as ordinary
exports. The public native value ABI and the bridge calling convention stay
at version 1. No new Goose compiler feature is needed for this annotation.

## Producing results directly

```rust
#[goose::export(zero_copy)]
pub fn squares(n: i64, out: &mut goose::GooseVec<i64>) {
    for i in 0..n {
        out.push(i * i);
    }
}
```

This computes each element directly into Goose-owned storage, avoiding the
Rust vector and transfer copy of a `Vec<i64>` result. Computing/storing
values and updating builder metadata remain ordinary work. The bridge
cannot enforce the copy policy on a user function's own implementation.

Borrowed results rooted in a slice argument carry an offset and length and
keep the argument's lifetime. Span lists write descriptor records while
leaving the referenced payload in place. Creating a handle can allocate;
borrowing one retains its locking cost. Zero-copy does not mean zero
allocation or zero overhead.

## Validation and measurement

- Compile-fail fixtures reject owned inputs/results, coroutine callbacks,
  by-value aggregates, converted layouts and copied views of handles.
- Conformance tests exercise strict borrows, failures before entry, mutable
  updates, aliasing, validation, optional references, scalar aliases,
  handles, span results and direct builders.
- An allocation-metered shim test checks selected successful steady-state
  calls after panic-catcher/TLS initialization. It verifies zero heap
  allocations for borrowed mutation, span-list output and direct builder
  output. Pointer identity and the copy counter check borrowing separately.
- Binding tests reject equal-size enums with wrong tags/payload offsets and
  prove that `--no-verify` cannot bypass strict verification.

The measurements in [bench/RESULTS.md](../bench/RESULTS.md) predate the strict
annotation. They cover selected ordinary calls on one machine, not re-entry,
coroutines or every conversion. Further latency benchmarks should compare
ordinary and strict paths, aligned and rejected inputs, first and subsequent
callbacks, and handle contention, reporting copies, allocation and latency
separately. A zero copy counter alone proves neither zero allocation nor
zero overhead.

Compile-time interface emission and native build providers add build work.
Library loading and hooks add startup/shutdown work. Reentrant calls publish
stack positions and use callback lookup/checks; the Rust side caches a
successful lookup. Those costs remain independent of the copy policy.
