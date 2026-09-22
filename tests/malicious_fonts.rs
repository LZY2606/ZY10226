//! Structured adversarial-font tests for the bounded-outline guarantees.
//!
//! ttf-parser parses untrusted fonts with no `unsafe`, bounded stack use and (apart
//! from the opt-in `gvar-alloc` feature) no heap allocations. The practical danger
//! is therefore *work amplification*: `glyf` components and CFF/CFF2 subroutines
//! let a few hundred bytes describe a graph whose expansion produces astronomically
//! many outline callbacks (exponential fan-out, cycles, lying offsets/lengths). The
//! parser stops this with explicit, documented budgets, and these tests verify the
//! structural outcome instead of relying on a fuzzing wall-clock timeout:
//!
//! - a malicious glyph returns `None` (or the documented `CFFError`) instead of
//!   hanging, overflowing the stack or panicking;
//! - a rejected glyph emits no callbacks that describe unverified data, and every
//!   callback from an accepted glyph stays inside the font's declared domain;
//! - accepted work obeys a bound provable from the parsed input (points per leaf
//!   times accepted visits), never from elapsed time.
//!
//! Mirrored limit constants duplicate private `src/` constants on purpose:
//! changing a limit must be a deliberate act that updates parser and tests together.

#[path = "malicious_fonts_support/recording.rs"]
mod recording;
#[path = "malicious_fonts_support/tt_builder.rs"]
mod tt_builder;

#[path = "malicious_fonts_support/cff_builder.rs"]
mod cff_builder;
#[path = "malicious_fonts_support/cff_tests.rs"]
mod cff_tests;
#[path = "malicious_fonts_support/corpus_tests.rs"]
mod corpus_tests;
#[path = "malicious_fonts_support/glyf_tests.rs"]
mod glyf_tests;
#[cfg(feature = "variable-fonts")]
#[path = "malicious_fonts_support/gvar_tests.rs"]
mod gvar_tests;
#[path = "malicious_fonts_support/var_builder.rs"]
mod var_builder;
