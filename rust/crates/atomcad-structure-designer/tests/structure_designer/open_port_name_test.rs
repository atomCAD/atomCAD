//! The name an open-port drop gives a new input
//! (`doc/design_wire_declared_inputs.md`, "Name derivation"). A pure function,
//! so it is tested as a table with no network.

use atomcad_structure_designer::declared_inputs::{NameSource, derive_open_port_name};
use std::collections::HashSet;

const RESERVED: &[&str] = &[
    "if",
    "then",
    "else",
    "true",
    "false",
    "expression",
    "parameters",
];

fn derive(node: Option<&str>, pin: Option<&str>, prefer_pin: bool, taken: &[&str]) -> String {
    let taken: HashSet<String> = taken.iter().map(|s| s.to_string()).collect();
    derive_open_port_name(
        &NameSource {
            node_name: node,
            pin_name: pin,
            prefer_pin,
        },
        &taken,
        RESERVED,
        "x",
    )
}

#[test]
fn a_single_output_source_gives_its_node_name() {
    assert_eq!(derive(Some("gap"), Some("result"), false, &[]), "gap");
}

#[test]
fn a_multi_output_source_gives_its_pin_name() {
    assert_eq!(derive(Some("unpack1"), Some("a"), true, &[]), "a");
}

#[test]
fn a_taken_pin_name_becomes_node_underscore_pin() {
    assert_eq!(
        derive(Some("unpack1"), Some("a"), true, &["a"]),
        "unpack1_a"
    );
}

#[test]
fn a_zone_input_gives_its_pin_name_then_the_hof_prefix() {
    assert_eq!(derive(Some("map1"), Some("element"), true, &[]), "element");
    assert_eq!(
        derive(Some("map1"), Some("element"), true, &["element"]),
        "map1_element"
    );
}

#[test]
fn a_function_pin_gives_its_node_name() {
    // The orchestrator passes no pin name and `prefer_pin: false` for `-1`.
    assert_eq!(derive(Some("sphere1"), None, false, &[]), "sphere1");
}

#[test]
fn a_relaxed_node_name_is_sanitized() {
    assert_eq!(
        derive(Some("hexirod_[0001]30°"), Some("result"), false, &[]),
        "hexirod_0001_30"
    );
    assert_eq!(derive(Some("a  b--c"), None, false, &[]), "a_b_c");
}

#[test]
fn a_leading_digit_gets_an_underscore() {
    assert_eq!(derive(Some("3d_cut"), None, false, &[]), "_3d_cut");
}

#[test]
fn a_reserved_word_gets_an_underscore_appended() {
    assert_eq!(derive(Some("if"), None, false, &[]), "if_");
    assert_eq!(derive(Some("expression"), None, false, &[]), "expression_");
}

#[test]
fn a_taken_name_is_deduplicated_with_numeric_suffixes() {
    assert_eq!(derive(Some("gap"), None, false, &["gap"]), "gap_2");
    assert_eq!(derive(Some("gap"), None, false, &["gap", "gap_2"]), "gap_3");
}

#[test]
fn nothing_usable_gives_the_fallback() {
    assert_eq!(derive(None, None, false, &[]), "x");
    assert_eq!(derive(Some("°°°"), None, false, &[]), "x");
    assert_eq!(derive(None, None, false, &["x"]), "x_2");
}

#[test]
fn case_is_kept() {
    assert_eq!(derive(Some("MyNode"), None, false, &[]), "MyNode");
}
