//! Phase 4 of `doc/design_error_management.md` (D1, D2, D6) — the unified
//! per-network error list and the eval-error snapshot store behind it.
//!
//! The user-types panel consumes **one merged list** per network: validation
//! errors (whole design, always fresh) plus the network's last-known
//! evaluation errors. Eval entries are harvested from the live scene after
//! each evaluating refresh of the active network (replace-wholesale), persist
//! dimmed when the user switches away, and follow D6's key lifecycle: rename
//! re-keys a snapshot, deletion drops it, and a jump target that vanished is
//! dropped rather than returned. The D8 dedupe applies: a poisoned node's
//! synthesized eval entry never appears next to its blocking validation row.

use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::nodes::expr::{ExprData, ExprParameter};
use atomcad_structure_designer::nodes::map::MapData;
use atomcad_structure_designer::nodes::motif_sub::MotifSubData;
use atomcad_structure_designer::nodes::range::RangeData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec2;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::{
    APIErrorSource, APINetworkWithValidationErrors, APIValidationError,
};
use std::cell::RefCell;
use std::collections::HashMap;

// ============================================================================
// Helpers
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn full_refresh(designer: &mut StructureDesigner) {
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
}

fn validate_and_refresh(designer: &mut StructureDesigner) {
    designer.validate_active_network();
    full_refresh(designer);
}

/// Add a displayed top-level `expr` node with one unwired `Int` parameter —
/// the canonical "clean runtime error" node (see `error_display_test.rs`).
fn add_displayed_expr_with_unwired_param(designer: &mut StructureDesigner) -> u64 {
    let expr_id = designer.add_node_scoped(&[], "expr", DVec2::new(0.0, 200.0), None);
    let mut data = ExprData {
        parameters: vec![ExprParameter {
            id: None,
            name: "x".to_string(),
            data_type: DataType::Int,
            data_type_str: None,
        }],
        expression: "x + 1".to_string(),
        expr: None,
        error: None,
        output_type: None,
    };
    let _ = data.parse_and_validate(0);
    designer.set_node_network_data_scoped(&[], expr_id, Box::new(data));
    designer.set_node_display(expr_id, true);
    expr_id
}

/// Replace the expr's data with a parameterless expression, clearing the
/// runtime failure — "the user fixed it".
fn fix_expr(designer: &mut StructureDesigner, expr_id: u64) {
    let mut data = ExprData {
        parameters: vec![],
        expression: "1 + 1".to_string(),
        expr: None,
        error: None,
        output_type: None,
    };
    let _ = data.parse_and_validate(0);
    designer.set_node_network_data_scoped(&[], expr_id, Box::new(data));
}

/// Add a displayed `motif_sub` whose stored parameter-element definition does
/// not parse: a **non-blocking** validation warning on the node
/// ("Parameter element parse error: …", surfaced by `NodeData::get_data_error`
/// since `doc/design_error_management.md` Phase 6) *and*, because its required
/// `motif` input is unwired, an independent runtime failure. The canonical
/// warning + eval-error pair after the D9 severity sweep promoted the
/// zone-output rule to blocking.
fn add_displayed_motif_sub_with_bad_definition(designer: &mut StructureDesigner) -> u64 {
    let id = designer.add_node("motif_sub", DVec2::new(0.0, 0.0));
    let mut data = MotifSubData {
        parameter_element_value_definition: "PRIMARY C EXTRA TOKENS".to_string(),
        error: None,
        parameter_element_values: HashMap::new(),
        available_parameters: RefCell::new(Vec::new()),
    };
    let _ = data.parse_and_validate(0);
    designer.set_node_network_data_scoped(&[], id, Box::new(data));
    designer.set_node_display(id, true);
    id
}

/// Add a displayed `range -> map` where the map's inline body is left empty:
/// a **blocking** validation error on the map ("Zone-output pin 'result' has
/// no incoming wire" — blocking since the D9 severity sweep) whose eval entry
/// is the deduped synthesized vehicle.
#[allow(dead_code)]
fn add_displayed_map_with_empty_body(designer: &mut StructureDesigner) -> u64 {
    let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
    designer.set_node_network_data_scoped(
        &[],
        range_id,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 3,
        }),
    );
    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
    designer.set_node_network_data_scoped(
        &[],
        map_id,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );
    designer.connect_nodes(range_id, 0, map_id, 0);
    designer.set_node_display(map_id, true);
    map_id
}

fn network_entry<'a>(
    networks: &'a [APINetworkWithValidationErrors],
    name: &str,
) -> &'a APINetworkWithValidationErrors {
    networks
        .iter()
        .find(|n| n.name == name)
        .unwrap_or_else(|| panic!("network '{name}' present in the error list"))
}

fn eval_entries(entry: &APINetworkWithValidationErrors) -> Vec<&APIValidationError> {
    entry
        .validation_errors
        .iter()
        .filter(|e| e.source == APIErrorSource::Evaluation)
        .collect()
}

// ============================================================================
// Merged-list getter — severity/source tagging and the D8 dedupe
// ============================================================================

/// An evaluation error joins the unified list as a red (`blocking == true`),
/// bolt-source, non-stale entry for the active network, carrying the same
/// jump addressing a validation row has.
#[test]
fn eval_entry_is_tagged_with_source_and_severity() {
    let mut designer = setup_designer_with_network("main");
    let expr_id = add_displayed_expr_with_unwired_param(&mut designer);
    validate_and_refresh(&mut designer);

    let networks =
        rust_lib_flutter_cad::api::structure_designer::view_builders::get_node_networks_with_errors(
            &designer,
        );
    let main = network_entry(&networks, "main");
    let evals = eval_entries(main);
    assert_eq!(
        evals.len(),
        1,
        "exactly one eval entry for the failing expr"
    );
    let entry = evals[0];
    assert!(entry.blocking, "an eval error is Error severity (red)");
    assert!(
        !entry.stale,
        "the active network's entries are live, not stale"
    );
    assert!(entry.scope_path.is_empty());
    assert_eq!(entry.node_id, Some(expr_id));
    assert!(entry.node_label.is_some(), "picker rows need a node label");
    assert!(entry.body_qualifier.is_none());
}

/// A poisoned node (blocking validation error → skip-and-synthesize)
/// contributes exactly **one** entry: its validation row. The synthesized
/// eval entry is deduped by predicate (D8), never shown as a second row.
#[test]
fn poisoned_node_contributes_exactly_one_entry() {
    let mut designer = setup_designer_with_network("main");

    // A bare `relax`: its `SameAsInput` output pin cannot resolve with the
    // input unwired → blocking validation error attributed to the node.
    let relax_id = designer.add_node("relax", DVec2::new(0.0, 0.0));
    designer.set_node_display(relax_id, true);
    validate_and_refresh(&mut designer);

    // The scene records the synthesized propagation vehicle for the node...
    assert!(
        designer
            .last_generated_structure_designer_scene
            .get_node_error(&[], relax_id)
            .is_some(),
        "the poisoned node carries a synthesized eval entry in the scene"
    );

    // ...but the unified list shows one row: the validation error.
    let networks =
        rust_lib_flutter_cad::api::structure_designer::view_builders::get_node_networks_with_errors(
            &designer,
        );
    let main = network_entry(&networks, "main");
    let relax_rows: Vec<_> = main
        .validation_errors
        .iter()
        .filter(|e| e.node_id == Some(relax_id))
        .collect();
    assert_eq!(
        relax_rows.len(),
        1,
        "poisoned node contributes exactly one entry (D8 dedupe); got: {:?}",
        relax_rows
            .iter()
            .map(|e| (&e.error_text, e.source))
            .collect::<Vec<_>>()
    );
    assert_eq!(relax_rows[0].source, APIErrorSource::Validation);
    assert!(relax_rows[0].blocking);
}

/// A node with a *warning* and its own runtime failure contributes **two**
/// rows: the amber validation warning and the red eval entry — a warning
/// never suppresses anything (the runtime failure can be a different, more
/// specific fact).
#[test]
fn warning_plus_eval_error_contributes_two_entries() {
    let mut designer = setup_designer_with_network("main");
    let node_id = add_displayed_motif_sub_with_bad_definition(&mut designer);
    validate_and_refresh(&mut designer);

    let networks =
        rust_lib_flutter_cad::api::structure_designer::view_builders::get_node_networks_with_errors(
            &designer,
        );
    let main = network_entry(&networks, "main");
    let rows: Vec<_> = main
        .validation_errors
        .iter()
        .filter(|e| e.node_id == Some(node_id))
        .collect();
    assert_eq!(
        rows.len(),
        2,
        "warning + eval error must both appear; got: {:?}",
        rows.iter()
            .map(|e| (&e.error_text, e.source))
            .collect::<Vec<_>>()
    );
    let warning = rows
        .iter()
        .find(|e| e.source == APIErrorSource::Validation)
        .expect("the amber parse-error warning is present");
    assert!(
        !warning.blocking,
        "the motif_sub parse-error rule is a warning (its eval still emits a usable motif)"
    );
    let eval = rows
        .iter()
        .find(|e| e.source == APIErrorSource::Evaluation)
        .expect("the red runtime entry is present");
    assert!(eval.blocking);
}

// ============================================================================
// Snapshot lifecycle — replace wholesale, persist dimmed, re-harvest
// ============================================================================

/// Each refresh replaces the active network's eval entries wholesale: two
/// refreshes don't accumulate, and fixing the failure clears the entry on the
/// next refresh.
#[test]
fn refresh_replaces_snapshot_wholesale() {
    let mut designer = setup_designer_with_network("main");
    let expr_id = add_displayed_expr_with_unwired_param(&mut designer);
    validate_and_refresh(&mut designer);

    // A second refresh must not duplicate the entry.
    full_refresh(&mut designer);
    let networks =
        rust_lib_flutter_cad::api::structure_designer::view_builders::get_node_networks_with_errors(
            &designer,
        );
    assert_eq!(eval_entries(network_entry(&networks, "main")).len(), 1);

    // Fix the expr → the entry disappears on the next refresh (replace, not
    // accumulate).
    fix_expr(&mut designer, expr_id);
    validate_and_refresh(&mut designer);
    let networks =
        rust_lib_flutter_cad::api::structure_designer::view_builders::get_node_networks_with_errors(
            &designer,
        );
    assert!(
        eval_entries(network_entry(&networks, "main")).is_empty(),
        "fixed failure must vanish from the list"
    );
}

/// Switching to another network preserves the first network's eval entries as
/// a dimmed (`stale == true`) snapshot; re-activating and refreshing replaces
/// it with live state.
#[test]
fn switching_networks_preserves_snapshot_dimmed() {
    let mut designer = setup_designer_with_network("main");
    let expr_id = add_displayed_expr_with_unwired_param(&mut designer);
    validate_and_refresh(&mut designer);

    designer.add_node_network("other");
    designer.set_active_node_network_name(Some("other".to_string()));
    validate_and_refresh(&mut designer);

    let networks =
        rust_lib_flutter_cad::api::structure_designer::view_builders::get_node_networks_with_errors(
            &designer,
        );
    let main = network_entry(&networks, "main");
    let evals = eval_entries(main);
    assert_eq!(evals.len(), 1, "inactive network keeps its snapshot");
    assert!(evals[0].stale, "inactive network's entries render dimmed");
    assert!(
        eval_entries(network_entry(&networks, "other")).is_empty(),
        "the never-failing network reports no eval entries"
    );

    // Re-activate, fix, refresh → the snapshot is replaced with live state.
    designer.set_active_node_network_name(Some("main".to_string()));
    fix_expr(&mut designer, expr_id);
    validate_and_refresh(&mut designer);
    let networks =
        rust_lib_flutter_cad::api::structure_designer::view_builders::get_node_networks_with_errors(
            &designer,
        );
    assert!(
        eval_entries(network_entry(&networks, "main")).is_empty(),
        "re-activating and refreshing replaces the stale snapshot"
    );
}

// ============================================================================
// Key lifecycle — jump-target validation, rename, delete
// ============================================================================

/// A snapshot entry whose node was deleted is dropped, not returned — a jump
/// must never target a dead node.
#[test]
fn deleted_node_entry_is_dropped() {
    let mut designer = setup_designer_with_network("main");
    let expr_id = add_displayed_expr_with_unwired_param(&mut designer);
    validate_and_refresh(&mut designer);
    assert_eq!(
        eval_entries(network_entry(
            &rust_lib_flutter_cad::api::structure_designer::view_builders::get_node_networks_with_errors(&designer),
            "main"
        ))
        .len(),
        1
    );

    // Remove the node without a refresh (simulating any path on which the
    // stored snapshot outlives its target).
    designer
        .node_type_registry
        .node_networks
        .get_mut("main")
        .unwrap()
        .nodes
        .remove(&expr_id);

    let networks =
        rust_lib_flutter_cad::api::structure_designer::view_builders::get_node_networks_with_errors(
            &designer,
        );
    assert!(
        eval_entries(network_entry(&networks, "main")).is_empty(),
        "an entry whose node vanished must be dropped, not returned"
    );
}

/// Renaming a network re-keys its snapshot: its dimmed entries survive under
/// the new name (and follow undo/redo of the rename too).
#[test]
fn rename_rekeys_snapshot() {
    let mut designer = setup_designer_with_network("main");
    add_displayed_expr_with_unwired_param(&mut designer);
    validate_and_refresh(&mut designer);

    // Make "main" inactive so its entries come from the long-lived snapshot.
    designer.add_node_network("other");
    designer.set_active_node_network_name(Some("other".to_string()));
    validate_and_refresh(&mut designer);

    assert!(designer.rename_node_network("main", "renamed"));
    let networks =
        rust_lib_flutter_cad::api::structure_designer::view_builders::get_node_networks_with_errors(
            &designer,
        );
    let renamed = network_entry(&networks, "renamed");
    assert_eq!(
        eval_entries(renamed).len(),
        1,
        "entries follow the network under its new name"
    );

    // Undo the rename → the entries follow it back.
    assert!(designer.undo());
    let networks =
        rust_lib_flutter_cad::api::structure_designer::view_builders::get_node_networks_with_errors(
            &designer,
        );
    assert_eq!(
        eval_entries(network_entry(&networks, "main")).len(),
        1,
        "undoing the rename re-keys the snapshot back"
    );

    // Redo → forward again.
    assert!(designer.redo());
    let networks =
        rust_lib_flutter_cad::api::structure_designer::view_builders::get_node_networks_with_errors(
            &designer,
        );
    assert_eq!(eval_entries(network_entry(&networks, "renamed")).len(), 1);
}

/// Deleting a network drops its snapshot, so a new network later created
/// under the same name starts with no inherited errors.
#[test]
fn delete_drops_snapshot_no_ghost_on_recreate() {
    let mut designer = setup_designer_with_network("main");
    add_displayed_expr_with_unwired_param(&mut designer);
    validate_and_refresh(&mut designer);
    assert!(designer.eval_error_snapshots.contains_key("main"));

    designer.add_node_network("other");
    designer.set_active_node_network_name(Some("other".to_string()));
    designer
        .delete_node_network("main")
        .expect("unreferenced network deletes cleanly");
    assert!(
        !designer.eval_error_snapshots.contains_key("main"),
        "deletion drops the snapshot"
    );

    // A newcomer under the deleted name reports no eval entries.
    designer.add_node_network("main");
    let networks =
        rust_lib_flutter_cad::api::structure_designer::view_builders::get_node_networks_with_errors(
            &designer,
        );
    assert!(
        eval_entries(network_entry(&networks, "main")).is_empty(),
        "a re-created network must not inherit the deleted network's errors"
    );
}

/// The banner-style aggregate distinction Flutter needs: an eval-error-only
/// design has no `Validation`-source entries (the direct-editing banner keys
/// on validation entries only, so it must not trip here).
#[test]
fn eval_only_design_has_no_validation_entries() {
    let mut designer = setup_designer_with_network("main");
    add_displayed_expr_with_unwired_param(&mut designer);
    validate_and_refresh(&mut designer);

    let networks =
        rust_lib_flutter_cad::api::structure_designer::view_builders::get_node_networks_with_errors(
            &designer,
        );
    let main = network_entry(&networks, "main");
    assert!(
        !main.validation_errors.is_empty(),
        "the eval entry is in the unified list"
    );
    assert!(
        main.validation_errors
            .iter()
            .all(|e| e.source == APIErrorSource::Evaluation),
        "an eval-error-only design carries no Validation-source entries"
    );
}
