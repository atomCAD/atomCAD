use rust_lib_flutter_cad::structure_designer::identifier::{
    InvalidNameReason, is_valid_user_name,
};

#[test]
fn empty_name_is_rejected() {
    assert_eq!(is_valid_user_name(""), Err(InvalidNameReason::Empty));
}

#[test]
fn backtick_anywhere_is_rejected() {
    assert_eq!(
        is_valid_user_name("`"),
        Err(InvalidNameReason::ContainsBacktick)
    );
    assert_eq!(
        is_valid_user_name("foo`bar"),
        Err(InvalidNameReason::ContainsBacktick)
    );
    assert_eq!(
        is_valid_user_name("`foo"),
        Err(InvalidNameReason::ContainsBacktick)
    );
    assert_eq!(
        is_valid_user_name("foo`"),
        Err(InvalidNameReason::ContainsBacktick)
    );
}

#[test]
fn control_chars_are_rejected() {
    assert_eq!(
        is_valid_user_name("foo\nbar"),
        Err(InvalidNameReason::ContainsControl)
    );
    assert_eq!(
        is_valid_user_name("foo\tbar"),
        Err(InvalidNameReason::ContainsControl)
    );
    assert_eq!(
        is_valid_user_name("foo\rbar"),
        Err(InvalidNameReason::ContainsControl)
    );
    assert_eq!(
        is_valid_user_name("foo\x00bar"),
        Err(InvalidNameReason::ContainsControl)
    );
}

#[test]
fn leading_or_trailing_whitespace_is_rejected() {
    assert_eq!(
        is_valid_user_name(" foo"),
        Err(InvalidNameReason::EdgeWhitespace)
    );
    assert_eq!(
        is_valid_user_name("foo "),
        Err(InvalidNameReason::EdgeWhitespace)
    );
    assert_eq!(
        is_valid_user_name(" "),
        Err(InvalidNameReason::EdgeWhitespace)
    );
    assert_eq!(
        is_valid_user_name("\u{00A0}foo"),
        Err(InvalidNameReason::EdgeWhitespace)
    );
}

#[test]
fn simple_identifiers_are_accepted() {
    assert!(is_valid_user_name("foo").is_ok());
    assert!(is_valid_user_name("foo_bar_42").is_ok());
    assert!(is_valid_user_name("_underscore").is_ok());
    assert!(is_valid_user_name("CamelCase").is_ok());
}

#[test]
fn relaxed_names_are_accepted() {
    assert!(is_valid_user_name("lib.x_rect▭□▯{100}_positive").is_ok());
    assert!(is_valid_user_name("lib.hexirod_[0001]30°").is_ok());
    assert!(is_valid_user_name("name with spaces").is_ok());
    assert!(is_valid_user_name("name(with)parens").is_ok());
    assert!(is_valid_user_name("123starts_with_digit").is_ok());
    assert!(is_valid_user_name("a=b").is_ok());
    assert!(is_valid_user_name("=").is_ok());
    assert!(is_valid_user_name("name.with.dots").is_ok());
    assert!(is_valid_user_name("name,with,commas").is_ok());
}

#[test]
fn long_unicode_strings_are_accepted() {
    let s: String = std::iter::repeat('Ω').take(200).collect();
    assert!(is_valid_user_name(&s).is_ok());
}

#[test]
fn internal_whitespace_is_allowed() {
    assert!(is_valid_user_name("a b").is_ok());
    assert!(is_valid_user_name("part one part two").is_ok());
}

#[test]
fn display_messages_are_distinct() {
    let messages = [
        InvalidNameReason::Empty.to_string(),
        InvalidNameReason::ContainsBacktick.to_string(),
        InvalidNameReason::ContainsControl.to_string(),
        InvalidNameReason::EdgeWhitespace.to_string(),
    ];
    let unique: std::collections::HashSet<_> = messages.iter().collect();
    assert_eq!(unique.len(), messages.len());
}
