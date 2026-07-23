//! Scope-aware validation-error collection (error-navigation feature):
//! `scoped_validation_errors::collect_scoped_validation_errors` and its
//! surfacing through `NodeTypeRegistry::get_node_networks_with_validation`.
//!
//! The collection walk is what lets the user-types panel turn an error into a
//! *jump to the offending node*: each error must carry the scope path of the
//! body it lives in, and the generic "Zone body is invalid" HOF marker must be
//! dropped in favor of the real, precisely-located body error.
//!
//! Errors are injected directly here (rather than driven through the validator)
//! so the walk is tested in isolation — that the validator stamps the right
//! `node_id` on body-scope errors is covered by `zones_test.rs` (Phase U7).

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::network_validator::ZONE_BODY_INVALID_MARKER;
use rust_lib_flutter_cad::structure_designer::node_network::ValidationError;
use rust_lib_flutter_cad::structure_designer::scoped_validation_errors::collect_scoped_validation_errors;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

/// Builds `main` containing a single `map` node (which owns an inline body) and
/// an `int` node inside that body. Returns `(designer, map_id, body_int_id)`.
/// The network's `validation_errors` lists are cleared so a test can inject a
/// known set without validator noise.
fn setup_map_with_body_node() -> (StructureDesigner, u64, u64) {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    let map_id = designer.add_node("map", DVec2::ZERO);
    let body_int_id = designer.add_node_scoped(&[map_id], "int", DVec2::ZERO, None);

    // Clear any validation state produced while building the structure so the
    // test controls exactly which errors are present.
    let main = designer
        .node_type_registry
        .node_networks
        .get_mut("main")
        .unwrap();
    main.validation_errors.clear();
    let body = main.nodes.get_mut(&map_id).unwrap().zone_mut().unwrap();
    body.validation_errors.clear();

    (designer, map_id, body_int_id)
}

/// A top-level error is collected with an empty scope path, and its
/// blocking-ness is carried through unchanged.
#[test]
fn top_level_error_has_empty_scope_path() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);

    let main = designer
        .node_type_registry
        .node_networks
        .get_mut("main")
        .unwrap();
    main.validation_errors.clear();
    main.validation_errors.push(ValidationError::warning(
        "just a warning".to_string(),
        Some(sphere_id),
    ));

    let collected = collect_scoped_validation_errors(main);
    assert_eq!(collected.len(), 1);
    assert_eq!(collected[0].scope_path, Vec::<u64>::new());
    assert_eq!(collected[0].node_id, Some(sphere_id));
    assert_eq!(collected[0].error_text, "just a warning");
    assert!(!collected[0].blocking);
}

/// A body error is collected with the scope path of the body it lives in, and
/// the generic "Zone body is invalid" marker the validator attaches to the HOF
/// is dropped — the real body error is the navigable one.
#[test]
fn body_error_carries_scope_path_and_marker_is_skipped() {
    let (mut designer, map_id, body_int_id) = setup_map_with_body_node();

    let main = designer
        .node_type_registry
        .node_networks
        .get_mut("main")
        .unwrap();
    // The generic marker the validator would attach to the HOF in the parent.
    main.validation_errors.push(ValidationError::new(
        ZONE_BODY_INVALID_MARKER.to_string(),
        Some(map_id),
    ));
    // The real, precisely-located error inside the body.
    let body = main.nodes.get_mut(&map_id).unwrap().zone_mut().unwrap();
    body.validation_errors.push(ValidationError::new(
        "the real problem".to_string(),
        Some(body_int_id),
    ));

    let main = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    let collected = collect_scoped_validation_errors(main);

    // Only the real body error survives — the marker is gone.
    assert_eq!(collected.len(), 1);
    assert_eq!(collected[0].scope_path, vec![map_id]);
    assert_eq!(collected[0].node_id, Some(body_int_id));
    assert_eq!(collected[0].error_text, "the real problem");
    assert!(
        !collected
            .iter()
            .any(|e| e.error_text == ZONE_BODY_INVALID_MARKER),
        "the generic HOF marker must not be collected"
    );
}

/// The API-level getter resolves the offending node's label and a body
/// qualifier for a body error, and leaves the qualifier absent for a top-level
/// error — mirroring a Find Usages row.
#[test]
fn api_getter_resolves_label_and_body_qualifier() {
    let (mut designer, map_id, body_int_id) = setup_map_with_body_node();

    let main = designer
        .node_type_registry
        .node_networks
        .get_mut("main")
        .unwrap();
    // One top-level error (no body) and one body error.
    main.validation_errors.push(ValidationError::new(
        "top problem".to_string(),
        Some(map_id),
    ));
    let body = main.nodes.get_mut(&map_id).unwrap().zone_mut().unwrap();
    body.validation_errors.push(ValidationError::new(
        "body problem".to_string(),
        Some(body_int_id),
    ));

    let networks = designer
        .node_type_registry
        .get_node_networks_with_validation();
    let main_entry = networks
        .iter()
        .find(|n| n.name == "main")
        .expect("main network present");
    assert_eq!(main_entry.validation_errors.len(), 2);

    let top = main_entry
        .validation_errors
        .iter()
        .find(|e| e.error_text == "top problem")
        .expect("top-level error present");
    assert!(top.scope_path.is_empty());
    assert!(top.body_qualifier.is_none());
    assert!(top.node_label.is_some());

    let body_err = main_entry
        .validation_errors
        .iter()
        .find(|e| e.error_text == "body problem")
        .expect("body error present");
    assert_eq!(body_err.scope_path, vec![map_id]);
    assert_eq!(body_err.node_id, Some(body_int_id));
    assert!(body_err.node_label.is_some());
    let qualifier = body_err
        .body_qualifier
        .as_ref()
        .expect("body error has a qualifier");
    assert!(
        qualifier.starts_with("in ") && qualifier.ends_with(" body"),
        "unexpected body qualifier: {qualifier:?}"
    );
}

/// A valid network surfaces an empty error list.
#[test]
fn valid_network_has_no_errors() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    designer
        .node_type_registry
        .node_networks
        .get_mut("main")
        .unwrap()
        .set_return_node(sphere_id);
    designer.validate_active_network();

    let networks = designer
        .node_type_registry
        .get_node_networks_with_validation();
    let main_entry = networks.iter().find(|n| n.name == "main").unwrap();
    assert!(main_entry.validation_errors.is_empty());
}
