use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    GeometrySummary, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::nodes::lattice_symop::LatticeSymopEvalCache;
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::util::transform::Transform;

// ============================================================================
// Helper Functions
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn add_geometry_value_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    position: DVec2,
    unit_cell: UnitCellStruct,
) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();

    let geo_summary = GeometrySummary {
        unit_cell,
        frame_transform: Transform::new(DVec3::ZERO, glam::f64::DQuat::IDENTITY),
        geo_tree_root: GeoNode::sphere(DVec3::ZERO, 1.0),
    };

    let value_data = Box::new(ValueData {
        value: NetworkResult::Geometry(geo_summary),
    });
    network.add_node("value", position, 0, value_data)
}

fn do_full_refresh(designer: &mut StructureDesigner) {
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
}

fn do_partial_refresh(designer: &mut StructureDesigner) {
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
}

// ============================================================================
// Test: eval_cache accessible when lattice_symop node is visible
// ============================================================================

#[test]
fn lattice_symop_visibility_eval_cache_accessible_when_visible() {
    let network_name = "test_visible_cache";
    let mut designer = setup_designer_with_network(network_name);

    let unit_cell = UnitCellStruct::cubic_diamond();
    let geo_node_id =
        add_geometry_value_node(&mut designer, network_name, DVec2::ZERO, unit_cell);

    let symop_node_id = designer.add_node("lattice_symop", DVec2::new(200.0, 0.0));
    designer.connect_nodes(geo_node_id, 0, symop_node_id, 0);

    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        network.active_node_id = Some(symop_node_id);
    }

    do_full_refresh(&mut designer);

    let eval_cache = designer.get_selected_node_eval_cache();
    assert!(
        eval_cache.is_some(),
        "Eval cache should be available when lattice_symop is visible"
    );

    let cache = eval_cache
        .unwrap()
        .downcast_ref::<LatticeSymopEvalCache>();
    assert!(
        cache.is_some(),
        "Eval cache should be a LatticeSymopEvalCache"
    );
}

// ============================================================================
// Test: eval_cache accessible when lattice_symop node is invisible (THE BUG)
// ============================================================================

#[test]
fn lattice_symop_visibility_eval_cache_accessible_when_invisible() {
    let network_name = "test_invisible_cache";
    let mut designer = setup_designer_with_network(network_name);

    let unit_cell = UnitCellStruct::cubic_diamond();
    let geo_node_id =
        add_geometry_value_node(&mut designer, network_name, DVec2::ZERO, unit_cell);

    let symop_node_id = designer.add_node("lattice_symop", DVec2::new(200.0, 0.0));
    designer.connect_nodes(geo_node_id, 0, symop_node_id, 0);

    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        network.active_node_id = Some(symop_node_id);
    }

    do_full_refresh(&mut designer);

    assert!(
        designer.get_selected_node_eval_cache().is_some(),
        "Eval cache should exist while node is visible"
    );

    // Toggle visibility OFF
    designer.set_node_display(symop_node_id, false);
    do_partial_refresh(&mut designer);

    // THIS IS THE BUG: eval cache should still be accessible for invisible nodes
    let eval_cache = designer.get_selected_node_eval_cache();
    assert!(
        eval_cache.is_some(),
        "Eval cache should be accessible even when lattice_symop is invisible (issue #128)"
    );

    let cache = eval_cache
        .unwrap()
        .downcast_ref::<LatticeSymopEvalCache>();
    assert!(
        cache.is_some(),
        "Eval cache should be a LatticeSymopEvalCache even when node is invisible"
    );
}

// ============================================================================
// Test: eval_cache restored after visibility toggle off then on
// ============================================================================

#[test]
fn lattice_symop_visibility_eval_cache_restored_after_toggle() {
    let network_name = "test_toggle_cache";
    let mut designer = setup_designer_with_network(network_name);

    let unit_cell = UnitCellStruct::cubic_diamond();
    let geo_node_id =
        add_geometry_value_node(&mut designer, network_name, DVec2::ZERO, unit_cell);

    let symop_node_id = designer.add_node("lattice_symop", DVec2::new(200.0, 0.0));
    designer.connect_nodes(geo_node_id, 0, symop_node_id, 0);

    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        network.active_node_id = Some(symop_node_id);
    }

    do_full_refresh(&mut designer);
    assert!(designer.get_selected_node_eval_cache().is_some());

    // Toggle OFF
    designer.set_node_display(symop_node_id, false);
    do_partial_refresh(&mut designer);

    // Toggle back ON
    designer.set_node_display(symop_node_id, true);
    do_partial_refresh(&mut designer);

    let eval_cache = designer.get_selected_node_eval_cache();
    assert!(
        eval_cache.is_some(),
        "Eval cache should be restored after toggling visibility off then on"
    );

    let cache = eval_cache
        .unwrap()
        .downcast_ref::<LatticeSymopEvalCache>();
    assert!(
        cache.is_some(),
        "Restored eval cache should be a LatticeSymopEvalCache"
    );
}

// ============================================================================
// Test: crystal system recognition works when node is invisible
// ============================================================================

#[test]
fn lattice_symop_visibility_crystal_system_available_when_invisible() {
    let network_name = "test_crystal_system_invisible";
    let mut designer = setup_designer_with_network(network_name);

    let unit_cell = UnitCellStruct::cubic_diamond();
    let geo_node_id =
        add_geometry_value_node(&mut designer, network_name, DVec2::ZERO, unit_cell);

    let symop_node_id = designer.add_node("lattice_symop", DVec2::new(200.0, 0.0));
    designer.connect_nodes(geo_node_id, 0, symop_node_id, 0);

    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        network.active_node_id = Some(symop_node_id);
    }

    do_full_refresh(&mut designer);

    let eval_cache = designer.get_selected_node_eval_cache();
    assert!(eval_cache.is_some());
    let cache = eval_cache
        .unwrap()
        .downcast_ref::<LatticeSymopEvalCache>()
        .unwrap();
    let visible_unit_cell = cache.unit_cell.clone();

    // Toggle OFF
    designer.set_node_display(symop_node_id, false);
    do_partial_refresh(&mut designer);

    let eval_cache_invisible = designer.get_selected_node_eval_cache();
    assert!(
        eval_cache_invisible.is_some(),
        "Eval cache with unit cell should be available when node is invisible (issue #128)"
    );

    let cache_invisible = eval_cache_invisible
        .unwrap()
        .downcast_ref::<LatticeSymopEvalCache>()
        .unwrap();

    assert_eq!(
        cache_invisible.unit_cell.a,
        visible_unit_cell.a,
        "Unit cell should be preserved when node becomes invisible"
    );
}

// ============================================================================
// Test: provide_gadget works when node is invisible
// ============================================================================

#[test]
fn lattice_symop_visibility_gadget_creation_when_invisible() {
    let network_name = "test_gadget_invisible";
    let mut designer = setup_designer_with_network(network_name);

    let unit_cell = UnitCellStruct::cubic_diamond();
    let geo_node_id =
        add_geometry_value_node(&mut designer, network_name, DVec2::ZERO, unit_cell);

    let symop_node_id = designer.add_node("lattice_symop", DVec2::new(200.0, 0.0));
    designer.connect_nodes(geo_node_id, 0, symop_node_id, 0);

    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        network.active_node_id = Some(symop_node_id);
    }

    do_full_refresh(&mut designer);

    // Toggle OFF
    designer.set_node_display(symop_node_id, false);
    do_partial_refresh(&mut designer);

    // Get the node data and try to create a gadget
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    let node = network.nodes.get(&symop_node_id).unwrap();
    let gadget = node.data.provide_gadget(&designer);

    assert!(
        gadget.is_some(),
        "Gadget should be creatable even when lattice_symop is invisible (issue #128)"
    );
}
