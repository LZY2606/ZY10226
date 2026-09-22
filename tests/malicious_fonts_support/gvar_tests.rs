//! Adversarial `gvar` tuple-count and composite tests.
//!
//! Only compiled with `variable-fonts`.
//!
//! Limits under test (private in `src/tables/gvar.rs`):
//!
//! - `MAX_STACK_TUPLES_LEN = 32`: without `gvar-alloc` a glyph variation blob
//!   claiming more than 32 tuples cannot be buffered and outlining bails with
//!   `None`; the count is an attacker-controlled u12 (up to 4095 by spec), so it
//!   must not drive an allocation or unbounded parse loop.
//! - the same `glyf::MAX_COMPONENTS` depth and `MAX_COMPONENT_VISITS` budgets
//!   apply when `gvar` drives a composite outline.

use super::recording::RecordingBuilder;
use super::tt_builder::{CompositeGlyph, GlyphData, build_sfnt, fan_out_components, triangle_leaf};
use super::var_builder::{fvar_one_axis, glyph_variation_data, gvar_table};
use ttf_parser::GlyphId;

/// Mirrors `gvar::MAX_STACK_TUPLES_LEN` (private).
#[allow(dead_code)]
const MAX_STACK_TUPLES_LEN: u16 = 32;

const EXPECTED_RECT: ttf_parser::Rect = ttf_parser::Rect {
    x_min: 10,
    y_min: 10,
    x_max: 30,
    y_max: 30,
};

fn variable_font(glyphs: &[GlyphData], glyph_blobs: &[Vec<u8>]) -> Vec<u8> {
    let extras = [
        (*b"fvar", fvar_one_axis()),
        (*b"gvar", gvar_table(glyph_blobs)),
    ];
    build_sfnt(glyphs, &extras)
}

fn outline_with_weight(data: &[u8], glyph: u16) -> (Option<ttf_parser::Rect>, RecordingBuilder) {
    let mut face = ttf_parser::Face::parse(data, 0).expect("variable sfnt must parse");
    assert!(face.is_variable());
    face.set_variation(ttf_parser::Tag::from_bytes(b"wght"), 900.0)
        .expect("wght axis must be present");
    let mut builder = RecordingBuilder::new();
    let bbox = face.outline_glyph(GlyphId(glyph), &mut builder);
    (bbox, builder)
}

#[test]
fn gvar_tuple_count_over_32_never_drives_unbounded_work_and_emits_no_callbacks() {
    // 33 declared tuples crosses the 32-slot stack boundary. Without `gvar-alloc`
    // `VariationTuples::reserve` refuses the glyph up front; with `gvar-alloc` the
    // headers spill to the heap, but this blob carries none, so the per-tuple
    // parse fails the same way. Either way the conclusion is structural: `None`,
    // zero callbacks, and the u12 count never multiplies into parse work that
    // depends on its value. (The feature-dependent reason is documented in
    // `gvar::VariationTuples::reserve`; only the bounded outcome is asserted.)
    let blob = glyph_variation_data(MAX_STACK_TUPLES_LEN + 1, &[], 4);
    let data = variable_font(&[triangle_leaf()], &[blob]);

    let (bbox, builder) = outline_with_weight(&data, 0);
    assert_eq!(bbox, None);
    assert!(builder.is_empty(), "tuple-overflow glyph drew: {builder:?}");
}

#[cfg(not(feature = "gvar-alloc"))]
#[test]
fn gvar_tuple_count_over_32_is_rejected_at_the_stack_reserve_boundary() {
    // Records the exact non-alloc failure path: the count alone, before any
    // tuple header is read, makes `reserve` return false. Only compiled when the
    // 32-slot stack buffer is the only available storage.
    let blob = glyph_variation_data(MAX_STACK_TUPLES_LEN + 1, &[], 4);
    let data = variable_font(&[triangle_leaf()], &[blob]);
    let (bbox, builder) = outline_with_weight(&data, 0);
    assert_eq!(bbox, None);
    assert!(builder.is_empty());
}

#[test]
fn gvar_tuple_count_at_the_u12_spec_limit_is_bounded() {
    // 4095 is the spec maximum tuple count. A blob this large cannot be valid in a
    // tiny font, but the count must never multiply into allocation/loop work that
    // depends on the value itself.
    let blob = glyph_variation_data(4095, &[0u8; 8], 4);
    let data = variable_font(&[triangle_leaf()], &[blob]);

    let (bbox, builder) = outline_with_weight(&data, 0);
    assert_eq!(bbox, None);
    assert!(builder.is_empty());
}

#[test]
fn gvar_tuple_count_zero_is_a_format_boundary_and_emits_no_callbacks() {
    // The low-12-bit tuple count is required to be 1..=4095; the parser rejects a
    // zero count with `None` before applying any deltas, so this boundary value
    // never produces partial output. (Non-variable outlining still works: `gvar`
    // is only consulted at non-default coordinates.)
    let blob = glyph_variation_data(0, &[], 4);
    let data = variable_font(&[triangle_leaf()], &[blob]);

    let (bbox, builder) = outline_with_weight(&data, 0);
    assert_eq!(bbox, None);
    assert!(builder.is_empty());
}

#[test]
fn gvar_data_offset_past_the_blob_is_rejected_without_partial_callbacks() {
    // `data_offset` points beyond the variation blob. The serialized stream must
    // fail to construct rather than walk arbitrary font bytes; rejected => `None`.
    let blob = glyph_variation_data(1, &[], 60_000);
    let data = variable_font(&[triangle_leaf()], &[blob]);

    let (bbox, builder) = outline_with_weight(&data, 0);
    assert_eq!(bbox, None);
    assert!(builder.is_empty());
}

#[test]
fn gvar_composite_self_loop_is_bounded_like_plain_glyf() {
    // Under `gvar`, a composite glyph 1 -> glyph 1 must still terminate on the
    // depth budget with no callbacks and no recursion blow-up. Each composite gets
    // an empty-but-well-formed variation blob (zero tuples is tolerated for
    // composite component adjustments; zero components would terminate earlier).
    let leaf = triangle_leaf();
    let self_comp = GlyphData::Composite(CompositeGlyph {
        bbox: [10, 10, 30, 30],
        components: fan_out_components(1, 1),
        ..Default::default()
    });
    // Empty variation data (start == end) is ignored by parse_variation_data.
    let data = variable_font(&[leaf, self_comp], &[Vec::new(), Vec::new()]);

    let (bbox, builder) = outline_with_weight(&data, 1);
    assert_eq!(bbox, None);
    assert!(builder.is_empty(), "gvar self-loop drew: {builder:?}");
}

#[test]
fn gvar_fan_out_composite_sharing_a_child_stays_within_visit_budget() {
    // The variable outline path uses the same MAX_COMPONENT_VISITS budget. A
    // fan-out 3 / depth 20 diamond (well below depth 32) must return `None` with a
    // callback count bounded by budget * input bytes.
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
    let blobs: Vec<Vec<u8>> = (0..glyphs.len()).map(|_| Vec::new()).collect();
    let data = variable_font(&glyphs, &blobs);
    let bound = 100_000u64 * data.len() as u64;

    let (bbox, builder) = outline_with_weight(&data, DEPTH);
    assert_eq!(bbox, None);
    assert!(builder.counts.draw_verbs() <= bound);
}

#[test]
fn set_variation_on_unknown_axis_returns_none_and_does_not_change_outline() {
    // `Face::set_variation` must return `None` for an absent axis (documented
    // behaviour) and the face must still outline identically (no coordinates set).
    // No glyph variation blob at all: with default coordinates the plain `glyf`
    // outline is used unchanged.
    let data = variable_font(&[triangle_leaf()], &[Vec::new()]);
    let mut face = ttf_parser::Face::parse(&data, 0).unwrap();

    assert_eq!(
        face.set_variation(ttf_parser::Tag::from_bytes(b"wdth"), 100.0),
        None
    );

    let mut builder = RecordingBuilder::new();
    let bbox = face.outline_glyph(GlyphId(0), &mut builder);
    assert_eq!(bbox, Some(EXPECTED_RECT));
    assert!(builder.counts.draw_verbs() >= 1);
}
