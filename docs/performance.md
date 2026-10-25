# Copy and overhead contracts

The bridge currently borrows compatible memory and copies when alignment,
layout or ownership requires it. Safe borrowing still has boundary checks.
There is no zero-overhead guarantee. The measurements in
[bench/RESULTS.md](../bench/RESULTS.md) cover selected calls on one machine;
they do not cover re-entry, coroutine callbacks or every layout conversion.

## Proposed strict zero-copy exports

This section describes a proposed bridge option, **not an implemented
attribute**. It requires no project-specific Goose language feature:

```rust,ignore
#[goose::export(zero_copy)]
pub fn sum(xs: &[f64]) -> f64 {
    xs.iter().sum()
}
```

Its contract would be: the bridge never duplicates an argument or result's
bulk payload. Scalar passing, computing output values and writing those
values to their final destination remain ordinary work.

- Reject incompatible layouts during binding generation, using Rust's
  metadata and the compiler's native-interface JSON. Check aggregate
  alignment, offsets, size, array stride and enum representation.
- For addresses whose alignment cannot be proved statically, check at
  entry and report failure rather than allocate a fallback copy.
- Permit borrowed slices and references, results expressed as spans into
  input arguments, and direct append-only output builders.
- Reject owned `String`/`Vec` inputs and outputs on these exports: the
  current ownership boundary requires copies. Offer borrowed inputs and
  direct builders instead. Returning a view of an object behind a handle
  also needs a separate lifetime/guard API before it can qualify.
- Reject coroutine callback forms which retain copied arguments. Direct
  re-entry may borrow compatible arguments within the running call, with
  its existing thread and lifetime restrictions.
- Keep UTF-8, value-validity, mutable-aliasing and handle checks wherever
  their invariants cannot be proved. A zero-copy `&str` still validates
  bytes; it does not make arbitrary Goose byte arrays trusted text.

An explicit converting API would retain today's copy fallback. This makes
copy policy reviewable at the function declaration instead of dependent on
the particular address passed at runtime.

## Producing results directly

A Rust function returning `Vec<T>` allocates its vector and the bridge copies
its contents into Goose storage. A function taking `&mut GooseVec<T>` can
compute each element and write it straight to the final Goose builder.
That avoids the temporary vector and transfer copy. Builder bookkeeping and,
when required, representation conversion still have costs.

Borrowed results rooted in a slice argument use an offset and length and
keep the argument's lifetime. They need no payload copy. Lists of spans may
allocate/write span records while leaving the referenced payload in place.

## Measuring overhead

Treat these as separate properties:

| Property | Evidence required |
|---|---|
| No payload copy | Exact-layout proofs plus instrumented copy/allocation checks on exercised paths |
| No bridge heap allocation on the steady-state path | Allocation measurements, separated from application work and first-use setup |
| Minimal per-call overhead | Equivalent bare-native and bridged workloads, multiple sizes and repeated measurements |
| No cost for unused machinery | Generated-code inspection for ordinary versus reentrant calls, with and without native providers |

A useful next benchmark expansion would cover aligned and unaligned
arguments, converted and aligned enums (including mutable fixups), owned
vector results versus direct builders, first and subsequent callback calls,
coroutines and contended/uncontended handle operations. Report payload
copies, bytes allocated and latency separately; a zero copy counter alone
cannot prove zero allocation or zero overhead.

Compile-time interface emission and native build providers add build work.
Library loading and runtime hooks add startup/shutdown work. Reentrant calls
publish stack positions and use callback lookup/checks; the Rust side caches
a successful symbol lookup. These costs should be paid by the features that
need them and measured independently of ordinary numeric calls.
