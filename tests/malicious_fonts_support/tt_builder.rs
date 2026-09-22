//! Minimal sfnt/TrueType (`glyf`) font constructor for adversarial tests.
//!
//! Every field the parser trusts an attacker with is explicit here: component depth,
//! per-glyph fan-out, point count, hinting-instruction length, coordinate-array
//! truncation and component-argument truncation. The constructor performs *no*
//! validation, so a test can feed the parser a font that lies about every length it
//! contains. The only mandatory tables are `head`, `hhea`, `maxp` and `hmtx`; `glyf`
//! and `loca` are added when glyph data is present and arbitrary extra tables
//! (`fvar`, `gvar`, ...) can be attached for variable-font tests.

/// Composite glyph flag bits (`glyf` spec, Composite Glyph Description).
#[allow(dead_code)]
pub mod composite_flags {
    pub const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
    pub const ARGS_ARE_XY_VALUES: u16 = 0x0002;
    pub const WE_HAVE_A_SCALE: u16 = 0x0008;
    pub const MORE_COMPONENTS: u16 = 0x0020;
    pub const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
    pub const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;
    pub const WE_HAVE_INSTRUCTIONS: u16 = 0x0100;
}

/// Simple glyph flag bits (`glyf` spec, Simple Glyph Flags).
#[allow(dead_code)]
pub mod simple_flags {
    pub const ON_CURVE: u8 = 0x01;
    pub const X_SHORT: u8 = 0x02;
    pub const Y_SHORT: u8 = 0x04;
    pub const REPEAT: u8 = 0x08;
    pub const X_SAME_OR_POSITIVE_SHORT: u8 = 0x10;
    pub const Y_SAME_OR_POSITIVE_SHORT: u8 = 0x20;
}

/// One component reference inside a composite glyph.
#[derive(Clone, Copy)]
pub struct Component {
    pub glyph_index: u16,
    pub flags: u16,
    /// `Some((dx, dy))` emits x/y args (word or byte per `flags`); `None` emits
    /// two point-number args so the parser still has to consume them.
    pub args: Option<(i16, i16)>,
    /// Affine entries appended according to the scale flags.
    pub scale: Option<Scale>,
}

#[derive(Clone, Copy)]
#[allow(dead_code)] // Only `Uniform` is used by the current fixtures.
pub enum Scale {
    Uniform(i16),
    Xy(i16, i16),
    TwoByTwo([i16; 4]),
}

/// Glyph program: either hand-built bytes (raw, for truncation/lying-length cases)
/// or a structured simple/composite description.
#[allow(dead_code)] // `Empty`/`Raw` variants are part of the constructor surface.
pub enum GlyphData {
    /// Use the provided bytes verbatim, including the 10-byte glyph header.
    Raw(Vec<u8>),
    Empty,
    Simple(SimpleGlyph),
    Composite(CompositeGlyph),
}

#[derive(Default)]
pub struct SimpleGlyph {
    pub bbox: [i16; 4],
    pub endpoints: Vec<u16>,
    pub instructions: Vec<u8>,
    /// One flag per point; the constructor emits a repeat run for each run.
    pub flags: Vec<u8>,
    /// Coordinate delta bytes, copied verbatim. Pass fewer bytes than the flags
    /// promise to build a coordinate-truncated glyph.
    pub coords: Vec<u8>,
    /// When false, the declared `instructionLength` is set but the bytes are not
    /// appended: a truncated-instructions glyph.
    pub include_instructions: bool,
}

#[derive(Default)]
pub struct CompositeGlyph {
    pub bbox: [i16; 4],
    pub components: Vec<Component>,
    pub instructions: Vec<u8>,
    /// Append the component-instruction length/bytes after the components. The
    /// `WE_HAVE_INSTRUCTIONS` flag on the last component must request this.
    pub include_instructions: bool,
    /// Truncate the component record stream after this many components, keeping
    /// the `MORE_COMPONENTS` flag set, to test mid-record bounds checks.
    pub truncate_after: Option<usize>,
}

/// A triangle leaf: one contour, three on-curve points, all coordinates encoded as
/// positive short deltas (`(10,10) -> (30,10) -> (30,30)`). Its outline produces
/// exactly one `move_to`, three `line_to`s (the last closes back to the start) and
/// one `close` from the `glyf` flattener.
pub fn triangle_leaf() -> GlyphData {
    use simple_flags::*;
    GlyphData::Simple(SimpleGlyph {
        bbox: [10, 10, 30, 30],
        endpoints: vec![2],
        instructions: Vec::new(),
        flags: vec![
            ON_CURVE | X_SHORT | Y_SHORT | X_SAME_OR_POSITIVE_SHORT | Y_SAME_OR_POSITIVE_SHORT,
            ON_CURVE | X_SHORT | Y_SHORT | X_SAME_OR_POSITIVE_SHORT | Y_SAME_OR_POSITIVE_SHORT,
            ON_CURVE | X_SHORT | Y_SHORT | X_SAME_OR_POSITIVE_SHORT | Y_SAME_OR_POSITIVE_SHORT,
        ],
        coords: vec![
            10, 20, 0, // x deltas
            10, 0, 20, // y deltas
        ],
        include_instructions: true,
    })
}

/// Callbacks the `glyf` flattener emits for `triangle_leaf`, counted exactly.
pub const TRIANGLE_DRAW_VERBS: u64 = 1 /* move_to */ + 3 /* line_to */;
pub const TRIANGLE_CLOSES: u64 = 1;

/// A composite that references the same child `fan_out` times with zero
/// translation. All fan-out but no depth: the depth guard never sees it.
pub fn fan_out_components(child: u16, fan_out: u16) -> Vec<Component> {
    use composite_flags::*;
    (0..fan_out)
        .map(|i| Component {
            glyph_index: child,
            flags: ARG_1_AND_2_ARE_WORDS
                | ARGS_ARE_XY_VALUES
                | if i + 1 < fan_out { MORE_COMPONENTS } else { 0 },
            args: Some((0, 0)),
            scale: None,
        })
        .collect()
}

fn glyph_bytes(data: &GlyphData) -> Vec<u8> {
    match data {
        GlyphData::Raw(bytes) => bytes.clone(),
        GlyphData::Empty => Vec::new(),
        GlyphData::Simple(g) => {
            let mut out = Vec::new();
            let contours = g.endpoints.len() as i16;
            out.extend_from_slice(&contours.to_be_bytes());
            for v in &g.bbox {
                out.extend_from_slice(&v.to_be_bytes());
            }
            for e in &g.endpoints {
                out.extend_from_slice(&e.to_be_bytes());
            }
            out.extend_from_slice(&(g.instructions.len() as u16).to_be_bytes());
            if g.include_instructions {
                out.extend_from_slice(&g.instructions);
            }
            out.extend_from_slice(&g.flags);
            out.extend_from_slice(&g.coords);
            out
        }
        GlyphData::Composite(g) => {
            let mut out = Vec::new();
            out.extend_from_slice(&(-1i16).to_be_bytes());
            for v in &g.bbox {
                out.extend_from_slice(&v.to_be_bytes());
            }
            let count = g
                .truncate_after
                .map_or(g.components.len(), |n| n.min(g.components.len()));
            for (i, c) in g.components.iter().enumerate().take(count) {
                out.extend_from_slice(&c.flags.to_be_bytes());
                out.extend_from_slice(&c.glyph_index.to_be_bytes());
                match c.args {
                    Some((dx, dy)) => {
                        if c.flags & composite_flags::ARG_1_AND_2_ARE_WORDS != 0 {
                            out.extend_from_slice(&dx.to_be_bytes());
                            out.extend_from_slice(&dy.to_be_bytes());
                        } else {
                            out.push(dx as i8 as u8);
                            out.push(dy as i8 as u8);
                        }
                    }
                    None => {
                        if c.flags & composite_flags::ARG_1_AND_2_ARE_WORDS != 0 {
                            out.extend_from_slice(&0u16.to_be_bytes());
                            out.extend_from_slice(&0u16.to_be_bytes());
                        } else {
                            out.push(0);
                            out.push(0);
                        }
                    }
                }
                match c.scale {
                    Some(Scale::Uniform(s)) => out.extend_from_slice(&s.to_be_bytes()),
                    Some(Scale::Xy(x, y)) => {
                        out.extend_from_slice(&x.to_be_bytes());
                        out.extend_from_slice(&y.to_be_bytes());
                    }
                    Some(Scale::TwoByTwo(m)) => {
                        for v in m {
                            out.extend_from_slice(&v.to_be_bytes());
                        }
                    }
                    None => {}
                }
                // Deliberately leave the last written record claiming more records
                // when truncating mid-stream.
                if g.truncate_after == Some(i + 1) {
                    break;
                }
            }
            if g.include_instructions {
                out.extend_from_slice(&(g.instructions.len() as u16).to_be_bytes());
                out.extend_from_slice(&g.instructions);
            }
            out
        }
    }
}

/// Builds a TrueType sfnt from glyph programs and any extra tables.
pub fn build_sfnt(glyphs: &[GlyphData], extra_tables: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let number_of_glyphs = glyphs.len().max(1) as u16;

    let mut glyf = Vec::new();
    let mut loca = Vec::new();
    for glyph in glyphs {
        loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
        glyf.extend_from_slice(&glyph_bytes(glyph));
    }
    // The last glyph is empty when the caller supplied no data: loca needs
    // numGlyphs + 1 entries, and empty ranges collapse to nothing when the font
    // actually has glyphs. Append a terminator only when glyphs are present; an
    // all-empty `glyf` table still needs a legal two-entry loca.
    loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());

    let head = head_table();
    let hhea = hhea_table();
    let maxp = maxp_table(number_of_glyphs);
    // One long horizontal metric per glyph (advance 600, lsb 0).
    let mut hmtx = Vec::new();
    for _ in 0..number_of_glyphs {
        hmtx.extend_from_slice(&600u16.to_be_bytes());
        hmtx.extend_from_slice(&0i16.to_be_bytes());
    }

    let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"glyf", glyf),
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"hmtx", hmtx),
        (*b"loca", loca),
        (*b"maxp", maxp),
    ];
    tables.extend(extra_tables.iter().map(|(t, d)| (*t, d.clone())));
    tables.sort_by_key(|(tag, _)| *tag);

    let mut font = Vec::new();
    font.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    font.extend_from_slice(&(tables.len() as u16).to_be_bytes());
    font.extend_from_slice(&0u16.to_be_bytes()); // searchRange (unchecked)
    font.extend_from_slice(&0u16.to_be_bytes()); // entrySelector (unchecked)
    font.extend_from_slice(&0u16.to_be_bytes()); // rangeShift (unchecked)

    let mut offset = 12u32 + 16 * tables.len() as u32;
    let mut records = Vec::new();
    for (tag, data) in &tables {
        records.push((*tag, offset, data.len() as u32));
        offset += data.len() as u32;
    }
    for (tag, off, len) in &records {
        font.extend_from_slice(tag);
        font.extend_from_slice(&0u32.to_be_bytes()); // checkSum, not validated
        font.extend_from_slice(&off.to_be_bytes());
        font.extend_from_slice(&len.to_be_bytes());
    }
    for (_, data) in tables.iter() {
        font.extend_from_slice(data);
    }
    font
}

fn head_table() -> Vec<u8> {
    let mut head = Vec::new();
    head.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version
    head.extend_from_slice(&0u32.to_be_bytes()); // fontRevision
    head.extend_from_slice(&0u32.to_be_bytes()); // checkSumAdjustment
    head.extend_from_slice(&0x5F0F_3CF5u32.to_be_bytes()); // magicNumber
    head.extend_from_slice(&0u16.to_be_bytes()); // flags
    head.extend_from_slice(&1000u16.to_be_bytes()); // unitsPerEm
    head.extend_from_slice(&0u64.to_be_bytes()); // created
    head.extend_from_slice(&0u64.to_be_bytes()); // modified
    head.extend_from_slice(&0i16.to_be_bytes()); // xMin
    head.extend_from_slice(&0i16.to_be_bytes()); // yMin
    head.extend_from_slice(&0i16.to_be_bytes()); // xMax
    head.extend_from_slice(&0i16.to_be_bytes()); // yMax
    head.extend_from_slice(&0u16.to_be_bytes()); // macStyle
    head.extend_from_slice(&8u16.to_be_bytes()); // lowestRecPPEM
    head.extend_from_slice(&2i16.to_be_bytes()); // fontDirectionHint
    head.extend_from_slice(&1i16.to_be_bytes()); // indexToLocFormat: long
    head.extend_from_slice(&0i16.to_be_bytes()); // glyphDataFormat
    assert_eq!(head.len(), 54);
    head
}

fn hhea_table() -> Vec<u8> {
    let mut hhea = Vec::new();
    hhea.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    hhea.extend_from_slice(&800i16.to_be_bytes());
    hhea.extend_from_slice(&(-200i16).to_be_bytes());
    hhea.extend_from_slice(&0i16.to_be_bytes());
    hhea.extend_from_slice(&[0u8; 24]);
    hhea.extend_from_slice(&1u16.to_be_bytes());
    assert_eq!(hhea.len(), 36);
    hhea
}

fn maxp_table(number_of_glyphs: u16) -> Vec<u8> {
    let mut maxp = Vec::new();
    maxp.extend_from_slice(&0x0000_5000u32.to_be_bytes()); // version 0.5
    maxp.extend_from_slice(&number_of_glyphs.to_be_bytes());
    maxp
}

/// Builds an OpenType (`OTTO`) sfnt that carries a CFF or CFF2 table instead of
/// `glyf`/`loca`. The mandatory `head`, `hhea`, `maxp`, `hmtx` tables are shared
/// with the TrueType constructor.
pub fn build_otto(table_tag: [u8; 4], table_data: Vec<u8>) -> Vec<u8> {
    let number_of_glyphs = 1u16;
    let head = head_table();
    let hhea = hhea_table();
    let maxp = maxp_table(number_of_glyphs);
    let mut hmtx = Vec::new();
    hmtx.extend_from_slice(&600u16.to_be_bytes());
    hmtx.extend_from_slice(&0i16.to_be_bytes());

    let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"hmtx", hmtx),
        (*b"maxp", maxp),
        (table_tag, table_data),
    ];
    tables.sort_by_key(|(tag, _)| *tag);

    let mut font = Vec::new();
    font.extend_from_slice(b"OTTO");
    font.extend_from_slice(&(tables.len() as u16).to_be_bytes());
    font.extend_from_slice(&0u16.to_be_bytes());
    font.extend_from_slice(&0u16.to_be_bytes());
    font.extend_from_slice(&0u16.to_be_bytes());

    let mut offset = 12u32 + 16 * tables.len() as u32;
    let mut records = Vec::new();
    for (tag, data) in &tables {
        records.push((*tag, offset, data.len() as u32));
        offset += data.len() as u32;
    }
    for (tag, off, len) in &records {
        font.extend_from_slice(tag);
        font.extend_from_slice(&0u32.to_be_bytes());
        font.extend_from_slice(&off.to_be_bytes());
        font.extend_from_slice(&len.to_be_bytes());
    }
    for (_, data) in tables.iter() {
        font.extend_from_slice(data);
    }
    font
}

/// Public re-exports of the mandatory-table builders, for tests that craft raw
/// `glyf`/`loca` bytes themselves.
pub fn public_head() -> Vec<u8> {
    head_table()
}
pub fn public_hhea() -> Vec<u8> {
    hhea_table()
}
pub fn public_maxp(number_of_glyphs: u16) -> Vec<u8> {
    maxp_table(number_of_glyphs)
}
