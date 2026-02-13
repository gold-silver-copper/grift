#![no_std]
#![forbid(unsafe_code)]

//! # Grift Unicode
//!
//! Minimal Unicode character operations for the Grift Scheme interpreter.
//!
//! Provides case mapping, case folding, and character property queries
//! without requiring `std` or `alloc`. Uses Rust's built-in Unicode-aware
//! `core::char` methods and a small static table for full case folding.

/// Maximum number of characters a single character can expand to under
/// any Unicode case mapping or folding operation.
const MAX_CASE_EXPANSION: usize = 3;

/// Result of a full case mapping operation that may expand a single character
/// into up to 3 characters.
#[derive(Clone, Copy)]
pub struct CaseMapResult {
    chars: [char; MAX_CASE_EXPANSION],
    len: usize,
}

impl CaseMapResult {
    /// Create an empty CaseMapResult.
    #[inline]
    pub fn default() -> Self {
        CaseMapResult {
            chars: ['\0'; MAX_CASE_EXPANSION],
            len: 0,
        }
    }

    /// Number of characters in the result.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the result is empty (should never be for valid input).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get the character at the given index.
    #[inline]
    pub fn get(&self, index: usize) -> Option<char> {
        if index < self.len {
            Some(self.chars[index])
        } else {
            None
        }
    }

    /// Get the first character (always present for valid case mappings).
    #[inline]
    pub fn first(&self) -> char {
        self.chars[0]
    }
}

// --- Simple case mapping (char → char) ---

/// Return the simple uppercase mapping of a character.
///
/// Maps a single character to a single uppercase character.
/// If the uppercase mapping expands to multiple characters (e.g., ß → SS),
/// the original character is returned unchanged (simple mapping only).
#[inline]
pub fn char_upcase(c: char) -> char {
    let mut iter = c.to_uppercase();
    let first = iter.next().unwrap_or(c);
    // If there's a second character, this is a full (expanding) mapping.
    // For simple mapping, return the original character unchanged.
    if iter.next().is_some() {
        c
    } else {
        first
    }
}

/// Return the simple lowercase mapping of a character.
///
/// Maps a single character to a single lowercase character.
/// If the lowercase mapping expands to multiple characters,
/// the original character is returned unchanged (simple mapping only).
#[inline]
pub fn char_downcase(c: char) -> char {
    let mut iter = c.to_lowercase();
    let first = iter.next().unwrap_or(c);
    if iter.next().is_some() {
        c
    } else {
        first
    }
}

/// Return the simple case fold of a character.
///
/// Simple case folding maps each character to a single character,
/// used for case-insensitive comparisons. For R7RS, simple case
/// folding is equivalent to simple lowercasing.
#[inline]
pub fn char_foldcase(c: char) -> char {
    char_downcase(c)
}

// --- Character property predicates ---

/// Check if a character has the Unicode Alphabetic property.
#[inline]
pub fn char_is_alphabetic(c: char) -> bool {
    c.is_alphabetic()
}

/// Check if a character has the Unicode White_Space property.
#[inline]
pub fn char_is_whitespace(c: char) -> bool {
    c.is_whitespace()
}

/// Check if a character has the Unicode Uppercase property.
#[inline]
pub fn char_is_uppercase(c: char) -> bool {
    c.is_uppercase()
}

/// Check if a character has the Unicode Lowercase property.
#[inline]
pub fn char_is_lowercase(c: char) -> bool {
    c.is_lowercase()
}

/// Check if a character is a Unicode decimal digit (General Category Nd).
///
/// This is stricter than `char::is_numeric()` which also includes
/// Nl (Number_Letter) and No (Number_Other) categories.
#[inline]
pub fn char_is_numeric(c: char) -> bool {
    digit_value(c).is_some()
}

/// Return the digit value of a Unicode decimal digit character.
///
/// Returns `Some(0..=9)` for characters in the Unicode Nd (Decimal_Number)
/// category, `None` for all other characters.
#[inline]
pub fn digit_value(c: char) -> Option<u32> {
    if c.is_ascii_digit() {
        return Some(c as u32 - '0' as u32);
    }
    digit_value_inner(c)
}

/// Compute digit value for non-ASCII numeric characters by walking back
/// to find the zero digit of the decimal digit block.
///
/// Unicode Nd (decimal digit) characters always appear in contiguous
/// blocks of exactly 10 characters (0-9) within each script.
fn digit_value_inner(c: char) -> Option<u32> {
    // Quick reject: char::is_numeric() is a superset of Nd
    if !c.is_numeric() {
        return None;
    }
    let cp = c as u32;
    let mut zero = cp;
    // Walk back at most 9 positions to find the '0' of this digit block.
    let mut count = 0u32;
    while count < 9 && zero > 0 {
        let prev = zero - 1;
        if let Some(prev_char) = char::from_u32(prev) {
            if prev_char.is_numeric() {
                zero = prev;
                count += 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    let val = cp - zero;
    if val > 9 {
        return None;
    }
    // Verify zero is actually the start of a 10-digit block
    // by checking that zero-1 is not numeric (if zero > 0)
    if zero > 0 {
        if let Some(before_zero) = char::from_u32(zero - 1) {
            if before_zero.is_numeric() {
                return None;
            }
        }
    }
    Some(val)
}

// --- Full case mapping (char → up to 3 chars) ---

/// Full uppercase mapping of a character.
///
/// May expand a single character into multiple characters.
/// For example, 'ß' → ['S', 'S'].
pub fn full_upcase(c: char) -> CaseMapResult {
    from_char_iter(c.to_uppercase(), c)
}

/// Full lowercase mapping of a character.
///
/// May expand a single character into multiple characters.
/// For example, 'İ' → ['i', '\u{0307}'].
pub fn full_downcase(c: char) -> CaseMapResult {
    from_char_iter(c.to_lowercase(), c)
}

/// Full case folding of a character.
///
/// Used for case-insensitive string comparisons. May expand a single
/// character into multiple characters. For example, 'ß' → ['s', 's'].
pub fn full_foldcase(c: char) -> CaseMapResult {
    // Check the static table for full case folding expansion entries.
    // These come from Unicode CaseFolding.txt (status 'F').
    match lookup_full_casefold(c) {
        Some(result) => result,
        None => {
            // Simple case fold: single character result
            let folded = char_foldcase(c);
            CaseMapResult {
                chars: [folded, '\0', '\0'],
                len: 1,
            }
        }
    }
}

/// Build a CaseMapResult from a core::char iterator (ToUppercase or ToLowercase).
fn from_char_iter<I: Iterator<Item = char>>(iter: I, original: char) -> CaseMapResult {
    let mut result = CaseMapResult {
        chars: ['\0'; MAX_CASE_EXPANSION],
        len: 0,
    };
    for c in iter {
        if result.len < MAX_CASE_EXPANSION {
            result.chars[result.len] = c;
            result.len += 1;
        }
    }
    if result.len == 0 {
        result.chars[0] = original;
        result.len = 1;
    }
    result
}

/// Lookup table for Unicode full case folding (CaseFolding.txt status 'F').
/// These are the characters that expand to multiple characters under full case folding.
fn lookup_full_casefold(c: char) -> Option<CaseMapResult> {
    // Data from Unicode 16.0 CaseFolding.txt, status 'F' entries.
    let (chars, len) = match c as u32 {
        0x00DF => (['s', 's', '\0'], 2),          // ß LATIN SMALL LETTER SHARP S
        0x0130 => (['i', '\u{0307}', '\0'], 2),   // İ LATIN CAPITAL LETTER I WITH DOT ABOVE
        0x0149 => (['\u{02BC}', 'n', '\0'], 2),   // ŉ LATIN SMALL LETTER N PRECEDED BY APOSTROPHE
        0x01F0 => (['j', '\u{030C}', '\0'], 2),   // ǰ LATIN SMALL LETTER J WITH CARON
        0x0390 => (['\u{03B9}', '\u{0308}', '\u{0301}'], 3), // ΐ
        0x03B0 => (['\u{03C5}', '\u{0308}', '\u{0301}'], 3), // ΰ
        0x0587 => (['\u{0565}', '\u{0582}', '\0'], 2), // և ARMENIAN SMALL LIGATURE ECH YIWN
        0x1E96 => (['h', '\u{0331}', '\0'], 2),   // ḫ
        0x1E97 => (['t', '\u{0308}', '\0'], 2),   // ẗ
        0x1E98 => (['w', '\u{030A}', '\0'], 2),   // ẘ
        0x1E99 => (['y', '\u{030A}', '\0'], 2),   // ẙ
        0x1E9A => (['a', '\u{02BE}', '\0'], 2),   // ẚ
        0x1E9E => (['s', 's', '\0'], 2),           // ẞ LATIN CAPITAL LETTER SHARP S
        0x1F50 => (['\u{03C5}', '\u{0313}', '\0'], 2), // ὐ
        0x1F52 => (['\u{03C5}', '\u{0313}', '\u{0300}'], 3), // ὒ
        0x1F54 => (['\u{03C5}', '\u{0313}', '\u{0301}'], 3), // ὔ
        0x1F56 => (['\u{03C5}', '\u{0313}', '\u{0342}'], 3), // ὖ
        0x1F80..=0x1F87 => {
            let base = '\u{1F00}' as u32 + (c as u32 - 0x1F80);
            ([char_from_u32_or(base, c), '\u{03B9}', '\0'], 2)
        }
        0x1F88..=0x1F8F => {
            let base = '\u{1F00}' as u32 + (c as u32 - 0x1F88);
            ([char_from_u32_or(base, c), '\u{03B9}', '\0'], 2)
        }
        0x1F90..=0x1F97 => {
            let base = '\u{1F20}' as u32 + (c as u32 - 0x1F90);
            ([char_from_u32_or(base, c), '\u{03B9}', '\0'], 2)
        }
        0x1F98..=0x1F9F => {
            let base = '\u{1F20}' as u32 + (c as u32 - 0x1F98);
            ([char_from_u32_or(base, c), '\u{03B9}', '\0'], 2)
        }
        0x1FA0..=0x1FA7 => {
            let base = '\u{1F60}' as u32 + (c as u32 - 0x1FA0);
            ([char_from_u32_or(base, c), '\u{03B9}', '\0'], 2)
        }
        0x1FA8..=0x1FAF => {
            let base = '\u{1F60}' as u32 + (c as u32 - 0x1FA8);
            ([char_from_u32_or(base, c), '\u{03B9}', '\0'], 2)
        }
        0x1FB2 => (['\u{1F70}', '\u{03B9}', '\0'], 2),
        0x1FB3 => (['\u{03B1}', '\u{03B9}', '\0'], 2),
        0x1FB4 => (['\u{03AC}', '\u{03B9}', '\0'], 2),
        0x1FB6 => (['\u{03B1}', '\u{0342}', '\0'], 2),
        0x1FB7 => (['\u{03B1}', '\u{0342}', '\u{03B9}'], 3),
        0x1FBC => (['\u{03B1}', '\u{03B9}', '\0'], 2),
        0x1FC2 => (['\u{1F74}', '\u{03B9}', '\0'], 2),
        0x1FC3 => (['\u{03B7}', '\u{03B9}', '\0'], 2),
        0x1FC4 => (['\u{03AE}', '\u{03B9}', '\0'], 2),
        0x1FC6 => (['\u{03B7}', '\u{0342}', '\0'], 2),
        0x1FC7 => (['\u{03B7}', '\u{0342}', '\u{03B9}'], 3),
        0x1FCC => (['\u{03B7}', '\u{03B9}', '\0'], 2),
        0x1FD2 => (['\u{03B9}', '\u{0308}', '\u{0300}'], 3),
        0x1FD3 => (['\u{03B9}', '\u{0308}', '\u{0301}'], 3),
        0x1FD6 => (['\u{03B9}', '\u{0342}', '\0'], 2),
        0x1FD7 => (['\u{03B9}', '\u{0308}', '\u{0342}'], 3),
        0x1FE2 => (['\u{03C5}', '\u{0308}', '\u{0300}'], 3),
        0x1FE3 => (['\u{03C5}', '\u{0308}', '\u{0301}'], 3),
        0x1FE4 => (['\u{03C1}', '\u{0313}', '\0'], 2),
        0x1FE6 => (['\u{03C5}', '\u{0342}', '\0'], 2),
        0x1FE7 => (['\u{03C5}', '\u{0308}', '\u{0342}'], 3),
        0x1FF2 => (['\u{1F7C}', '\u{03B9}', '\0'], 2),
        0x1FF3 => (['\u{03C9}', '\u{03B9}', '\0'], 2),
        0x1FF4 => (['\u{03CE}', '\u{03B9}', '\0'], 2),
        0x1FF6 => (['\u{03C9}', '\u{0342}', '\0'], 2),
        0x1FF7 => (['\u{03C9}', '\u{0342}', '\u{03B9}'], 3),
        0x1FFC => (['\u{03C9}', '\u{03B9}', '\0'], 2),
        0xFB00 => (['f', 'f', '\0'], 2),
        0xFB01 => (['f', 'i', '\0'], 2),
        0xFB02 => (['f', 'l', '\0'], 2),
        0xFB03 => (['f', 'f', 'i'], 3),
        0xFB04 => (['f', 'f', 'l'], 3),
        0xFB05 => (['s', 't', '\0'], 2),
        0xFB06 => (['s', 't', '\0'], 2),
        _ => return None,
    };
    Some(CaseMapResult { chars, len })
}

/// Helper to convert u32 to char with a fallback.
#[inline]
fn char_from_u32_or(cp: u32, fallback: char) -> char {
    char::from_u32(cp).unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_char_upcase_ascii() {
        assert_eq!(char_upcase('a'), 'A');
        assert_eq!(char_upcase('z'), 'Z');
        assert_eq!(char_upcase('A'), 'A');
        assert_eq!(char_upcase('0'), '0');
    }

    #[test]
    fn test_char_upcase_unicode() {
        assert_eq!(char_upcase('é'), 'É');
        assert_eq!(char_upcase('ω'), 'Ω');
        assert_eq!(char_upcase('ß'), 'ß'); // No simple uppercase for ß
    }

    #[test]
    fn test_char_downcase_ascii() {
        assert_eq!(char_downcase('A'), 'a');
        assert_eq!(char_downcase('Z'), 'z');
        assert_eq!(char_downcase('a'), 'a');
    }

    #[test]
    fn test_char_downcase_unicode() {
        assert_eq!(char_downcase('É'), 'é');
        assert_eq!(char_downcase('Ω'), 'ω');
    }

    #[test]
    fn test_char_foldcase() {
        assert_eq!(char_foldcase('A'), 'a');
        assert_eq!(char_foldcase('a'), 'a');
        assert_eq!(char_foldcase('É'), 'é');
        assert_eq!(char_foldcase('ß'), 'ß'); // Simple fold doesn't change ß
    }

    #[test]
    fn test_char_is_alphabetic() {
        assert!(char_is_alphabetic('a'));
        assert!(char_is_alphabetic('é'));
        assert!(char_is_alphabetic('中'));
        assert!(!char_is_alphabetic('0'));
        assert!(!char_is_alphabetic(' '));
    }

    #[test]
    fn test_char_is_whitespace() {
        assert!(char_is_whitespace(' '));
        assert!(char_is_whitespace('\t'));
        assert!(char_is_whitespace('\n'));
        assert!(!char_is_whitespace('a'));
    }

    #[test]
    fn test_char_is_uppercase() {
        assert!(char_is_uppercase('A'));
        assert!(char_is_uppercase('É'));
        assert!(!char_is_uppercase('a'));
        assert!(!char_is_uppercase('0'));
    }

    #[test]
    fn test_char_is_lowercase() {
        assert!(char_is_lowercase('a'));
        assert!(char_is_lowercase('é'));
        assert!(!char_is_lowercase('A'));
        assert!(!char_is_lowercase('0'));
    }

    #[test]
    fn test_char_is_numeric() {
        assert!(char_is_numeric('0'));
        assert!(char_is_numeric('5'));
        assert!(char_is_numeric('9'));
        assert!(!char_is_numeric('a'));
        assert!(!char_is_numeric(' '));
    }

    #[test]
    fn test_char_is_numeric_unicode() {
        assert!(char_is_numeric('\u{0660}')); // Arabic-Indic ٠
        assert!(char_is_numeric('\u{0663}')); // Arabic-Indic ٣
        assert!(char_is_numeric('\u{0966}')); // Devanagari ०
    }

    #[test]
    fn test_digit_value() {
        assert_eq!(digit_value('0'), Some(0));
        assert_eq!(digit_value('5'), Some(5));
        assert_eq!(digit_value('9'), Some(9));
        assert_eq!(digit_value('a'), None);
    }

    #[test]
    fn test_digit_value_unicode() {
        assert_eq!(digit_value('\u{0660}'), Some(0));
        assert_eq!(digit_value('\u{0663}'), Some(3));
        assert_eq!(digit_value('\u{0669}'), Some(9));
    }

    #[test]
    fn test_full_upcase() {
        let result = full_upcase('a');
        assert_eq!(result.len(), 1);
        assert_eq!(result.first(), 'A');

        let result = full_upcase('ß');
        assert_eq!(result.len(), 2);
        assert_eq!(result.get(0), Some('S'));
        assert_eq!(result.get(1), Some('S'));
    }

    #[test]
    fn test_full_downcase() {
        let result = full_downcase('A');
        assert_eq!(result.len(), 1);
        assert_eq!(result.first(), 'a');

        let result = full_downcase('a');
        assert_eq!(result.len(), 1);
        assert_eq!(result.first(), 'a');
    }

    #[test]
    fn test_full_foldcase() {
        let result = full_foldcase('A');
        assert_eq!(result.len(), 1);
        assert_eq!(result.first(), 'a');

        let result = full_foldcase('ß');
        assert_eq!(result.len(), 2);
        assert_eq!(result.get(0), Some('s'));
        assert_eq!(result.get(1), Some('s'));

        let result = full_foldcase('\u{FB01}');
        assert_eq!(result.len(), 2);
        assert_eq!(result.get(0), Some('f'));
        assert_eq!(result.get(1), Some('i'));
    }

    #[test]
    fn test_full_foldcase_capital_sharp_s() {
        let result = full_foldcase('\u{1E9E}');
        assert_eq!(result.len(), 2);
        assert_eq!(result.get(0), Some('s'));
        assert_eq!(result.get(1), Some('s'));
    }
}
