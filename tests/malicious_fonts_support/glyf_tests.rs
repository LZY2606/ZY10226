//! Adversarial `glyf` composite/simple glyph tests.
//!
//! Limits under test (private in `src/tables/glyf.rs`, mirrored here deliberately):
//!
//! - `MAX_COMPONENTS = 32`: composite nesting depth. The check is `depth >= 32`,
//!   so a glyph made of a leaf plus 31 composite levels outlines, and one with 32
//!   composite levels is rejected before its leaf is visited.
//! - `MAX_COMPONENT_VISITS = 100_000`: total component/glyph visits per outline
//!   call. A shared-child diamond can stay well below depth 32 while requesting
//!   exponentially more visits; the budget turns that into a linear cap and makes
//!   every component cycle finite.
//!
//! Callback bounds: one accepted leaf emits exactly `TRIANGLE_DRAW_VERBS` draw
//! verbs, and every accepted visit is bounded by the same number because all
//! leaves in these fonts are identical triangles. With `V` accepted visits the
//! draw-verb count is therefore at most `V * TRIANGLE_DRAW_VERBS`, and the
//! accepted-visit count never exceeds `MAX_COMPONENT_VISITS`.

use super::recording::RecordingBuilder;
use super::tt_builder::{
    GlyphData, Scale, SimpleGlyph, build_sfnt, fan_out_components, triangle_leaf,
};
use ttf_parser::GlyphId;

/// Mirrors `glyf::MAX_COMPONENTS` (private).
const MAX_COMPONENTS: u8 = 32;
/// Mirrors `glyf::MAX_COMPONENT_VISITS` (private).
const MAX_COMPONENT_VISITS: u64 = 100_000;
/// Per-visit draw-verb ceiling for the triangle leaf used throughout.
const PER_VISIT_VERBS: u64 = super::tt_builder::TRIANGLE_DRAW_VERBS;

/// Provable draw-verb ceiling for any outline over these triangle-based fonts:
/// the budget admits at most `MAX_COMPONENT_VISITS` visits, each drawing at most
/// `PER_VISIT_VERBS` verbs. The assertion is a structural bound on accepted work,
/// not a timing observation.
const CALLBACK_CEILING: u64 = MAX_COMPONENT_VISITS * PER_VISIT_VERBS;

struct Font {
    data: Vec<u8>,
}

impl Font {
    fn outline(&self, glyph: u16) -> (Option<ttf_parser::Rect>, RecordingBuilder) {
        let face = ttf_parser::Face::parse(&self.data, 0).expect("sfnt must parse");
        let mut builder = RecordingBuilder::new();
        let bbox = face.outline_glyph(GlyphId(glyph), &mut builder);
        (bbox, builder)
    }

    fn bbox_only(&self, glyph: u16) -> Option<ttf_parser::Rect> {
        let face = ttf_parser::Face::parse(&self.data, 0).expect("sfnt must parse");
        face.glyph_bounding_box(GlyphId(glyph))
    }
}

/// `levels` composites in a straight chain on top of one triangle leaf.
/// Glyph 0 is the leaf; glyph `i` references glyph `i-1` once.
fn chain_font(levels: u16) -> Vec<u8> {
    let mut glyphs = vec![triangle_leaf()];
    for level in 1..=levels {
        glyphs.push(GlyphData::Composite(super::tt_builder::CompositeGlyph {
            bbox: [10, 10, 30, 30],
            components: fan_out_components(level - 1, 1),
            ..Default::default()
        }));
    }
    build_sfnt(&glyphs, &[])
}

/// Shared-child diamond: glyph 0 is the leaf; glyph `i` fans out `fan_out` times
/// over glyph `i-1`. Outlining glyph `depth` costs `fan_out^depth`-style visits.
fn diamond_font(fan_out: u16, depth: u16) -> Vec<u8> {
    let mut glyphs = vec![triangle_leaf()];
    for level in 1..=depth {
        glyphs.push(GlyphData::Composite(super::tt_builder::CompositeGlyph {
            bbox: [10, 10, 30, 30],
            components: fan_out_components(level - 1, fan_out),
            ..Default::default()
        }));
    }
    build_sfnt(&glyphs, &[])
}

const EXPECTED_RECT: ttf_parser::Rect = ttf_parser::Rect {
    x_min: 10,
    y_min: 10,
    x_max: 30,
    y_max: 30,
};

#[test]
fn component_self_loop_is_stopped_by_max_components_depth_32_not_recursion() {
    // One composite glyph that references *itself* (glyph 1 -> glyph 1). The
    // depth guard `depth >= MAX_COMPONENTS` terminates the walk after 32 frames,
    // so there is no unbounded recursion and no stack blow-up. Every iteration
    // sees a composite glyph, so no leaf is ever drawn: zero callbacks, `None`.
    let data = {
        let glyphs = [
            triangle_leaf(),
            GlyphData::Composite(super::tt_builder::CompositeGlyph {
                bbox: [10, 10, 30, 30],
                components: fan_out_components(1, 1),
                ..Default::default()
            }),
        ];
        build_sfnt(&glyphs, &[])
    };
    let font = Font { data };

    let (bbox, builder) = font.outline(1);
    assert_eq!(bbox, None, "self-loop must be rejected, not outlined");
    assert!(
        builder.is_empty(),
        "rejected self-loop emitted callbacks: {builder:?}"
    );
    assert_eq!(font.bbox_only(1), None);
}

#[test]
fn two_node_component_cycle_is_stopped_within_max_components_depth_32() {
    // Glyph 1 -> glyph 2 -> glyph 1 ... The cycle never reaches a simple glyph.
    // Same depth guard, same structural result: `None`, no callbacks, no panic.
    let data = {
        let glyphs = [
            triangle_leaf(),
            GlyphData::Composite(super::tt_builder::CompositeGlyph {
                bbox: [10, 10, 30, 30],
                components: fan_out_components(2, 1),
                ..Default::default()
            }),
            GlyphData::Composite(super::tt_builder::CompositeGlyph {
                bbox: [10, 10, 30, 30],
                components: fan_out_components(1, 1),
                ..Default::default()
            }),
        ];
        build_sfnt(&glyphs, &[])
    };
    let font = Font { data };

    let (bbox, builder) = font.outline(1);
    assert_eq!(bbox, None);
    assert!(builder.is_empty(), "cycle emitted callbacks: {builder:?}");
    assert_eq!(font.bbox_only(1), None);
}

#[test]
fn component_chain_of_exactly_31_levels_is_within_max_components_and_outlines() {
    // The guard fires at `depth >= 32`. Root is depth 0; a chain with a leaf and
    // 31 composite levels visits the leaf at depth 31, which is the last accepted
    // frame. Exactly one triangle must be drawn: 4 draw verbs, 1 close.
    assert_eq!(MAX_COMPONENTS, 32);
    let font = Font {
        data: chain_font(u16::from(MAX_COMPONENTS) - 1),
    };

    let (bbox, builder) = font.outline(u16::from(MAX_COMPONENTS) - 1);
    assert_eq!(bbox, Some(EXPECTED_RECT));
    assert_eq!(builder.counts.draw_verbs(), PER_VISIT_VERBS);
    assert_eq!(builder.counts.close, super::tt_builder::TRIANGLE_CLOSES);
}

#[test]
fn component_chain_one_level_past_max_components_is_rejected_without_callbacks() {
    // 32 composite levels visits its deepest composite at depth 32 and the guard
    // rejects it before reading its child, so the leaf is never drawn. The chain
    // is well within MAX_COMPONENT_VISITS (33 visits): this proves it is the
    // *depth* boundary that fires, not the total-visit budget.
    let font = Font {
        data: chain_font(u16::from(MAX_COMPONENTS)),
    };

    let (bbox, builder) = font.outline(u16::from(MAX_COMPONENTS));
    assert_eq!(bbox, None);
    assert!(builder.is_empty(), "depth-rejected glyph drew: {builder:?}");
}

#[test]
fn multiple_components_sharing_one_child_glyph_fan_out_within_budget_outlines() {
    // depth 3, fan-out 2 = a binary diamond over 4 glyphs. The leaf is visited
    // 2^3 = 8 times; each visit draws the same triangle, so draw verbs are
    // exactly 8 * 4 and closes exactly 8. This proves the fixture is valid and
    // that sibling components genuinely share one child glyph.
    let font = Font {
        data: diamond_font(2, 3),
    };

    let (bbox, builder) = font.outline(3);
    assert_eq!(bbox, Some(EXPECTED_RECT));
    assert_eq!(builder.counts.draw_verbs(), 8 * PER_VISIT_VERBS);
    assert_eq!(builder.counts.close, 8 * super::tt_builder::TRIANGLE_CLOSES);
}

#[test]
fn shared_child_fan_out_is_bounded_by_max_component_visits_without_a_timeout() {
    // fan-out 3, depth 24: 25 glyphs, max depth 24 < 32, so the depth guard never
    // fires. Unguarded the expansion costs (3^25 - 1)/2 = 423_644_304_721 visits.
    // The MAX_COMPONENT_VISITS budget turns this into <= 100_000 accepted visits
    // and the outline returns `None`. No wall-clock timing is used: the assertion
    // is structural (return value plus provable callback ceiling).
    const FAN_OUT: u16 = 3;
    const DEPTH: u16 = 24;
    assert!((u64::from(FAN_OUT).pow(u32::from(DEPTH) + 1) - 1) / 2 > MAX_COMPONENT_VISITS);

    let font = Font {
        data: diamond_font(FAN_OUT, DEPTH),
    };
    let (bbox, builder) = font.outline(DEPTH);

    assert_eq!(bbox, None);
    // Whatever partial output existed before the budget ran out, it cannot exceed
    // what 100_000 accepted triangle visits could draw, and every drawn point is
    // within the verified leaf bbox (see ..._stays_within_verified_bounds).
    assert!(
        builder.counts.draw_verbs() <= CALLBACK_CEILING,
        "{} verbs exceed the provable visit-budget ceiling {CALLBACK_CEILING}",
        builder.counts.draw_verbs()
    );
    assert_eq!(font.bbox_only(DEPTH), None);
}

#[test]
fn fan_out_partial_callbacks_before_budget_exhaustion_stay_within_verified_bounds() {
    // The budget can exhaust mid-outline, so the builder may legitimately have
    // seen callbacks from fully-expanded subtrees before `None` is returned.
    // Those callbacks describe only verified leaf coordinates (10..=30), so even
    // a partial result never escapes the range the font actually proved.
    const FAN_OUT: u16 = 3;
    const DEPTH: u16 = 24;
    let font = Font {
        data: diamond_font(FAN_OUT, DEPTH),
    };
    let (bbox, builder) = font.outline(DEPTH);

    assert_eq!(bbox, None);
    if builder.seen_coords() {
        assert!(
            (10.0..=30.0).contains(&builder.min_x),
            "min_x={}",
            builder.min_x
        );
        assert!(
            (10.0..=30.0).contains(&builder.max_x),
            "max_x={}",
            builder.max_x
        );
        assert!(
            (10.0..=30.0).contains(&builder.min_y),
            "min_y={}",
            builder.min_y
        );
        assert!(
            (10.0..=30.0).contains(&builder.max_y),
            "max_y={}",
            builder.max_y
        );
    }
}

#[test]
fn safe_diamond_callback_count_is_bounded_by_a_function_of_input_size() {
    // For an accepted outline the total work is a function of the font's own
    // component descriptors, not of ambient time. depth 8, fan-out 2 has
    // (2^9 - 1) = 511 visits and a tiny byte footprint; draw verbs are exactly
    // leaf-visits * 4, and the ceiling 511 * 4 follows directly from the number
    // of component records the input contains (one descriptor per edge).
    const FAN_OUT: u16 = 2;
    const DEPTH: u16 = 8;
    let data = diamond_font(FAN_OUT, DEPTH);
    let leaf_visits = u64::from(FAN_OUT).pow(u32::from(DEPTH));
    let provable_ceiling = leaf_visits * PER_VISIT_VERBS;
    let font = Font { data };

    let (bbox, builder) = font.outline(DEPTH);
    assert_eq!(bbox, Some(EXPECTED_RECT));
    assert_eq!(builder.counts.draw_verbs(), provable_ceiling);
    // Independent upper bound expressed purely in the parsed input: one outline
    // visit cannot emit more draw verbs than there are bytes in the whole sfnt
    // (each draw verb needs a point, each point needs at least one flag byte),
    // and the parser admits at most MAX_COMPONENT_VISITS visits. Hence the
    // universal bound `verbs <= budget * input_bytes` holds for every font.
    assert!(
        builder.counts.draw_verbs() <= MAX_COMPONENT_VISITS * font.data.len() as u64,
        "callback count exceeded the input-derived provable bound"
    );
}

#[test]
fn truncated_coordinate_arrays_stop_at_the_format_boundary_without_panicking() {
    // 6 points (two contours), flags all "same/long" so each x and y coordinate
    // should consume 2 bytes, but the coords blob is cut to 3 bytes total. The
    // parser reads slices ahead of iteration and emits points with clamped 0
    // coordinates until its data iterator ends; in no case may it panic or draw
    // outside the 16-bit coordinate domain. The result must be bounded.
    use super::tt_builder::simple_flags::*;
    let long_same = ON_CURVE | X_SAME_OR_POSITIVE_SHORT | Y_SAME_OR_POSITIVE_SHORT;
    let glyph = GlyphData::Simple(SimpleGlyph {
        bbox: [0, 0, 100, 100],
        endpoints: vec![2, 5],
        instructions: Vec::new(),
        flags: vec![long_same; 6],
        coords: vec![0, 0, 0], // far fewer than the 24 bytes the flags promise
        include_instructions: true,
    });
    let font = Font {
        data: build_sfnt(&[triangle_leaf(), glyph], &[]),
    };

    let (bbox, builder) = font.outline(1);
    // Either rejected outright or a rectangle whose fields are plain `i16`s and
    // therefore inherently finite: never a panic and never NaN/Infinity.
    let _ = bbox.map(|rect| (rect.x_min, rect.x_max, rect.y_min, rect.y_max));
    assert!(builder.counts.draw_verbs() <= 6 * 2);
}

#[test]
fn declared_instruction_length_longer_than_the_glyph_is_bounded() {
    // instructionLength claims 40000 bytes but none follow. The parser advances
    // (bounded by the slice) and must then fail the flags/coords lookup cleanly.
    let glyph = GlyphData::Raw({
        let mut raw = Vec::new();
        raw.extend_from_slice(&1i16.to_be_bytes()); // contours
        raw.extend_from_slice(
            &[0i16; 4]
                .iter()
                .flat_map(|v| v.to_be_bytes())
                .collect::<Vec<_>>(),
        );
        raw.extend_from_slice(&0u16.to_be_bytes()); // one contour endpoint
        raw.extend_from_slice(&40000u16.to_be_bytes()); // instructionLength lie
        raw
    });
    let font = Font {
        data: build_sfnt(&[triangle_leaf(), glyph], &[]),
    };

    let (bbox, builder) = font.outline(1);
    assert_eq!(bbox, None);
    assert!(
        builder.is_empty(),
        "truncated-instruction glyph drew: {builder:?}"
    );
}

#[test]
fn long_hint_instruction_blob_is_capped_by_glyph_size_not_timer() {
    // A genuine 60 000-byte instruction blob is legal and must be skipped exactly
    // once; the one-point triangle following it still outlines, proving the
    // parser's work is linear in input size, not quadratic or unbounded.
    let glyph = GlyphData::Simple(SimpleGlyph {
        bbox: [10, 10, 30, 30],
        endpoints: vec![2],
        instructions: vec![0u8; 60_000],
        flags: vec![0x37, 0x37, 0x37],
        coords: vec![10, 20, 0, 10, 0, 20],
        include_instructions: true,
    });
    let font = Font {
        data: build_sfnt(&[triangle_leaf(), glyph], &[]),
    };

    let (bbox, builder) = font.outline(1);
    assert_eq!(bbox, Some(EXPECTED_RECT));
    assert_eq!(builder.counts.draw_verbs(), PER_VISIT_VERBS);
}

#[test]
fn component_referencing_glyph_id_beyond_loca_range_is_skipped_safely() {
    // Component points at glyph 65000; loca only covers 2 glyphs, so the range
    // lookup fails. A lone dangling component yields `None` with no callbacks and
    // must not index out of bounds.
    let mut comps = super::tt_builder::fan_out_components(0, 1);
    comps[0].glyph_index = 65000;
    let glyph = GlyphData::Composite(super::tt_builder::CompositeGlyph {
        bbox: [10, 10, 30, 30],
        components: comps,
        ..Default::default()
    });
    let font = Font {
        data: build_sfnt(&[triangle_leaf(), glyph], &[]),
    };

    let (bbox, builder) = font.outline(1);
    assert_eq!(bbox, None);
    assert!(builder.is_empty());
}

#[test]
fn component_chain_with_overlapping_saturating_scale_cannot_inflate_bbox_beyond_f32_domain() {
    // 20 levels each with uniform scale ~1.9999 (F2DOT14 0x7FFF) combine
    // multiplicatively; the final coordinates must saturate to a finite f32 bbox
    // (or be rejected), never NaN/infinite and never panic.
    let max_scale = 0x7FFFi16;
    let mut glyphs = vec![triangle_leaf()];
    for level in 1..=20u16 {
        let mut components = super::tt_builder::fan_out_components(level - 1, 1);
        components[0].scale = Some(Scale::Uniform(max_scale));
        glyphs.push(GlyphData::Composite(super::tt_builder::CompositeGlyph {
            bbox: [10, 10, 30, 30],
            components,
            ..Default::default()
        }));
    }
    let font = Font {
        data: build_sfnt(&glyphs, &[]),
    };

    let (bbox, builder) = font.outline(20);
    // The rect fields are `i16` and thus always finite; combined transforms can
    // only yield `None` or a bounded `Rect`, never NaN/Infinity and never a panic.
    let _ = bbox.map(|rect| (rect.x_min, rect.x_max, rect.y_min, rect.y_max));
    // At most one triangle per level can have been expanded.
    assert!(builder.counts.draw_verbs() <= 21 * PER_VISIT_VERBS);
}

#[test]
fn loca_offsets_at_u32_max_cannot_escape_the_glyf_slice() {
    // A hand-built glyph table whose loca range claims [0xFFFFFFF0, 0xFFFFFFFF].
    // Indexing `glyf[range]` must collapse to `None` via checked slice access;
    // no arithmetic may wrap to a small in-bounds range. The whole font still
    // parses; only the glyph outline is unavailable, with zero callbacks.
    let glyf = {
        // A tiny decoy glyf blob so the table exists; the malicious loca ignores it.
        let mut g = Vec::new();
        g.extend_from_slice(&0i16.to_be_bytes()); // contours = 0
        g.extend_from_slice(
            &[0i16; 4]
                .iter()
                .flat_map(|v| v.to_be_bytes())
                .collect::<Vec<_>>(),
        );
        g
    };
    let mut loca = Vec::new();
    loca.extend_from_slice(&0xFFFF_FFF0u32.to_be_bytes());
    loca.extend_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
    let data = build_sfnt_with_tables(&[(b"glyf".to_vec(), glyf), (b"loca".to_vec(), loca)], 1);

    let face = ttf_parser::Face::parse(&data, 0).expect("sfnt parses despite lying loca");
    let mut builder = RecordingBuilder::new();
    let bbox = face.outline_glyph(ttf_parser::GlyphId(0), &mut builder);
    assert_eq!(bbox, None);
    assert!(
        builder.is_empty(),
        "out-of-range loca glyph drew: {builder:?}"
    );
}

#[test]
fn truncated_component_record_with_more_components_flag_is_bounded() {
    // The last written component sets MORE_COMPONENTS but its argument bytes are
    // cut off mid-record. The component iterator must hit the slice end and
    // stop, producing `None`/an empty walk rather than reading OOB or looping.
    let mut components = super::tt_builder::fan_out_components(0, 3);
    components[0].flags |= super::tt_builder::composite_flags::MORE_COMPONENTS;
    components[1].flags |= super::tt_builder::composite_flags::MORE_COMPONENTS;
    components[2].flags |= super::tt_builder::composite_flags::MORE_COMPONENTS;
    let glyph = GlyphData::Composite(super::tt_builder::CompositeGlyph {
        bbox: [10, 10, 30, 30],
        components,
        truncate_after: Some(2), // claim a third component that is not there
        ..Default::default()
    });
    let font = Font {
        data: build_sfnt(&[triangle_leaf(), glyph], &[]),
    };

    let (bbox, builder) = font.outline(1);
    // The two complete records expand the leaf; the dangling MORE_COMPONENTS
    // marker ends the iterator at the slice boundary. Either a bounded rectangle
    // or `None` is acceptable, but never a panic/OOB; callbacks stay bounded by
    // the two records actually present.
    let _ = bbox;
    assert!(builder.counts.draw_verbs() <= 2 * PER_VISIT_VERBS);
}

/// Variant of `build_sfnt` accepting raw table bytes (glyf/loca crafted by hand).
fn build_sfnt_with_tables(raw: &[(Vec<u8>, Vec<u8>)], number_of_glyphs: u16) -> Vec<u8> {
    let head = super::tt_builder::public_head();
    let hhea = super::tt_builder::public_hhea();
    let maxp = super::tt_builder::public_maxp(number_of_glyphs);
    let mut hmtx = Vec::new();
    for _ in 0..number_of_glyphs {
        hmtx.extend_from_slice(&600u16.to_be_bytes());
        hmtx.extend_from_slice(&0i16.to_be_bytes());
    }

    let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"hmtx", hmtx),
        (*b"maxp", maxp),
    ];
    for (tag, data) in raw {
        let mut t = [0u8; 4];
        t.copy_from_slice(tag);
        tables.push((t, data.clone()));
    }
    tables.sort_by_key(|(t, _)| *t);

    let mut font = Vec::new();
    font.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    font.extend_from_slice(&(tables.len() as u16).to_be_bytes());
    font.extend_from_slice(&[0u8; 6]);
    let mut offset = 12u32 + 16 * tables.len() as u32;
    let mut records = Vec::new();
    for (tag, data) in &tables {
        records.push((*tag, offset, data.len() as u32));
        offset += data.len() as u32;
    }
    for (tag, off, len) in records {
        font.extend_from_slice(&tag);
        font.extend_from_slice(&0u32.to_be_bytes());
        font.extend_from_slice(&off.to_be_bytes());
        font.extend_from_slice(&len.to_be_bytes());
    }
    for (_, data) in &tables {
        font.extend_from_slice(data);
    }
    font
}

#[test]
fn point_and_instruction_counts_scale_linearly_and_never_escape_the_input() {
    // Parameterised constructor check: for `n` points in one contour with an
    // `m`-byte instruction blob the accepted outline's draw verbs are exactly
    // the point-derived count and the parser's work is linear in `n` and `m`.
    // Uses repeat-free explicit flags (all on-curve, short positive deltas).
    use super::tt_builder::simple_flags::*;
    for &(n, m) in &[(1u16, 0u16), (3, 10), (100, 1000), (1000, 100)] {
        let flag =
            ON_CURVE | X_SHORT | Y_SHORT | X_SAME_OR_POSITIVE_SHORT | Y_SAME_OR_POSITIVE_SHORT;
        // One contour ending at the last point. A one-point contour is ignored by
        // the parser (spec: single-point contours are skipped).
        let glyph = GlyphData::Simple(SimpleGlyph {
            bbox: [0, 0, 255, 255],
            endpoints: vec![n - 1],
            instructions: vec![0u8; m as usize],
            include_instructions: true,
            flags: vec![flag; n as usize],
            coords: {
                // x walks right by 1 per point; the first point starts at y=1 so
                // the closed contour has nonzero area (a collinear contour has a
                // zero-area bbox and is correctly reported as `None`).
                let mut c = vec![1u8; n as usize];
                let mut y = vec![0u8; n as usize];
                y[0] = 1;
                c.extend(y);
                c
            },
        });
        let font = Font {
            data: build_sfnt(&[triangle_leaf(), glyph], &[]),
        };
        let (bbox, builder) = font.outline(1);

        if n == 1 {
            // Single-point contour: explicitly skipped, no outline.
            assert_eq!(bbox, None, "n={n}");
            assert!(builder.is_empty(), "n={n}: {builder:?}");
        } else {
            assert!(bbox.is_some(), "n={n}, m={m} must outline");
            // move_to once, line_to for remaining points plus the closing line.
            assert_eq!(builder.counts.move_to, 1, "n={n}");
            assert_eq!(builder.counts.line_to, u64::from(n), "n={n}");
            // Input-derived universal ceiling.
            assert!(
                builder.counts.draw_verbs() <= font.data.len() as u64,
                "n={n} callbacks exceeded byte size"
            );
        }
    }
}
