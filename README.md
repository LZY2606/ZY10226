## ttf-parser

![Build Status](https://github.com/harfbuzz/ttf-parser/workflows/Rust/badge.svg)
[![Crates.io](https://img.shields.io/crates/v/ttf-parser.svg)](https://crates.io/crates/ttf-parser)
[![Documentation](https://docs.rs/ttf-parser/badge.svg)](https://docs.rs/ttf-parser)
[![Rust 1.88+](https://img.shields.io/badge/rust-1.88+-orange.svg)](https://www.rust-lang.org)
![Unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-brightgreen.svg)

> **This crate is in maintenance mode. Bug fixes only — no new features.**
>
> Bug reports and fixes are welcome and will be reviewed promptly. Correctness, panics and
> security issues are in scope. New table support, new API surface and performance work are
> not, and feature requests will be closed.
>
> **For new projects, we recommend [fontations](https://github.com/googlefonts/fontations)**
> (`read-fonts` and `skrifa`), which is actively developed by Google Fonts, has broader table
> support, and is the direction the Rust font ecosystem is moving.

A high-level, safe, zero-allocation font parser for
[TrueType](https://docs.microsoft.com/en-us/typography/truetype/),
[OpenType](https://docs.microsoft.com/en-us/typography/opentype/spec/), and
[AAT](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6AATIntro.html).

Can be used as a Rust or C library.

Requires Rust 1.88 and uses edition 2024.

### Features

- A high-level API for most common properties, hiding all parsing and data resolving logic.
- A low-level, but safe API to access TrueType tables data.
- Highly configurable. You can disable most of the features, reducing binary size.
  You can also parse TrueType tables separately, without loading the whole font/face.
- Zero heap allocations.
- Zero unsafe.
- Zero dependencies.
- `no_std`/WASM compatible.
- A basic [C API](./c-api).
- Fast.
- Stateless. All parsing methods are immutable.
- Simple and maintainable code (no magic numbers).

### Safety

- The library must not panic. Any panic considered as a critical bug and should be reported.
- The library forbids unsafe code.
- No heap allocations, so crash due to OOM is not possible.
- All recursive methods have a depth limit, and the ones whose input forms a graph
  (composite glyphs, the COLRv1 paint graph, CFF subroutines) additionally bound the
  *total* work per call. A depth limit alone does not: with fan-out `b` and depth `d`,
  a small font can force `b^d` visits without ever exceeding the depth.
- The exact hardening budgets are named constants in the source:

  | Bound | Value | What it stops |
  |---|---|---|
  | `glyf::MAX_COMPONENTS` | 32 | composite nesting depth (self-loops, cycles) |
  | `glyf::MAX_COMPONENT_VISITS` | 100 000 | total component visits per outline, so shared-child fan-out and cycles are linear |
  | CFF `STACK_LIMIT` | 10 | CFF/CFF2 subroutine nesting depth |
  | CFF `MAX_SUBROUTINE_CALLS` | 4 096 | total subroutine invocations per glyph |
  | `gvar::MAX_STACK_TUPLES_LEN` | 32 | variation tuples buffered on the stack (more require the opt-in `gvar-alloc` feature) |

  Semantics: when a budget is exhausted the outline fails closed — `Face::outline_glyph` /
  `Face::glyph_bounding_box` return `None`, and the low-level CFF tables report
  `CFFError::NestingLimitReached` or `CFFError::SubroutineCallLimitReached` (collapsed to
  `None` by the high-level API). Callbacks already emitted for fully verified sub-trees are
  retained; no callback can describe coordinates outside the font's declared domain.
  Complexity is therefore `O(budget)` work and `O(depth)` stack per call, independent of how
  the input graph is shaped. Compatibility trade-off: a font deliberately engineered past
  these limits (not observed in real corpora) outlines as `None` rather than consuming
  unbounded CPU; this is the intended maintenance-mode behaviour and is not a public-API
  change. The structured fixtures proving all of this live in
  `tests/malicious_fonts.rs`; a small directed fuzz seed corpus lives in
  `testing-tools/ttf-fuzz/corpus/`.
- Stack usage is bounded, but not tightly: outlining a composite variable glyph nests up to
  32 frames, each holding a variation-tuple buffer, for roughly 80KiB in the worst case.
- Most of arithmetic operations are checked.
- Most of numeric casts are checked.

### License

Licensed under either of

- Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license
  ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

See [CONTRIBUTING.md](./CONTRIBUTING.md) for how to build and test the library, the C API
and the benchmarks, the lint and formatting workflow, and what is expected of a change to a
parser of untrusted input.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
