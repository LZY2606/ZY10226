//! Directed fuzz corpus for `testing-tools/ttf-fuzz`.
//!
//! AFL needs seed inputs; an empty corpus wastes hours rediscovering shapes that
//! are already known to stress the outline budgets. The fixtures here are the
//! smallest fonts that reach each bounded path (component cycles/fan-out, gvar
//! tuple overflow, CFF/CFF2 subroutine cycles). They live both as committed
//! bytes under `testing-tools/ttf-fuzz/corpus/` and, in source form, as the
//! structured constructors in this suite.
//!
//! Two kinds of tests:
//!
//! - `corpus_*`: every committed fixture parses (if it parses at all) to a
//!   *bounded* result — no panic, no infinite loop, callback counts under the
//!   input-derived ceiling. These run on every `cargo test` and make the corpus
//!   self-checking without needing AFL installed.
//! - `regenerate_corpus` (ignored): rebuilds the corpus files from the
//!   constructors, keeping bytes and structured fixtures in sync. Run with
//!   `cargo test --test malicious_fonts regenerate_corpus -- --ignored`.
//!
//! Reading the corpus uses a fixed relative path and an explicit file list, so
//! the tests do not depend on directory traversal order, the network or a clock.

use super::recording::RecordingBuilder;
use super::tt_builder::{
    CompositeGlyph, GlyphData, build_otto, build_sfnt, fan_out_components, triangle_leaf,
};
use std::fs;
use std::path::PathBuf;
use ttf_parser::GlyphId;

const OUTLINE_CORPUS_DIR: &str = "testing-tools/ttf-fuzz/corpus/outline";
const VARIABLE_CORPUS_DIR: &str = "testing-tools/ttf-fuzz/corpus/variable-outline";

/// One fixture: file name, bytes, and whether it is exercised through a variable
/// outline (with a coordinate applied). `target_glyphs` lists only the glyphs
/// that actually reach the bounded path: outlining *every* glyph of a fan-out
/// font would spend the 100 000-visit budget once per glyph (seconds per file)
/// while proving nothing extra about that same budget.
struct Fixture {
    name: &'static str,
    variable: bool,
    target_glyphs: &'static [u16],
    data: Vec<u8>,
}

/// Same per-visit ceiling used by the structured glyf tests.
const INPUT_BOUND: u64 = 100_000;

/// Asserts the selected glyphs of one font are bounded through both
/// `outline_glyph` and `glyph_bounding_box`.
fn assert_outline_is_bounded(data: &[u8], variable: bool, glyphs: &[u16]) {
    let mut face = match ttf_parser::Face::parse(data, 0) {
        Ok(face) => face,
        Err(_) => return, // unparseable seeds are legal fuzz input and trivially bounded
    };

    #[cfg(feature = "variable-fonts")]
    if variable && face.is_variable() {
        let axis = face.variation_axes().get(0).map(|a| a.tag);
        if let Some(tag) = axis {
            let _ = face.set_variation(tag, 900.0);
        }
    }
    #[cfg(not(feature = "variable-fonts"))]
    let _ = variable;

    for &id in glyphs {
        let mut builder = RecordingBuilder::new();
        let bbox = face.outline_glyph(GlyphId(id), &mut builder);
        let _ = face.glyph_bounding_box(GlyphId(id));

        // Input-derived provable bound: each callback needs a parsed point and the
        // parser admits at most INPUT_BOUND component visits, so the total is
        // bounded by budget * input bytes regardless of whether the font uses
        // glyf, gvar, CFF or CFF2.
        assert!(
            builder.counts.draw_verbs() <= INPUT_BOUND * data.len() as u64,
            "fixture glyph {id}: {} draw verbs exceeds input-derived bound",
            builder.counts.draw_verbs()
        );
        // A returned rectangle has plain integer coordinates; nothing infinite.
        let _ = bbox;
    }
}

fn corpus_fixtures() -> Vec<Fixture> {
    use super::cff_builder::{cff1_table, cff2_op, cff2_table, cs_int};

    let mut fixtures = Vec::new();

    // 1. glyf self-loop.
    {
        let glyphs = [
            triangle_leaf(),
            GlyphData::Composite(CompositeGlyph {
                bbox: [10, 10, 30, 30],
                components: fan_out_components(1, 1),
                ..Default::default()
            }),
        ];
        fixtures.push(Fixture {
            name: "glyf-self-loop.ttf",
            variable: false,
            target_glyphs: &[1],
            data: build_sfnt(&glyphs, &[]),
        });
    }

    // 2. glyf two-node cycle.
    {
        let glyphs = [
            triangle_leaf(),
            GlyphData::Composite(CompositeGlyph {
                bbox: [10, 10, 30, 30],
                components: fan_out_components(2, 1),
                ..Default::default()
            }),
            GlyphData::Composite(CompositeGlyph {
                bbox: [10, 10, 30, 30],
                components: fan_out_components(1, 1),
                ..Default::default()
            }),
        ];
        fixtures.push(Fixture {
            name: "glyf-two-node-cycle.ttf",
            variable: false,
            target_glyphs: &[1],
            data: build_sfnt(&glyphs, &[]),
        });
    }

    // 3. glyf fan-out diamond (fan-out 3, depth 20).
    {
        const FAN_OUT: u16 = 3;
        const DEPTH: u16 = 20;
        let mut glyphs = vec![triangle_leaf()];
        for level in 1..=DEPTH {
            glyphs.push(GlyphData::Composite(CompositeGlyph {
                bbox: [10, 10, 30, 30],
                components: fan_out_components(level - 1, FAN_OUT),
                ..Default::default()
            }));
        }
        fixtures.push(Fixture {
            name: "glyf-fanout-diamond.ttf",
            variable: false,
            target_glyphs: &[DEPTH],
            data: build_sfnt(&glyphs, &[]),
        });
    }

    // 4. CFF1 global subr self-loop.
    {
        let subr = {
            let bias = 107i32;
            let mut b = cs_int(0 - bias);
            b.push(29); // callgsubr
            b.push(11); // return
            b
        };
        let glyph = {
            let mut g = cs_int(100);
            g.push(22); // hmoveto
            g.extend_from_slice(&{
                let mut c = cs_int(-107);
                c.push(29);
                c
            });
            g.push(14); // endchar
            g
        };
        let cff = cff1_table(&glyph, &[subr], &[]);
        fixtures.push(Fixture {
            name: "cff1-global-subr-loop.otf",
            variable: false,
            target_glyphs: &[0],
            data: build_otto(*b"CFF ", cff),
        });
    }

    // 5. CFF2 global subr self-loop (only reachable with variable-fonts).
    {
        let subr = {
            let mut b = cs_int(-107);
            b.push(cff2_op::CALL_GLOBAL_SUBROUTINE);
            b
        };
        let glyph = {
            let mut g = cs_int(100);
            g.push(cff2_op::HORIZONTAL_MOVE_TO);
            g.extend_from_slice(&subr);
            g
        };
        let cff = cff2_table(&glyph, std::slice::from_ref(&subr), &[]);
        fixtures.push(Fixture {
            name: "cff2-global-subr-loop.otf",
            variable: false,
            target_glyphs: &[0],
            data: build_otto(*b"CFF2", cff),
        });
    }

    // 6. Variable gvar fan-out diamond (exercised with an applied coordinate).
    #[cfg(feature = "variable-fonts")]
    {
        use super::var_builder::{fvar_one_axis, gvar_table};
        const FAN_OUT: u16 = 3;
        const DEPTH: u16 = 16;
        let mut glyphs = vec![triangle_leaf()];
        for level in 1..=DEPTH {
            glyphs.push(GlyphData::Composite(CompositeGlyph {
                bbox: [10, 10, 30, 30],
                components: fan_out_components(level - 1, FAN_OUT),
                ..Default::default()
            }));
        }
        let blobs: Vec<Vec<u8>> = (0..glyphs.len()).map(|_| Vec::new()).collect();
        let extras = [(*b"fvar", fvar_one_axis()), (*b"gvar", gvar_table(&blobs))];
        fixtures.push(Fixture {
            name: "gvar-fanout-diamond.ttf",
            variable: true,
            target_glyphs: &[DEPTH],
            data: build_sfnt(&glyphs, &extras),
        });
    }

    fixtures
}

#[test]
fn committed_corpus_fixtures_are_all_bounded() {
    for fixture in corpus_fixtures() {
        let dir = if fixture.variable {
            VARIABLE_CORPUS_DIR
        } else {
            OUTLINE_CORPUS_DIR
        };
        let path: PathBuf = [dir, fixture.name].iter().collect();
        let bytes = fs::read(&path).unwrap_or_else(|e| {
            panic!(
                "missing corpus fixture {} (run `cargo test --test malicious_fonts \
                 regenerate_corpus -- --ignored`): {e}",
                path.display()
            )
        });
        assert_eq!(
            bytes, fixture.data,
            "corpus fixture {} is out of sync with the structured constructor; \
             regenerate with `cargo test --test malicious_fonts regenerate_corpus -- --ignored`",
            fixture.name
        );
        assert_outline_is_bounded(&bytes, fixture.variable, fixture.target_glyphs);
    }
}

#[test]
#[ignore = "writes the directed fuzz corpus from the structured constructors"]
fn regenerate_corpus() {
    for fixture in corpus_fixtures() {
        let dir = if fixture.variable {
            VARIABLE_CORPUS_DIR
        } else {
            OUTLINE_CORPUS_DIR
        };
        fs::create_dir_all(dir).unwrap();
        let path: PathBuf = [dir, fixture.name].iter().collect();
        fs::write(&path, &fixture.data).unwrap();
        eprintln!("wrote {} ({} bytes)", path.display(), fixture.data.len());
    }
}
