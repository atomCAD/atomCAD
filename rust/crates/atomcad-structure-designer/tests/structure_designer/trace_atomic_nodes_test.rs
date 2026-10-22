//! Phase 3 of `doc/design_output_driven_editing.md`: `trace` on the atomic
//! nodes (the movement nodes, `relax`, `atom_union`, `atom_edit`,
//! `materialize`) and on the array plumbing nodes (`sequence`, `array_at`,
//! `array_concat`).
//!
//! Phase 2 of `doc/design_trace_coverage.md`: the selectors (`if`, `switch`),
//! `array_append`, and the atom paths of the phase pass-throughs
//! (`exit_structure`, `enter_structure`).
//!
//! Phase 3 of `doc/design_trace_coverage.md`: the region-gated atom ops
//! (`trace_region_op`), `apply_style`, `edit_atom`, `proxy`, `apply_diff`.
//!
//! Phase 4 of `doc/design_trace_coverage.md`: the multi-participant nodes
//! (`mechanosynth`, `mechanosynth_edit`, `patch_latticefill`, `chemisorb`'s
//! debug pin).

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::atomic_structure::SelectModifier;
use atomcad_crystolecule::guided_placement::crystal_bond_length;
use atomcad_crystolecule::io::xyz_loader::load_xyz;
use atomcad_crystolecule::lattice_fill::DEFAULT_REGION_MARGIN;
use atomcad_crystolecule::mechanosynth::{APEX_FRAME_TAG, Step};
use atomcad_crystolecule::structure::Structure;
use atomcad_geo_tree::GeoNode;
use atomcad_structure_designer::data_type::{DataType, RecordType};
use atomcad_structure_designer::evaluator::atom_op::region_contains;
use atomcad_structure_designer::evaluator::network_result::{MoleculeData, NetworkResult};
use atomcad_structure_designer::node_network::NodeRef;
use atomcad_structure_designer::nodes::array_append::ArrayAppendData;
use atomcad_structure_designer::nodes::array_at::ArrayAtData;
use atomcad_structure_designer::nodes::array_concat::ArrayConcatData;
use atomcad_structure_designer::nodes::atom_cut::AtomCutData;
use atomcad_structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
use atomcad_structure_designer::nodes::atom_replace::AtomReplaceData;
use atomcad_structure_designer::nodes::build_script::BuildScriptData;
use atomcad_structure_designer::nodes::edit_atom::commands::add_atom_command::AddAtomCommand;
use atomcad_structure_designer::nodes::edit_atom::commands::select_command::SelectCommand;
use atomcad_structure_designer::nodes::edit_atom::commands::transform_command::TransformCommand;
use atomcad_structure_designer::nodes::edit_atom::edit_atom::EditAtomData;
use atomcad_structure_designer::nodes::free_move::FreeMoveData;
use atomcad_structure_designer::nodes::free_rot::FreeRotData;
use atomcad_structure_designer::nodes::if_else::IfData;
use atomcad_structure_designer::nodes::materialize::MaterializeData;
use atomcad_structure_designer::nodes::mechanosynth::MechanosynthData;
use atomcad_structure_designer::nodes::mechanosynth_edit::{AuthoredStep, MechanosynthEditData};
use atomcad_structure_designer::nodes::ops_library::OpsLibraryData;
use atomcad_structure_designer::nodes::patch_latticefill::PatchLatticeFillData;
use atomcad_structure_designer::nodes::proxy::ProxyData;
use atomcad_structure_designer::nodes::sequence::SequenceData;
use atomcad_structure_designer::nodes::structure_invert::StructureInvertData;
use atomcad_structure_designer::nodes::structure_move::StructureMoveData;
use atomcad_structure_designer::nodes::structure_rot::StructureRotData;
use atomcad_structure_designer::nodes::switch::{SwitchCase, SwitchCaseValue, SwitchData};
use atomcad_structure_designer::nodes::tag::TagData;
use atomcad_structure_designer::nodes::value::add_typed_value_node;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::trace::{
    ATOM_POINT_DELTA_BONDS, TraceProbe, atom_point_delta, is_terminator,
};
use atomcad_structure_designer::trace::{TraceHit, TraceHitTarget};
use atomcad_test_support::fixture_path_str;
use atomcad_util::transform::Transform;
use glam::f64::{DQuat, DVec2, DVec3};
use glam::i32::IVec3;

use crate::trace_support::*;

const A: DVec3 = DVec3::new(0.0, 0.0, 0.0);
/// Off every axis, so each rotation and inversion moves it.
const B: DVec3 = DVec3::new(1.0, 0.5, 0.3);

/// The probe a source received, which must be the one atom it was traced at.
fn only_probe(probes: &std::sync::Mutex<Vec<TraceProbe>>) -> TraceProbe {
    let probes = probes.lock().unwrap();
    assert_eq!(probes.len(), 1, "{probes:?}");
    probes[0].clone()
}

fn atom_probe(id: u32, position: DVec3) -> TraceProbe {
    TraceProbe::Atom {
        id: Some(id),
        position,
    }
}

// ============================================================================
// Movement nodes and relax
// ============================================================================

/// `src` (atoms A = id 1, B = id 2) → `node_type` with `data`; trace atom B
/// where the node put it. The node must report `label` and hand the source
/// atom 2 at its *input* position.
fn assert_traces_same_atom(
    node_type: &str,
    kind: SourceKind,
    data: Option<Box<dyn atomcad_structure_designer::node_data::NodeData>>,
    label: &str,
) {
    let mut d = setup();
    let (src, probes) = add_probe_source(&mut d, "src", kind, vec![(6, A), (6, B)]);
    let node = add(&mut d, node_type, "node");
    if let Some(data) = data {
        set_node_data(&mut d, MAIN, node, data);
        name(&mut d, MAIN, node, "node");
    }
    d.connect_nodes(src, 0, node, 0);

    let moved = output_atoms(&mut d, node).get_atom(2).unwrap().position;
    assert!(
        moved.distance(B) > 0.1,
        "{node_type}: precondition, atom 2 must move (it is at {moved})"
    );
    assert_eq!(
        trace(&mut d, &atom_hit(node, moved)),
        format!("node {label}\nsrc source"),
        "{node_type}"
    );
    assert_eq!(only_probe(&probes), atom_probe(2, B), "{node_type}");
}

#[test]
fn free_move_traces_a_moved_atom_to_the_same_input_atom() {
    assert_traces_same_atom(
        "free_move",
        SourceKind::Molecule,
        Some(Box::new(FreeMoveData {
            translation: DVec3::new(3.0, 0.0, 0.0),
        })),
        "moved",
    );
}

#[test]
fn free_rot_traces_a_moved_atom_to_the_same_input_atom() {
    assert_traces_same_atom(
        "free_rot",
        SourceKind::Molecule,
        Some(Box::new(FreeRotData {
            angle_degrees: 90.0,
            rot_axis: DVec3::Z,
            pivot_point: DVec3::ZERO,
        })),
        "moved",
    );
}

#[test]
fn structure_move_traces_a_moved_atom_to_the_same_input_atom() {
    assert_traces_same_atom(
        "structure_move",
        SourceKind::Crystal,
        Some(Box::new(StructureMoveData {
            translation: IVec3::new(1, 0, 0),
            lattice_subdivision: IVec3::ONE,
        })),
        "moved",
    );
}

#[test]
fn structure_rot_traces_a_moved_atom_to_the_same_input_atom() {
    assert_traces_same_atom(
        "structure_rot",
        SourceKind::Crystal,
        Some(Box::new(StructureRotData {
            axis_index: Some(2),
            step: 1,
            pivot_point: IVec3::ZERO,
        })),
        "moved",
    );
}

#[test]
fn structure_invert_traces_a_moved_atom_to_the_same_input_atom() {
    assert_traces_same_atom(
        "structure_invert",
        SourceKind::Crystal,
        Some(Box::new(StructureInvertData {
            pivot_point: IVec3::ZERO,
            subdivision: 1,
        })),
        "moved",
    );
}

#[test]
fn relax_traces_a_relaxed_atom_to_the_same_input_atom() {
    // Methane with one stretched C–H bond: relaxing it moves that hydrogen.
    let stretched = DVec3::new(1.6, 0.0, 0.0);
    let mut d = setup();
    let mut methane = AtomicStructure::new();
    let c = methane.add_atom(6, DVec3::ZERO);
    for p in [
        stretched,
        DVec3::new(-0.36, 1.03, 0.0),
        DVec3::new(-0.36, -0.51, 0.89),
        DVec3::new(-0.36, -0.51, -0.89),
    ] {
        let h = methane.add_atom(1, p);
        methane.add_bond(c, h, 1);
    }
    let src = add_value(
        &mut d,
        atomcad_structure_designer::evaluator::network_result::NetworkResult::Molecule(
            atomcad_structure_designer::evaluator::network_result::MoleculeData {
                atoms: methane,
                geo_tree_root: None,
            },
        ),
        "src",
    );
    let relax = add(&mut d, "relax", "relax");
    d.connect_nodes(src, 0, relax, 0);

    let relaxed = output_atoms(&mut d, relax).get_atom(2).unwrap().position;
    assert!(relaxed.distance(stretched) > 0.1, "precondition: {relaxed}");
    // The `value` leaf is an origin; what matters is that relax answered and
    // the hydrogen was found again upstream.
    assert_eq!(
        trace(&mut d, &atom_hit(relax, relaxed)),
        "relax relaxed\nsrc source"
    );
}

#[test]
fn a_movement_node_hands_its_diff_pin_to_the_fallback() {
    // The `diff` output pin (1) is not traced: `trace` answers `None` there.
    let mut d = setup();
    let (src, _) = add_probe_source(&mut d, "src", SourceKind::Molecule, vec![(6, B)]);
    let mover = add(&mut d, "free_move", "mover");
    set_node_data(
        &mut d,
        MAIN,
        mover,
        Box::new(FreeMoveData {
            translation: DVec3::new(3.0, 0.0, 0.0),
        }),
    );
    name(&mut d, MAIN, mover, "mover");
    d.connect_nodes(src, 0, mover, 0);
    let moved = B + DVec3::new(3.0, 0.0, 0.0);

    let hit = atomcad_structure_designer::trace::TraceHit {
        pin: 1,
        ..atom_hit(mover, moved)
    };
    let rendered = trace(&mut d, &hit);
    assert!(
        rendered.starts_with("mover possibly contributed"),
        "{rendered}"
    );
}

// ============================================================================
// atom_union
// ============================================================================

#[test]
fn atom_union_lists_only_the_branch_an_atom_came_from() {
    let far = DVec3::new(10.0, 0.0, 0.0);
    for (hit, want) in [(A, "a"), (far, "b")] {
        let mut d = setup();
        let (a, _) = add_probe_source(&mut d, "a", SourceKind::Molecule, vec![(6, A)]);
        let (b, _) = add_probe_source(&mut d, "b", SourceKind::Molecule, vec![(6, far)]);
        let union = add(&mut d, "atom_union", "union");
        d.connect_nodes(a, 0, union, 0);
        d.connect_nodes(b, 0, union, 0);

        assert_eq!(
            trace(&mut d, &atom_hit(union, hit)),
            format!("union passed\n{want} source")
        );
    }
}

#[test]
fn atom_union_fed_by_a_sequence_carries_the_element_to_the_right_input() {
    let far = DVec3::new(10.0, 0.0, 0.0);
    for (hit, want) in [(A, "a"), (far, "b")] {
        let mut d = setup();
        let (a, _) = add_probe_source(&mut d, "a", SourceKind::Molecule, vec![(6, A)]);
        let (b, b_probes) = add_probe_source(&mut d, "b", SourceKind::Molecule, vec![(6, far)]);
        let seq = add(&mut d, "sequence", "seq");
        // Three pins with the middle one unwired: `eval` skips it, so output
        // element 1 is pin 2.
        set_node_data(
            &mut d,
            MAIN,
            seq,
            Box::new(SequenceData {
                element_type: DataType::Molecule,
                input_count: 3,
            }),
        );
        name(&mut d, MAIN, seq, "seq");
        d.connect_nodes(a, 0, seq, 0);
        d.connect_nodes(b, 0, seq, 2);
        let union = add(&mut d, "atom_union", "union");
        d.connect_nodes(seq, 0, union, 0);

        assert_eq!(
            trace(&mut d, &atom_hit(union, hit)),
            format!("union passed\nseq passed\n{want} source")
        );
        if want == "b" {
            assert_eq!(only_probe(&b_probes), atom_probe(1, far));
        }
    }
}

// ============================================================================
// atom_edit
// ============================================================================

/// `src` (C at 0, 3 and 6 Å along x, ids 1–3) → `edit`, whose diff moves atom
/// 1 up by 1 Å and adds an N at 10 Å.
fn atom_edit_chain() -> (
    StructureDesigner,
    u64,
    std::sync::Arc<std::sync::Mutex<Vec<TraceProbe>>>,
) {
    let mut d = setup();
    let (src, probes) = add_probe_source(
        &mut d,
        "src",
        SourceKind::Molecule,
        vec![
            (6, A),
            (6, DVec3::new(3.0, 0.0, 0.0)),
            (6, DVec3::new(6.0, 0.0, 0.0)),
        ],
    );
    let edit = add(&mut d, "atom_edit", "edit");
    d.connect_nodes(src, 0, edit, 0);
    let data = data_mut::<AtomEditData>(&mut d, edit);
    let moved = data.diff.add_atom(6, DVec3::new(0.0, 0.0, 1.0));
    data.diff.set_anchor_position(moved, A);
    data.diff.add_atom(7, DVec3::new(10.0, 0.0, 0.0));
    (d, edit, probes)
}

#[test]
fn atom_edit_an_added_atom_stops_at_the_node() {
    let (mut d, edit, probes) = atom_edit_chain();
    assert_eq!(
        trace(&mut d, &atom_hit(edit, DVec3::new(10.0, 0.0, 0.0))),
        "edit added"
    );
    assert!(probes.lock().unwrap().is_empty());
}

#[test]
fn atom_edit_an_edited_atom_continues_to_its_base_atom() {
    let (mut d, edit, probes) = atom_edit_chain();
    assert_eq!(
        trace(&mut d, &atom_hit(edit, DVec3::new(0.0, 0.0, 1.0))),
        "edit edited\nsrc source"
    );
    assert_eq!(only_probe(&probes), atom_probe(1, A));
}

#[test]
fn atom_edit_an_untouched_atom_is_passed() {
    let (mut d, edit, probes) = atom_edit_chain();
    let untouched = DVec3::new(3.0, 0.0, 0.0);
    assert_eq!(
        trace(&mut d, &atom_hit(edit, untouched)),
        "edit passed\nsrc source"
    );
    assert_eq!(only_probe(&probes), atom_probe(2, untouched));
}

// ============================================================================
// materialize
// ============================================================================

/// A Blueprint source (a sphere of radius `radius` Å) → `materialize`,
/// passivated with `passivant`, surface-reconstructed if `surf_recon`.
fn materialize_chain(
    radius: f64,
    passivant: i16,
    surf_recon: bool,
) -> (
    StructureDesigner,
    u64,
    std::sync::Arc<std::sync::Mutex<Vec<TraceProbe>>>,
) {
    let mut d = setup();
    let (shape, probes) = add_probe_source(
        &mut d,
        "shape",
        SourceKind::Blueprint(Box::new(GeoNode::sphere(DVec3::ZERO, radius))),
        vec![],
    );
    let fill = add(&mut d, "materialize", "fill");
    let data = data_mut::<MaterializeData>(&mut d, fill);
    data.hydrogen_passivation = true;
    data.passivation_element = passivant;
    data.surface_reconstruction = surf_recon;
    d.connect_nodes(shape, 0, fill, 0);
    (d, fill, probes)
}

fn assert_point(probe: &TraceProbe, position: DVec3, delta: f64) {
    let TraceProbe::Point {
        position: p,
        on_surface,
        delta: dl,
    } = probe
    else {
        panic!("expected a Point, got {probe:?}");
    };
    assert!(p.distance(position) < 1e-9, "{p} vs {position}");
    assert!(!on_surface);
    assert!((dl - delta).abs() < 1e-9, "δ {dl} vs {delta}");
}

#[test]
fn materialize_hands_a_point_with_a_bond_length_delta_to_the_blueprint() {
    let (mut d, fill, probes) = materialize_chain(4.0, 1, false);
    let atoms = output_atoms(&mut d, fill);
    // A surface atom: carbon bonded to a terminator and to carbon.
    let (id, atom) = atoms
        .iter_atoms()
        .find(|(_, a)| {
            a.atomic_number == 6
                && a.bonds
                    .iter()
                    .any(|b| is_terminator(atoms.get_atom(b.other_atom_id()).unwrap()))
        })
        .map(|(id, a)| (*id, a.clone()))
        .expect("a bulk atom");

    assert_eq!(
        trace(&mut d, &atom_hit(fill, atom.position)),
        "fill filled\nshape source"
    );
    let delta = atom_point_delta(&atoms, id);
    assert!(
        (delta - ATOM_POINT_DELTA_BONDS * 1.545).abs() < 0.01,
        "δ is two C–C bonds, not two 1.09 Å C–H bonds: {delta}"
    );
    assert_point(&only_probe(&probes), atom.position, delta);
}

#[test]
fn materialize_probes_a_passivating_terminator_at_its_parent() {
    // Hydrogen, and a halogen terminator (#405): both carry the passivation
    // flag.
    for passivant in [1, 9] {
        let (mut d, fill, probes) = materialize_chain(4.0, passivant, false);
        let atoms = output_atoms(&mut d, fill);
        let (_, cap) = atoms
            .iter_atoms()
            .find(|(_, a)| is_terminator(a))
            .expect("a terminator");
        assert_eq!(cap.atomic_number, passivant);
        let parent = cap.bonds[0].other_atom_id();
        let parent_position = atoms.get_atom(parent).unwrap().position;

        assert_eq!(
            trace(&mut d, &atom_hit(fill, cap.position)),
            "fill passivated\nshape source",
            "passivant {passivant}"
        );
        assert_point(
            &only_probe(&probes),
            parent_position,
            atom_point_delta(&atoms, parent),
        );
    }
}

#[test]
fn materialize_traces_a_flagged_dimer_host_as_filled_at_its_own_position() {
    // Surface reconstruction flags the dimer *hosts* as passivated
    // (`doc/design_halogen_passivation.md` D5); a host is a filled atom, not a
    // terminator.
    let (mut d, fill, probes) = materialize_chain(6.0, 1, true);
    let atoms = output_atoms(&mut d, fill);
    let (_, host) = atoms
        .iter_atoms()
        .find(|(_, a)| a.is_hydrogen_passivation() && a.atomic_number == 6)
        .expect("precondition: a reconstructed dimer host");

    assert_eq!(
        trace(&mut d, &atom_hit(fill, host.position)),
        "fill filled
shape source"
    );
    let TraceProbe::Point { position, .. } = only_probe(&probes) else {
        panic!("expected a Point");
    };
    assert!(position.distance(host.position) < 1e-9);
}

#[test]
fn atom_point_delta_is_two_shortest_bonds_to_a_non_terminator() {
    let mut atoms = AtomicStructure::new();
    let c = atoms.add_atom(6, DVec3::ZERO);
    let far = atoms.add_atom(6, DVec3::new(1.6, 0.0, 0.0));
    let near = atoms.add_atom(6, DVec3::new(0.0, 1.5, 0.0));
    let h = atoms.add_atom(1, DVec3::new(0.0, 0.0, 1.0));
    atoms.add_bond(c, far, 1);
    atoms.add_bond(c, near, 1);
    atoms.add_bond(c, h, 1);

    // The 1.0 Å bond to the terminator does not count.
    assert!((atom_point_delta(&atoms, c) - ATOM_POINT_DELTA_BONDS * 1.5).abs() < 1e-9);

    // The passivation flag is not what makes a terminator: surface
    // reconstruction sets it on dimer hosts, whose bonds still count.
    atoms.set_atom_hydrogen_passivation(near, true);
    assert!((atom_point_delta(&atoms, c) - ATOM_POINT_DELTA_BONDS * 1.5).abs() < 1e-9);

    let lone = atoms.add_atom(14, DVec3::new(10.0, 0.0, 0.0));
    assert_eq!(
        atom_point_delta(&atoms, lone),
        ATOM_POINT_DELTA_BONDS * crystal_bond_length(14, 14).unwrap()
    );
}

// ============================================================================
// array_at and array_concat
// ============================================================================

/// Molecule sources `a` (at A) and `b` (at 10 Å) gathered by a `sequence`.
fn two_source_sequence(d: &mut StructureDesigner, label: &str, sources: &[(&str, DVec3)]) -> u64 {
    let seq = add(d, "sequence", label);
    set_node_data(
        d,
        MAIN,
        seq,
        Box::new(SequenceData {
            element_type: DataType::Molecule,
            input_count: sources.len(),
        }),
    );
    name(d, MAIN, seq, label);
    for (pin, (source, position)) in sources.iter().enumerate() {
        let (id, _) = add_probe_source(d, source, SourceKind::Molecule, vec![(6, *position)]);
        d.connect_nodes(id, 0, seq, pin);
    }
    seq
}

#[test]
fn array_at_maps_its_output_to_the_indexed_input_element() {
    let far = DVec3::new(10.0, 0.0, 0.0);
    let mut d = setup();
    let seq = two_source_sequence(&mut d, "seq", &[("a", A), ("b", far)]);
    let at = add(&mut d, "array_at", "at");
    set_node_data(
        &mut d,
        MAIN,
        at,
        Box::new(ArrayAtData {
            element_type: DataType::Molecule,
            index: 1,
        }),
    );
    name(&mut d, MAIN, at, "at");
    d.connect_nodes(seq, 0, at, 0);

    assert_eq!(
        trace(&mut d, &atom_hit(at, far)),
        "at passed\nseq passed\nb source"
    );
}

#[test]
fn array_concat_maps_an_output_element_to_the_right_input_element() {
    let b_at = DVec3::new(10.0, 0.0, 0.0);
    let c_at = DVec3::new(20.0, 0.0, 0.0);
    for (hit, want) in [
        (b_at, "first passed\nb source"),
        (c_at, "second passed\nc source"),
    ] {
        let mut d = setup();
        let first = two_source_sequence(&mut d, "first", &[("a", A), ("b", b_at)]);
        let second = two_source_sequence(&mut d, "second", &[("c", c_at)]);
        let concat = add(&mut d, "array_concat", "concat");
        set_node_data(
            &mut d,
            MAIN,
            concat,
            Box::new(ArrayConcatData {
                element_type: DataType::Molecule,
            }),
        );
        name(&mut d, MAIN, concat, "concat");
        d.connect_nodes(first, 0, concat, 0);
        d.connect_nodes(second, 0, concat, 1);

        assert_eq!(
            trace(&mut d, &atom_hit(concat, hit)),
            format!("concat passed\n{want}")
        );
    }
}

// ============================================================================
// Selectors: if, switch (doc/design_trace_coverage.md Phase 2)
// ============================================================================

/// `cond` (a Bool value) → `if` over Molecules; the branch `cond` takes is a
/// probe source `src` with a carbon at A, the other a counting leaf whose one
/// carbon is at A too, so only the selection can tell them apart.
fn if_chain(cond: bool) -> (StructureDesigner, u64) {
    let mut d = setup();
    let c = add_value(&mut d, NetworkResult::Bool(cond), "cond");
    let (src, _) = add_probe_source(&mut d, "src", SourceKind::Molecule, vec![(6, A)]);
    let leaf = add(&mut d, "counting_leaf", "leaf");
    let node = add(&mut d, "if", "if");
    set_node_data(
        &mut d,
        MAIN,
        node,
        Box::new(IfData {
            value_type: DataType::Molecule,
        }),
    );
    name(&mut d, MAIN, node, "if");
    let (taken, untaken) = if cond { (1, 2) } else { (2, 1) };
    d.connect_nodes(c, 0, node, 0);
    d.connect_nodes(src, 0, node, taken);
    d.connect_nodes(leaf, 0, node, untaken);
    (d, node)
}

#[test]
fn if_lists_only_the_taken_branch_and_never_evaluates_the_other() {
    for cond in [true, false] {
        LEAF_EVALS.set(0);
        let (mut d, node) = if_chain(cond);
        assert_eq!(
            trace(&mut d, &atom_hit(node, A)),
            "if passed
src source",
            "cond = {cond}"
        );
        assert_eq!(LEAF_EVALS.get(), 0, "cond = {cond}: the untaken branch ran");
    }
}

/// `sel` (an Int value) → `switch` over Molecules with cases 1, 2, 3 and a
/// default; each input is a probe source with its carbon at A.
fn switch_chain(selector: i32) -> (StructureDesigner, u64) {
    let mut d = setup();
    let sel = add_value(&mut d, NetworkResult::Int(selector), "sel");
    let node = add(&mut d, "switch", "switch");
    set_node_data(
        &mut d,
        MAIN,
        node,
        Box::new(SwitchData {
            selector_type: DataType::Int,
            value_type: DataType::Molecule,
            cases: (1..=3)
                .map(|v| SwitchCase {
                    id: Some(v as u64),
                    value: SwitchCaseValue::Int(v),
                })
                .collect(),
            next_case_id: 4,
        }),
    );
    name(&mut d, MAIN, node, "switch");
    d.connect_nodes(sel, 0, node, 0);
    for (pin, label) in ["one", "two", "three", "default"].iter().enumerate() {
        let (src, _) = add_probe_source(&mut d, label, SourceKind::Molecule, vec![(6, A)]);
        d.connect_nodes(src, 0, node, pin + 1);
    }
    (d, node)
}

#[test]
fn switch_lists_only_the_matching_case() {
    let (mut d, node) = switch_chain(2);
    assert_eq!(
        trace(&mut d, &atom_hit(node, A)),
        "switch passed
two source"
    );
}

#[test]
fn switch_with_an_unmatched_selector_lists_the_default() {
    let (mut d, node) = switch_chain(7);
    assert_eq!(
        trace(&mut d, &atom_hit(node, A)),
        "switch passed
default source"
    );
}

// ============================================================================
// array_append
// ============================================================================

#[test]
fn array_append_maps_an_output_element_to_the_array_or_the_appended_element() {
    let b_at = DVec3::new(10.0, 0.0, 0.0);
    let c_at = DVec3::new(20.0, 0.0, 0.0);
    for (hit, want) in [
        (
            b_at,
            "seq passed
b source",
        ),
        (c_at, "c source"),
    ] {
        let mut d = setup();
        let seq = two_source_sequence(&mut d, "seq", &[("a", A), ("b", b_at)]);
        let (c, _) = add_probe_source(&mut d, "c", SourceKind::Molecule, vec![(6, c_at)]);
        let append = add(&mut d, "array_append", "append");
        set_node_data(
            &mut d,
            MAIN,
            append,
            Box::new(ArrayAppendData {
                element_type: DataType::Molecule,
            }),
        );
        name(&mut d, MAIN, append, "append");
        d.connect_nodes(seq, 0, append, 0);
        d.connect_nodes(c, 0, append, 1);
        let union = add(&mut d, "atom_union", "union");
        d.connect_nodes(append, 0, union, 0);

        assert_eq!(
            trace(&mut d, &atom_hit(union, hit)),
            format!(
                "union passed
append passed
{want}"
            )
        );
    }
}

// ============================================================================
// Phase pass-throughs: exit_structure, enter_structure
// ============================================================================

/// `src` (a Crystal, atoms A = id 1 and B = id 2) → `exit` → `enter`, with
/// `enter`'s structure from a `value`. Atom B passes both with its id, and the
/// structure is not listed: it does not place the atom.
#[test]
fn exit_and_enter_structure_pass_an_atom_with_its_id() {
    let mut d = setup();
    let (src, probes) = add_probe_source(&mut d, "src", SourceKind::Crystal, vec![(6, A), (6, B)]);
    let exit = add(&mut d, "exit_structure", "exit");
    d.connect_nodes(src, 0, exit, 0);
    let structure = add_value(&mut d, NetworkResult::Structure(Structure::diamond()), "st");
    let enter = add(&mut d, "enter_structure", "enter");
    d.connect_nodes(exit, 0, enter, 0);
    d.connect_nodes(structure, 0, enter, 1);

    assert_eq!(
        trace(&mut d, &atom_hit(enter, B)),
        "enter passed
exit passed
src source"
    );
    assert_eq!(only_probe(&probes), atom_probe(2, B));
}

// ============================================================================
// Region-gated atom ops (`doc/design_trace_coverage.md` Phase 3)
// ============================================================================

/// Inside the test region (x ≤ 1, x ≥ −50): the atom the node acts on.
const INSIDE: DVec3 = DVec3::new(0.0, 0.0, 0.0);
/// Outside the test region.
const OUTSIDE: DVec3 = DVec3::new(10.0, 0.0, 0.0);
/// Inside the test region: an atom the node leaves unchanged.
const INSIDE_KEPT: DVec3 = DVec3::new(-5.0, 0.0, 0.0);

type ProbeLog = std::sync::Arc<std::sync::Mutex<Vec<TraceProbe>>>;

fn molecule_of(atoms: AtomicStructure) -> NetworkResult {
    NetworkResult::Molecule(MoleculeData {
        atoms,
        geo_tree_root: None,
    })
}

/// `src` (a Molecule value of `atoms`) → `node` of `node_type`, configured by
/// `configure`. With `region_pin`, `region` = `intersect(hs, far)` is wired to
/// it: `hs` (x ≤ 1, a probe source, so the test reads the Point it is handed)
/// decides near the atoms, `far` (x ≥ −50) never does.
fn region_net(
    node_type: &str,
    atoms: AtomicStructure,
    region_pin: Option<usize>,
    configure: fn(&mut StructureDesigner, u64),
) -> (StructureDesigner, u64, ProbeLog) {
    let mut d = setup();
    let src = add_value(&mut d, molecule_of(atoms), "src");
    let node = add(&mut d, node_type, "node");
    configure(&mut d, node);
    d.connect_nodes(src, 0, node, 0);
    let (hs, probes) = add_probe_source(
        &mut d,
        "hs",
        SourceKind::Blueprint(Box::new(GeoNode::half_space(
            DVec3::X,
            DVec3::new(1.0, 0.0, 0.0),
        ))),
        vec![],
    );
    if let Some(region_pin) = region_pin {
        let far = add_value(
            &mut d,
            blueprint(GeoNode::half_space(-DVec3::X, DVec3::new(-50.0, 0.0, 0.0))),
            "far",
        );
        let region = add(&mut d, "intersect", "region");
        d.connect_nodes(hs, 0, region, 0);
        d.connect_nodes(far, 0, region, 0);
        d.connect_nodes(region, 0, node, region_pin);
    }
    (d, node, probes)
}

/// The Point the region's `hs` was handed, which must be at `at`.
fn assert_region_probed_at(probes: &ProbeLog, at: DVec3) {
    let TraceProbe::Point { position, .. } = only_probe(probes) else {
        panic!("the region gets a Point");
    };
    assert!(position.distance(at) < 1e-9, "{position} vs {at}");
}

/// The three wired-region cases and the unwired one for a region-gated op.
/// `atoms` has the atom the node acts on at [`INSIDE`], one at [`OUTSIDE`]
/// that it would act on were it inside, and, if `inside_kept`, one at
/// [`INSIDE_KEPT`] that it leaves unchanged.
fn assert_region_op(
    node_type: &str,
    region_pin: usize,
    label: &str,
    atoms: fn() -> AtomicStructure,
    configure: fn(&mut StructureDesigner, u64),
    inside_kept: bool,
) {
    // Outside the region: passed, region not listed.
    let (mut d, node, probes) = region_net(node_type, atoms(), Some(region_pin), configure);
    assert_eq!(
        trace(&mut d, &atom_hit(node, OUTSIDE)),
        "node passed\nsrc source",
        "{node_type}: outside"
    );
    assert!(probes.lock().unwrap().is_empty());

    // Inside, changed: the label, and the region down to the deciding `hs`,
    // probed at the atom.
    let (mut d, node, probes) = region_net(node_type, atoms(), Some(region_pin), configure);
    assert_eq!(
        trace(&mut d, &atom_hit(node, INSIDE)),
        format!("node {label}\nsrc source\nregion passed\nhs source"),
        "{node_type}: inside"
    );
    assert_region_probed_at(&probes, INSIDE);

    // Inside, unchanged: passed, region not listed.
    if inside_kept {
        let (mut d, node, probes) = region_net(node_type, atoms(), Some(region_pin), configure);
        assert_eq!(
            trace(&mut d, &atom_hit(node, INSIDE_KEPT)),
            "node passed\nsrc source",
            "{node_type}: inside, unchanged"
        );
        assert!(probes.lock().unwrap().is_empty());
    }

    // Region unwired: every atom is in the region.
    let (mut d, node, _) = region_net(node_type, atoms(), None, configure);
    assert_eq!(
        trace(&mut d, &atom_hit(node, INSIDE)),
        format!("node {label}\nsrc source"),
        "{node_type}: unwired"
    );
}

fn no_config(_: &mut StructureDesigner, _: u64) {}

fn carbons(positions: &[DVec3]) -> AtomicStructure {
    let mut atoms = AtomicStructure::new();
    for p in positions {
        atoms.add_atom(6, *p);
    }
    atoms
}

/// A carbon at `at` with one hydrogen bonded to it on its −x side.
fn add_ch(atoms: &mut AtomicStructure, at: DVec3) {
    let c = atoms.add_atom(6, at);
    let h = atoms.add_atom(1, at - DVec3::new(1.09, 0.0, 0.0));
    atoms.add_bond(c, h, 1);
}

#[test]
fn passivate_follows_its_region_only_for_a_host_it_passivated() {
    assert_region_op(
        "passivate",
        1,
        "passivated",
        || {
            let mut atoms = carbons(&[INSIDE, OUTSIDE]);
            // A saturated methane carbon: nothing to passivate.
            let c = atoms.add_atom(6, INSIDE_KEPT);
            for dir in [
                DVec3::new(1.0, 1.0, 1.0),
                DVec3::new(1.0, -1.0, -1.0),
                DVec3::new(-1.0, 1.0, -1.0),
                DVec3::new(-1.0, -1.0, 1.0),
            ] {
                let h = atoms.add_atom(1, INSIDE_KEPT + dir.normalize() * 1.09);
                atoms.add_bond(c, h, 1);
            }
            atoms
        },
        no_config,
        true,
    );
}

#[test]
fn passivate_traces_a_placed_hydrogen_to_its_host_and_the_region_at_the_host() {
    let (mut d, node, probes) =
        region_net("passivate", carbons(&[INSIDE, OUTSIDE]), Some(1), no_config);
    let atoms = output_atoms(&mut d, node);
    let (_, h) = atoms
        .iter_atoms()
        .find(|(_, a)| a.atomic_number == 1)
        .expect("passivate placed a hydrogen");
    assert_eq!(
        atoms.get_atom(h.bonds[0].other_atom_id()).unwrap().position,
        INSIDE,
        "precondition: only the in-region carbon is passivated"
    );
    assert_eq!(
        trace(&mut d, &atom_hit(node, h.position)),
        "node passivated\nsrc source\nregion passed\nhs source"
    );
    assert_region_probed_at(&probes, INSIDE);
}

#[test]
fn remove_hydrogen_follows_its_region_only_for_a_host_it_stripped() {
    assert_region_op(
        "remove_hydrogen",
        1,
        "depassivated",
        || {
            let mut atoms = AtomicStructure::new();
            add_ch(&mut atoms, INSIDE);
            add_ch(&mut atoms, OUTSIDE);
            // A bare carbon: nothing to strip.
            atoms.add_atom(6, INSIDE_KEPT);
            atoms
        },
        no_config,
        true,
    );
}

#[test]
fn atom_replace_follows_its_region_only_for_an_atom_it_replaced() {
    assert_region_op(
        "atom_replace",
        2,
        "replaced",
        || {
            let mut atoms = carbons(&[INSIDE, OUTSIDE]);
            // Nitrogen: no rule for it.
            atoms.add_atom(7, INSIDE_KEPT);
            atoms
        },
        |d, node| data_mut::<AtomReplaceData>(d, node).replacements = vec![(6, 14)],
        true,
    );
}

#[test]
fn freeze_and_unfreeze_follow_their_region_only_for_an_atom_they_flipped() {
    assert_region_op(
        "freeze",
        1,
        "frozen",
        || {
            let mut atoms = carbons(&[INSIDE, OUTSIDE, INSIDE_KEPT]);
            atoms.set_atom_frozen(3, true);
            atoms
        },
        no_config,
        true,
    );
    assert_region_op(
        "unfreeze",
        1,
        "unfrozen",
        || {
            let mut atoms = carbons(&[INSIDE, OUTSIDE, INSIDE_KEPT]);
            atoms.set_atom_frozen(1, true);
            atoms.set_atom_frozen(2, true);
            atoms
        },
        no_config,
        true,
    );
}

#[test]
fn tag_and_untag_follow_their_region_only_for_an_atom_they_changed() {
    assert_region_op(
        "tag",
        2,
        "tagged",
        || {
            let mut atoms = carbons(&[INSIDE, OUTSIDE, INSIDE_KEPT]);
            atoms.add_atom_tag(3, "t").unwrap();
            atoms
        },
        |d, node| data_mut::<TagData>(d, node).name = "t".to_string(),
        true,
    );
    // `untag` with an empty name clears every tag.
    assert_region_op(
        "untag",
        2,
        "untagged",
        || {
            let mut atoms = carbons(&[INSIDE, OUTSIDE, INSIDE_KEPT]);
            atoms.add_atom_tag(1, "t").unwrap();
            atoms.add_atom_tag(2, "t").unwrap();
            atoms
        },
        no_config,
        true,
    );
}

#[test]
fn xray_follows_its_region_for_every_atom_inside_it() {
    // The alpha lives in the decorator, so every in-region atom is "faded":
    // there is no unchanged in-region case.
    assert_region_op(
        "xray",
        2,
        "faded",
        || carbons(&[INSIDE, OUTSIDE]),
        no_config,
        false,
    );
}

/// `infer_bonds` (`RegionGate::Bond`): C at x = 0.5 (in the region) and 2.0
/// (outside) bond to each other; C at 10 (outside) and −10 (inside) stay
/// alone.
fn infer_bonds_net() -> (StructureDesigner, u64, ProbeLog) {
    region_net(
        "infer_bonds",
        carbons(&[
            DVec3::new(0.5, 0.0, 0.0),
            DVec3::new(2.0, 0.0, 0.0),
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(-10.0, 0.0, 0.0),
        ]),
        Some(3),
        no_config,
    )
}

#[test]
fn infer_bonds_follows_its_region_for_an_outside_atom_bonded_to_an_inside_one() {
    let inside = DVec3::new(0.5, 0.0, 0.0);
    let (mut d, node, probes) = infer_bonds_net();
    assert!(
        output_atoms(&mut d, node).has_bond_between(1, 2),
        "precondition: the pair bonded"
    );
    assert_eq!(
        trace(&mut d, &atom_hit(node, DVec3::new(2.0, 0.0, 0.0))),
        "node bonded\nsrc source\nregion passed\nhs source"
    );
    // Probed at the in-region partner, not at the atom itself.
    assert_region_probed_at(&probes, inside);

    let (mut d, node, probes) = infer_bonds_net();
    assert_eq!(
        trace(&mut d, &atom_hit(node, inside)),
        "node bonded\nsrc source\nregion passed\nhs source"
    );
    assert_region_probed_at(&probes, inside);
}

#[test]
fn infer_bonds_passes_an_atom_whose_bonds_did_not_change() {
    for at in [DVec3::new(10.0, 0.0, 0.0), DVec3::new(-10.0, 0.0, 0.0)] {
        let (mut d, node, probes) = infer_bonds_net();
        assert_eq!(
            trace(&mut d, &atom_hit(node, at)),
            "node passed\nsrc source",
            "{at}"
        );
        assert!(probes.lock().unwrap().is_empty());
    }
}

#[test]
fn region_contains_agrees_with_the_region_gated_ops_at_the_margin() {
    // A half space through the origin: an atom at x has signed distance x, so
    // x = DEFAULT_REGION_MARGIN is exactly on the margin.
    let geo = GeoNode::half_space(DVec3::X, DVec3::ZERO);
    let positions: Vec<DVec3> = [
        0.05,
        DEFAULT_REGION_MARGIN,
        DEFAULT_REGION_MARGIN + 1e-9,
        0.2,
    ]
    .iter()
    .map(|x| DVec3::new(*x, 0.0, 0.0))
    .collect();
    let mut d = setup();
    let src = add_value(&mut d, molecule_of(carbons(&positions)), "src");
    let region = add_value(&mut d, blueprint(geo.clone()), "region");
    let freeze = add(&mut d, "freeze", "freeze");
    d.connect_nodes(src, 0, freeze, 0);
    d.connect_nodes(region, 0, freeze, 1);
    let atoms = output_atoms(&mut d, freeze);
    for (i, p) in positions.iter().enumerate() {
        let frozen = atoms.get_atom(i as u32 + 1).unwrap().is_frozen();
        assert_eq!(frozen, region_contains(&geo, *p), "x = {}", p.x);
    }
    assert!(region_contains(&geo, positions[1]), "the margin is inside");
    assert!(!region_contains(&geo, positions[3]));
}

// ============================================================================
// apply_style, edit_atom, proxy
// ============================================================================

#[test]
fn apply_style_passes_an_atom_without_listing_its_rules() {
    let mut d = setup();
    let (src, probes) = add_probe_source(&mut d, "src", SourceKind::Molecule, vec![(6, A), (6, B)]);
    let rules = {
        let network = d.node_type_registry.node_networks.get_mut(MAIN).unwrap();
        add_typed_value_node(
            network,
            DVec2::ZERO,
            NetworkResult::Array(vec![]),
            DataType::Array(Box::new(DataType::Record(RecordType::Named(
                "StyleRule".to_string(),
            )))),
        )
    };
    let style = add(&mut d, "apply_style", "style");
    d.connect_nodes(src, 0, style, 0);
    d.connect_nodes(rules, 0, style, 1);
    assert_eq!(
        trace(&mut d, &atom_hit(style, B)),
        "style passed\nsrc source"
    );
    assert_eq!(only_probe(&probes), atom_probe(2, B));
}

/// `src` (C at 0, 3 and 6 Å along x, ids 1–3) → `edit` (`edit_atom`), whose
/// history moves atom 1 up by 1 Å and adds an N at 10 Å.
fn edit_atom_chain() -> (StructureDesigner, u64, ProbeLog) {
    let mut d = setup();
    let (src, probes) = add_probe_source(
        &mut d,
        "src",
        SourceKind::Molecule,
        vec![
            (6, A),
            (6, DVec3::new(3.0, 0.0, 0.0)),
            (6, DVec3::new(6.0, 0.0, 0.0)),
        ],
    );
    let edit = add(&mut d, "edit_atom", "edit");
    d.connect_nodes(src, 0, edit, 0);
    let data = data_mut::<EditAtomData>(&mut d, edit);
    data.add_command(Box::new(SelectCommand::new(
        vec![1],
        vec![],
        SelectModifier::Replace,
    )));
    data.add_command(Box::new(TransformCommand::new(Transform::new(
        DVec3::new(0.0, 0.0, 1.0),
        DQuat::IDENTITY,
    ))));
    data.add_command(Box::new(AddAtomCommand::new(7, DVec3::new(10.0, 0.0, 0.0))));
    (d, edit, probes)
}

#[test]
fn edit_atom_an_added_atom_stops_at_the_node() {
    let (mut d, edit, probes) = edit_atom_chain();
    assert_eq!(
        trace(&mut d, &atom_hit(edit, DVec3::new(10.0, 0.0, 0.0))),
        "edit added"
    );
    assert!(probes.lock().unwrap().is_empty());
}

#[test]
fn edit_atom_a_moved_atom_is_edited_with_its_id() {
    let (mut d, edit, probes) = edit_atom_chain();
    assert_eq!(
        trace(&mut d, &atom_hit(edit, DVec3::new(0.0, 0.0, 1.0))),
        "edit edited\nsrc source"
    );
    assert_eq!(only_probe(&probes), atom_probe(1, A));
}

#[test]
fn edit_atom_an_untouched_atom_is_passed() {
    let (mut d, edit, probes) = edit_atom_chain();
    let untouched = DVec3::new(3.0, 0.0, 0.0);
    assert_eq!(
        trace(&mut d, &atom_hit(edit, untouched)),
        "edit passed\nsrc source"
    );
    assert_eq!(only_probe(&probes), atom_probe(2, untouched));
}

/// A chain of six carbons along x (ids 1–6, 1.54 Å apart), atom 1 tagged `f`
/// → `proxy` keeping two hops around it and capping the severed bond.
fn proxy_chain() -> (StructureDesigner, u64) {
    let mut d = setup();
    let mut atoms = AtomicStructure::new();
    let ids: Vec<u32> = (0..6)
        .map(|i| atoms.add_atom(6, DVec3::new(1.54 * i as f64, 0.0, 0.0)))
        .collect();
    for pair in ids.windows(2) {
        atoms.add_bond(pair[0], pair[1], 1);
    }
    atoms.add_atom_tag(ids[0], "f").unwrap();
    let src = add_value(&mut d, molecule_of(atoms), "src");
    let proxy = add(&mut d, "proxy", "proxy");
    let data = data_mut::<ProxyData>(&mut d, proxy);
    data.focus = "f".to_string();
    data.hops = 2;
    data.rim = 0;
    data.rm_single = false;
    data.passivate = true;
    data.fill = false;
    data.core = -1;
    d.connect_nodes(src, 0, proxy, 0);
    (d, proxy)
}

#[test]
fn proxy_passes_a_kept_atom_found_by_position() {
    let (mut d, proxy) = proxy_chain();
    assert_eq!(
        trace(&mut d, &atom_hit(proxy, DVec3::new(1.54, 0.0, 0.0))),
        "proxy passed\nsrc source"
    );
}

#[test]
fn proxy_traces_a_cap_to_its_host() {
    let (mut d, proxy) = proxy_chain();
    let atoms = output_atoms(&mut d, proxy);
    let (_, cap) = atoms
        .iter_atoms()
        .find(|(_, a)| a.atomic_number == 1)
        .expect("precondition: a severed bond was capped");
    let host = atoms.get_atom(cap.bonds[0].other_atom_id()).unwrap();
    assert_eq!(
        host.atomic_number, 6,
        "precondition: the cap sits on a carbon"
    );
    assert_eq!(
        trace(&mut d, &atom_hit(proxy, cap.position)),
        "proxy capped\nsrc source"
    );
}

// ============================================================================
// apply_diff
// ============================================================================

#[test]
fn apply_diff_of_an_atom_cut_lists_only_the_base_chain_for_an_untouched_atom() {
    let mut d = setup();
    let (src, probes) = add_probe_source(
        &mut d,
        "src",
        SourceKind::Molecule,
        vec![(6, A), (6, DVec3::new(3.0, 0.0, 0.0))],
    );
    let cutter = add_value(
        &mut d,
        blueprint(GeoNode::half_space(DVec3::X, DVec3::new(1.0, 0.0, 0.0))),
        "cutter",
    );
    let cut = add(&mut d, "atom_cut", "cut");
    set_node_data(
        &mut d,
        MAIN,
        cut,
        Box::new(AtomCutData {
            cut_sdf_value: 0.0,
            unit_cell_size: 1.0,
        }),
    );
    d.connect_nodes(src, 0, cut, 0);
    d.connect_nodes(cutter, 0, cut, 1);
    let apply = add(&mut d, "apply_diff", "apply");
    d.connect_nodes(src, 0, apply, 0);
    d.connect_nodes(cut, 1, apply, 1);
    assert_eq!(
        output_atoms(&mut d, apply).get_num_of_atoms(),
        1,
        "precondition: the cut applied"
    );

    assert_eq!(
        trace(&mut d, &atom_hit(apply, A)),
        "apply passed\nsrc source"
    );
    assert_eq!(only_probe(&probes), atom_probe(1, A));
}

/// `src` → `edit` (`atom_edit`, as in `atom_edit_chain`); `apply` applies
/// `edit`'s `diff` pin to `src`.
fn apply_atom_edit_diff() -> (StructureDesigner, u64) {
    let (mut d, edit, _) = atom_edit_chain();
    let src = d
        .get_active_node_network()
        .unwrap()
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some("src"))
        .unwrap()
        .id;
    let apply = add(&mut d, "apply_diff", "apply");
    d.connect_nodes(src, 0, apply, 0);
    d.connect_nodes(edit, 1, apply, 1);
    (d, apply)
}

#[test]
fn apply_diff_of_an_atom_edit_follows_each_chain_only_for_its_own_atoms() {
    // Untouched: the base chain only.
    let (mut d, apply) = apply_atom_edit_diff();
    assert_eq!(
        trace(&mut d, &atom_hit(apply, DVec3::new(3.0, 0.0, 0.0))),
        "apply passed\nsrc source"
    );
    // Edited: both chains. `edit`'s `diff` pin has no `trace` and falls back
    // (`doc/design_trace_coverage.md`, apply_diff).
    let (mut d, apply) = apply_atom_edit_diff();
    assert_eq!(
        trace(&mut d, &atom_hit(apply, DVec3::new(0.0, 0.0, 1.0))),
        "apply edited\nsrc source\nedit possibly contributed"
    );
    // Added: the diff chain only. The fallback loses the probe at `edit`'s
    // input (the added atom is not there), so `src` is only wired upstream.
    let (mut d, apply) = apply_atom_edit_diff();
    assert_eq!(
        trace(&mut d, &atom_hit(apply, DVec3::new(10.0, 0.0, 0.0))),
        "apply added\nedit possibly contributed\nsrc wired upstream"
    );
}

// ============================================================================
// mechanosynth / mechanosynth_edit
// ============================================================================

/// The tool fixtures of `mechanosynth_tools_node_test.rs`: two methanes
/// (`tool_scene.xyz`, ids 1–10), the reservoir (`tool_dump.xyz`), the
/// charged tip and the bare probe, against `tool_ops.json` /
/// `tool_build.json`.
fn ms_fixture(name: &str) -> String {
    fixture_path_str(&format!("mechanosynth/{name}"))
}

fn ms_molecule(name: &str) -> AtomicStructure {
    load_xyz(&ms_fixture(name), true).unwrap()
}

fn sorted_ids(s: &AtomicStructure) -> Vec<u32> {
    let mut ids: Vec<u32> = s.iter_atoms().map(|(id, _)| *id).collect();
    ids.sort_unstable();
    ids
}

/// A tool molecule tagged as a design tags it: the type on every atom, then
/// `apex` and the three legs on the frame atoms (apex first, legs at 3, 4, 5).
fn ms_tool(name: &str, tool_type: &str) -> AtomicStructure {
    let mut s = ms_molecule(name);
    let ids = sorted_ids(&s);
    for id in &ids {
        s.add_atom_tag(*id, tool_type).unwrap();
    }
    s.add_atom_tag(ids[0], APEX_FRAME_TAG).unwrap();
    for (tag, index) in ["a", "b", "c"].into_iter().zip([3, 4, 5]) {
        s.add_atom_tag(ids[index], tag).unwrap();
    }
    s
}

/// `base`, `dump`, `tip`, `probe` (value nodes), `ops` and `steps` (from the
/// fixture files) wired into a `node_type` (`mechanosynth` or
/// `mechanosynth_edit`) named `ms`, whose feedstock and tool pins are
/// `feedstocks_pin` / `tools_pin`.
fn ms_net(node_type: &str, feedstocks_pin: usize, tools_pin: usize) -> (StructureDesigner, u64) {
    ms_net_with_probe(node_type, feedstocks_pin, tools_pin, DVec3::ZERO)
}

/// [`ms_net`] with the probe translated by `probe_shift`.
fn ms_net_with_probe(
    node_type: &str,
    feedstocks_pin: usize,
    tools_pin: usize,
    probe_shift: DVec3,
) -> (StructureDesigner, u64) {
    let mut probe_atoms = ms_tool("tool_probe.xyz", "probe");
    for id in sorted_ids(&probe_atoms) {
        let p = probe_atoms.get_atom(id).unwrap().position;
        probe_atoms.set_atom_position(id, p + probe_shift);
    }
    let mut d = setup();
    let base = add_value(&mut d, molecule_of(ms_molecule("tool_scene.xyz")), "base");
    let dump = add_value(&mut d, molecule_of(ms_molecule("tool_dump.xyz")), "dump");
    let tip = add_value(
        &mut d,
        molecule_of(ms_tool("tool_tip.xyz", "habst_tool")),
        "tip",
    );
    let probe = add_value(&mut d, molecule_of(probe_atoms), "probe");
    let ops = add(&mut d, "ops_library", "ops");
    let data = data_mut::<OpsLibraryData>(&mut d, ops);
    data.file = Some(ms_fixture("tool_ops.json"));
    data.reload_missing(None);
    let steps = add(&mut d, "build_script", "steps");
    let data = data_mut::<BuildScriptData>(&mut d, steps);
    data.file = Some(ms_fixture("tool_build.json"));
    data.reload_missing(None);
    let ms = add(&mut d, node_type, "ms");
    d.connect_nodes(base, 0, ms, 0);
    d.connect_nodes(ops, 0, ms, 1);
    d.connect_nodes(steps, 0, ms, 2);
    d.connect_nodes(dump, 0, ms, feedstocks_pin);
    d.connect_nodes(tip, 0, ms, tools_pin);
    d.connect_nodes(probe, 0, ms, tools_pin);
    (d, ms)
}

fn mechanosynth_net(step: i32, time: f64) -> (StructureDesigner, u64) {
    let (mut d, ms) = ms_net("mechanosynth", 4, 5);
    let data = data_mut::<MechanosynthData>(&mut d, ms);
    data.step = step;
    data.time = time;
    (d, ms)
}

fn pin_hit(node_id: u64, pin: usize, position: DVec3) -> TraceHit {
    TraceHit {
        node: NodeRef::top(node_id),
        pin,
        target: TraceHitTarget::Atom { position },
    }
}

fn pin_atoms(d: &mut StructureDesigner, node_id: u64, pin: i32) -> AtomicStructure {
    d.evaluate_node_output(&[], node_id, pin)
        .extract_atomic()
        .expect("an atomic output")
}

fn has_atom_at(atoms: &AtomicStructure, position: DVec3) -> bool {
    atoms
        .iter_atoms()
        .any(|(_, a)| a.position.distance(position) < 1e-4)
}

/// Methane B's `+x` hydrogen: no step touches it.
const MS_UNTOUCHED_H: DVec3 = DVec3::new(11.027662, 0.0, -0.363333);
/// Where `settle` moves methane A's hydrogen at (−0.51, 0.89, −0.36): 0.2 Å up.
const MS_SETTLED_H: DVec3 = DVec3::new(-0.513831, 0.889981, -0.163333);
/// The fluorine the first `expose` deposits on methane A's carbon.
const MS_DEPOSITED_F: DVec3 = DVec3::new(0.0, 0.0, 1.4);

#[test]
fn mechanosynth_passes_an_untouched_base_atom_without_listing_tools_or_feedstocks() {
    let (mut d, ms) = mechanosynth_net(-1, 1.0);
    assert_eq!(
        trace(&mut d, &atom_hit(ms, MS_UNTOUCHED_H)),
        "ms passed\nbase source"
    );
}

#[test]
fn mechanosynth_reports_a_base_atom_a_step_moved_as_reacted() {
    let (mut d, ms) = mechanosynth_net(-1, 1.0);
    assert!(
        has_atom_at(&pin_atoms(&mut d, ms, 0), MS_SETTLED_H),
        "precondition: settle moved the hydrogen"
    );
    assert_eq!(
        trace(&mut d, &atom_hit(ms, MS_SETTLED_H)),
        "ms reacted\nbase source"
    );
}

#[test]
fn mechanosynth_stops_at_a_deposited_atom() {
    let (mut d, ms) = mechanosynth_net(-1, 1.0);
    assert!(
        has_atom_at(&pin_atoms(&mut d, ms, 0), MS_DEPOSITED_F),
        "precondition: expose deposited the fluorine"
    );
    assert_eq!(trace(&mut d, &atom_hit(ms, MS_DEPOSITED_F)), "ms deposited");
}

#[test]
fn mechanosynth_scene_lists_only_the_tool_or_reservoir_an_atom_belongs_to() {
    // The probe is shifted so that its leg `a` sits exactly where the tip's
    // does in the inputs: a match by position alone would find both, so only
    // the participant map keeps the probe off the list.
    let tip = ms_molecule("tool_tip.xyz");
    let input = tip.get_atom(sorted_ids(&tip)[3]).unwrap().position;
    let probe = ms_molecule("tool_probe.xyz");
    let probe_leg = probe.get_atom(sorted_ids(&probe)[3]).unwrap().position;
    let (mut d, ms) = ms_net_with_probe("mechanosynth", 4, 5, input - probe_leg);
    // Step 1 a quarter in: the tip is flying to its site, away from where
    // its input has it.
    let data = data_mut::<MechanosynthData>(&mut d, ms);
    data.step = 1;
    data.time = 0.25;
    let scene = pin_atoms(&mut d, ms, 2);
    let leg = scene
        .atoms_with_tag("habst_tool")
        .into_iter()
        .find(|id| scene.atom_has_tag(*id, "a"))
        .expect("the tip's leg a is in the scene");
    let posed = scene.get_atom(leg).unwrap().position;
    assert!(
        posed.distance(input) > 1.0,
        "precondition: the tip is posed away from its input position"
    );
    assert_eq!(
        trace(&mut d, &pin_hit(ms, 2, posed)),
        "ms passed\ntip source"
    );
    // A reservoir carbon: the reservoir alone.
    assert_eq!(
        trace(&mut d, &pin_hit(ms, 2, DVec3::new(20.0, 0.0, 0.0))),
        "ms passed\ndump source"
    );
}

#[test]
fn mechanosynth_edit_reports_prefix_and_block_deposits_and_passes_a_base_atom() {
    let (mut d, edit) = ms_net("mechanosynth_edit", 3, 4);
    let data = data_mut::<MechanosynthEditData>(&mut d, edit);
    // After the whole prefix, fluorinate the reservoir's first carbon.
    data.authored = vec![AuthoredStep {
        step: Step::new("expose".to_string(), DVec3::new(20.0, 0.0, 0.0)),
        residual: 0.0,
        approximate: false,
    }];
    data.cursor = 1;
    let block_f = DVec3::new(20.0, 0.0, 1.4);
    let scene = pin_atoms(&mut d, edit, 2);
    assert!(
        has_atom_at(&scene, block_f) && has_atom_at(&scene, MS_DEPOSITED_F),
        "precondition: both the prefix and the block deposited"
    );
    assert_eq!(
        trace(&mut d, &pin_hit(edit, 2, MS_DEPOSITED_F)),
        "ms deposited"
    );
    assert_eq!(trace(&mut d, &pin_hit(edit, 2, block_f)), "ms deposited");
    assert_eq!(
        trace(&mut d, &pin_hit(edit, 2, MS_UNTOUCHED_H)),
        "ms passed\nbase source"
    );
}

// ============================================================================
// patch_latticefill
// ============================================================================

/// A bulk carbon far from the region.
const BULK: DVec3 = DVec3::new(10.0, 10.0, 10.0);
/// The tile's one interior atom, as cell 0 places it.
const TILE_ATOM: DVec3 = DVec3::new(1.0, 1.0, 1.0);

/// `target` (one bulk carbon) and `region` (a sphere admitting cell 0 only)
/// wired into `fill`, with a one-atom tile tiled along x and a cut volume that
/// cuts nothing.
fn patch_net() -> (StructureDesigner, u64) {
    let mut d = setup();
    let target = add_value(&mut d, crystal(&[(6, BULK)], None), "target");
    let region = add_value(&mut d, blueprint(GeoNode::sphere(TILE_ATOM, 1.5)), "region");
    let mut tile = AtomicStructure::new();
    tile.add_atom(6, TILE_ATOM);
    let patch_value = NetworkResult::record(vec![
        ("tile".to_string(), molecule_of(tile)),
        (
            "tiling_vectors".to_string(),
            NetworkResult::Array(vec![NetworkResult::IVec3(IVec3::new(1, 0, 0))]),
        ),
        (
            "cut_volume".to_string(),
            blueprint(GeoNode::sphere(DVec3::new(-50.0, -50.0, -50.0), 0.0)),
        ),
    ]);
    let network = d.node_type_registry.node_networks.get_mut(MAIN).unwrap();
    let patch = add_typed_value_node(
        network,
        DVec2::ZERO,
        patch_value,
        DataType::Record(RecordType::Named("Patch".to_string())),
    );
    name(&mut d, MAIN, patch, "patch");
    let fill = add(&mut d, "patch_latticefill", "fill");
    let data = data_mut::<PatchLatticeFillData>(&mut d, fill);
    data.passivate = false;
    // Test on the plane through the lattice origin: cell k's atom tests at
    // (1 + 3.567 k, 0, 0), which the sphere admits for k = 0 only.
    data.test_height_at_origin = true;
    d.connect_nodes(target, 0, fill, 0);
    d.connect_nodes(region, 0, fill, 1);
    d.connect_nodes(patch, 0, fill, 2);
    (d, fill)
}

#[test]
fn patch_latticefill_passes_a_bulk_atom_without_listing_the_region() {
    let (mut d, fill) = patch_net();
    assert_eq!(
        trace(&mut d, &atom_hit(fill, BULK)),
        "fill passed\ntarget source"
    );
}

#[test]
fn patch_latticefill_traces_a_tile_atom_to_the_region() {
    let (mut d, fill) = patch_net();
    assert!(
        has_atom_at(&output_atoms(&mut d, fill), TILE_ATOM),
        "precondition: cell 0 was tiled"
    );
    assert_eq!(
        trace(&mut d, &atom_hit(fill, TILE_ATOM)),
        "fill patched\nregion shape"
    );
}

// ============================================================================
// chemisorb (debug pin)
// ============================================================================

#[test]
fn chemisorb_debug_root_view_lists_only_the_side_an_atom_came_from() {
    let mut d = setup();
    // •OH over a silyl radical.
    let mut adsorbate = AtomicStructure::new();
    let o = adsorbate.add_atom(8, DVec3::new(0.3, 0.2, 2.2));
    let h = adsorbate.add_atom(1, DVec3::new(0.3, 0.2, 3.17));
    adsorbate.add_bond(o, h, 1);
    let mut substrate = AtomicStructure::new();
    let si = substrate.add_atom(14, DVec3::ZERO);
    let (sin, cos) = ((8.0f64 / 9.0).sqrt(), -1.0 / 3.0);
    for deg in [0.0f64, 120.0, 240.0] {
        let phi = deg.to_radians();
        let h = substrate.add_atom(1, DVec3::new(sin * phi.cos(), sin * phi.sin(), cos) * 1.48);
        substrate.add_bond(si, h, 1);
    }
    let ads = add_value(&mut d, molecule_of(adsorbate), "ads");
    let sub = add_value(&mut d, molecule_of(substrate), "sub");
    let chem = add(&mut d, "chemisorb", "chem");
    d.connect_nodes(ads, 0, chem, 0);
    d.connect_nodes(sub, 0, chem, 1);
    assert_eq!(
        pin_atoms(&mut d, chem, 2).get_num_of_atoms(),
        6,
        "precondition: the root view is the posed pair"
    );

    assert_eq!(
        trace(&mut d, &pin_hit(chem, 2, DVec3::new(0.3, 0.2, 2.2))),
        "chem passed\nads source"
    );
    assert_eq!(
        trace(&mut d, &pin_hit(chem, 2, DVec3::ZERO)),
        "chem passed\nsub source"
    );
}
