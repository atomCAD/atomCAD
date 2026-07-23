// Tests for the relax node's atom limits (issue #271, extended by
// doc/design_relax_frozen_atoms.md).
//
// The relax node delegates all limit checking to minimize_energy(), which is
// frozen-aware: only unfrozen atoms count against the 2000-atom limit, so a
// large frozen bulk with a small free region relaxes successfully (the
// reported bug scenario). Errors surface through the node's existing
// Err → NetworkResult::Error path.

use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use rust_lib_flutter_cad::crystolecule::simulation::MAX_MINIMIZE_FREE_ATOMS;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// ============================================================================
// Helpers
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn add_value_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    value: NetworkResult,
) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    network.add_node("value", DVec2::ZERO, 0, Box::new(ValueData { value }))
}

fn molecule_value(structure: AtomicStructure) -> NetworkResult {
    NetworkResult::Molecule(MoleculeData {
        atoms: structure,
        geo_tree_root: None,
    })
}

fn evaluate_node(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
    use_vdw_cutoff: bool,
) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    context.use_vdw_cutoff = use_vdw_cutoff;
    let network_stack = vec![NetworkStackElement {
        is_zone_body: false,
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

/// `n_free` unfrozen disconnected carbons on a grid.
fn free_carbon_grid(n_free: usize) -> AtomicStructure {
    let mut structure = AtomicStructure::new();
    let spacing = 2.0;
    let side = (n_free as f64).cbrt().ceil() as usize;
    let mut count = 0;
    'outer: for ix in 0..side {
        for iy in 0..side {
            for iz in 0..side {
                if count >= n_free {
                    break 'outer;
                }
                structure.add_atom(
                    6,
                    DVec3::new(
                        ix as f64 * spacing,
                        iy as f64 * spacing,
                        iz as f64 * spacing,
                    ),
                );
                count += 1;
            }
        }
    }
    structure
}

/// A frozen bulk over the old 2000 total-atom limit plus one free, stretched,
/// bonded C–C pair placed outside the bulk's vdW shell.
fn frozen_bulk_with_free_pair(n_frozen: usize) -> AtomicStructure {
    let mut structure = free_carbon_grid(n_frozen);
    let ids: Vec<u32> = structure.iter_atoms().map(|(id, _)| *id).collect();
    for id in ids {
        structure.set_atom_frozen(id, true);
    }
    let a1 = structure.add_atom(6, DVec3::new(-30.0, 0.0, 0.0));
    let a2 = structure.add_atom(6, DVec3::new(-28.0, 0.0, 0.0));
    structure.add_bond(a1, a2, BOND_SINGLE);
    structure
}

// ============================================================================
// Regression for the reported bug: frozen bulk over the old limit relaxes
// ============================================================================

#[test]
fn relax_node_accepts_frozen_bulk_over_old_limit() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let n_frozen = MAX_MINIMIZE_FREE_ATOMS + 500;
    let value_id = add_value_node(
        &mut designer,
        net,
        molecule_value(frozen_bulk_with_free_pair(n_frozen)),
    );
    let relax_id = designer.add_node("relax", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, relax_id, 0);

    let result = evaluate_node(&designer, net, relax_id, true);
    let atoms = match result {
        NetworkResult::Molecule(m) => m.atoms,
        NetworkResult::Error(e) => panic!("frozen bulk over the old limit must relax: {}", e),
        other => panic!("expected Molecule, got {:?}", other.infer_data_type()),
    };

    assert_eq!(atoms.get_num_of_atoms(), n_frozen + 2);
    let moved_frozen = atoms
        .iter_atoms()
        .filter(|(_, a)| a.is_frozen())
        .filter(|(_, a)| a.position.x < -1.0)
        .count();
    assert_eq!(
        moved_frozen, 0,
        "no frozen atom may drift toward the free pair"
    );

    // The stretched free pair relaxed inward from its 2.0 Å separation.
    let free_xs: Vec<f64> = atoms
        .iter_atoms()
        .filter(|(_, a)| !a.is_frozen())
        .map(|(_, a)| a.position.x)
        .collect();
    assert_eq!(free_xs.len(), 2);
    let dist = (free_xs[0] - free_xs[1]).abs();
    assert!(
        dist < 2.0 && (dist - 2.0).abs() > 1e-3,
        "free pair should relax inward from 2.0 Å (got {} Å)",
        dist
    );
}

// ============================================================================
// Errors surface through the node's NetworkResult::Error path
// ============================================================================

#[test]
fn relax_node_errors_on_too_many_free_atoms() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        molecule_value(free_carbon_grid(MAX_MINIMIZE_FREE_ATOMS + 1)),
    );
    let relax_id = designer.add_node("relax", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, relax_id, 0);

    let result = evaluate_node(&designer, net, relax_id, true);
    match result {
        NetworkResult::Error(e) => {
            assert!(
                e.contains("unfrozen") && e.contains("Freeze"),
                "error should be the free-atom message with freezing advice: {}",
                e
            );
        }
        other => panic!("expected Error, got {:?}", other.infer_data_type()),
    }
}

#[test]
fn relax_node_errors_on_frozen_bulk_without_vdw_cutoff() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let n_frozen = MAX_MINIMIZE_FREE_ATOMS + 500;
    let value_id = add_value_node(
        &mut designer,
        net,
        molecule_value(frozen_bulk_with_free_pair(n_frozen)),
    );
    let relax_id = designer.add_node("relax", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, relax_id, 0);

    let result = evaluate_node(&designer, net, relax_id, false);
    match result {
        NetworkResult::Error(e) => {
            assert!(
                e.contains("Use vdW distance cutoff for energy minimization"),
                "error should point at the preference checkbox: {}",
                e
            );
        }
        other => panic!("expected Error, got {:?}", other.infer_data_type()),
    }
}
