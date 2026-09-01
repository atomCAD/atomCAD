//! Phase 2 of `doc/design_wire_annotations.md` (issue #427) — comment anchors
//! in the text format: the `->` token and `PropertyValue::WireRef`, the
//! serializer's fourth pass, and the two `network_editor` special cases.
//!
//! The scenario the whole phase exists for is
//! [`anchors_survive_a_serialize_edit_round_trip`]: an AI edit reads the
//! network as text and writes the whole thing back, so an anchor that does not
//! survive that trip is an anchor the AI silently deletes on its next edit.
//!
//! Note the text format projects **one network** — it has no syntax for a HOF
//! zone body, and replace mode rebuilds the network from the text alone. So
//! there is nothing here about anchors inside a body: an anchor can be
//! authored in a body (Phase 1 covers that), just not through this surface.

use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::node_data::NoData;
use atomcad_structure_designer::node_network::NodeNetwork;
use atomcad_structure_designer::node_type::NodeTypeCategory;
use atomcad_structure_designer::node_type::{
    NodeType, OutputPinDefinition, no_data_loader, no_data_saver,
};
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::comment::{CommentAnchor, CommentData};
use atomcad_structure_designer::text_format::{
    EditResult, Lexer, Parser, PropertyValue, Statement, Token, edit_network, serialize_network,
};

// ============================================================================
// Helpers
// ============================================================================

/// A standalone network (not owned by the registry) so `edit_network` can take
/// `&mut network` and `&registry` at once — the shape the other text-format
/// tests use.
fn standalone_network() -> NodeNetwork {
    NodeNetwork::new(NodeType {
        name: "test".to_string(),
        description: String::new(),
        summary: None,
        category: NodeTypeCategory::Custom,
        parameters: vec![],
        output_pins: OutputPinDefinition::single(DataType::Blueprint),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(NoData {}),
        node_data_saver: no_data_saver,
        node_data_loader: no_data_loader,
    })
}

/// Author `code` into a fresh network, requiring the edit to succeed.
fn author(code: &str) -> (NodeNetwork, NodeTypeRegistry, EditResult) {
    let registry = NodeTypeRegistry::new();
    let mut network = standalone_network();
    let result = edit_network(&mut network, &registry, code, true);
    assert!(
        result.success,
        "authoring must succeed: {:?}",
        result.errors
    );
    (network, registry, result)
}

fn node_id(network: &NodeNetwork, name: &str) -> u64 {
    network
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node named '{}'", name))
        .id
}

fn anchors_of(network: &NodeNetwork, comment_name: &str) -> Vec<CommentAnchor> {
    network
        .nodes
        .get(&node_id(network, comment_name))
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<CommentData>()
        .expect("node must be a comment")
        .anchors
        .clone()
}

/// The `on:` property as the serializer wrote it, or `None` when it wrote none.
fn serialized_on(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    comment_name: &str,
) -> Option<String> {
    let text = serialize_network(network, registry, None);
    let line = text
        .lines()
        .find(|l| l.starts_with(&format!("{} = ", comment_name)))
        .unwrap_or_else(|| panic!("no line for '{}' in:\n{}", comment_name, text))
        .to_string();
    let start = line.find("on: ")? + "on: ".len();
    let rest = &line[start..];
    // The `on:` property is emitted last, so it runs to the closing brace.
    Some(rest.trim_end().trim_end_matches('}').trim_end().to_string())
}

fn assert_wire_anchor(anchor: &CommentAnchor, source: u64, dest: u64, arg_index: usize) {
    match anchor {
        CommentAnchor::Wire(w) => {
            assert_eq!(w.source_node_id, source, "wire anchor source");
            assert_eq!(w.destination_node_id, dest, "wire anchor destination");
            assert_eq!(
                w.destination_argument_index, arg_index,
                "wire anchor slot index"
            );
        }
        other => panic!("expected a wire anchor, got {:?}", other),
    }
}

/// `a -> vec3.y`, plus a comment anchored to that wire.
const WIRE_ANCHORED: &str = r#"
a = float { value: 1.5 }
v = vec3 { y: a }
note = Comment { label: "", text: "the y source", width: 200, height: 100, on: a -> v.y }
"#;

// ============================================================================
// Lexing and parsing
// ============================================================================

#[test]
fn arrow_lexes_as_its_own_token() {
    let tokens: Vec<Token> = Lexer::tokenize("a -> b.c")
        .unwrap()
        .into_iter()
        .map(|t| t.token)
        .collect();
    assert_eq!(
        tokens,
        vec![
            Token::Identifier("a".to_string()),
            Token::Arrow,
            Token::Identifier("b".to_string()),
            Token::Dot,
            Token::Identifier("c".to_string()),
            Token::Eof,
        ]
    );
}

#[test]
fn negative_numbers_still_lex_after_the_arrow_rule() {
    // The arrow arm claims a `-`, so the number arm has to keep claiming the
    // ones it always did.
    let tokens: Vec<Token> = Lexer::tokenize("-5 -1.5 -> x")
        .unwrap()
        .into_iter()
        .map(|t| t.token)
        .collect();
    assert_eq!(
        tokens,
        vec![
            Token::Int(-5),
            Token::Float(-1.5),
            Token::Arrow,
            Token::Identifier("x".to_string()),
            Token::Eof,
        ]
    );
}

#[test]
fn wire_ref_parses_with_and_without_a_source_pin() {
    let stmts = Parser::parse("n = Comment { on: mybox -> union.a }").unwrap();
    match &stmts[0] {
        Statement::Assignment { properties, .. } => match &properties[0].1 {
            PropertyValue::WireRef {
                source,
                source_pin,
                dest,
                dest_param,
                ..
            } => {
                assert_eq!(source, "mybox");
                assert_eq!(source_pin, &None);
                assert_eq!(dest, "union");
                assert_eq!(dest_param, "a");
            }
            other => panic!("expected a wire ref, got {:?}", other),
        },
        other => panic!("expected an assignment, got {:?}", other),
    }

    let stmts = Parser::parse("n = Comment { on: edit1.diff -> applied.diff }").unwrap();
    match &stmts[0] {
        Statement::Assignment { properties, .. } => match &properties[0].1 {
            PropertyValue::WireRef {
                source, source_pin, ..
            } => {
                assert_eq!(source, "edit1");
                assert_eq!(source_pin.as_deref(), Some("diff"));
            }
            other => panic!("expected a wire ref, got {:?}", other),
        },
        other => panic!("expected an assignment, got {:?}", other),
    }
}

#[test]
fn a_bare_function_ref_is_still_a_function_ref() {
    // The `@name` branch now peeks for an arrow; the un-arrowed form must be
    // untouched.
    let stmts = Parser::parse("n = map { f: @pattern }").unwrap();
    match &stmts[0] {
        Statement::Assignment { properties, .. } => {
            assert!(matches!(&properties[0].1, PropertyValue::FunctionRef(s) if s == "pattern"));
        }
        other => panic!("expected an assignment, got {:?}", other),
    }
}

// ============================================================================
// Authoring and serializing
// ============================================================================

#[test]
fn node_anchor_round_trips() {
    let (network, registry, _) = author(
        r#"
        a = float { value: 1.0 }
        note = Comment { text: "about a", on: a }
        "#,
    );

    let anchors = anchors_of(&network, "note");
    assert_eq!(anchors, vec![CommentAnchor::Node(node_id(&network, "a"))]);
    assert_eq!(
        serialized_on(&network, &registry, "note").as_deref(),
        Some("a")
    );
}

#[test]
fn wire_anchor_round_trips() {
    let (network, registry, _) = author(WIRE_ANCHORED);

    let anchors = anchors_of(&network, "note");
    assert_eq!(anchors.len(), 1);
    assert_wire_anchor(
        &anchors[0],
        node_id(&network, "a"),
        node_id(&network, "v"),
        1, // vec3's parameters are [x, y, z]
    );
    assert_eq!(
        serialized_on(&network, &registry, "note").as_deref(),
        Some("a -> v.y")
    );
}

#[test]
fn the_array_form_carries_a_mix_of_node_and_wire_anchors() {
    let (network, registry, _) = author(
        r#"
        a = float { value: 1.0 }
        v = vec3 { y: a }
        note = Comment { text: "both", on: [a, a -> v.y] }
        "#,
    );

    let anchors = anchors_of(&network, "note");
    assert_eq!(anchors.len(), 2);
    assert_eq!(anchors[0], CommentAnchor::Node(node_id(&network, "a")));
    assert_wire_anchor(
        &anchors[1],
        node_id(&network, "a"),
        node_id(&network, "v"),
        1,
    );
    assert_eq!(
        serialized_on(&network, &registry, "note").as_deref(),
        Some("[a, a -> v.y]")
    );
}

#[test]
fn a_multi_output_source_keeps_its_pin_qualifier() {
    let (network, registry, _) = author(
        r#"
        edit1 = atom_edit { diff: "" }
        applied = apply_diff { diff: edit1.diff }
        note = Comment { text: "the diff branch", on: edit1.diff -> applied.diff }
        "#,
    );

    let anchors = anchors_of(&network, "note");
    assert_wire_anchor(
        &anchors[0],
        node_id(&network, "edit1"),
        node_id(&network, "applied"),
        1, // apply_diff's parameters are [base, diff, tolerance]
    );
    assert_eq!(
        serialized_on(&network, &registry, "note").as_deref(),
        Some("edit1.diff -> applied.diff")
    );
}

#[test]
fn a_dynamic_arity_destination_contributes_its_param_id() {
    // `record_construct`'s pins come from a record schema, so its parameters
    // carry persistent ids — the half of D4 the index alone cannot address.
    let (network, _registry, _) = author(
        r#"
        one = int { value: 14 }
        two = int { value: 6 }
        rec = record_construct { schema: "ElementMapping", from: one, to: two }
        note = Comment { text: "the source element", on: one -> rec.from }
        "#,
    );

    match &anchors_of(&network, "note")[0] {
        CommentAnchor::Wire(w) => assert!(
            w.destination_param_id.is_some(),
            "a dynamic-arity destination must contribute its param id"
        ),
        other => panic!("expected a wire anchor, got {:?}", other),
    }
}

#[test]
fn an_unanchored_comment_emits_no_on_property() {
    let (network, registry, _) = author(
        r#"
        a = float { value: 1.0 }
        note = Comment { text: "free-floating" }
        "#,
    );

    assert!(anchors_of(&network, "note").is_empty());
    let text = serialize_network(&network, &registry, None);
    assert!(
        !text.contains("on:"),
        "an unanchored comment must serialize exactly as before:\n{}",
        text
    );
}

#[test]
fn anchors_survive_a_serialize_edit_round_trip() {
    // The motivating case: this is precisely what an AI edit does — read the
    // whole network as text, write the whole thing back.
    let (network, registry, _) = author(WIRE_ANCHORED);
    let text = serialize_network(&network, &registry, None);

    let mut reparsed = standalone_network();
    let result = edit_network(&mut reparsed, &registry, &text, true);
    assert!(
        result.success,
        "re-applying must succeed: {:?}",
        result.errors
    );
    assert!(
        result.warnings.is_empty(),
        "a clean round trip must warn about nothing: {:?}",
        result.warnings
    );

    let anchors = anchors_of(&reparsed, "note");
    assert_eq!(anchors.len(), 1, "the anchor must survive the round trip");
    assert_wire_anchor(
        &anchors[0],
        node_id(&reparsed, "a"),
        node_id(&reparsed, "v"),
        1,
    );
    // Ids differ across the rebuild, so the text is the stable comparison.
    assert_eq!(serialize_network(&reparsed, &registry, None), text);
}

// ============================================================================
// Unresolvable anchors warn and drop (D6)
// ============================================================================

#[test]
fn an_on_naming_a_nonexistent_node_warns_and_drops() {
    let (network, _registry, result) = author(
        r#"
        a = float { value: 1.0 }
        note = Comment { text: "dangling", on: nowhere }
        "#,
    );

    assert!(anchors_of(&network, "note").is_empty());
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("nowhere") && w.contains("dropped")),
        "expected a warning naming the missing node: {:?}",
        result.warnings
    );
}

#[test]
fn an_on_naming_a_wire_that_does_not_exist_warns_and_drops() {
    let (network, _registry, result) = author(
        r#"
        a = float { value: 1.0 }
        v = vec3 { y: a }
        note = Comment { text: "wrong slot", on: a -> v.z }
        "#,
    );

    assert!(anchors_of(&network, "note").is_empty());
    assert!(
        result.warnings.iter().any(|w| w.contains("no wire from")),
        "expected a warning about the missing wire: {:?}",
        result.warnings
    );
}

#[test]
fn on_is_rejected_on_a_non_comment_node() {
    let (network, _registry, result) = author(
        r#"
        a = float { value: 1.0 }
        v = vec3 { y: a, on: a }
        "#,
    );

    // The property is ignored rather than wired to a parameter named `on`.
    assert!(network.nodes.contains_key(&node_id(&network, "v")));
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("only supported on comment nodes")),
        "expected a warning about the node type: {:?}",
        result.warnings
    );
}

#[test]
fn an_empty_array_clears_the_anchors() {
    let registry = NodeTypeRegistry::new();
    let mut network = standalone_network();
    assert!(
        edit_network(&mut network, &registry, WIRE_ANCHORED, true).success,
        "authoring must succeed"
    );
    assert_eq!(anchors_of(&network, "note").len(), 1);

    let result = edit_network(
        &mut network,
        &registry,
        r#"note = Comment { text: "the y source", on: [] }"#,
        false,
    );
    assert!(
        result.success,
        "the clearing edit must succeed: {:?}",
        result.errors
    );
    assert!(anchors_of(&network, "note").is_empty());
}

#[test]
fn an_incremental_edit_that_omits_on_keeps_the_anchors() {
    let registry = NodeTypeRegistry::new();
    let mut network = standalone_network();
    assert!(edit_network(&mut network, &registry, WIRE_ANCHORED, true).success);

    let result = edit_network(
        &mut network,
        &registry,
        r#"note = Comment { text: "retitled" }"#,
        false,
    );
    assert!(result.success, "the edit must succeed: {:?}", result.errors);
    assert_eq!(
        anchors_of(&network, "note").len(),
        1,
        "an edit that says nothing about `on` must leave the anchors alone"
    );
}
