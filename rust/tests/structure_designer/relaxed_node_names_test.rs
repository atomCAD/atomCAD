//! Integration tests for `doc/design_relaxed_node_names.md`.
//!
//! Covers:
//! - Lexer: backtick-quoted identifier tokenization (incl. error cases).
//! - Parser: quoted form accepted in every identifier position.
//! - `needs_quoting` predicate: bare-safe vs. needs-quoting cases.
//! - Serializer: emits quoted form when needed, bare otherwise.
//! - Round-trip: networks with relaxed names parse back to structural equality.
//! - Round-trip stability: bare-only networks produce byte-identical output.
//! - Validator integration with `add_node_network_with_undo`,
//!   `rename_node_network`, and `factor_selection_into_subnetwork`.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::{
    Lexer, Parser, PropertyValue, Statement, Token, edit_network, serialize_network,
};

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn edit_designer_network(
    designer: &mut StructureDesigner,
    network_name: &str,
    code: &str,
    replace: bool,
) -> rust_lib_flutter_cad::structure_designer::text_format::EditResult {
    let mut network = designer
        .node_type_registry
        .node_networks
        .remove(network_name)
        .unwrap();
    let result = edit_network(&mut network, &designer.node_type_registry, code, replace);
    designer
        .node_type_registry
        .node_networks
        .insert(network_name.to_string(), network);
    result
}

// ----- Lexer -----

#[test]
fn lexer_emits_quoted_identifier() {
    let tokens = Lexer::tokenize("`foo bar`").unwrap();
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].token, Token::Identifier("foo bar".to_string()));
    assert_eq!(tokens[1].token, Token::Eof);
}

#[test]
fn lexer_quoted_identifier_admits_reserved_chars() {
    let tokens = Lexer::tokenize("`lib.x_rect▭□▯{100}_positive`").unwrap();
    assert_eq!(tokens.len(), 2);
    assert_eq!(
        tokens[0].token,
        Token::Identifier("lib.x_rect▭□▯{100}_positive".to_string())
    );
}

#[test]
fn lexer_unterminated_quoted_identifier_errors() {
    let err = Lexer::tokenize("`foo").unwrap_err();
    assert!(err.message.contains("Unterminated"));
}

#[test]
fn lexer_empty_quoted_identifier_errors() {
    let err = Lexer::tokenize("``").unwrap_err();
    assert!(err.message.contains("Empty"));
}

#[test]
fn lexer_quoted_form_bypasses_keyword_path() {
    // Bare `output` lexes to Token::Output; quoted `` `output` `` must lex to
    // Token::Identifier("output") so it can be used as a node name.
    let bare = Lexer::tokenize("output").unwrap();
    assert_eq!(bare[0].token, Token::Output);

    let quoted = Lexer::tokenize("`output`").unwrap();
    assert_eq!(quoted[0].token, Token::Identifier("output".to_string()));
}

// ----- needs_quoting predicate -----

#[test]
fn needs_quoting_bare_safe_names() {
    assert!(!Parser::needs_quoting("foo"));
    assert!(!Parser::needs_quoting("foo_bar_42"));
    assert!(!Parser::needs_quoting("_underscore"));
    assert!(!Parser::needs_quoting("CamelCase"));
}

#[test]
fn needs_quoting_keyword_collisions() {
    for kw in ["true", "false", "output", "delete", "description", "summary"] {
        assert!(
            Parser::needs_quoting(kw),
            "expected `{}` to require quoting",
            kw
        );
    }
}

#[test]
fn needs_quoting_leading_digit() {
    assert!(Parser::needs_quoting("123abc"));
    assert!(Parser::needs_quoting("9"));
}

#[test]
fn needs_quoting_reserved_chars() {
    for s in [
        "a.b", "a,b", "a:b", "a=b", "a{b", "a}b", "a[b", "a]b", "a(b", "a)b", "a@b", "a#b", "a b",
        "a\"b",
    ] {
        assert!(
            Parser::needs_quoting(s),
            "expected `{}` to require quoting",
            s
        );
    }
}

#[test]
fn needs_quoting_relaxed_names() {
    assert!(Parser::needs_quoting("lib.x_rect▭□▯{100}_positive"));
    assert!(Parser::needs_quoting("lib.hexirod_[0001]30°"));
}

#[test]
fn needs_quoting_empty_and_backtick_starts() {
    assert!(Parser::needs_quoting(""));
    assert!(Parser::needs_quoting("`abc`"));
}

// ----- Parser -----

#[test]
fn parser_accepts_quoted_lhs() {
    let stmts = Parser::parse("`weird name` = sphere { radius: 5 }").unwrap();
    match &stmts[0] {
        Statement::Assignment {
            name, node_type, ..
        } => {
            assert_eq!(name, "weird name");
            assert_eq!(node_type, "sphere");
        }
        _ => panic!("expected assignment"),
    }
}

#[test]
fn parser_accepts_quoted_node_type() {
    let stmts = Parser::parse("foo = `lib.MyType` {}").unwrap();
    match &stmts[0] {
        Statement::Assignment {
            name, node_type, ..
        } => {
            assert_eq!(name, "foo");
            assert_eq!(node_type, "lib.MyType");
        }
        _ => panic!("expected assignment"),
    }
}

#[test]
fn parser_accepts_quoted_reference_with_pin() {
    let stmts = Parser::parse("u = union { a: `lib.x_rect`.diff }").unwrap();
    match &stmts[0] {
        Statement::Assignment { properties, .. } => {
            let (key, val) = &properties[0];
            assert_eq!(key, "a");
            match val {
                PropertyValue::NodeRef(name, pin) => {
                    assert_eq!(name, "lib.x_rect");
                    assert_eq!(pin.as_deref(), Some("diff"));
                }
                _ => panic!("expected NodeRef"),
            }
        }
        _ => panic!("expected assignment"),
    }
}

#[test]
fn parser_accepts_quoted_function_ref() {
    let stmts = Parser::parse("u = call { f: @`my.network` }").unwrap();
    match &stmts[0] {
        Statement::Assignment { properties, .. } => {
            let (key, val) = &properties[0];
            assert_eq!(key, "f");
            match val {
                PropertyValue::FunctionRef(name) => assert_eq!(name, "my.network"),
                _ => panic!("expected FunctionRef"),
            }
        }
        _ => panic!("expected assignment"),
    }
}

#[test]
fn parser_accepts_quoted_output_and_delete() {
    let stmts = Parser::parse("output `lib.x_rect`\ndelete `lib.x_rect`").unwrap();
    match &stmts[0] {
        Statement::Output { node_name } => assert_eq!(node_name, "lib.x_rect"),
        _ => panic!("expected output"),
    }
    match &stmts[1] {
        Statement::Delete { node_name } => assert_eq!(node_name, "lib.x_rect"),
        _ => panic!("expected delete"),
    }
}

// ----- Serializer / round-trip -----

#[test]
fn serializer_emits_bare_form_for_bare_safe_names() {
    let mut designer = setup_designer_with_network("net");
    let result = edit_designer_network(
        &mut designer,
        "net",
        "mybox = cuboid { min_corner: (0, 0, 0), extent: (10, 10, 10) }\noutput mybox",
        true,
    );
    assert!(result.success, "edit failed: {:?}", result.errors);

    let network = designer.node_type_registry.node_networks.get("net").unwrap();
    let text = serialize_network(network, &designer.node_type_registry, None);
    assert!(text.contains("mybox = cuboid"));
    assert!(text.contains("output mybox"));
    // Bare-safe names must NOT be wrapped in backticks.
    assert!(!text.contains("`mybox`"));
}

#[test]
fn serializer_emits_quoted_form_for_relaxed_names() {
    let mut designer = setup_designer_with_network("net");
    let result = edit_designer_network(
        &mut designer,
        "net",
        "`weird name` = cuboid { min_corner: (0, 0, 0), extent: (10, 10, 10) }\noutput `weird name`",
        true,
    );
    assert!(result.success, "edit failed: {:?}", result.errors);

    let network = designer.node_type_registry.node_networks.get("net").unwrap();
    let text = serialize_network(network, &designer.node_type_registry, None);
    assert!(text.contains("`weird name` = cuboid"));
    assert!(text.contains("output `weird name`"));
}

#[test]
fn roundtrip_preserves_relaxed_name() {
    let mut designer = setup_designer_with_network("net");
    let code = "`x.shape` = cuboid { min_corner: (0, 0, 0), extent: (1, 2, 3) }\n\
                `union#1` = union { shapes: [`x.shape`] }\n\
                output `union#1`";
    let result = edit_designer_network(&mut designer, "net", code, true);
    assert!(result.success, "edit failed: {:?}", result.errors);

    let network = designer.node_type_registry.node_networks.get("net").unwrap();
    let serialized = serialize_network(network, &designer.node_type_registry, None);

    // Re-parse the serialized text into a fresh network and verify the same
    // node names are present.
    let mut designer2 = setup_designer_with_network("net2");
    let result2 = edit_designer_network(&mut designer2, "net2", &serialized, true);
    assert!(
        result2.success,
        "roundtrip parse failed: {:?}",
        result2.errors
    );

    assert!(designer2.find_node_id_by_name("x.shape").is_some());
    assert!(designer2.find_node_id_by_name("union#1").is_some());
}

#[test]
fn roundtrip_byte_stable_for_bare_names() {
    let mut designer = setup_designer_with_network("net");
    let code = "a = cuboid { min_corner: (0, 0, 0), extent: (1, 2, 3) }\n\
                b = sphere { center: (0, 0, 0), radius: 1.0 }\n\
                u = union { shapes: [a, b] }\n\
                output u";
    let result = edit_designer_network(&mut designer, "net", code, true);
    assert!(result.success, "edit failed: {:?}", result.errors);

    let network = designer.node_type_registry.node_networks.get("net").unwrap();
    let first = serialize_network(network, &designer.node_type_registry, None);

    let mut designer2 = setup_designer_with_network("net");
    let result2 = edit_designer_network(&mut designer2, "net", &first, true);
    assert!(
        result2.success,
        "roundtrip parse failed: {:?}",
        result2.errors
    );
    let network2 = designer2.node_type_registry.node_networks.get("net").unwrap();
    let second = serialize_network(network2, &designer2.node_type_registry, None);

    assert_eq!(first, second, "bare-only round-trip should be byte-stable");
}

#[test]
fn editor_rejects_invalid_node_name() {
    let mut designer = setup_designer_with_network("net");
    // A leading-space name is rejected by the validator.
    let result = edit_designer_network(
        &mut designer,
        "net",
        "` foo` = cuboid { min_corner: (0, 0, 0), extent: (1, 1, 1) }",
        true,
    );
    assert!(!result.success);
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.contains("Invalid node name")),
        "expected validator error, got {:?}",
        result.errors
    );
}

// ----- Validator integration in StructureDesigner -----

#[test]
fn add_node_network_with_undo_rejects_invalid_name() {
    let mut designer = StructureDesigner::new();
    let res = designer.add_node_network_with_undo("");
    assert!(res.is_err());

    let res = designer.add_node_network_with_undo("has`backtick");
    assert!(res.is_err());

    let res = designer.add_node_network_with_undo(" leading_space");
    assert!(res.is_err());

    let res = designer.add_node_network_with_undo("legit_name");
    assert!(res.is_ok());
}

#[test]
fn rename_node_network_rejects_invalid_name() {
    let mut designer = setup_designer_with_network("source");
    assert!(!designer.rename_node_network("source", ""));
    assert!(!designer.rename_node_network("source", "bad`name"));
    assert!(!designer.rename_node_network("source", " leading"));
    // Sanity: a relaxed-but-valid name is accepted.
    assert!(designer.rename_node_network("source", "lib.relaxed name"));
}

#[test]
fn factor_selection_rejects_invalid_subnetwork_name() {
    let mut designer = setup_designer_with_network("net");
    let id = designer.add_node("sphere", DVec2::ZERO);
    designer.select_nodes(vec![id]);
    let err = designer
        .factor_selection_into_subnetwork("bad`name", vec![])
        .unwrap_err();
    assert!(err.contains("Invalid subnetwork name"));
}
