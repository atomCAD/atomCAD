//! A text edit is all or nothing: an editor error puts the network back as it
//! was, for AI edits (`ai_text_edit.rs`) and the in-app Text tab
//! (`text_tab_edit.rs`) alike. Validation errors are not editor errors: such an
//! edit stays, and is one undo step.

use std::collections::BTreeMap;

use glam::DVec2;

use atomcad_structure_designer::ai_text_edit::EDIT_NOT_APPLIED_MESSAGE;
use atomcad_structure_designer::node_network::NodeNetwork;
use atomcad_structure_designer::preferences::NodeDisplayPolicy;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::serialize_network;
use atomcad_structure_designer::text_tab_edit::TextTabEditOutcome;

// ============================================================================
// Harness
// ============================================================================

/// A designer whose active network `main` holds `a` (made by an AI edit, so
/// `ai_placed`) and `u` (moved by hand), with a clean undo stack and dirty flag.
fn designer() -> StructureDesigner {
    let mut sd = StructureDesigner::new();
    sd.preferences.node_display_preferences.display_policy = NodeDisplayPolicy::Manual;
    sd.add_node_network("main");
    sd.set_active_node_network_name(Some("main".to_string()));
    let outcome = sd.ai_text_edit(
        "a = sphere { radius: 3 }\nu = cuboid { extent: (2, 2, 2) }\ns = union { shapes: [a, u] }",
        false,
    );
    assert!(outcome.result.success, "{:?}", outcome.result.errors);
    let u = id_of(network(&sd), "u");
    let main = sd.node_type_registry.node_networks.get_mut("main").unwrap();
    main.nodes.get_mut(&u).unwrap().position = DVec2::new(-500.0, 700.0);
    main.nodes.get_mut(&u).unwrap().ai_placed = false;
    sd.undo_stack.clear();
    sd.set_dirty(false);
    sd
}

fn network(sd: &StructureDesigner) -> &NodeNetwork {
    sd.node_type_registry.node_networks.get("main").unwrap()
}

fn id_of(network: &NodeNetwork, name: &str) -> u64 {
    network
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(name))
        .map(|n| n.id)
        .unwrap_or_else(|| panic!("no node named {name}"))
}

/// Everything an edit could change and the test can see: the text, and each
/// node's id, position and ownership tag.
#[derive(Debug, PartialEq)]
struct Fingerprint {
    text: String,
    nodes: BTreeMap<u64, (String, (i64, i64), bool)>,
    return_node: Option<u64>,
}

fn fingerprint(sd: &StructureDesigner) -> Fingerprint {
    let network = network(sd);
    Fingerprint {
        text: serialize_network(network, &sd.node_type_registry, Some("main")),
        nodes: network
            .nodes
            .values()
            .map(|n| {
                (
                    n.id,
                    (
                        n.custom_name.clone().unwrap_or_default(),
                        (n.position.x as i64, n.position.y as i64),
                        n.ai_placed,
                    ),
                )
            })
            .collect(),
        return_node: network.return_node_id,
    }
}

/// Run a failing AI edit and check that it left nothing behind.
fn assert_ai_edit_rolled_back(code: &str, replace: bool) {
    let mut sd = designer();
    let before = fingerprint(&sd);
    let log_before = sd.ai_edit_log.len();

    let outcome = sd.ai_text_edit(code, replace);

    assert!(!outcome.result.success);
    assert_eq!(
        outcome.result.errors.last().map(String::as_str),
        Some(EDIT_NOT_APPLIED_MESSAGE),
        "{:?}",
        outcome.result.errors
    );
    assert!(outcome.result.nodes_created.is_empty());
    assert!(outcome.result.connections_made.is_empty());
    assert!(!outcome.needs_refresh);
    assert_eq!(fingerprint(&sd), before, "the network is unchanged");
    assert_eq!(sd.undo_stack.history_len(), 0, "no undo step");
    assert!(!sd.is_dirty(), "no dirty flag");

    assert_eq!(
        sd.ai_edit_log.len(),
        log_before + 1,
        "the attempt is logged"
    );
    let record = sd.ai_edit_log.last().unwrap();
    assert!(!record.applied);
    assert!(!record.success);
    assert_eq!(record.code, code);
}

// ============================================================================
// AI edits
// ============================================================================

#[test]
fn an_unknown_node_type_rolls_back_the_statements_around_it() {
    assert_ai_edit_rolled_back(
        "b = cuboid {}\nc = no_such_node {}\na = sphere { radius: 7 }",
        false,
    );
}

#[test]
fn an_error_in_replace_mode_restores_the_cleared_network() {
    assert_ai_edit_rolled_back("b = cuboid {}\nc = no_such_node {}", true);
}

#[test]
fn a_missing_delete_target_rolls_back() {
    assert_ai_edit_rolled_back("b = cuboid {}\ndelete ghost", false);
}

#[test]
fn a_missing_output_target_rolls_back() {
    assert_ai_edit_rolled_back("a = sphere { radius: 7 }\noutput ghost", false);
}

#[test]
fn a_path_into_a_missing_body_rolls_back() {
    assert_ai_edit_rolled_back("b = cuboid {}\nghost/x = sphere {}", false);
}

#[test]
fn a_bad_literal_rolls_back() {
    assert_ai_edit_rolled_back("b = cuboid {}\na = sphere { radius: \"big\" }", false);
}

#[test]
fn a_failing_dry_run_logs_nothing() {
    let mut sd = designer();
    let before = fingerprint(&sd);
    let log_before = sd.ai_edit_log.len();

    let outcome = sd.ai_text_edit_with_options("b = cuboid {}\nc = no_such_node {}", false, true);

    assert!(outcome.dry_run);
    assert!(!outcome.result.success);
    assert_eq!(fingerprint(&sd), before);
    assert_eq!(sd.ai_edit_log.len(), log_before);
    assert_eq!(sd.undo_stack.history_len(), 0);
}

#[test]
fn warnings_alone_do_not_roll_back() {
    let mut sd = designer();
    let outcome = sd.ai_text_edit("b = cuboid { no_such_property: 1 }", false);
    assert!(outcome.result.success, "{:?}", outcome.result.errors);
    assert!(!outcome.result.warnings.is_empty());
    let _ = id_of(network(&sd), "b");
    assert_eq!(sd.undo_stack.history_len(), 1);
}

#[test]
fn a_validation_error_keeps_the_edit_and_it_can_be_undone() {
    let mut sd = designer();
    let before = fingerprint(&sd);

    // A wire cycle parses and applies; only validation rejects it.
    let outcome = sd.ai_text_edit(
        "p = union { shapes: [q] }\nq = union { shapes: [p] }",
        false,
    );

    assert!(!outcome.result.success, "the cycle is reported");
    assert!(
        !outcome
            .result
            .errors
            .iter()
            .any(|e| e == EDIT_NOT_APPLIED_MESSAGE)
    );
    let _ = id_of(network(&sd), "p");
    assert!(sd.ai_edit_log.last().unwrap().applied);
    assert_eq!(sd.undo_stack.history_len(), 1);
    assert!(sd.is_dirty());

    assert!(sd.undo());
    assert_eq!(fingerprint(&sd).text, before.text);
}

// ============================================================================
// The Text tab
// ============================================================================

#[test]
fn a_text_tab_error_restores_the_network() {
    let mut sd = designer();
    let before = fingerprint(&sd);

    let outcome = sd.apply_text_to_active_network("a = sphere { radius: 3 }\nc = no_such_node {}");

    let TextTabEditOutcome::Edited(result) = outcome else {
        panic!("the text parses, so the editor runs");
    };
    assert!(!result.success);
    assert_eq!(
        result.errors.last().map(String::as_str),
        Some(EDIT_NOT_APPLIED_MESSAGE)
    );
    assert!(result.nodes_created.is_empty());
    assert_eq!(fingerprint(&sd), before);
    assert_eq!(sd.undo_stack.history_len(), 0);
    assert!(!sd.is_dirty());
}

#[test]
fn a_text_tab_parse_error_changes_nothing() {
    let mut sd = designer();
    let before = fingerprint(&sd);
    let outcome = sd.apply_text_to_active_network("a = sphere { radius: ");
    assert!(matches!(outcome, TextTabEditOutcome::ParseError(_)));
    assert_eq!(fingerprint(&sd), before);
}

#[test]
fn a_text_tab_apply_keeps_positions_by_name_and_is_one_undo_step() {
    let mut sd = designer();
    let before = fingerprint(&sd);
    let u_position = network(&sd).nodes[&id_of(network(&sd), "u")].position;
    let text = format!("{}\nb = cuboid {{}}", before.text);

    let outcome = sd.apply_text_to_active_network(&text);

    let TextTabEditOutcome::Edited(result) = outcome else {
        panic!("the text parses");
    };
    assert!(result.success, "{:?}", result.errors);
    let _ = id_of(network(&sd), "b");
    assert_eq!(
        network(&sd).nodes[&id_of(network(&sd), "u")].position,
        u_position
    );
    assert_eq!(sd.undo_stack.history_len(), 1);
    assert!(sd.is_dirty());

    assert!(sd.undo());
    assert_eq!(fingerprint(&sd).text, before.text);
}
