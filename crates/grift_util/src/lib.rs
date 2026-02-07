//! Shared utilities for the Grift Scheme implementation.
//!
//! This crate provides common functions used by both the proc-macro crate
//! (`grift_macros`) and the rest of the workspace, avoiding duplication
//! of logic such as Scheme-name → Rust-name conversion.

/// Convert a Scheme-style name to PascalCase for use as a Rust enum variant.
///
/// # Conversion rules
///
/// | Scheme character | Rust equivalent |
/// |-----------------|-----------------|
/// | `-` or `_`      | capitalize next |
/// | `!`             | stripped        |
/// | `?`             | `P` (predicate) |
/// | `=`             | `Eq`            |
/// | `>`             | `Gt`            |
/// | `<`             | `Lt`            |
///
/// # Examples
///
/// ```
/// use grift_util::to_pascal_case;
///
/// assert_eq!(to_pascal_case("map"), "Map");
/// assert_eq!(to_pascal_case("set-car!"), "SetCar");
/// assert_eq!(to_pascal_case("null?"), "NullP");
/// assert_eq!(to_pascal_case("char->integer"), "CharGtInteger");
/// ```
pub fn to_pascal_case(name: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;

    for c in name.chars() {
        match c {
            '-' | '_' => {
                capitalize_next = true;
            }
            '!' => {
                // Skip exclamation marks
            }
            '?' => {
                // Replace with 'P' (predicate convention)
                result.push('P');
                capitalize_next = true;
            }
            '=' => {
                // Replace with 'Eq' for equality operators
                result.push_str("Eq");
                capitalize_next = true;
            }
            '>' => {
                // Replace with 'Gt' for greater-than
                result.push_str("Gt");
                capitalize_next = true;
            }
            '<' => {
                // Replace with 'Lt' for less-than
                result.push_str("Lt");
                capitalize_next = true;
            }
            _ => {
                if capitalize_next {
                    result.extend(c.to_uppercase());
                    capitalize_next = false;
                } else {
                    result.push(c);
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_names() {
        assert_eq!(to_pascal_case("map"), "Map");
        assert_eq!(to_pascal_case("car"), "Car");
        assert_eq!(to_pascal_case("cdr"), "Cdr");
        assert_eq!(to_pascal_case("cons"), "Cons");
        assert_eq!(to_pascal_case("list"), "List");
    }

    #[test]
    fn test_hyphenated_names() {
        assert_eq!(to_pascal_case("set-car!"), "SetCar");
        assert_eq!(to_pascal_case("set-cdr!"), "SetCdr");
        assert_eq!(to_pascal_case("make-vector"), "MakeVector");
        assert_eq!(to_pascal_case("vector-ref"), "VectorRef");
        assert_eq!(to_pascal_case("exact-integer?"), "ExactIntegerP");
    }

    #[test]
    fn test_predicates() {
        assert_eq!(to_pascal_case("null?"), "NullP");
        assert_eq!(to_pascal_case("pair?"), "PairP");
        assert_eq!(to_pascal_case("number?"), "NumberP");
        assert_eq!(to_pascal_case("zero?"), "ZeroP");
        assert_eq!(to_pascal_case("eq?"), "EqP");
    }

    #[test]
    fn test_operators() {
        // Single-character operators don't have letters, so to_pascal_case
        // strips or transforms them according to the rules:
        // '+' and '-' are letter-less, they become empty after rule application
        assert_eq!(to_pascal_case("+"), "+");
        assert_eq!(to_pascal_case("="), "Eq");
        assert_eq!(to_pascal_case(">"), "Gt");
        assert_eq!(to_pascal_case("<"), "Lt");
        assert_eq!(to_pascal_case(">="), "GtEq");
        assert_eq!(to_pascal_case("<="), "LtEq");
    }

    #[test]
    fn test_conversion_arrows() {
        assert_eq!(to_pascal_case("char->integer"), "CharGtInteger");
        assert_eq!(to_pascal_case("integer->char"), "IntegerGtChar");
        assert_eq!(to_pascal_case("vector->list"), "VectorGtList");
        assert_eq!(to_pascal_case("list->vector"), "ListGtVector");
    }

    #[test]
    fn test_exclamation_stripping() {
        assert_eq!(to_pascal_case("vector-set!"), "VectorSet");
        assert_eq!(to_pascal_case("vector-fill!"), "VectorFill");
        assert_eq!(to_pascal_case("string-set!"), "StringSet");
    }
}
