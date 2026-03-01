#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]
#![allow(
    clippy::must_use_candidate,
    clippy::doc_markdown,
    clippy::module_name_repetitions,
    clippy::cast_possible_wrap,
    clippy::match_same_arms,
    clippy::too_many_lines,
    clippy::doc_link_with_quotes
)]

//! # Grift Unicode
//!
//! Minimal Unicode character operations for Grift .
//!
//! Provides case mapping, case folding, and character property queries
//! without requiring `std` or `alloc`. Uses Rust's built-in Unicode-aware
//! `core::char` methods and a small static table for full case folding.

/// Maximum number of characters a single character can expand to under
/// any Unicode case mapping or folding operation.
const MAX_CASE_EXPANSION: usize = 3;

/// Code point value of ASCII digit '0'.
const ASCII_ZERO: u32 = '0' as u32;

/// Result of a full case mapping operation that may expand a single character
/// into up to 3 characters.
#[derive(Clone, Copy)]
pub struct CaseMapResult {
    chars: [char; MAX_CASE_EXPANSION],
    len: usize,
}

impl Default for CaseMapResult {
    #[inline]
    fn default() -> Self {
        CaseMapResult {
            chars: ['\0'; MAX_CASE_EXPANSION],
            len: 0,
        }
    }
}

impl CaseMapResult {
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
    if iter.next().is_some() { c } else { first }
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
    if iter.next().is_some() { c } else { first }
}

/// Return the simple case fold of a character.
///
/// Simple case folding maps each character to a single character,
/// used for case-insensitive comparisons. For R7RS, simple case
/// folding is equivalent to simple lowercasing.
#[inline]
pub fn char_foldcase(c: char) -> char {
    // Simple case folding: handles Unicode case fold mappings that differ
    // from simple lowercasing (CaseFolding.txt status 'C' and 'S').
    match c {
        '\u{017F}' => 's',        // LATIN SMALL LETTER LONG S
        '\u{0345}' => '\u{03B9}', // COMBINING GREEK YPOGEGRAMMENI
        '\u{03C2}' => '\u{03C3}', // GREEK SMALL LETTER FINAL SIGMA
        '\u{03D0}' => '\u{03B2}', // GREEK BETA SYMBOL
        '\u{03D1}' => '\u{03B8}', // GREEK THETA SYMBOL
        '\u{03D5}' => '\u{03C6}', // GREEK PHI SYMBOL
        '\u{03D6}' => '\u{03C0}', // GREEK PI SYMBOL
        '\u{03F0}' => '\u{03BA}', // GREEK KAPPA SYMBOL
        '\u{03F1}' => '\u{03C1}', // GREEK RHO SYMBOL
        '\u{03F5}' => '\u{03B5}', // GREEK LUNATE EPSILON SYMBOL
        '\u{1E9B}' => '\u{1E61}', // LATIN SMALL LETTER LONG S WITH DOT ABOVE
        _ => char_downcase(c),
    }
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
        return Some(c as u32 - ASCII_ZERO);
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
    if zero > 0
        && let Some(before_zero) = char::from_u32(zero - 1)
            && before_zero.is_numeric() {
                return None;
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
    if let Some(result) = lookup_full_casefold(c) { result } else {
        // Simple case fold: single character result
        let folded = char_foldcase(c);
        CaseMapResult {
            chars: [folded, '\0', '\0'],
            len: 1,
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

// Auto-generated lookup_full_casefold() from CaseFolding.txt via build.rs.
include!(concat!(env!("OUT_DIR"), "/casefold_generated.rs"));

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

    #[test]
    fn test_full_foldcase_armenian_ligatures() {
        // FB13: ARMENIAN SMALL LIGATURE MEN NOW → men + now
        let result = full_foldcase('\u{FB13}');
        assert_eq!(result.len(), 2);
        assert_eq!(result.get(0), Some('\u{0574}'));
        assert_eq!(result.get(1), Some('\u{0576}'));

        // FB14: ARMENIAN SMALL LIGATURE MEN ECH → men + ech
        let result = full_foldcase('\u{FB14}');
        assert_eq!(result.len(), 2);
        assert_eq!(result.get(0), Some('\u{0574}'));
        assert_eq!(result.get(1), Some('\u{0565}'));

        // FB15: ARMENIAN SMALL LIGATURE MEN INI → men + ini
        let result = full_foldcase('\u{FB15}');
        assert_eq!(result.len(), 2);
        assert_eq!(result.get(0), Some('\u{0574}'));
        assert_eq!(result.get(1), Some('\u{056B}'));

        // FB16: ARMENIAN SMALL LIGATURE VEW NOW → vew + now
        let result = full_foldcase('\u{FB16}');
        assert_eq!(result.len(), 2);
        assert_eq!(result.get(0), Some('\u{057E}'));
        assert_eq!(result.get(1), Some('\u{0576}'));

        // FB17: ARMENIAN SMALL LIGATURE MEN XEH → men + xeh
        let result = full_foldcase('\u{FB17}');
        assert_eq!(result.len(), 2);
        assert_eq!(result.get(0), Some('\u{0574}'));
        assert_eq!(result.get(1), Some('\u{056D}'));
    }

    #[test]
    fn test_full_foldcase_no_match() {
        // ASCII 'a' should not have a full casefold entry; falls back to simple
        let result = full_foldcase('a');
        assert_eq!(result.len(), 1);
        assert_eq!(result.first(), 'a');
    }

    #[test]
    fn test_cyrillic_case_mapping() {
        // Uppercase
        let result = full_upcase('а'); // Cyrillic A
        assert_eq!(result.len(), 1);
        assert_eq!(result.first(), 'А');

        let result = full_upcase('я'); // Cyrillic YA
        assert_eq!(result.len(), 1);
        assert_eq!(result.first(), 'Я');

        let result = full_upcase('ё'); // YO with diaeresis
        assert_eq!(result.len(), 1);
        assert_eq!(result.first(), 'Ё');

        // Lowercase
        let result = full_downcase('Б'); // Cyrillic BE
        assert_eq!(result.len(), 1);
        assert_eq!(result.first(), 'б');

        let result = full_downcase('Ж'); // Cyrillic ZHE
        assert_eq!(result.len(), 1);
        assert_eq!(result.first(), 'ж');

        // Case folding
        let result = full_foldcase('Щ'); // Cyrillic SHCHA
        assert_eq!(result.len(), 1);
        assert_eq!(result.first(), 'щ');
    }

    #[test]
    fn test_georgian_case_mapping() {
        // Georgian Mtavruli (U+1C90-U+1CBF) uppercase of Mkhedruli (U+10D0-U+10FF)
        let result = full_upcase('ა'); // Georgian AN
        assert_eq!(result.len(), 1);
        assert_eq!(result.first(), 'Ა');

        let result = full_upcase('ბ'); // Georgian BAN
        assert_eq!(result.len(), 1);
        assert_eq!(result.first(), 'Ბ');

        let result = full_downcase('Გ'); // Georgian GAN
        assert_eq!(result.len(), 1);
        assert_eq!(result.first(), 'გ');
    }

    #[test]
    fn test_unicode_whitespace() {
        // ASCII whitespace
        assert!(char_is_whitespace(' '));
        assert!(char_is_whitespace('\t'));
        assert!(char_is_whitespace('\n'));

        // Unicode whitespace
        assert!(char_is_whitespace('\u{00A0}')); // NO-BREAK SPACE
        assert!(char_is_whitespace('\u{2003}')); // EM SPACE
        assert!(char_is_whitespace('\u{3000}')); // IDEOGRAPHIC SPACE

        // Not whitespace
        assert!(!char_is_whitespace('a'));
        assert!(!char_is_whitespace('ა')); // Georgian
    }
}
