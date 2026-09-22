## Build

Install AFL first:

```text
cargo install afl
```

and then build via `cargo-afl`:

```text
cargo afl build
```

## Run

Before running, we have to collect some test data.
Using raw fonts is too wasteful, so we are using the `strip-tables.py` script
to remove unneeded tables.

Here is an example to test `cmap`/`Face::glyph_index`:

```text
strip-tables.py glyph-index in /usr/share/fonts
cargo afl fuzz -i in -o out target/debug/fuzz-glyph-index
```

## Seed corpus

A small directed seed corpus is committed under `corpus/outline/` and
`corpus/variable-outline/`. These are the smallest fonts that reach each bounded
outline path (component self-loops/cycles, shared-child fan-out diamonds, CFF/CFF2
subroutine self-calls). They are generated from and verified by the library test
suite — see `tests/malicious_fonts.rs` and regenerate them with:

```text
cargo test --test malicious_fonts regenerate_corpus -- --ignored
```

The library test `committed_corpus_fixtures_are_all_bounded` fails if the committed
bytes drift from the structured constructors.
