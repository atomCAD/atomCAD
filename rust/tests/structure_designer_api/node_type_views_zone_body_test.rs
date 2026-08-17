//! The api-level half of `parameter_in_zone_body_test.rs` (issue #417).
//!
//! One test: that the node-type views the add-node popup filters on carry the
//! `allowed_in_zone_body` flag. `get_node_type_views` is a view-builder that
//! D10 moved up into the root crate's `api/`, and a member crate cannot depend
//! on the root — so this test cannot travel with the rest of the file into
//! `atomcad-structure-designer`. The other three enforcement layers (authoring
//! refusal, validator backstop, `ParameterData::eval` guard) are pure domain
//! and stay there.

use atomcad_structure_designer::structure_designer::StructureDesigner;

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

/// The add-node popup filters on this flag; it must be `false` exactly for
/// `parameter` and `true` for everything else the registry publishes.
#[test]
fn node_type_views_expose_allowed_in_zone_body() {
    let designer = setup_designer_with_network("main");
    let categories =
        rust_lib_flutter_cad::api::structure_designer::view_builders::get_node_type_views(
            &designer.node_type_registry,
        );

    let mut saw_parameter = false;
    for category in &categories {
        for view in &category.nodes {
            if view.name == "parameter" {
                saw_parameter = true;
                assert!(
                    !view.allowed_in_zone_body,
                    "`parameter` must be flagged as body-disallowed"
                );
            } else {
                assert!(
                    view.allowed_in_zone_body,
                    "`{}` must be allowed in a zone body",
                    view.name
                );
            }
        }
    }
    assert!(
        saw_parameter,
        "`parameter` missing from the node type views"
    );
}
