//! Minimal CFF1 and CFF2 table constructors for subroutine-cycle tests.
//!
//! CFF subroutines form a call graph: a subroutine can call itself or another
//! subroutine forever, and a small INDEX can be re-entered exponentially. The
//! parser bounds this two ways (`src/tables/cff/cff1.rs` / `cff2.rs`):
//!
//! - `STACK_LIMIT = 10` bounds nesting depth;
//! - `MAX_SUBROUTINE_CALLS = 4_096` bounds total invocations per glyph.
//!
//! The builders emit just enough of each format to reach the charstring
//! interpreter with attacker-chosen subroutine bodies. All DICT operands use the
//! fixed 5-byte integer form, so every layout offset is known before the offsets
//! themselves are written (single-pass construction, no fix-up loops).

/// Charstring integer operand (Adobe Technical Note #5177, Table 3).
pub fn cs_int(value: i32) -> Vec<u8> {
    match value {
        -107..=107 => vec![(value + 139) as u8],
        108..=1131 => {
            let n = value - 108;
            vec![((n >> 8) + 247) as u8, (n & 0xFF) as u8]
        }
        -1131..=-108 => {
            let n = -value - 108;
            vec![((n >> 8) + 251) as u8, (n & 0xFF) as u8]
        }
        _ => {
            let mut v = vec![28];
            v.extend_from_slice(&(value as i16).to_be_bytes());
            v
        }
    }
}

/// A 5-byte DICT integer operand.
pub fn dict_int(value: i32) -> Vec<u8> {
    let mut v = vec![29];
    v.extend_from_slice(&value.to_be_bytes());
    v
}

fn index_with_count_width(objects: &[Vec<u8>], count_bytes: usize) -> Vec<u8> {
    let empty = vec![0u8; count_bytes];
    if objects.is_empty() {
        return empty;
    }

    let mut offsets: Vec<usize> = Vec::with_capacity(objects.len() + 1);
    let mut current = 1usize;
    offsets.push(current);
    for object in objects {
        current += object.len();
        offsets.push(current);
    }
    let last = *offsets.last().unwrap();
    let off_size = if last <= 0xFF {
        1
    } else if last <= 0xFFFF {
        2
    } else if last <= 0xFF_FFFF {
        3
    } else {
        4
    };

    let mut out = Vec::new();
    let count = objects.len() as u32;
    match count_bytes {
        2 => out.extend_from_slice(&(count as u16).to_be_bytes()),
        4 => out.extend_from_slice(&count.to_be_bytes()),
        _ => unreachable!(),
    }
    out.push(off_size as u8);
    for o in offsets {
        for shift in (0..off_size).rev() {
            out.push(((o >> (shift * 8)) & 0xFF) as u8);
        }
    }
    for object in objects {
        out.extend_from_slice(object);
    }
    out
}

/// A CFF1 INDEX (u16 count).
pub fn index16(objects: &[Vec<u8>]) -> Vec<u8> {
    index_with_count_width(objects, 2)
}

/// A CFF2 INDEX (u32 count).
pub fn index32(objects: &[Vec<u8>]) -> Vec<u8> {
    index_with_count_width(objects, 4)
}

fn wrap_cff_dict(body: &[u8]) -> Vec<u8> {
    // One-object CFF1 INDEX: count=1, offSize=1, offsets [1, len+1].
    debug_assert!(body.len() <= 254);
    let mut out = vec![0u8, 1, 1, 1, (body.len() + 1) as u8];
    out.extend_from_slice(body);
    out
}

/// Builds a minimal CFF1 table: header, empty Name INDEX, Top DICT INDEX, empty
/// String INDEX, Global Subr INDEX, CharStrings INDEX, and (when local subrs are
/// present) a Private DICT followed by the Local Subr INDEX. Charset offset is
/// omitted, so the predefined ISOAdobe charset applies (`.notdef`, one glyph).
pub fn cff1_table(
    char_string: &[u8],
    global_subrs: &[Vec<u8>],
    local_subrs: &[Vec<u8>],
) -> Vec<u8> {
    let name_index = vec![0u8, 0];
    let strings_index = vec![0u8, 0];
    let global_index = index16(global_subrs);
    let chars_index = index16(&[char_string.to_vec()]);
    let local_index = index16(local_subrs);

    // The Private DICT carries one entry: the LocalSubrs offset (op 19 takes a
    // single operand) relative to the Private DICT start. The Local Subr INDEX
    // immediately follows the 6-byte DICT, so the relative offset is 6.
    let private_dict: Vec<u8> = if local_subrs.is_empty() {
        Vec::new()
    } else {
        let mut d = dict_int(6);
        d.push(19);
        debug_assert_eq!(d.len(), 6);
        d
    };

    // Top DICT body sizes are fixed: charstrings entry is 6 bytes; private entry
    // is 11 bytes.
    let top_dict_len = 6 + if local_subrs.is_empty() { 0 } else { 11 };
    let top_dict_index = wrap_cff_dict(&vec![0; top_dict_len]); // same length wrapper

    let mut cursor = 4usize; // header
    cursor += name_index.len();
    cursor += top_dict_index.len();
    cursor += strings_index.len();
    cursor += global_index.len();
    let charstrings_offset = cursor;
    cursor += chars_index.len();
    let private_offset = cursor;

    let mut top_body = Vec::new();
    if !local_subrs.is_empty() {
        top_body.extend_from_slice(&dict_int(private_dict.len() as i32));
        top_body.extend_from_slice(&dict_int(private_offset as i32));
        top_body.push(18);
    }
    top_body.extend_from_slice(&dict_int(charstrings_offset as i32));
    top_body.push(17);
    debug_assert_eq!(top_body.len(), top_dict_len);

    let mut table = vec![1, 0, 4, 4];
    table.extend_from_slice(&name_index);
    table.extend_from_slice(&wrap_cff_dict(&top_body));
    table.extend_from_slice(&strings_index);
    table.extend_from_slice(&global_index);
    table.extend_from_slice(&chars_index);
    if !local_subrs.is_empty() {
        table.extend_from_slice(&private_dict);
        table.extend_from_slice(&local_index);
    }
    table
}

/// CFF2 charstring operators used here.
pub mod cff2_op {
    pub const HORIZONTAL_MOVE_TO: u8 = 22;
    pub const CALL_LOCAL_SUBROUTINE: u8 = 10;
    pub const CALL_GLOBAL_SUBROUTINE: u8 = 29;
}

/// Builds a minimal *static* CFF2 table (no VariationStore) with one charstring
/// and the given global/local subroutine INDEXes.
///
/// Layout (offsets fixed because every DICT operand is 5 bytes):
///
/// ```text
/// header | Top DICT | GlobalSubr INDEX | CharStrings INDEX |
/// FDArray INDEX | Private DICT | LocalSubr INDEX
/// ```
///
/// CharStrings must follow GlobalSubrs; LocalSubrs are reached through the
/// FDArray's Font DICT -> Private DICT and may sit last.
pub fn cff2_table(
    char_string: &[u8],
    global_subrs: &[Vec<u8>],
    local_subrs: &[Vec<u8>],
) -> Vec<u8> {
    const HEADER_LEN: usize = 5;
    // Private DICT body is fixed at 6 bytes to keep every layout offset known.
    const PRIVATE_DICT_LEN: usize = 6;
    const FONT_DICT_LEN: usize = 11;
    const TOP_DICT_LEN: usize = 13; // CharStrings entry 6 + FDArray entry 7 (12 36)
    // FDArray built with explicit offSize=2: u32 count + offSize + 2*u16 + body.
    const FD_ARRAY_LEN: usize = 4 + 1 + 4 + FONT_DICT_LEN;

    let global_index = index32(global_subrs);
    let local_index = index32(local_subrs);
    let chars_index = index32(&[char_string.to_vec()]);

    // First the offsets that do not depend on later tables.
    let global_subrs_offset = HEADER_LEN + TOP_DICT_LEN;
    let char_strings_offset = global_subrs_offset + global_index.len();
    let fd_array_offset = char_strings_offset + chars_index.len();
    let private_dict_offset = fd_array_offset + FD_ARRAY_LEN;
    let local_subrs_offset = private_dict_offset + PRIVATE_DICT_LEN;

    // --- Top DICT: CharStrings off (op 17), FDArray off (12 36). ---
    let mut top_dict = Vec::new();
    top_dict.extend_from_slice(&dict_int(char_strings_offset as i32));
    top_dict.push(17);
    top_dict.extend_from_slice(&dict_int(fd_array_offset as i32));
    top_dict.extend_from_slice(&[12, 36]);
    debug_assert_eq!(top_dict.len(), TOP_DICT_LEN);

    // --- Font DICT: Private size/off (op 18). ---
    let mut font_dict = Vec::new();
    font_dict.extend_from_slice(&dict_int(PRIVATE_DICT_LEN as i32));
    font_dict.extend_from_slice(&dict_int(private_dict_offset as i32));
    font_dict.push(18);
    debug_assert_eq!(font_dict.len(), FONT_DICT_LEN);
    let mut fd_array = Vec::new();
    fd_array.extend_from_slice(&1u32.to_be_bytes());
    fd_array.push(2); // offSize
    fd_array.extend_from_slice(&1u16.to_be_bytes());
    fd_array.extend_from_slice(&((1 + FONT_DICT_LEN) as u16).to_be_bytes());
    fd_array.extend_from_slice(&font_dict);
    debug_assert_eq!(fd_array.len(), FD_ARRAY_LEN);

    // --- Private DICT: LocalSubrs entry (op 19). Relative offset 0 means
    // "absent"; otherwise the LocalSubr INDEX immediately follows the DICT. ---
    let mut private_dict = if local_subrs.is_empty() {
        // No LocalSubrs entry at all. Fill the fixed slot with Private-DICT
        // operators that this parser does not act on (1/3 hstem/vstem), so the
        // DICT parses without naming a subroutine offset.
        vec![1u8, 3, 1, 3, 1, 3]
    } else {
        let mut d = dict_int(PRIVATE_DICT_LEN as i32); // LocalSubrs relative off
        d.push(19);
        d
    };
    let _ = &mut private_dict;
    debug_assert_eq!(private_dict.len(), PRIVATE_DICT_LEN);

    let mut table = Vec::new();
    table.extend_from_slice(&[2, 0, HEADER_LEN as u8]);
    table.extend_from_slice(&(TOP_DICT_LEN as u16).to_be_bytes());
    table.extend_from_slice(&top_dict);
    debug_assert_eq!(table.len(), global_subrs_offset);
    table.extend_from_slice(&global_index);
    debug_assert_eq!(table.len(), char_strings_offset);
    table.extend_from_slice(&chars_index);
    debug_assert_eq!(table.len(), fd_array_offset);
    table.extend_from_slice(&fd_array);
    debug_assert_eq!(table.len(), private_dict_offset);
    table.extend_from_slice(&private_dict);
    if !local_subrs.is_empty() {
        debug_assert_eq!(table.len(), local_subrs_offset);
        table.extend_from_slice(&local_index);
    }
    table
}
