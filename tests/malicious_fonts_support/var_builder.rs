//! Minimal `fvar`/`gvar` table constructors for variable-font adversarial tests.
//!
//! `gvar` glyph variation data begins with a tuple count (low 12 bits) and a data
//! offset. Without the `gvar-alloc` feature more than 32 tuples cannot be stored on
//! the 32-slot stack buffer, so `VariationTuples::reserve` rejects the glyph; with
//! `gvar-alloc` it would spill to the heap. These builders let a test declare the
//! tuple count directly and attach whatever (possibly empty/truncated) serialized
//! bytes follow, exercising the format boundary rather than real delta math.

/// Builds a one-axis (`wght`, 0..=1000, default 400) `fvar` table.
pub fn fvar_one_axis() -> Vec<u8> {
    let mut t = Vec::new();
    t.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // majorMinorVersion
    t.extend_from_slice(&16u16.to_be_bytes()); // axesArrayOffset
    t.extend_from_slice(&[0u8, 0]); // reserved
    t.extend_from_slice(&1u16.to_be_bytes()); // axisCount
    t.extend_from_slice(&0u16.to_be_bytes()); // axisSize (unchecked)
    t.extend_from_slice(&0u16.to_be_bytes()); // instanceCount
    t.extend_from_slice(&0u16.to_be_bytes()); // instanceSize

    // Axis record.
    t.extend_from_slice(b"wght");
    t.extend_from_slice(&(0u32).to_be_bytes()); // minValue Fixed
    t.extend_from_slice(&((400.0f32 * 65536.0) as i32).to_be_bytes()); // defaultValue
    t.extend_from_slice(&((1000.0f32 * 65536.0) as i32).to_be_bytes()); // maxValue
    t.extend_from_slice(&0u16.to_be_bytes()); // flags
    t.extend_from_slice(&256u16.to_be_bytes()); // axisNameID
    t
}

/// Glyph variation data for one glyph: a header claiming `tuple_count` tuples,
/// followed by `payload` bytes at `data_offset`.
pub fn glyph_variation_data(tuple_count: u16, payload: &[u8], data_offset: u16) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&(tuple_count & 0x0FFF).to_be_bytes());
    d.extend_from_slice(&data_offset.to_be_bytes());
    d.extend_from_slice(payload);
    d
}

/// Builds a `gvar` table.
///
/// - one axis, no shared tuples;
/// - `glyphs` holds the raw variation-data blob per glyph (may be empty);
/// - offsets are long (Offset32) so short-offset doubling arithmetic is avoided
///   in the fixture.
pub fn gvar_table(glyph_blobs: &[Vec<u8>]) -> Vec<u8> {
    let header_len: u32 = 20;
    let offsets_len: u32 = 4 * (glyph_blobs.len() as u32 + 1);
    let data_array_offset = header_len + offsets_len;

    let mut t = Vec::new();
    t.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version
    t.extend_from_slice(&1u16.to_be_bytes()); // axisCount
    t.extend_from_slice(&0u16.to_be_bytes()); // globalTupleCount
    t.extend_from_slice(&0u32.to_be_bytes()); // globalTupleCoordsOffset (0 = none)
    t.extend_from_slice(&(glyph_blobs.len() as u16).to_be_bytes()); // glyphCount
    t.extend_from_slice(&1u16.to_be_bytes()); // flags: long offsets
    t.extend_from_slice(&data_array_offset.to_be_bytes());

    let mut cursor = 0u32;
    for blob in glyph_blobs {
        t.extend_from_slice(&(data_array_offset + cursor).to_be_bytes());
        cursor += blob.len() as u32;
    }
    t.extend_from_slice(&(data_array_offset + cursor).to_be_bytes());

    for blob in glyph_blobs {
        t.extend_from_slice(blob);
    }
    t
}
