//! Phase 1 alignment tests for the Blueprint/Crystal alignment state.
//!
//! See `doc/design_blueprint_alignment.md`. Each test exercises a single node's
//! alignment transition rule.

use glam::f64::{DVec2, DVec3};
use glam::i32::IVec3;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    Alignment, BlueprintData, CrystalData, MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::nodes::free_move::FreeMoveData;
use rust_lib_flutter_cad::structure_designer::nodes::free_rot::FreeRotData;
use rust_lib_flutter_cad::structure_designer::nodes::structure_move::StructureMoveData;
use rust_lib_flutter_cad::structure_designer::nodes::structure_rot::StructureRotData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

fn evaluate_raw(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

fn setup_designer(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn blueprint(result: NetworkResult) -> BlueprintData {
    match result {
        NetworkResult::Blueprint(bp) => bp,
        NetworkResult::Error(e) => panic!("expected Blueprint, got Error: {}", e),
        other => panic!("expected Blueprint, got {:?}", other.infer_data_type()),
    }
}

fn crystal(result: NetworkResult) -> CrystalData {
    match result {
        NetworkResult::Crystal(c) => c,
        NetworkResult::Error(e) => panic!("expected Crystal, got Error: {}", e),
        other => panic!("expected Crystal, got {:?}", other.infer_data_type()),
    }
}

fn molecule(result: NetworkResult) -> MoleculeData {
    match result {
        NetworkResult::Molecule(m) => m,
        NetworkResult::Error(e) => panic!("expected Molecule, got Error: {}", e),
        other => panic!("expected Molecule, got {:?}", other.infer_data_type()),
    }
}

// ---- Alignment enum ordering ----

#[test]
fn alignment_ordering() {
    assert!(Alignment::Aligned < Alignment::MotifUnaligned);
    assert!(Alignment::MotifUnaligned < Alignment::LatticeUnaligned);
    assert_eq!(Alignment::default(), Alignment::Aligned);
}

#[test]
fn alignment_worsen_to_is_monotonic_max() {
    let mut a = Alignment::Aligned;
    a.worsen_to(Alignment::MotifUnaligned);
    assert_eq!(a, Alignment::MotifUnaligned);
    a.worsen_to(Alignment::Aligned);
    assert_eq!(a, Alignment::MotifUnaligned, "worsen_to must not improve");
    a.worsen_to(Alignment::LatticeUnaligned);
    assert_eq!(a, Alignment::LatticeUnaligned);
    a.worsen_to(Alignment::MotifUnaligned);
    assert_eq!(a, Alignment::LatticeUnaligned, "worsen_to must not improve");
}

// ---- Construction: primitives produce Aligned ----

#[test]
fn sphere_is_aligned() {
    let mut designer = setup_designer("t");
    let id = designer.add_node("sphere", DVec2::ZERO);
    let bp = blueprint(evaluate_raw(&designer, "t", id));
    assert_eq!(bp.alignment, Alignment::Aligned);
}

#[test]
fn cuboid_is_aligned() {
    let mut designer = setup_designer("t");
    let id = designer.add_node("cuboid", DVec2::ZERO);
    let bp = blueprint(evaluate_raw(&designer, "t", id));
    assert_eq!(bp.alignment, Alignment::Aligned);
}

// ---- structure_move ----

fn add_structure_move(
    designer: &mut StructureDesigner,
    network_name: &str,
    translation: IVec3,
    lattice_subdivision: i32,
) -> u64 {
    let id = designer.add_node("structure_move", DVec2::new(300.0, 0.0));
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let node = network.nodes.get_mut(&id).unwrap();
    let data = node
        .data
        .as_any_mut()
        .downcast_mut::<StructureMoveData>()
        .unwrap();
    data.translation = translation;
    data.lattice_subdivision = lattice_subdivision;
    id
}

#[test]
fn structure_move_divisible_translation_preserves_alignment_blueprint() {
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    // translation (2,4,6) is divisible by 2 componentwise
    let move_id = add_structure_move(&mut designer, "t", IVec3::new(2, 4, 6), 2);
    designer.connect_nodes(cuboid_id, 0, move_id, 0);

    let bp = blueprint(evaluate_raw(&designer, "t", move_id));
    assert_eq!(bp.alignment, Alignment::Aligned);
}

#[test]
fn structure_move_non_divisible_becomes_lattice_unaligned_blueprint() {
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    // translation.x = 1 is not divisible by 2
    let move_id = add_structure_move(&mut designer, "t", IVec3::new(1, 0, 0), 2);
    designer.connect_nodes(cuboid_id, 0, move_id, 0);

    let bp = blueprint(evaluate_raw(&designer, "t", move_id));
    assert_eq!(bp.alignment, Alignment::LatticeUnaligned);
}

#[test]
fn structure_move_subdivision_one_is_always_aligned() {
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    // Any integer translation divides by 1.
    let move_id = add_structure_move(&mut designer, "t", IVec3::new(7, 3, -5), 1);
    designer.connect_nodes(cuboid_id, 0, move_id, 0);

    let bp = blueprint(evaluate_raw(&designer, "t", move_id));
    assert_eq!(bp.alignment, Alignment::Aligned);
}

#[test]
fn structure_move_non_divisible_becomes_lattice_unaligned_crystal() {
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let mat_id = designer.add_node("materialize", DVec2::new(200.0, 0.0));
    let move_id = add_structure_move(&mut designer, "t", IVec3::new(0, 1, 0), 2);
    designer.connect_nodes(cuboid_id, 0, mat_id, 0);
    designer.connect_nodes(mat_id, 0, move_id, 0);

    let c = crystal(evaluate_raw(&designer, "t", move_id));
    assert_eq!(c.alignment, Alignment::LatticeUnaligned);
}

// ---- structure_rot (Phase 2: motif-symmetry detection) ----

/// Helper to configure a structure_rot node's axis and step.
fn set_structure_rot(
    designer: &mut StructureDesigner,
    network_name: &str,
    rot_id: u64,
    axis_index: Option<i32>,
    step: i32,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let node = network.nodes.get_mut(&rot_id).unwrap();
    let data = node
        .data
        .as_any_mut()
        .downcast_mut::<StructureRotData>()
        .unwrap();
    data.axis_index = axis_index;
    data.step = step;
}

#[test]
fn structure_rot_four_fold_90deg_is_motif_unaligned_on_diamond() {
    // axis 0 is the a-axis 4-fold; step 1 = 90°. That is NOT a diamond motif
    // symmetry (sends INTERIOR2 off the motif), so the output must drop to
    // MotifUnaligned.
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let rot_id = designer.add_node("structure_rot", DVec2::new(300.0, 0.0));
    set_structure_rot(&mut designer, "t", rot_id, Some(0), 1);
    designer.connect_nodes(cuboid_id, 0, rot_id, 0);

    let bp = blueprint(evaluate_raw(&designer, "t", rot_id));
    assert_eq!(bp.alignment, Alignment::MotifUnaligned);
}

#[test]
fn structure_rot_180deg_about_a_axis_stays_aligned_on_diamond() {
    // axis 0, step 2 = 180°: a proper C2 rotation of diamond. Alignment is
    // preserved.
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let rot_id = designer.add_node("structure_rot", DVec2::new(300.0, 0.0));
    set_structure_rot(&mut designer, "t", rot_id, Some(0), 2);
    designer.connect_nodes(cuboid_id, 0, rot_id, 0);

    let bp = blueprint(evaluate_raw(&designer, "t", rot_id));
    assert_eq!(bp.alignment, Alignment::Aligned);
}

#[test]
fn structure_rot_three_fold_body_diagonal_stays_aligned_on_diamond() {
    // axis 3 is [111] 3-fold; step 1 = 120°. Cyclic permutation of (x,y,z)
    // is a motif symmetry of diamond.
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let rot_id = designer.add_node("structure_rot", DVec2::new(300.0, 0.0));
    set_structure_rot(&mut designer, "t", rot_id, Some(3), 1);
    designer.connect_nodes(cuboid_id, 0, rot_id, 0);

    let bp = blueprint(evaluate_raw(&designer, "t", rot_id));
    assert_eq!(bp.alignment, Alignment::Aligned);
}

#[test]
fn structure_rot_identity_keeps_alignment_unchanged() {
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let rot_id = designer.add_node("structure_rot", DVec2::new(300.0, 0.0));
    // step = 0 → identity rotation, axis irrelevant.
    set_structure_rot(&mut designer, "t", rot_id, Some(0), 0);
    designer.connect_nodes(cuboid_id, 0, rot_id, 0);

    let bp = blueprint(evaluate_raw(&designer, "t", rot_id));
    assert_eq!(bp.alignment, Alignment::Aligned);
}

#[test]
fn structure_rot_degrades_crystal_to_motif_unaligned() {
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let mat_id = designer.add_node("materialize", DVec2::new(200.0, 0.0));
    let rot_id = designer.add_node("structure_rot", DVec2::new(500.0, 0.0));
    set_structure_rot(&mut designer, "t", rot_id, Some(0), 1);
    designer.connect_nodes(cuboid_id, 0, mat_id, 0);
    designer.connect_nodes(mat_id, 0, rot_id, 0);

    let c = crystal(evaluate_raw(&designer, "t", rot_id));
    assert_eq!(c.alignment, Alignment::MotifUnaligned);
}

#[test]
fn structure_rot_preserves_degraded_alignment() {
    // If the upstream is already LatticeUnaligned (via a non-divisible
    // structure_move), structure_rot must not report a better alignment,
    // even when it would otherwise degrade to MotifUnaligned.
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let move_id = add_structure_move(&mut designer, "t", IVec3::new(1, 0, 0), 2);
    let rot_id = designer.add_node("structure_rot", DVec2::new(600.0, 0.0));
    set_structure_rot(&mut designer, "t", rot_id, Some(0), 1);
    designer.connect_nodes(cuboid_id, 0, move_id, 0);
    designer.connect_nodes(move_id, 0, rot_id, 0);

    let bp = blueprint(evaluate_raw(&designer, "t", rot_id));
    assert_eq!(bp.alignment, Alignment::LatticeUnaligned);
}

// ---- free_move / free_rot ----

#[test]
fn free_move_forces_lattice_unaligned_blueprint() {
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let move_id = designer.add_node("free_move", DVec2::new(300.0, 0.0));
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("t")
            .unwrap();
        let node = network.nodes.get_mut(&move_id).unwrap();
        let data = node
            .data
            .as_any_mut()
            .downcast_mut::<FreeMoveData>()
            .unwrap();
        data.translation = DVec3::new(0.5, 0.0, 0.0);
    }
    designer.connect_nodes(cuboid_id, 0, move_id, 0);

    let bp = blueprint(evaluate_raw(&designer, "t", move_id));
    assert_eq!(bp.alignment, Alignment::LatticeUnaligned);
}

#[test]
fn free_move_with_zero_translation_still_lattice_unaligned() {
    // §3.3: "we don't bother to special-case [zero]".
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let move_id = designer.add_node("free_move", DVec2::new(300.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, move_id, 0);

    let bp = blueprint(evaluate_raw(&designer, "t", move_id));
    assert_eq!(bp.alignment, Alignment::LatticeUnaligned);
}

#[test]
fn free_rot_forces_lattice_unaligned_blueprint() {
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let rot_id = designer.add_node("free_rot", DVec2::new(300.0, 0.0));
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("t")
            .unwrap();
        let node = network.nodes.get_mut(&rot_id).unwrap();
        let data = node
            .data
            .as_any_mut()
            .downcast_mut::<FreeRotData>()
            .unwrap();
        data.angle = 0.5;
        data.rot_axis = DVec3::new(0.0, 0.0, 1.0);
    }
    designer.connect_nodes(cuboid_id, 0, rot_id, 0);

    let bp = blueprint(evaluate_raw(&designer, "t", rot_id));
    assert_eq!(bp.alignment, Alignment::LatticeUnaligned);
}

// ---- Boolean CSG: max-propagation ----

#[test]
fn union_of_two_aligned_is_aligned() {
    let mut designer = setup_designer("t");
    let a = designer.add_node("sphere", DVec2::ZERO);
    let b = designer.add_node("cuboid", DVec2::new(0.0, 200.0));
    let u = designer.add_node("union", DVec2::new(300.0, 0.0));
    designer.connect_nodes(a, 0, u, 0);
    designer.connect_nodes(b, 0, u, 0);

    let bp = blueprint(evaluate_raw(&designer, "t", u));
    assert_eq!(bp.alignment, Alignment::Aligned);
}

/// Runs a polymorphic-output Blueprint (e.g. `structure_move`) through
/// `materialize`→`dematerialize` so it ends up with a fixed-typed Blueprint
/// output. This is a workaround for a pre-existing evaluator limitation:
/// polymorphic outputs (`SameAsInput`) don't get auto-wrapped into the
/// `Array[Blueprint]` inputs used by boolean CSG nodes. The alignment state
/// survives both transitions (both `materialize` and `dematerialize` are
/// alignment pass-through).
fn add_fixed_output_degrader(designer: &mut StructureDesigner, source_id: u64) -> u64 {
    let move_id = add_structure_move(designer, "t", IVec3::new(1, 0, 0), 2);
    let mat_id = designer.add_node("materialize", DVec2::new(400.0, 0.0));
    let demat_id = designer.add_node("dematerialize", DVec2::new(600.0, 0.0));
    designer.connect_nodes(source_id, 0, move_id, 0);
    designer.connect_nodes(move_id, 0, mat_id, 0);
    designer.connect_nodes(mat_id, 0, demat_id, 0);
    demat_id
}

#[test]
fn union_with_lattice_unaligned_input_becomes_lattice_unaligned() {
    let mut designer = setup_designer("t");
    let a = designer.add_node("sphere", DVec2::ZERO);
    let b = designer.add_node("cuboid", DVec2::new(0.0, 200.0));
    let degraded = add_fixed_output_degrader(&mut designer, b);
    let u = designer.add_node("union", DVec2::new(800.0, 0.0));
    designer.connect_nodes(a, 0, u, 0);
    designer.connect_nodes(degraded, 0, u, 0);

    let bp = blueprint(evaluate_raw(&designer, "t", u));
    assert_eq!(bp.alignment, Alignment::LatticeUnaligned);
}

#[test]
fn intersect_max_propagates_alignment() {
    let mut designer = setup_designer("t");
    let a = designer.add_node("sphere", DVec2::ZERO);
    let b = designer.add_node("cuboid", DVec2::new(0.0, 200.0));
    let degraded = add_fixed_output_degrader(&mut designer, b);
    let i = designer.add_node("intersect", DVec2::new(800.0, 0.0));
    designer.connect_nodes(a, 0, i, 0);
    designer.connect_nodes(degraded, 0, i, 0);

    let bp = blueprint(evaluate_raw(&designer, "t", i));
    assert_eq!(bp.alignment, Alignment::LatticeUnaligned);
}

#[test]
fn diff_propagates_alignment_from_base() {
    let mut designer = setup_designer("t");
    let base = designer.add_node("sphere", DVec2::ZERO);
    let sub = designer.add_node("cuboid", DVec2::new(0.0, 200.0));
    let degraded_base = add_fixed_output_degrader(&mut designer, base);
    let d = designer.add_node("diff", DVec2::new(800.0, 0.0));
    designer.connect_nodes(degraded_base, 0, d, 0);
    designer.connect_nodes(sub, 0, d, 1);

    let bp = blueprint(evaluate_raw(&designer, "t", d));
    assert_eq!(bp.alignment, Alignment::LatticeUnaligned);
}

#[test]
fn diff_propagates_alignment_from_subtractor() {
    let mut designer = setup_designer("t");
    let base = designer.add_node("sphere", DVec2::ZERO);
    let sub = designer.add_node("cuboid", DVec2::new(0.0, 200.0));
    let degraded_sub = add_fixed_output_degrader(&mut designer, sub);
    let d = designer.add_node("diff", DVec2::new(800.0, 0.0));
    designer.connect_nodes(base, 0, d, 0);
    designer.connect_nodes(degraded_sub, 0, d, 1);

    let bp = blueprint(evaluate_raw(&designer, "t", d));
    assert_eq!(bp.alignment, Alignment::LatticeUnaligned);
}

// ---- Phase transitions ----

#[test]
fn materialize_passes_alignment_through() {
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let move_id = add_structure_move(&mut designer, "t", IVec3::new(1, 0, 0), 2);
    let mat_id = designer.add_node("materialize", DVec2::new(600.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, move_id, 0);
    designer.connect_nodes(move_id, 0, mat_id, 0);

    let c = crystal(evaluate_raw(&designer, "t", mat_id));
    assert_eq!(c.alignment, Alignment::LatticeUnaligned);
}

#[test]
fn materialize_aligned_blueprint_produces_aligned_crystal() {
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let mat_id = designer.add_node("materialize", DVec2::new(200.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, mat_id, 0);

    let c = crystal(evaluate_raw(&designer, "t", mat_id));
    assert_eq!(c.alignment, Alignment::Aligned);
}

#[test]
fn dematerialize_passes_alignment_through() {
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let mat_id = designer.add_node("materialize", DVec2::new(200.0, 0.0));
    let move_id = add_structure_move(&mut designer, "t", IVec3::new(1, 0, 0), 2);
    let demat_id = designer.add_node("dematerialize", DVec2::new(800.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, mat_id, 0);
    designer.connect_nodes(mat_id, 0, move_id, 0);
    designer.connect_nodes(move_id, 0, demat_id, 0);

    let bp = blueprint(evaluate_raw(&designer, "t", demat_id));
    assert_eq!(bp.alignment, Alignment::LatticeUnaligned);
}

#[test]
fn exit_structure_drops_alignment_to_molecule() {
    // Molecules carry no alignment; we just verify the phase transition yields
    // a Molecule regardless of the Crystal's prior alignment state.
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let mat_id = designer.add_node("materialize", DVec2::new(200.0, 0.0));
    let move_id = add_structure_move(&mut designer, "t", IVec3::new(1, 0, 0), 2);
    let exit_id = designer.add_node("exit_structure", DVec2::new(800.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, mat_id, 0);
    designer.connect_nodes(mat_id, 0, move_id, 0);
    designer.connect_nodes(move_id, 0, exit_id, 0);

    let _ = molecule(evaluate_raw(&designer, "t", exit_id));
}

#[test]
fn enter_structure_always_produces_lattice_unaligned_crystal() {
    // Even if the upstream molecule came from an aligned crystal, enter_structure
    // conservatively reports LatticeUnaligned per §3.8.
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let mat_id = designer.add_node("materialize", DVec2::new(200.0, 0.0));
    let exit_id = designer.add_node("exit_structure", DVec2::new(400.0, 0.0));
    let structure_id = designer.add_node("structure", DVec2::new(200.0, 300.0));
    let enter_id = designer.add_node("enter_structure", DVec2::new(700.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, mat_id, 0);
    designer.connect_nodes(mat_id, 0, exit_id, 0);
    designer.connect_nodes(exit_id, 0, enter_id, 0);
    designer.connect_nodes(structure_id, 0, enter_id, 1);

    let c = crystal(evaluate_raw(&designer, "t", enter_id));
    assert_eq!(c.alignment, Alignment::LatticeUnaligned);
}

// ---- atom ops: pass-through ----

#[test]
fn atom_edit_passes_alignment_through() {
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let move_id = add_structure_move(&mut designer, "t", IVec3::new(1, 0, 0), 2);
    let mat_id = designer.add_node("materialize", DVec2::new(600.0, 0.0));
    let edit_id = designer.add_node("atom_edit", DVec2::new(900.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, move_id, 0);
    designer.connect_nodes(move_id, 0, mat_id, 0);
    designer.connect_nodes(mat_id, 0, edit_id, 0);

    let c = crystal(evaluate_raw(&designer, "t", edit_id));
    assert_eq!(c.alignment, Alignment::LatticeUnaligned);
}

#[test]
fn atom_edit_aligned_crystal_stays_aligned() {
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let mat_id = designer.add_node("materialize", DVec2::new(200.0, 0.0));
    let edit_id = designer.add_node("atom_edit", DVec2::new(500.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, mat_id, 0);
    designer.connect_nodes(mat_id, 0, edit_id, 0);

    let c = crystal(evaluate_raw(&designer, "t", edit_id));
    assert_eq!(c.alignment, Alignment::Aligned);
}

#[test]
fn relax_passes_alignment_through() {
    let mut designer = setup_designer("t");
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let move_id = add_structure_move(&mut designer, "t", IVec3::new(1, 0, 0), 2);
    let mat_id = designer.add_node("materialize", DVec2::new(600.0, 0.0));
    let relax_id = designer.add_node("relax", DVec2::new(900.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, move_id, 0);
    designer.connect_nodes(move_id, 0, mat_id, 0);
    designer.connect_nodes(mat_id, 0, relax_id, 0);

    let c = crystal(evaluate_raw(&designer, "t", relax_id));
    assert_eq!(c.alignment, Alignment::LatticeUnaligned);
}

// ---- atom_union: max over inputs ----

#[test]
fn atom_union_max_propagates_alignment() {
    let mut designer = setup_designer("t");
    let a_shape = designer.add_node("sphere", DVec2::ZERO);
    let b_shape = designer.add_node("cuboid", DVec2::new(0.0, 200.0));
    let move_id = add_structure_move(&mut designer, "t", IVec3::new(1, 0, 0), 2);
    let a_mat = designer.add_node("materialize", DVec2::new(300.0, 0.0));
    let b_mat = designer.add_node("materialize", DVec2::new(300.0, 200.0));
    let au = designer.add_node("atom_union", DVec2::new(700.0, 100.0));

    designer.connect_nodes(a_shape, 0, a_mat, 0);
    designer.connect_nodes(b_shape, 0, move_id, 0);
    designer.connect_nodes(move_id, 0, b_mat, 0);
    designer.connect_nodes(a_mat, 0, au, 0);
    designer.connect_nodes(b_mat, 0, au, 0);

    let c = crystal(evaluate_raw(&designer, "t", au));
    assert_eq!(c.alignment, Alignment::LatticeUnaligned);
}

#[test]
fn atom_union_all_aligned_is_aligned() {
    let mut designer = setup_designer("t");
    let a_shape = designer.add_node("sphere", DVec2::ZERO);
    let b_shape = designer.add_node("cuboid", DVec2::new(0.0, 200.0));
    let a_mat = designer.add_node("materialize", DVec2::new(300.0, 0.0));
    let b_mat = designer.add_node("materialize", DVec2::new(300.0, 200.0));
    let au = designer.add_node("atom_union", DVec2::new(700.0, 100.0));
    designer.connect_nodes(a_shape, 0, a_mat, 0);
    designer.connect_nodes(b_shape, 0, b_mat, 0);
    designer.connect_nodes(a_mat, 0, au, 0);
    designer.connect_nodes(b_mat, 0, au, 0);

    let c = crystal(evaluate_raw(&designer, "t", au));
    assert_eq!(c.alignment, Alignment::Aligned);
}
