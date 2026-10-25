# The overhead budget

What Ferrous Goose's boundary checks cost per call, measured by
`bench/run.sh`: each `#[goose::export]` function against a bare
`extern "C"` function doing the same work, both called from a Goose
program built with `goose build` (-O2). "checks" is the difference.

- machine: Darwin arm64, Apple M1 Max
- rustc: rustc 1.99.0 (b940084d7 2026-09-28)
- goose: goose runtime 408c611c8952 ferrous: typed-builders export-results emit-interface runtime-hooks cargo-crate package-dylibs align 
- date: 2026-10-25

| call | bare (ns) | bridged (ns) | checks (ns) |
|---|---|---|---|
| scalar `add(i64, i64)` | 1.9 | 3.7 | 1.7 |
| `&[i64]` of 64, summed | 6.5 | 12.1 | 5.6 |
| `&[u8]` of 64 | 1.1 | 4.2 | 3.0 |
| `&str` of 64 (UTF-8 check) | 1.1 | 11.2 | 10.0 |
| `&str` of 4096 (UTF-8 check) | 1.1 | 148.4 | 147.3 |
| `axpy(&mut [f64], &[f64])` of 64 (overlap check) | 8.5 | 21.9 | 13.4 |
| handle `&mut Counter` |  | 23.0 |  |
| handle `&Counter` |  | 22.9 |  |
| `Result<i64, String>` | 1.9 | 4.1 | 2.2 |
| builder, 16 bytes appended |  | 36.0 |  |

(sink 0)

A scalar call pays for the panic catcher; a slice adds an alignment
test (and an aligned copy when Goose's packed storage leaves it
unaligned, which `align n` rules out); `&str` adds UTF-8 validation,
linear in the length (take `&[u8]` to skip it); a `&mut` argument
adds the overlap check; a handle adds the generation check and its lock.
