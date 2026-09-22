//! Adversarial CFF1 and CFF2 subroutine tests.
//!
//! Limits under test (private constants mirrored here):
//!
//! - `STACK_LIMIT = 10`: charstring subroutine nesting depth;
//! - `MAX_SUBROUTINE_CALLS = 4_096`: total subroutine invocations per glyph.
//!
//! Both a direct self call and a two-subroutine cycle reach the depth limit long
//! before the total-call limit, so they return `NestingLimitReached` from the
//! table API. The high-level `Face::outline_glyph` collapses CFF errors to
//! `None`, which is what applications see; the table-level result is asserted
//! separately so the exact error path is recorded.

use super::cff_builder::{cff1_table, cs_int};
#[cfg(feature = "variable-fonts")]
use super::cff_builder::{cff2_op, cff2_table};
use super::recording::RecordingBuilder;
use super::tt_builder::build_otto;
#[cfg(feature = "variable-fonts")]
use ttf_parser::cff2;
use ttf_parser::{CFFError, GlyphId, cff};

/// Mirrors CFF `STACK_LIMIT` (private).
#[allow(dead_code)]
const STACK_LIMIT: u8 = 10;
/// Mirrors CFF `MAX_SUBROUTINE_CALLS` (private).
const MAX_SUBROUTINE_CALLS: u32 = 4_096;

fn subr_call(index: i32, subrs_len: usize, operator: u8) -> Vec<u8> {
    let bias = if subrs_len < 1240 {
        107
    } else if subrs_len < 33900 {
        1131
    } else {
        32768
    };
    let mut cs = cs_int(index - bias);
    cs.push(operator);
    cs
}

// --- CFF1 ---------------------------------------------------------------

const CFF1_CALLGSUBR: u8 = 29;
const CFF1_CALLSUBR: u8 = 10;
const CFF1_RETURN: u8 = 11;
const CFF1_ENDCHAR: u8 = 14;
const CFF1_HMOVETO: u8 = 22;
const CFF1_LINETO: u8 = 5;

fn cff1_face(data: Vec<u8>) -> ttf_parser::Face<'static> {
    // The returned Face borrows the Vec behind a leaked box, which is fine for a
    // short-lived test and keeps the `'static` lifetime ergonomic.
    let leaked: &'static [u8] = Box::leak(data.into_boxed_slice());
    ttf_parser::Face::parse(leaked, 0).expect("OTTO/CFF sfnt must parse")
}

#[test]
fn cff1_global_subroutine_self_loop_returns_nesting_limit_from_table_api() {
    // One global subroutine that calls itself forever. Depth reaches STACK_LIMIT
    // (10 nested frames) and the interpreter returns NestingLimitReached.
    let subr_body = {
        let mut b = subr_call(0, 1, CFF1_CALLGSUBR);
        b.push(CFF1_RETURN);
        b
    };
    // Glyph: 100 hmoveto, call global subr 0, endchar.
    let glyph = {
        let mut g = cs_int(100);
        g.push(CFF1_HMOVETO);
        g.extend_from_slice(&subr_call(0, 1, CFF1_CALLGSUBR));
        g.push(CFF1_ENDCHAR);
        g
    };
    let table = cff1_table(&glyph, &[subr_body], &[]);

    let parsed = cff::Table::parse(&table).expect("cff parses");
    let mut builder = RecordingBuilder::new();
    let err = parsed.outline(GlyphId(0), &mut builder).unwrap_err();
    assert_eq!(err, CFFError::NestingLimitReached);
    // The move-to happened in the root charstring; the error means the path is
    // abandoned and the high-level API reports `None` rather than a partial bbox.
    assert_eq!(builder.counts.move_to, 1);
}

#[test]
fn cff1_self_loop_surfaces_as_none_from_face_outline_glyph() {
    let subr_body = {
        let mut b = subr_call(0, 1, CFF1_CALLGSUBR);
        b.push(CFF1_RETURN);
        b
    };
    let glyph = {
        let mut g = cs_int(100);
        g.push(CFF1_HMOVETO);
        g.extend_from_slice(&subr_call(0, 1, CFF1_CALLGSUBR));
        g.push(CFF1_ENDCHAR);
        g
    };
    let font = build_otto(*b"CFF ", cff1_table(&glyph, &[subr_body], &[]));
    let face = cff1_face(font);

    let mut builder = RecordingBuilder::new();
    let bbox = face.outline_glyph(GlyphId(0), &mut builder);
    assert_eq!(bbox, None);
    assert_eq!(face.glyph_bounding_box(GlyphId(0)), None);
}

#[test]
fn cff1_two_subroutine_cycle_is_bounded() {
    // Global subr 0 calls subr 1 and vice versa: an even cycle that can never
    // finish. Depth limit applies identically to local/global nesting.
    let subr0 = {
        let mut b = subr_call(1, 2, CFF1_CALLGSUBR);
        b.push(CFF1_RETURN);
        b
    };
    let subr1 = {
        let mut b = subr_call(0, 2, CFF1_CALLGSUBR);
        b.push(CFF1_RETURN);
        b
    };
    let glyph = {
        let mut g = cs_int(100);
        g.push(CFF1_HMOVETO);
        g.extend_from_slice(&subr_call(0, 2, CFF1_CALLGSUBR));
        g.push(CFF1_ENDCHAR);
        g
    };
    let table = cff1_table(&glyph, &[subr0, subr1], &[]);

    let parsed = cff::Table::parse(&table).unwrap();
    let mut builder = RecordingBuilder::new();
    assert_eq!(
        parsed.outline(GlyphId(0), &mut builder).unwrap_err(),
        CFFError::NestingLimitReached
    );
}

#[test]
fn cff1_local_subroutine_self_loop_is_bounded() {
    let subr_body = {
        let mut b = subr_call(0, 1, CFF1_CALLSUBR);
        b.push(CFF1_RETURN);
        b
    };
    let glyph = {
        let mut g = cs_int(100);
        g.push(CFF1_HMOVETO);
        g.extend_from_slice(&subr_call(0, 1, CFF1_CALLSUBR));
        g.push(CFF1_ENDCHAR);
        g
    };
    let table = cff1_table(&glyph, &[], &[subr_body]);

    let parsed = cff::Table::parse(&table).unwrap();
    let mut builder = RecordingBuilder::new();
    assert_eq!(
        parsed.outline(GlyphId(0), &mut builder).unwrap_err(),
        CFFError::NestingLimitReached
    );
}

/// A chain of `depth` global subroutines; subr `i` calls subr `i+1` once. The
/// deepest draws a line. CFF1 subroutines must `return`.
fn cff1_chain(depth: usize) -> (Vec<u8>, Vec<u8>) {
    let mut subrs: Vec<Vec<u8>> = Vec::new();
    for level in 0..depth {
        let mut body = Vec::new();
        if level + 1 < depth {
            body.extend_from_slice(&subr_call(level as i32 + 1, depth, CFF1_CALLGSUBR));
            body.push(CFF1_RETURN);
        } else {
            body.extend_from_slice(&cs_int(50));
            body.extend_from_slice(&cs_int(50));
            body.push(CFF1_LINETO);
            body.push(CFF1_RETURN);
        }
        subrs.push(body);
    }
    let mut glyph = cs_int(100);
    glyph.push(CFF1_HMOVETO);
    glyph.extend_from_slice(&subr_call(0, depth, CFF1_CALLGSUBR));
    glyph.push(CFF1_ENDCHAR);
    let table = cff1_table(&glyph, &subrs, &[]);
    (glyph, table)
}

#[test]
fn cff1_subroutine_chain_at_stacking_depth_10_still_outlines() {
    // Root frame is depth 0; entering the tenth nested subroutine happens when
    // `depth == 10`, which is rejected. Therefore a chain that calls at most 9
    // nested subroutines from the root (10 frames total incl. root glyph) is the
    // accepted boundary. Using 9 nested calls: each subr calls once.
    let (_glyph, table) = cff1_chain(9);
    let parsed = cff::Table::parse(&table).unwrap();
    let mut builder = RecordingBuilder::new();
    let rect = parsed.outline(GlyphId(0), &mut builder).unwrap();
    assert_eq!(
        rect,
        ttf_parser::Rect {
            x_min: 100,
            y_min: 0,
            x_max: 150,
            y_max: 50
        }
    );
    // 100 0 moveto; 150 50 lineto.
    assert_eq!(builder.counts.move_to, 1);
    assert_eq!(builder.counts.line_to, 1);
}

#[test]
fn cff1_subroutine_chain_one_frame_past_stacking_depth_is_rejected() {
    let (_glyph, table) = cff1_chain(11);
    let parsed = cff::Table::parse(&table).unwrap();
    let mut builder = RecordingBuilder::new();
    assert_eq!(
        parsed.outline(GlyphId(0), &mut builder).unwrap_err(),
        CFFError::NestingLimitReached
    );
}

#[test]
fn cff1_fanout_amplification_hits_the_total_call_budget() {
    // fan-out 4, depth 6: nesting maxes at 6 (< 10) but total invocations are
    // sum(4^L, L=1..=6) = 5460 > 4096, so the *total-call* budget (not depth)
    // must fire with SubroutineCallLimitReached.
    let depth = 6usize;
    let fanout = 4usize;
    let total_calls: u64 = (1..=depth).map(|l| (fanout as u64).pow(l as u32)).sum();
    assert!(total_calls > u64::from(MAX_SUBROUTINE_CALLS));

    let mut subrs = Vec::new();
    for level in 0..depth {
        let mut body = Vec::new();
        if level + 1 < depth {
            for _ in 0..fanout {
                body.extend_from_slice(&subr_call(level as i32 + 1, depth, CFF1_CALLGSUBR));
            }
        } else {
            body.extend_from_slice(&cs_int(0));
            body.extend_from_slice(&cs_int(0));
            body.push(CFF1_LINETO);
        }
        body.push(CFF1_RETURN);
        subrs.push(body);
    }
    let mut glyph = cs_int(100);
    glyph.push(CFF1_HMOVETO);
    for _ in 0..fanout {
        glyph.extend_from_slice(&subr_call(0, depth, CFF1_CALLGSUBR));
    }
    glyph.push(CFF1_ENDCHAR);
    let table = cff1_table(&glyph, &subrs, &[]);

    let parsed = cff::Table::parse(&table).unwrap();
    let mut builder = RecordingBuilder::new();
    assert_eq!(
        parsed.outline(GlyphId(0), &mut builder).unwrap_err(),
        CFFError::SubroutineCallLimitReached
    );
}

// --- CFF2 ---------------------------------------------------------------

#[cfg(feature = "variable-fonts")]
mod cff2_tests {
    use super::*;

    fn cff2_face(data: Vec<u8>) -> ttf_parser::Face<'static> {
        let leaked: &'static [u8] = Box::leak(data.into_boxed_slice());
        ttf_parser::Face::parse(leaked, 0).expect("OTTO/CFF2 sfnt must parse")
    }

    #[test]
    fn cff2_global_subroutine_self_loop_is_bounded() {
        // CFF2 subroutines have no `return`; calling global subr 0 from subr 0
        // nests until STACK_LIMIT.
        let subr = subr_call(0, 1, cff2_op::CALL_GLOBAL_SUBROUTINE);
        let mut glyph = cs_int(100);
        glyph.push(cff2_op::HORIZONTAL_MOVE_TO);
        glyph.extend_from_slice(&subr_call(0, 1, cff2_op::CALL_GLOBAL_SUBROUTINE));
        let table = cff2_table(&glyph, &[subr], &[]);

        let parsed = cff2::Table::parse(&table).unwrap();
        let mut builder = RecordingBuilder::new();
        assert_eq!(
            parsed.outline(&[], GlyphId(0), &mut builder).unwrap_err(),
            CFFError::NestingLimitReached
        );
    }

    #[test]
    fn cff2_local_subroutine_self_loop_is_bounded() {
        let subr = subr_call(0, 1, cff2_op::CALL_LOCAL_SUBROUTINE);
        let mut glyph = cs_int(100);
        glyph.push(cff2_op::HORIZONTAL_MOVE_TO);
        glyph.extend_from_slice(&subr_call(0, 1, cff2_op::CALL_LOCAL_SUBROUTINE));
        let table = cff2_table(&glyph, &[], &[subr]);

        let parsed = cff2::Table::parse(&table).unwrap();
        let mut builder = RecordingBuilder::new();
        assert_eq!(
            parsed.outline(&[], GlyphId(0), &mut builder).unwrap_err(),
            CFFError::NestingLimitReached
        );
    }

    #[test]
    fn cff2_two_subroutine_cycle_is_bounded_and_surfaces_as_none_from_face() {
        let subr0 = subr_call(1, 2, cff2_op::CALL_GLOBAL_SUBROUTINE);
        let subr1 = subr_call(0, 2, cff2_op::CALL_GLOBAL_SUBROUTINE);
        let mut glyph = cs_int(100);
        glyph.push(cff2_op::HORIZONTAL_MOVE_TO);
        glyph.extend_from_slice(&subr_call(0, 2, cff2_op::CALL_GLOBAL_SUBROUTINE));
        let table_bytes = cff2_table(&glyph, &[subr0, subr1], &[]);

        let parsed = cff2::Table::parse(&table_bytes).unwrap();
        let mut builder = RecordingBuilder::new();
        assert_eq!(
            parsed.outline(&[], GlyphId(0), &mut builder).unwrap_err(),
            CFFError::NestingLimitReached
        );

        // Through the high-level API the CFF2 error collapses to `None`, same as
        // every other outline failure, with no panic.
        let font = build_otto(*b"CFF2", table_bytes);
        let face = cff2_face(font);
        assert_eq!(
            face.outline_glyph(GlyphId(0), &mut RecordingBuilder::new()),
            None
        );
        assert_eq!(face.glyph_bounding_box(GlyphId(0)), None);
    }

    #[test]
    fn cff2_valid_short_chain_outlines_and_stays_input_bounded() {
        // Chain of 8 nested global subrs (no fan-out), deepest draws one line.
        // Well within both limits; the glyph outlines and emits exactly one
        // move-to and one line-to, which is bounded by the table byte count.
        const DEPTH: usize = 8;
        let mut subrs: Vec<Vec<u8>> = Vec::new();
        for level in 0..DEPTH {
            let mut body = Vec::new();
            if level + 1 < DEPTH {
                body.extend_from_slice(&subr_call(
                    level as i32 + 1,
                    DEPTH,
                    cff2_op::CALL_GLOBAL_SUBROUTINE,
                ));
            } else {
                body.extend_from_slice(&cs_int(50));
                body.extend_from_slice(&cs_int(50));
                body.push(5); // lineto
            }
            subrs.push(body);
        }
        let mut glyph = cs_int(100);
        glyph.push(cff2_op::HORIZONTAL_MOVE_TO);
        glyph.extend_from_slice(&subr_call(0, DEPTH, cff2_op::CALL_GLOBAL_SUBROUTINE));
        let table_bytes = cff2_table(&glyph, &subrs, &[]);

        let parsed = cff2::Table::parse(&table_bytes).unwrap();
        let mut builder = RecordingBuilder::new();
        let rect = parsed.outline(&[], GlyphId(0), &mut builder).unwrap();
        assert_eq!(
            rect,
            ttf_parser::Rect {
                x_min: 100,
                y_min: 0,
                x_max: 150,
                y_max: 50
            }
        );
        assert_eq!(builder.counts.draw_verbs(), 2);
        // Universal input-derived bound: verbs never exceed table bytes, and with
        // no fan-out total visits are a small linear function of the subr bytes.
        assert!((builder.counts.draw_verbs() as usize) < table_bytes.len());
    }

    #[test]
    fn cff2_fanout_amplification_hits_the_total_call_budget() {
        // fan-out 4, depth 8 => 87_380 invocations (> 4096) at nesting depth <= 8.
        const DEPTH: usize = 8;
        const FANOUT: usize = 4;
        let total_calls: u64 = (1..=DEPTH).map(|l| (FANOUT as u64).pow(l as u32)).sum();
        assert!(total_calls > u64::from(MAX_SUBROUTINE_CALLS));

        let mut subrs: Vec<Vec<u8>> = Vec::new();
        for level in 0..DEPTH {
            let mut body = Vec::new();
            if level + 1 < DEPTH {
                for _ in 0..FANOUT {
                    body.extend_from_slice(&subr_call(
                        level as i32 + 1,
                        DEPTH,
                        cff2_op::CALL_GLOBAL_SUBROUTINE,
                    ));
                }
            }
            subrs.push(body);
        }
        let mut glyph = cs_int(100);
        glyph.push(cff2_op::HORIZONTAL_MOVE_TO);
        for _ in 0..FANOUT {
            glyph.extend_from_slice(&subr_call(0, DEPTH, cff2_op::CALL_GLOBAL_SUBROUTINE));
        }
        let table_bytes = cff2_table(&glyph, &subrs, &[]);

        let parsed = cff2::Table::parse(&table_bytes).unwrap();
        let mut builder = RecordingBuilder::new();
        assert_eq!(
            parsed.outline(&[], GlyphId(0), &mut builder).unwrap_err(),
            CFFError::SubroutineCallLimitReached
        );
        // Only the root move-to can have run before the budget tripped; every
        // callback is bounded by the accepted-call budget times the input size.
        assert!(
            builder.counts.draw_verbs()
                <= 1 + u64::from(MAX_SUBROUTINE_CALLS) * table_bytes.len() as u64
        );
    }
}
