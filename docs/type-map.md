# The type map

What `#[goose::export]` accepts, what Goose sees, how the value crosses and
what the generated shim checks. A type the map does not cover is a compile
error from the macro, pointing at the type.

## Parameters

| Rust | Goose wrapper takes | At the boundary (spec §7.10) | Checked |
|---|---|---|---|
| `i8` ... `u64`, `f32`, `f64` | same names | by value | nothing |
| `bool` | `bool` | a byte | 0 or 1 |
| `usize`, `isize` | `u64`, `i64` | by value | 64-bit targets |
| `char` | `u32` | by value | a Unicode scalar value (a failure otherwise) |
| `#[derive(Flat)]` struct or enum, by value | `T` | `const T&` | as `&T`, then copied |
| `&T`, `&mut T` (a `Flat` type or a scalar) | `const T&`, `T&` | a pointer | aligned and laid out as Rust's, else copied in (and out for `&mut`); valid bools, chars, tags; no overlap with another argument when one is `&mut` |
| `&[T]`, `&mut [T]` | `const T[:]`, `T[:]` | `{ data, len }` | aligned and laid out as Rust's, else an aligned copy (written back for `&mut`); valid values; overlap when `&mut` |
| `&str` | `const u8[:]` | `{ data, len }` | valid UTF-8 (a failure otherwise); take `&[u8]` to skip it |
| `String`, `Vec<u8>`, `Vec<T>` | `const u8[:]`, `const T[:]` | `{ data, len }` | as the borrowed form, then copied into the owned value |
| `&mut GooseBuf` | `u8[>..]&` | header and stack | nothing: the type only appends |
| `&mut GooseVec<T>` | `T[>..]&` | header and stack | nothing (goose-patches/0001) |
| `&H`, `&mut H` for a `#[goose::handle]` type | `H` | `{ slot: u32, generation: u32 }` | the generation; the thread for a pinned type; the same handle twice with one `&mut` aborts |
| `H` by value | `H` | the handle | as above; the call takes the object and the handle is released |
| `Option<&T>`, `Option<T>`, `Option<&str>`, ... | `T?` | one entry point per combination (`S_some`/`S_none`, `S_sn`...) | as the inner type |
| `impl FnMut(A..) -> R`, `&mut dyn FnMut(..)` | the caller's block | the function runs as a coroutine: `S__start`, `S__next`, `S__reply`, `S__finish` | arguments are copied, since Goose code runs between the calls |
| `T` generic with `instantiate(T = a, b)` | one overload per type | one symbol per type (`S_a`, `S_b`) | as each instantiation |
| `&goose::Reentry` | nothing: not a Goose parameter | the extern is a `reentrant fn` (goose-patches/0008) | the program's main thread, and a compiler with the patch; otherwise the call is an `Err(ReentryError)` |

## Results

| Rust | Goose wrapper returns | At the boundary | Note |
|---|---|---|---|
| scalars | same | by value | `bool` as a byte |
| a `Flat` value | `T` | an out-pointer | written as Goose lays it out |
| a handle type | `H` | an out-pointer | the object goes into the slot table |
| `String`, `Vec<u8>`, `Box<str>`, `Cow<str>` | `u8[>..]` | a builder | one copy, into the caller's variable |
| `Vec<T>` | `T[>..]` | a typed builder (goose-patches/0001), else `S__len` then `S__take` into an array the wrapper sizes | |
| `&'a str`, `&'a [T]` borrowed from a slice parameter | `const u8[:]`, `const T[:]` | offset and length | re-sliced in Goose, so the result is rooted at the argument; must lie inside it |
| `&str`, `&[T]` borrowed from anything else (a handle's object) | `u8[>..]`, `T[>..]` | copied out | |
| `Option<T>` | `T, bool` | out-parameters and a bool | a panic is `None` |
| `Result<T, E: Display>` | `T, bool`, with an overload taking `err: u8[>..]&` for the text | out-parameters, an error builder, a bool | a panic, an invalid argument and `Err` are failures; `#[goose::export(throws)]` adds a wrapper that throws the text through Goose's `exception` module |
| `impl Iterator<Item = &'a str>` (or `&'a [T]`) over a slice parameter | `Span[>..]`, and `each_name<F>(..)` handing each part to a block | a span list | nothing is copied |
| `impl Iterator<Item = T>` with no borrow | an iterator handle, `next(it) -> T, bool`, `each_name<F>(..)` | a pinned handle | lazy |
| `impl Iterator<Item = T> + '_` | `T[>..]` | collected | |

## Types

| Rust | Goose |
|---|---|
| `#[derive(Flat)] #[repr(C)] struct` | `struct` with `pad n` where rustc put padding, `align n` with goose-patches/0007; crosses in place |
| a struct holding an enum with data | `struct` packed; converted field by field |
| fieldless `#[repr(u8)] enum`, variants numbered from 0 | `enum`, one byte, in place |
| a fieldless enum with explicit discriminants | `enum`; converted by index |
| an enum with data | `enum` (a tag and the largest payload, packed); converted, a slice in one pass |
| `#[goose(align)] #[repr(C, u8)] enum` | `enum` with `align n` and rustc's pads (goose-patches/0007); in place |
| `[T; N]` | `T[N]` |
| `#[goose::handle] struct` | `struct { slot: u32, generation: u32 }`, with `release`, `live_count_<name>`, and `with_<constructor>` scoped helpers |

## Calling back: `goose::callbacks!`

Each `fn name(..) -> R;` in the macro becomes `name(goose: &Reentry, ..) ->
Result<R, ReentryError>`, calling the program's `export fn name` with the
Goose signature below (goose-patches/0008).

| Callback parameter (Rust) | The export fn's parameter (Goose) |
|---|---|
| `i8` ... `u64`, `f32`, `f64`, `bool` | same names |
| `&str` | `const u8[:]` |
| `&[T]`, `&mut [T]` | `const T[:]`, `T[:]` |
| `&T`, `&mut T` (a `Flat` type laid out as Goose lays it out, asserted at compile time) | `const T&`, `T&` |
| result: a scalar, or `()` | the same result, or none |

## Still rejected

Async functions, Rust closures kept as values, trait objects other than
`dyn FnMut` callbacks, references inside `Flat` types, generic `Flat` types
(give each instantiation a type), tuples, and nesting such as
`Option<Option<T>>` or `Result<Option<T>, E>`.

## Rust using Goose (goose_build)

| Goose export | Rust wrapper |
|---|---|
| scalars | same; `bool` as a byte at the boundary |
| `T` (a struct or enum) by value or `const T&`/`T&` | the mirror struct (`#[repr(C, packed)]`, or `#[repr(C, align(n))]` for `align n`), `&T`, `&mut T` |
| `const T[:]`, `T[:]` | `&[T]`, `&mut [T]` |
| `const u8[:]` result | `&'a [u8]`, borrowed for as long as the runtime and the arguments |
| several results (goose-patches/0002) | a tuple |
| `u8[>..]&`, `T[>..]&` parameter (goose-patches/0002) | taken out of the parameters; the elements come back in the result, borrowed from the runtime's scratch builder for as long as `&mut Runtime` is |

Every wrapper takes `&Runtime` (or `&mut Runtime` where a builder is
involved), which is neither `Send` nor `Sync`: the Rust compiler keeps every
call on the thread that ran `goose_init`.

## Strict exports

`#[goose::export(zero_copy)]` rejects copy-producing signatures and requires
borrowing in place. Misalignment reports failure instead of allocating a
fallback copy. Borrowed views, handles and direct builders are supported;
owned payloads, by-value aggregates and coroutine callbacks are rejected.
Strict span results stream descriptor records directly into their builder.
The binding tools require compiler layout verification, even with
`--no-verify`. See [performance.md](performance.md) for the full contract and
its failure-path and allocation limits.
