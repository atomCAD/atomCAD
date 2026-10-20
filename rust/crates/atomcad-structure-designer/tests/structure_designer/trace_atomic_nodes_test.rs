//! Phase 3 of `doc/design_output_driven_editing.md`: `trace` on the atomic
//! nodes (the movement nodes, `relax`, `atom_union`, `atom_edit`,
//! `materialize`) and on the array plumbing nodes (`sequence`, `array_at`,
//! `array_concat`).

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::guided_placement::crystal_bond_length;
use atomcad_geo_tree::GeoNode;
use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::nodes::array_at::ArrayAtData;
use atomcad_structure_designer::nodes::array_concat::ArrayConcatData;
use atomcad_structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
use atomcad_structure_designer::nodes::free_move::FreeMoveData;
use atomcad_structure_designer::nodes::free_rot::FreeRotData;
use atomcad_structure_designer::nodes::materialize::MaterializeData;
use atomcad_structure_designer::nodes::sequence::SequenceData;
use atomcad_structure_designer::nodes::structure_invert::StructureInvertData;
use atomcad_structure_designer::nodes::structure_move::StructureMoveData;
use atomcad_structure_designer::nodes::structure_rot::StructureRotData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::trace::{
    ATOM_POINT_DELTA_BONDS, TraceProbe, atom_point_delta, is_terminator,
};
use glam::f64::DVec3;
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
