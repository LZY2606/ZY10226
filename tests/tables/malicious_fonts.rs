//! Structured adversarial outline fixtures.
//!
//! These tests construct every font in-process. The named limits are:
//! - `glyf::MAX_COMPONENTS = 32`: one root plus at most 31 nested component entries.
//! - `glyf::MAX_COMPONENT_VISITS = 100_000`: total composite-graph node visits.
//! - CFF `STACK_LIMIT = 10`: ten nested calls are accepted and the eleventh is rejected.
//! - CFF `MAX_SUBROUTINE_CALLS = 4_096`: total local/global subroutine invocations.
//! - `gvar::MAX_STACK_TUPLES_LEN = 32`: tuple capacity without the optional `gvar-alloc`.

#![allow(clippy::too_many_lines)]

use std::fs;
use std::path::Path;

use ttf_parser::{cff, CFFError, Face, GlyphId, OutlineBuilder, Rect};

#[cfg(feature = "variable-fonts")]
use ttf_parser::Tag;

const GLYF_MAX_COMPONENTS: usize = 32;
const GLYF_MAX_COMPONENT_VISITS: u32 = 100_000;
const CFF_STACK_LIMIT: usize = 10;
const CFF_MAX_SUBROUTINE_CALLS: u32 = 4_096;
const GVAR_STACK_TUPLE_LIMIT: u16 = 32;

#[derive(Default, Debug, PartialEq, Eq)]
struct CallbackCounts {
    move_to: usize,
    line_to: usize,
    quad_to: usize,
    curve_to: usize,
    close: usize,
}

impl CallbackCounts {
    fn total(&self) -> usize {
        self.move_to + self.line_to + self.quad_to + self.curve_to + self.close
    }
}

#[derive(Default)]
struct CountingBuilder {
    counts: CallbackCounts,
}

impl OutlineBuilder for CountingBuilder {
    fn move_to(&mut self, _: f32, _: f32) {
        self.counts.move_to += 1;
    }

    fn line_to(&mut self, _: f32, _: f32) {
        self.counts.line_to += 1;
    }

    fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {
        self.counts.quad_to += 1;
    }

    fn curve_to(
        &mut self,
        _: f32,
        _: f32,
        _: f32,
        _: f32,
        _: f32,
        _: f32,
    ) {
        self.counts.curve_to += 1;
    }

    fn close(&mut self) {
        self.counts.close += 1;
    }
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn push_i16(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

struct SfntBuilder {
    version: [u8; 4],
    tables: Vec<([u8; 4], Vec<u8>)>,
    invalid_glyf_offset: bool,
}

impl SfntBuilder {
    fn truetype() -> Self {
        SfntBuilder {
            version: [0x00, 0x01, 0x00, 0x00],
            tables: Vec::new(),
            invalid_glyf_offset: false,
        }
    }

    fn cff() -> Self {
        SfntBuilder {
            version: *b"OTTO",
            tables: Vec::new(),
            invalid_glyf_offset: false,
        }
    }

    fn add(&mut self, tag: [u8; 4], data: Vec<u8>) {
        self.tables.push((tag, data));
    }

    fn build(mut self) -> Vec<u8> {
        self.tables.sort_by_key(|(tag, _)| *tag);

        let count = u16::try_from(self.tables.len()).unwrap();
        let entry_selector = usize::BITS - 1 - usize::from(count).leading_zeros();
        let search_range = u16::try_from(2usize.pow(entry_selector) * 16).unwrap();
        let entry_selector = u16::try_from(entry_selector).unwrap();
        let range_shift = count * 16 - search_range;

        let mut font = Vec::new();
        font.extend_from_slice(&self.version);
        push_u16(&mut font, count);
        push_u16(&mut font, search_range);
        push_u16(&mut font, entry_selector);
        push_u16(&mut font, range_shift);

        let mut table_offset = u32::try_from(font.len() + usize::from(count) * 16).unwrap();
        for (tag, data) in &self.tables {
            font.extend_from_slice(tag);
            push_u32(&mut font, 0);
            let record_offset = if *tag == *b"glyf" && self.invalid_glyf_offset {
                u32::MAX
            } else {
                table_offset
            };
            push_u32(&mut font, record_offset);
            push_u32(&mut font, u32::try_from(data.len()).unwrap());
            table_offset = table_offset
                .checked_add(u32::try_from(data.len()).unwrap())
                .unwrap();
            table_offset = (table_offset + 3) & !3;
        }

        let mut payload_offset = u32::try_from(font.len()).unwrap();
        for (_, data) in &self.tables {
            while payload_offset % 4 != 0 {
                font.push(0);
                payload_offset += 1;
            }
            assert_eq!(usize::try_from(payload_offset).unwrap(), font.len());
            font.extend_from_slice(data);
            payload_offset = payload_offset
                .checked_add(u32::try_from(data.len()).unwrap())
                .unwrap();
            while payload_offset % 4 != 0 {
                font.push(0);
                payload_offset += 1;
            }
        }

        font
    }
}

fn required_tables(number_of_glyphs: u16) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut head = Vec::with_capacity(54);
    push_u32(&mut head, 0x0001_0000);
    push_u32(&mut head, 0);
    push_u32(&mut head, 0);
    push_u32(&mut head, 0x5F0F_3CF5);
    push_u16(&mut head, 0);
    push_u16(&mut head, 1000);
    head.extend_from_slice(&0u64.to_be_bytes());
    head.extend_from_slice(&0u64.to_be_bytes());
    push_i16(&mut head, 0);
    push_i16(&mut head, 0);
    push_i16(&mut head, 1000);
    push_i16(&mut head, 1000);
    push_u16(&mut head, 0);
    push_u16(&mut head, 8);
    push_i16(&mut head, 2);
    push_i16(&mut head, 1);
    push_i16(&mut head, 0);
    assert_eq!(head.len(), 54);

    let mut hhea = vec![0u8; 36];
    hhea[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    hhea[34..36].copy_from_slice(&1u16.to_be_bytes());

    let mut maxp = Vec::with_capacity(6);
    push_u32(&mut maxp, 0x0000_5000);
    push_u16(&mut maxp, number_of_glyphs);

    (head, hhea, maxp)
}

fn add_required_tables(font: &mut SfntBuilder, number_of_glyphs: u16) {
    let (head, hhea, maxp) = required_tables(number_of_glyphs);
    font.add(*b"head", head);
    font.add(*b"hhea", hhea);
    font.add(*b"maxp", maxp);
}

fn glyphs_to_tables(glyphs: &[Vec<u8>]) -> (Vec<u8>, Vec<u8>) {
    let mut glyf = Vec::new();
    let mut loca = Vec::new();

    for glyph in glyphs {
        push_u32(&mut loca, u32::try_from(glyf.len()).unwrap());
        glyf.extend_from_slice(glyph);
    }
    push_u32(&mut loca, u32::try_from(glyf.len()).unwrap());

    (glyf, loca)
}

fn truetype_font(glyphs: &[Vec<u8>]) -> Vec<u8> {
    let mut font = SfntBuilder::truetype();
    let (glyf, loca) = glyphs_to_tables(glyphs);
    add_required_tables(&mut font, u16::try_from(glyphs.len()).unwrap());
    font.add(*b"glyf", glyf);
    font.add(*b"loca", loca);
    font.build()
}

#[test]
fn sfnt_glyf_record_offset_overflow_is_rejected_before_outline_callbacks() {
    let mut font = SfntBuilder::truetype();
    let (glyf, loca) = glyphs_to_tables(&[simple_glyph(SimpleGlyphOptions::new(3))]);
    add_required_tables(&mut font, 1);
    font.invalid_glyf_offset = true;
    font.add(*b"glyf", glyf);
    font.add(*b"loca", loca);
    let data = font.build();

    let result = outline(&data, 0);
    assert_eq!(result.bbox, None);
    assert_eq!(result.counts, CallbackCounts::default());
}


#[derive(Clone, Copy)]
struct SimpleGlyphOptions {
    points: u16,
    instructions: u16,
    truncate_coordinates_by: usize,
}

impl SimpleGlyphOptions {
    fn new(points: u16) -> Self {
        SimpleGlyphOptions {
            points,
            instructions: 0,
            truncate_coordinates_by: 0,
        }
    }
}

fn simple_glyph(options: SimpleGlyphOptions) -> Vec<u8> {
    assert!(options.points >= 3);

    let mut glyph = Vec::new();
    push_i16(&mut glyph, 1);
    push_i16(&mut glyph, 0);
    push_i16(&mut glyph, 0);
    push_i16(&mut glyph, 30);
    push_i16(&mut glyph, 30);
    push_u16(&mut glyph, options.points - 1);
    push_u16(&mut glyph, options.instructions);

    for _ in 0..options.points {
        glyph.push(0x37);
    }

    glyph.resize(glyph.len() + usize::from(options.instructions), 0);

    for point in 0..options.points {
        glyph.push(match point % 3 {
            0 => 0x0a,
            1 => 0x14,
            _ => 0x00,
        });
    }
    for point in 0..options.points {
        glyph.push(match point % 3 {
            0 => 0x0a,
            1 => 0x00,
            _ => 0x14,
        });
    }

    glyph.truncate(glyph.len() - options.truncate_coordinates_by);
    glyph
}


#[derive(Clone, Copy)]
struct Component {
    glyph_id: u16,
    dx: i16,
    dy: i16,
    scale: Option<f32>,
}

fn composite_glyph(components: &[Component]) -> Vec<u8> {
    const ARG_WORDS: u16 = 0x0001;
    const ARG_XY: u16 = 0x0002;
    const HAS_SCALE: u16 = 0x0008;
    const MORE: u16 = 0x0020;

    let mut glyph = Vec::new();
    push_i16(&mut glyph, -1);
    push_i16(&mut glyph, 0);
    push_i16(&mut glyph, 0);
    push_i16(&mut glyph, 0x7fff);
    push_i16(&mut glyph, 0x7fff);

    for (index, component) in components.iter().enumerate() {
        let mut flags = ARG_WORDS | ARG_XY;
        if component.scale.is_some() {
            flags |= HAS_SCALE;
        }
        if index + 1 < components.len() {
            flags |= MORE;
        }

        push_u16(&mut glyph, flags);
        push_u16(&mut glyph, component.glyph_id);
        push_i16(&mut glyph, component.dx);
        push_i16(&mut glyph, component.dy);
        if let Some(scale) = component.scale {
            push_i16(&mut glyph, (scale * 16384.0) as i16);
        }
    }

    glyph
}

fn chain_glyphs(depth: usize, scale: Option<f32>) -> Vec<Vec<u8>> {
    let mut glyphs = vec![simple_glyph(SimpleGlyphOptions::new(3))];
    for level in 1..=depth {
        glyphs.push(composite_glyph(&[Component {
            glyph_id: u16::try_from(level - 1).unwrap(),
            dx: 0,
            dy: 0,
            scale,
        }]));
    }
    glyphs
}

fn fanout_glyphs(depth: u16, fanout: u16) -> Vec<Vec<u8>> {
    let mut glyphs = vec![simple_glyph(SimpleGlyphOptions::new(3))];
    for level in 1..=depth {
        let components = (0..fanout)
            .map(|_| Component {
                glyph_id: level - 1,
                dx: 0,
                dy: 0,
                scale: None,
            })
            .collect::<Vec<_>>();
        glyphs.push(composite_glyph(&components));
    }
    glyphs
}

struct OutlineResult {
    bbox: Option<Rect>,
    counts: CallbackCounts,
}

fn outline(data: &[u8], glyph_id: u16) -> OutlineResult {
    let face = Face::parse(data, 0).unwrap();
    let mut builder = CountingBuilder::default();
    let bbox = face.outline_glyph(GlyphId(glyph_id), &mut builder);
    OutlineResult {
        bbox,
        counts: builder.counts,
    }
}

#[cfg(feature = "variable-fonts")]
fn outline_with_variation(data: &[u8], glyph_id: u16, value: f32) -> (Face<'_>, OutlineResult) {
    let mut face = Face::parse(data, 0).unwrap();
    assert_eq!(face.set_variation(Tag::from_bytes(b"wght"), value), Some(()));
    let mut builder = CountingBuilder::default();
    let bbox = face.outline_glyph(GlyphId(glyph_id), &mut builder);
    (
        face,
        OutlineResult {
            bbox,
            counts: builder.counts,
        },
    )
}

fn expected_simple_callbacks(points: u16) -> CallbackCounts {
    CallbackCounts {
        move_to: 1,
        line_to: usize::from(points),
        quad_to: 0,
        curve_to: 0,
        close: 1,
    }
}

#[test]
fn simple_glyph_point_and_instruction_builder_emits_a_verified_complete_outline() {
    let data = truetype_font(&[simple_glyph(SimpleGlyphOptions::new(3))]);
    let result = outline(&data, 0);

    assert_eq!(result.counts, expected_simple_callbacks(3), "glyph={:?}", simple_glyph(SimpleGlyphOptions::new(3)));
    assert_eq!(result.bbox, Some(Rect {
        x_min: 10,
        y_min: 10,
        x_max: 30,
        y_max: 30,
    }));
    assert_eq!(result.counts, expected_simple_callbacks(3));
    assert!(result.counts.total() <= data.len() * 6);
}

#[test]
fn glyf_max_components_32_accepts_a_chain_of_exactly_31_components() {
    let glyphs = chain_glyphs(GLYF_MAX_COMPONENTS - 1, None);
    let data = truetype_font(&glyphs);
    let result = outline(&data, u16::try_from(glyphs.len() - 1).unwrap());

    assert_eq!(result.bbox, Some(Rect {
        x_min: 10,
        y_min: 10,
        x_max: 30,
        y_max: 30,
    }));
    assert_eq!(result.counts, expected_simple_callbacks(3));
}

#[test]
fn glyf_max_components_32_rejects_a_chain_one_component_past_the_limit() {
    let glyphs = chain_glyphs(GLYF_MAX_COMPONENTS, None);
    let data = truetype_font(&glyphs);
    let result = outline(&data, u16::try_from(glyphs.len() - 1).unwrap());

    assert_eq!(result.bbox, None);
    assert_eq!(result.counts, CallbackCounts::default());
}

#[test]
fn glyf_component_self_loop_is_stopped_by_max_component_visits_without_callbacks() {
    let glyph = composite_glyph(&[Component {
        glyph_id: 0,
        dx: 1,
        dy: 2,
        scale: None,
    }]);
    let data = truetype_font(&[glyph]);
    let result = outline(&data, 0);

    assert_eq!(result.bbox, None);
    assert_eq!(result.counts, CallbackCounts::default());
    assert!(result.counts.total() <= data.len());
}

#[test]
fn glyf_two_component_cycle_is_stopped_by_max_component_visits_without_callbacks() {
    let first = composite_glyph(&[Component {
        glyph_id: 1,
        dx: 1,
        dy: 0,
        scale: None,
    }]);
    let second = composite_glyph(&[Component {
        glyph_id: 0,
        dx: 0,
        dy: 1,
        scale: None,
    }]);
    let data = truetype_font(&[first, second]);
    let result = outline(&data, 0);

    assert_eq!(result.bbox, None);
    assert_eq!(result.counts, CallbackCounts::default());
    assert!(result.counts.total() <= data.len());
}

#[test]
fn glyf_max_component_visits_100_000_bounds_shared_child_fanout() {
    const FANOUT: u16 = 3;
    const DEPTH: u16 = 17;
    let glyphs = fanout_glyphs(DEPTH, FANOUT);
    let data = truetype_font(&glyphs);
    let result = outline(&data, DEPTH);

    let unguarded_visits = ((u64::from(FANOUT).pow(u32::from(DEPTH) + 1)) - 1) / 2;
    assert!(unguarded_visits > u64::from(GLYF_MAX_COMPONENT_VISITS));
    assert_eq!(result.bbox, None);
    assert_eq!(result.counts, CallbackCounts {
        move_to: 66_659,
        line_to: 199_977,
        quad_to: 0,
        curve_to: 0,
        close: 66_659,
    });
    assert!(result.counts.total() <= 5 * usize::try_from(GLYF_MAX_COMPONENT_VISITS).unwrap());
}

#[test]
fn multiple_components_sharing_one_child_outline_each_shared_copy_once() {
    let leaf = simple_glyph(SimpleGlyphOptions::new(3));
    let parent = composite_glyph(&[
        Component { glyph_id: 0, dx: 0, dy: 0, scale: None },
        Component { glyph_id: 0, dx: 50, dy: 0, scale: None },
        Component { glyph_id: 0, dx: 0, dy: 50, scale: None },
    ]);
    let data = truetype_font(&[leaf, parent]);
    let result = outline(&data, 1);

    assert_eq!(result.bbox, Some(Rect {
        x_min: 10,
        y_min: 10,
        x_max: 80,
        y_max: 80,
    }));
    assert_eq!(result.counts, CallbackCounts {
        move_to: 3,
        line_to: 9,
        quad_to: 0,
        curve_to: 0,
        close: 3,
    });
}

#[test]
fn truncated_simple_glyph_coordinates_return_none_before_any_verified_outline_callback() {
    let glyph = simple_glyph(SimpleGlyphOptions {
        points: 8,
        instructions: 0,
        truncate_coordinates_by: 1,
    });
    let data = truetype_font(&[glyph]);
    let result = outline(&data, 0);

    assert_eq!(result.bbox, None);
    assert_eq!(result.counts, CallbackCounts::default());
}

#[test]
fn declared_instructions_that_consume_coordinates_are_bounded_by_the_glyph_slice() {
    let glyph = simple_glyph(SimpleGlyphOptions {
        points: 8,
        instructions: 3,
        truncate_coordinates_by: 0,
    });
    let data = truetype_font(&[glyph]);
    let result = outline(&data, 0);

    assert_eq!(result.bbox, None);
    assert_eq!(result.counts, CallbackCounts::default());
}

#[test]
fn glyf_accumulated_component_scale_rejects_i16_range_without_panicking() {
    let glyphs = chain_glyphs(GLYF_MAX_COMPONENTS - 1, Some(2.0));
    let data = truetype_font(&glyphs);
    let result = outline(&data, u16::try_from(glyphs.len() - 1).unwrap());

    assert_eq!(result.bbox, None);
    assert_eq!(result.counts, expected_simple_callbacks(3));
}

#[cfg(feature = "variable-fonts")]
fn fvar_table() -> Vec<u8> {
    let mut data = Vec::new();
    push_u32(&mut data, 0x0001_0000);
    push_u16(&mut data, 14);
    push_u16(&mut data, 0);
    push_u16(&mut data, 1);
    push_u16(&mut data, 20);
    push_u16(&mut data, 0);
    assert_eq!(data.len(), 14);

    data.extend_from_slice(b"wght");
    data.extend_from_slice(&0xffff_0000u32.to_be_bytes());
    data.extend_from_slice(&0u32.to_be_bytes());
    data.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    push_u16(&mut data, 0);
    push_u16(&mut data, 256);
    data
}

#[cfg(feature = "variable-fonts")]
fn gvar_variation_data(tuple_count: u16) -> Vec<u8> {
    let mut data = Vec::new();
    let raw_count = tuple_count | if tuple_count == 0 { 0x8000 } else { 0 };
    data.extend_from_slice(&raw_count.to_le_bytes());

    if tuple_count >= 1 && tuple_count <= GVAR_STACK_TUPLE_LIMIT {
        push_u16(&mut data, 4 + 6 * tuple_count);
        for _ in 0..tuple_count {
            push_u16(&mut data, 1);
            push_u16(&mut data, 0x8000);
            push_u16(&mut data, 0x4000);
            data.push(0x82);
        }
    } else if tuple_count > GVAR_STACK_TUPLE_LIMIT {
        // The tuple table is structurally minimal; without `gvar-alloc` parsing
        // stops while reserving tuple storage, before any outline callbacks run.
        push_u16(&mut data, 0);
    } else {
        push_u16(&mut data, 0);
    }

    data
}

#[cfg(feature = "variable-fonts")]
fn variable_truetype_font(glyphs: &[Vec<u8>], tuple_count: u16) -> Vec<u8> {
    let mut font = SfntBuilder::truetype();
    let (glyf, loca) = glyphs_to_tables(glyphs);
    add_required_tables(&mut font, u16::try_from(glyphs.len()).unwrap());
    font.add(*b"glyf", glyf);
    font.add(*b"loca", loca);
    font.add(*b"fvar", fvar_table());

    let variation_data = gvar_variation_data(tuple_count);
    let data_offset = 20 + 4 * (glyphs.len() + 1);
    let valid_tuples = tuple_count >= 1 && tuple_count <= GVAR_STACK_TUPLE_LIMIT;
    let glyph_end = if tuple_count == 0 {
        u32::try_from(2).unwrap()
    } else if !valid_tuples {
        u32::try_from(variation_data.len()).unwrap()
    } else if valid_tuples {
        u32::try_from(variation_data.len()).unwrap()
    } else {
        u32::try_from(variation_data.len()).unwrap()
    };
    let mut gvar = Vec::new();
    push_u32(&mut gvar, 0x0001_0000);
    push_u16(&mut gvar, 1);
    push_u16(&mut gvar, 0);
    push_u32(&mut gvar, u32::try_from(data_offset).unwrap());
    push_u16(&mut gvar, u16::try_from(glyphs.len()).unwrap());
    push_u16(&mut gvar, 0);
    push_u32(&mut gvar, u32::try_from(data_offset).unwrap());
    assert_eq!(gvar.len(), 20);
    for glyph_id in 0..=u16::try_from(glyphs.len()).unwrap() {
        if glyph_id == 0 && tuple_count == 0 {
            push_u32(&mut gvar, 1);
        } else if glyph_id == 1 {
            push_u32(&mut gvar, glyph_end);
        } else {
            push_u32(&mut gvar, 0);
        }
    }
    gvar.extend_from_slice(&variation_data);
    font.add(*b"gvar", gvar);
    font.build()
}

#[cfg(feature = "variable-fonts")]
#[test]
fn gvar_32_stack_tuples_remain_a_complete_variation_outline_boundary() {
    let data = variable_truetype_font(
        &[simple_glyph(SimpleGlyphOptions::new(3))],
        GVAR_STACK_TUPLE_LIMIT,
    );
    let (_face, result) = outline_with_variation(&data, 0, 1.0);

    assert_eq!(result.bbox, Some(Rect {
        x_min: 10,
        y_min: 10,
        x_max: 30,
        y_max: 30,
    }));
    assert_eq!(result.counts, expected_simple_callbacks(3));
}

#[cfg(all(feature = "variable-fonts", not(feature = "gvar-alloc")))]
#[test]
fn gvar_tuple_count_33_rejects_without_gvar_alloc() {
    let data = variable_truetype_font(
        &[simple_glyph(SimpleGlyphOptions::new(3))],
        GVAR_STACK_TUPLE_LIMIT + 1,
    );
    let (_face, result) = outline_with_variation(&data, 0, 1.0);

    assert_eq!(result.bbox, None);
    assert_eq!(result.counts, CallbackCounts::default());
}

#[cfg(feature = "variable-fonts")]
#[test]
fn gvar_tuple_count_zero_is_a_format_boundary_for_enabled_and_disabled_features() {
    let static_font = truetype_font(&[simple_glyph(SimpleGlyphOptions::new(3))]);
    assert_eq!(outline(&static_font, 0).bbox, Some(Rect {
        x_min: 10,
        y_min: 10,
        x_max: 30,
        y_max: 30,
    }));

    let data = variable_truetype_font(
        &[simple_glyph(SimpleGlyphOptions::new(3))],
        0,
    );
    let (_face, varied) = outline_with_variation(&data, 0, 1.0);

    assert_eq!(varied.bbox, None);
    assert_eq!(varied.counts, CallbackCounts::default());
}

fn cff_int(value: i32) -> Vec<u8> {
    if (-107..=107).contains(&value) {
        vec![u8::try_from(value + 139).unwrap()]
    } else if (108..=1131).contains(&value) {
        let n = value - 108;
        vec![247 + u8::try_from(n >> 8).unwrap(), u8::try_from(n & 0xff).unwrap()]
    } else if (-1131..=-108).contains(&value) {
        let n = -value - 108;
        vec![255 - u8::try_from(n >> 8).unwrap(), u8::try_from(n & 0xff).unwrap()]
    } else {
        let mut data = vec![28];
        data.extend_from_slice(&i16::try_from(value).unwrap().to_be_bytes());
        data
    }
}

fn cff_index(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut data = Vec::new();
    push_u16(&mut data, u16::try_from(objects.len()).unwrap());
    if objects.is_empty() {
        return data;
    }

    let mut object_data_len = 0usize;
    for object in objects {
        object_data_len += object.len();
    }
    let last_offset = object_data_len + 1;
    let offset_size = if last_offset <= 0xff {
        1
    } else if last_offset <= 0xffff {
        2
    } else {
        3
    };
    data.push(offset_size);
    let write_offset = |data: &mut Vec<u8>, offset: usize| {
        match offset_size {
            1 => data.push(u8::try_from(offset).unwrap()),
            2 => push_u16(data, u16::try_from(offset).unwrap()),
            3 => data.extend_from_slice(&u32::try_from(offset).unwrap().to_be_bytes()[1..]),
            _ => unreachable!(),
        }
    };
    write_offset(&mut data, 1);
    let mut running_offset = 1usize;
    for object in objects {
        running_offset += object.len();
        write_offset(&mut data, running_offset);
    }
    for object in objects {
        data.extend_from_slice(object);
    }
    data
}

fn cff_call_global(index: usize) -> Vec<u8> {
    let bias = 107i32;
    let mut data = cff_int(i32::try_from(index).unwrap() - bias);
    data.push(29);
    data
}

struct CffFont {
    root: Vec<u8>,
    global_subrs: Vec<Vec<u8>>,
    invalid_private_offset: bool,
}

impl CffFont {
    fn build(self) -> Vec<u8> {
        const HEADER: [u8; 4] = [1, 0, 4, 0];

        let name = cff_index(&[vec![0]]);
        let strings = cff_index(&[]);
        let globals = cff_index(&self.global_subrs);
        let chars = cff_index(&[self.root]);

        let mut char_strings_offset = 0usize;
        let (top_index, private) = loop {
            let mut top = Vec::new();
            top.extend_from_slice(&cff_int(i32::try_from(char_strings_offset).unwrap()));
            top.push(17);

            let mut private = Vec::new();
            if self.invalid_private_offset {
                let private_offset = char_strings_offset + chars.len();
                top.extend_from_slice(&cff_int(6));
                top.extend_from_slice(&cff_int(i32::try_from(private_offset).unwrap()));
                top.push(18);
                private.extend_from_slice(&cff_int(6));
                private.extend_from_slice(&cff_int(0x7fff));
                private.push(19);
            }

            let top_index = cff_index(&[top]);
            let next_char_offset = HEADER.len()
                + name.len()
                + top_index.len()
                + strings.len()
                + globals.len()
                + private.len();

            if next_char_offset == char_strings_offset {
                break (top_index, private);
            }

            char_strings_offset = next_char_offset;
        };

        let mut data = Vec::new();
        data.extend_from_slice(&HEADER);
        data.extend_from_slice(&name);
        data.extend_from_slice(&top_index);
        data.extend_from_slice(&strings);
        data.extend_from_slice(&globals);
        data.extend_from_slice(&chars);
        data.extend_from_slice(&private);
        data
    }
}

fn cff_simple_root() -> Vec<u8> {
    let mut root = cff_int(10);
    root.push(22);
    root.extend_from_slice(&cff_int(50));
    root.extend_from_slice(&cff_int(50));
    root.push(5);
    root.push(14);
    root
}

fn cff_chain_font(depth: usize, fanout: usize) -> Vec<u8> {
    let mut subrs = Vec::new();
    for level in 0..depth {
        let mut body = Vec::new();
        if level + 1 < depth {
            for _ in 0..fanout {
                body.extend_from_slice(&cff_call_global(level + 1));
            }
            body.push(11);
        } else {
            body.extend_from_slice(&cff_int(50));
            body.extend_from_slice(&cff_int(50));
            body.push(5);
            body.push(11);
        }
        subrs.push(body);
    }

    let mut root = cff_int(10);
    root.push(22);
    for _ in 0..fanout {
        root.extend_from_slice(&cff_call_global(0));
    }
    root.push(14);

    cff_font_with_global_subrs(root, subrs)
}

fn cff_font_with_global_subrs(root: Vec<u8>, global_subrs: Vec<Vec<u8>>) -> Vec<u8> {
    CffFont {
        root,
        global_subrs,
        invalid_private_offset: false,
    }
    .build()
}

fn cff_loop_font() -> Vec<u8> {
    let mut root = cff_int(10);
    root.push(22);
    root.extend_from_slice(&cff_call_global(0));
    root.push(14);
    cff_font_with_global_subrs(root, vec![cff_call_global(0)])
}

fn cff_two_node_loop_font() -> Vec<u8> {
    let mut root = cff_int(10);
    root.push(22);
    root.extend_from_slice(&cff_call_global(0));
    root.push(14);
    cff_font_with_global_subrs(
        root,
        vec![cff_call_global(1), cff_call_global(0)],
    )
}

fn outline_cff_table(cff_data: &[u8]) -> Result<Rect, CFFError> {
    let table = cff::Table::parse(cff_data).unwrap();
    let mut builder = CountingBuilder::default();
    table.outline(GlyphId(0), &mut builder)
}

fn cff_otf(cff_data: Vec<u8>) -> Vec<u8> {
    let mut font = SfntBuilder::cff();
    add_required_tables(&mut font, 1);
    font.add(*b"CFF ", cff_data);
    font.build()
}

#[test]
fn cff_stack_limit_10_accepts_exactly_ten_nested_global_subroutines() {
    let cff_data = cff_chain_font(CFF_STACK_LIMIT, 1);
    let data = cff_otf(cff_data);
    let result = outline(&data, 0);

    assert_eq!(result.bbox, Some(Rect {
        x_min: 10,
        y_min: 0,
        x_max: 60,
        y_max: 50,
    }));
    assert_eq!(result.counts, CallbackCounts {
        move_to: 1,
        line_to: 1,
        quad_to: 0,
        curve_to: 0,
        close: 1,
    });
}

#[test]
fn cff_stack_limit_10_rejects_eleven_nested_global_subroutines_without_partial_output() {
    let cff_data = cff_chain_font(CFF_STACK_LIMIT + 1, 1);
    assert_eq!(
        outline_cff_table(&cff_data).unwrap_err(),
        CFFError::NestingLimitReached
    );

    let result = outline(&cff_otf(cff_data), 0);
    assert_eq!(result.bbox, None);
    assert_eq!(result.counts, CallbackCounts {
        move_to: 1,
        line_to: 0,
        quad_to: 0,
        curve_to: 0,
        close: 0,
    });
}

#[test]
fn cff_max_subroutine_calls_4096_bounds_fanout_within_a_small_table() {
    let cff_data = cff_chain_font(7, 4);
    let invocations = (1..=7u32).map(|level| 4u32.pow(level)).sum::<u32>();
    assert!(invocations > CFF_MAX_SUBROUTINE_CALLS);
    assert_eq!(
        outline_cff_table(&cff_data).unwrap_err(),
        CFFError::SubroutineCallLimitReached
    );

    let result = outline(&cff_otf(cff_data), 0);
    assert_eq!(result.bbox, None);
    assert!(result.counts.total() <= 5 * usize::try_from(CFF_MAX_SUBROUTINE_CALLS).unwrap() + 1);
}

#[test]
fn cff_global_subroutine_self_loop_is_stopped_at_depth_10() {
    let cff_data = cff_loop_font();

    assert_eq!(
        outline_cff_table(&cff_data).unwrap_err(),
        CFFError::NestingLimitReached
    );

    let result = outline(&cff_otf(cff_data), 0);
    assert_eq!(result.bbox, None);
    assert_eq!(result.counts, CallbackCounts {
        move_to: 1,
        line_to: 0,
        quad_to: 0,
        curve_to: 0,
        close: 0,
    });
}

#[test]
fn cff_global_subroutine_two_node_cycle_is_stopped_at_depth_10() {
    let cff_data = cff_two_node_loop_font();

    assert_eq!(
        outline_cff_table(&cff_data).unwrap_err(),
        CFFError::NestingLimitReached
    );

    let result = outline(&cff_otf(cff_data), 0);
    assert_eq!(result.bbox, None);
    assert_eq!(result.counts, CallbackCounts {
        move_to: 1,
        line_to: 0,
        quad_to: 0,
        curve_to: 0,
        close: 0,
    });
}

#[test]
fn cff_private_subroutine_offset_outside_the_table_is_rejected() {
    let cff_data = CffFont {
        root: cff_simple_root(),
        global_subrs: Vec::new(),
        invalid_private_offset: true,
    }
    .build();

    let result = outline(&cff_otf(cff_data), 0);
    assert_eq!(result.bbox, None);
    assert_eq!(result.counts, CallbackCounts::default());
}

#[test]
fn glyf_table_bbox_reports_stored_metadata_even_when_outline_validation_rejects_coordinates() {
    let glyph = simple_glyph(SimpleGlyphOptions {
        points: 8,
        instructions: 0,
        truncate_coordinates_by: 1,
    });
    let data = truetype_font(&[glyph]);
    let face = Face::parse(&data, 0).unwrap();

    assert_eq!(face.tables().glyf.unwrap().bbox(GlyphId(0)), Some(Rect {
        x_min: 0,
        y_min: 0,
        x_max: 30,
        y_max: 30,
    }));
    assert_eq!(outline(&data, 0).bbox, None);
}

#[test]
fn targeted_outline_fuzz_corpus_entries_parse_within_named_budgets() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testing-tools/ttf-fuzz/corpus-outline");

    let mut entries = fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();

    assert!(entries.len() >= 8);
    for path in entries {
        let data = fs::read(&path).unwrap();
        assert!(data.len() < 1024, "{} should remain a tiny regression fixture", path.display());
        let result = outline(&data, 0);
        assert!(result.counts.total() <= 5 * usize::try_from(GLYF_MAX_COMPONENT_VISITS).unwrap());
        if let Some(rect) = result.bbox {
            assert!(rect.x_min <= rect.x_max);
            assert!(rect.y_min <= rect.y_max);
        }
    }
}
