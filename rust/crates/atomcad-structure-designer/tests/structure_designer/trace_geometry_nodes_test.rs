//! Phase 4 of `doc/design_output_driven_editing.md`: `trace` on the geometry
//! nodes (`union`, `intersect`, `diff`, `geo_trans`) and the inverse-point path
//! of the movement nodes on Blueprints.
//!
//! Phase 2 of `doc/design_trace_coverage.md`: the point paths of the phase
//! pass-throughs (`exit_structure`, `enter_structure`, `with_structure`,
//! `dematerialize`), `materialize`'s Point path, and `lattice_symop`.
//!
//! Atoms are chosen **by query** on the materialized output (the outermost
//! carbon along a direction, see [`outermost`]), never by id or hard-coded
//! position, because both depend on the fill.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::guided_placement::crystal_bond_length;
use atomcad_crystolecule::structure::Structure;
use atomcad_geo_tree::GeoNode;
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::node_network::NodeRef;
use atomcad_structure_designer::nodes::free_move::FreeMoveData;
use atomcad_structure_designer::nodes::free_rot::FreeRotData;
use atomcad_structure_designer::nodes::geo_trans::GeoTransData;
use atomcad_structure_designer::nodes::lattice_symop::LatticeSymopData;
use atomcad_structure_designer::nodes::structure_invert::StructureInvertData;
use atomcad_structure_designer::nodes::structure_move::StructureMoveData;
use atomcad_structure_designer::nodes::structure_rot::StructureRotData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::trace::{
    SURFACE_HIT_DELTA, TraceHit, TraceHitTarget, TraceProbe, deciding_children, sdf_at,
};
use glam::f64::DVec3;
use glam::i32::IVec3;

use crate::trace_support::*;

/// The C–C bond length; `materialize` hands the geometry
/// `ATOM_POINT_DELTA_BONDS` of them as δ.
fn bond() -> f64 {
    crystal_bond_length(6, 6).unwrap()
}

/// How far from every other face an atom must be to be decided by its own
/// face alone: δ (two bonds) plus the up to ~2.5 Å a filled atom can sit below
/// its own face.
fn clear_of_other_faces() -> f64 {
    4.0 * bond()
}

/// A plane, as (outward unit normal, a point on it).
type Plane = (DVec3, DVec3);

fn half_space((normal, point): Plane) -> GeoNode {
    GeoNode::half_space(normal, point)
}

/// The axis-aligned box `[min, max]`.
fn cube(min: DVec3, max: DVec3) -> GeoNode {
    GeoNode::intersection_3d(vec![
        GeoNode::half_space(DVec3::X, max),
        GeoNode::half_space(DVec3::Y, max),
        GeoNode::half_space(DVec3::Z, max),
        GeoNode::half_space(-DVec3::X, min),
        GeoNode::half_space(-DVec3::Y, min),
        GeoNode::half_space(-DVec3::Z, min),
    ])
}

/// The carbon of `atoms` with the largest projection on `direction`, among
/// those `keep` accepts.
fn outermost_where(
    atoms: &AtomicStructure,
    direction: DVec3,
    keep: impl Fn(DVec3) -> bool,
) -> DVec3 {
    atoms
        .iter_atoms()
        .filter(|(_, a)| a.atomic_number == 6 && keep(a.position))
        .map(|(_, a)| a.position)
        .max_by(|a, b| a.dot(direction).total_cmp(&b.dot(direction)))
        .expect("no atom passes the query")
}

/// The carbon with the largest projection on `direction`, among those more
/// than `distance` inside each of `planes`.
fn outermost(atoms: &AtomicStructure, direction: DVec3, planes: &[Plane], distance: f64) -> DVec3 {
    outermost_where(atoms, direction, |p| {
        planes.iter().all(|(n, c)| n.dot(p - *c) < -distance)
    })
}

/// Add a node of `node_type` with `data` and name it `label`.
fn add_with(
    d: &mut StructureDesigner,
    node_type: &str,
    label: &str,
    data: Box<dyn NodeData>,
) -> u64 {
    let id = add(d, node_type, label);
    set_node_data(d, MAIN, id, data);
    name(d, MAIN, id, label);
    id
}

// ============================================================================
// Helpers
// ============================================================================

#[test]
fn deciding_children_keeps_ties_within_delta_and_none_outside() {
    assert_eq!(deciding_children(&[1.0, 1.05, 2.0], 1.0, 0.1), vec![0, 1]);
    assert_eq!(deciding_children(&[1.0, 1.2, 2.0], 1.0, 0.1), vec![0]);
    assert_eq!(
        deciding_children(&[3.0, 2.0], 1.0, 0.1),
        Vec::<usize>::new()
    );
}

#[test]
fn sdf_at_evaluates_a_geometry_value_and_ignores_others() {
    let value = blueprint(GeoNode::sphere(DVec3::ZERO, 2.0));
    assert!((sdf_at(&value, DVec3::new(5.0, 0.0, 0.0)).unwrap() - 3.0).abs() < 1e-9);
    assert!((sdf_at(&value, DVec3::ZERO).unwrap() + 2.0).abs() < 1e-9);
    assert!(sdf_at(&molecule(&[(6, DVec3::ZERO)]), DVec3::ZERO).is_none());
}

// ============================================================================
// union
// ============================================================================

/// `sphere` (r 5 at the origin) ∪ `cube` (to the right of it) → `fill`.
fn sphere_union_cube() -> (StructureDesigner, u64, u64) {
    let mut d = setup();
    let sphere = add_value(
        &mut d,
        blueprint(GeoNode::sphere(DVec3::ZERO, 5.0)),
        "sphere",
    );
    let cube = add_value(
        &mut d,
        blueprint(cube(
            DVec3::new(3.0, -4.0, -4.0),
            DVec3::new(11.0, 4.0, 4.0),
        )),
        "cube",
    );
    let union = add(&mut d, "union", "union");
    d.connect_nodes(sphere, 0, union, 0);
    d.connect_nodes(cube, 0, union, 0);
    let fill = add(&mut d, "materialize", "fill");
    d.connect_nodes(union, 0, fill, 0);
    (d, union, fill)
}

#[test]
fn union_an_atom_on_the_far_side_of_the_sphere_prunes_the_cube() {
    let (mut d, _, fill) = sphere_union_cube();
    let atoms = output_atoms(&mut d, fill);
    let atom = outermost(&atoms, -DVec3::X, &[], 0.0);
    assert_eq!(
        trace(&mut d, &atom_hit(fill, atom)),
        "fill filled\nunion passed\nsphere shape"
    );
}

#[test]
fn union_an_atom_on_the_far_side_of_the_cube_prunes_the_sphere() {
    let (mut d, _, fill) = sphere_union_cube();
    let atoms = output_atoms(&mut d, fill);
    let atom = outermost(&atoms, DVec3::X, &[], 0.0);
    assert_eq!(
        trace(&mut d, &atom_hit(fill, atom)),
        "fill filled\nunion passed\ncube shape"
    );
}

// ============================================================================
// intersect: the tetrahedron
// ============================================================================

/// The four faces of a regular tetrahedron with inradius 6 Å centred at
/// `centre`, as outward planes. Face *i*'s opposite vertex lies along
/// `-normal_i`.
fn tetrahedron_faces_at(centre: DVec3) -> [Plane; 4] {
    [
        DVec3::new(1.0, 1.0, 1.0),
        DVec3::new(1.0, -1.0, -1.0),
        DVec3::new(-1.0, 1.0, -1.0),
        DVec3::new(-1.0, -1.0, 1.0),
    ]
    .map(|n| {
        let n = n.normalize();
        (n, centre + n * 6.0)
    })
}

fn tetrahedron_faces() -> [Plane; 4] {
    tetrahedron_faces_at(DVec3::ZERO)
}

/// Where the tetrahedron tests put it. Centred on a lattice atom, diamond's
/// tetrahedral symmetry makes the four faces cut the lattice alike, which is
/// the easy case; off centre, the faces of an edge sit at different depths
/// below the outermost atoms (up to ~2.5 Å apart), which is what δ must cover.
fn tetrahedron_centres() -> Vec<DVec3> {
    (0..6)
        .map(|k| {
            DVec3::new(
                0.37 * k as f64,
                0.53 * (k % 5) as f64,
                0.29 * (k % 7) as f64,
            )
        })
        .collect()
}

/// `intersect` of the four half spaces `hs0`..`hs3` of a tetrahedron centred
/// at `centre`.
fn tetrahedron_at(d: &mut StructureDesigner, centre: DVec3) -> u64 {
    let intersect = add(d, "intersect", "intersect");
    for (i, face) in tetrahedron_faces_at(centre).into_iter().enumerate() {
        let hs = add_value(d, blueprint(half_space(face)), &format!("hs{i}"));
        d.connect_nodes(hs, 0, intersect, 0);
    }
    intersect
}

fn tetrahedron(d: &mut StructureDesigner) -> u64 {
    tetrahedron_at(d, DVec3::ZERO)
}

/// The tetrahedron at `centre` → `fill`, and `fill`'s atoms.
fn tetrahedron_chain_at(centre: DVec3) -> (StructureDesigner, u64, AtomicStructure) {
    let mut d = setup();
    let intersect = tetrahedron_at(&mut d, centre);
    let fill = add(&mut d, "materialize", "fill");
    d.connect_nodes(intersect, 0, fill, 0);
    let atoms = output_atoms(&mut d, fill);
    (d, fill, atoms)
}

/// The rendered trace for the given half spaces, in pin order.
fn tetrahedron_trace(faces: &[usize]) -> String {
    let mut rows = vec!["fill filled".to_string(), "intersect passed".to_string()];
    rows.extend(faces.iter().map(|i| format!("hs{i} shape")));
    rows.join(
        "
",
    )
}

#[test]
fn intersect_a_face_atom_lists_one_half_space() {
    for centre in tetrahedron_centres() {
        let (mut d, fill, atoms) = tetrahedron_chain_at(centre);
        let faces = tetrahedron_faces_at(centre);
        for i in 0..4 {
            let others: Vec<Plane> = (0..4).filter(|j| *j != i).map(|j| faces[j]).collect();
            let atom = outermost(&atoms, faces[i].0, &others, clear_of_other_faces());
            assert_eq!(
                trace(&mut d, &atom_hit(fill, atom)),
                tetrahedron_trace(&[i]),
                "face {i}, tetrahedron at {centre}"
            );
        }
    }
}

#[test]
fn intersect_the_outermost_atom_along_an_edge_lists_its_two_half_spaces() {
    for centre in tetrahedron_centres() {
        let (mut d, fill, atoms) = tetrahedron_chain_at(centre);
        let faces = tetrahedron_faces_at(centre);
        for a in 0..4 {
            for b in a + 1..4 {
                // Keep away from the edge's two vertices, which lie on the
                // other two faces.
                let others: Vec<Plane> = (0..4)
                    .filter(|j| *j != a && *j != b)
                    .map(|j| faces[j])
                    .collect();
                let atom = outermost(
                    &atoms,
                    faces[a].0 + faces[b].0,
                    &others,
                    clear_of_other_faces(),
                );
                assert_eq!(
                    trace(&mut d, &atom_hit(fill, atom)),
                    tetrahedron_trace(&[a, b]),
                    "edge {a}-{b}, tetrahedron at {centre}"
                );
            }
        }
    }
}

#[test]
fn intersect_the_outermost_atom_along_a_vertex_lists_its_three_half_spaces() {
    for centre in tetrahedron_centres() {
        let (mut d, fill, atoms) = tetrahedron_chain_at(centre);
        let faces = tetrahedron_faces_at(centre);
        for opposite in 0..4 {
            // The vertex opposite face `opposite` is shared by the other three.
            let atom = outermost(&atoms, -faces[opposite].0, &[], 0.0);
            let shared: Vec<usize> = (0..4).filter(|j| *j != opposite).collect();
            assert_eq!(
                trace(&mut d, &atom_hit(fill, atom)),
                tetrahedron_trace(&shared),
                "vertex opposite face {opposite}, tetrahedron at {centre}"
            );
        }
    }
}

// ============================================================================
// diff
// ============================================================================

#[test]
fn diff_a_cut_face_traces_to_the_cutter_and_an_uncut_face_to_the_base() {
    let mut d = setup();
    let base = add_value(
        &mut d,
        blueprint(cube(DVec3::splat(-8.0), DVec3::splat(8.0))),
        "base",
    );
    // Removes everything right of x = 4.
    let cutter = add_value(
        &mut d,
        blueprint(GeoNode::half_space(-DVec3::X, DVec3::new(4.0, 0.0, 0.0))),
        "cutter",
    );
    let diff = add(&mut d, "diff", "diff");
    d.connect_nodes(base, 0, diff, 0);
    d.connect_nodes(cutter, 0, diff, 1);
    let fill = add(&mut d, "materialize", "fill");
    d.connect_nodes(diff, 0, fill, 0);
    let atoms = output_atoms(&mut d, fill);

    // Away from the box's y and z faces, so only the x faces decide.
    let side_faces: Vec<Plane> = [DVec3::Y, DVec3::Z, -DVec3::Y, -DVec3::Z]
        .into_iter()
        .map(|n| (n, n * 8.0))
        .collect();
    let cut = outermost(&atoms, DVec3::X, &side_faces, clear_of_other_faces());
    assert_eq!(
        trace(&mut d, &atom_hit(fill, cut)),
        "fill filled\ndiff passed\ncutter shape"
    );
    let uncut = outermost(&atoms, -DVec3::X, &side_faces, clear_of_other_faces());
    assert_eq!(
        trace(&mut d, &atom_hit(fill, uncut)),
        "fill filled\ndiff passed\nbase shape"
    );
}

#[test]
fn diff_with_two_bases_and_two_cutters_traces_each_atom_to_its_own_element() {
    let mut d = setup();
    let left = DVec3::new(-8.0, 0.0, 0.0);
    let right = DVec3::new(8.0, 0.0, 0.0);
    let b0 = add_value(
        &mut d,
        blueprint(cube(
            DVec3::new(-14.0, -6.0, -6.0),
            DVec3::new(-2.0, 6.0, 6.0),
        )),
        "b0",
    );
    let b1 = add_value(
        &mut d,
        blueprint(cube(
            DVec3::new(2.0, -6.0, -6.0),
            DVec3::new(14.0, 6.0, 6.0),
        )),
        "b1",
    );
    // Each scoops a dent down to z = 1 out of the top of one box, deep enough
    // that the box's own top face (z = 6) stays out of the tie.
    let top = DVec3::new(0.0, 0.0, 8.0);
    let s0 = add_value(&mut d, blueprint(GeoNode::sphere(left + top, 7.0)), "s0");
    let s1 = add_value(&mut d, blueprint(GeoNode::sphere(right + top, 7.0)), "s1");
    let diff = add(&mut d, "diff", "diff");
    d.connect_nodes(b0, 0, diff, 0);
    d.connect_nodes(b1, 0, diff, 0);
    d.connect_nodes(s0, 0, diff, 1);
    d.connect_nodes(s1, 0, diff, 1);
    let fill = add(&mut d, "materialize", "fill");
    d.connect_nodes(diff, 0, fill, 0);
    let atoms = output_atoms(&mut d, fill);

    // An atom right under (or over) a box's centre line.
    let under = |centre: DVec3| move |p: DVec3| (p - centre).truncate().length() < 1.5;
    let cases = [
        (left, DVec3::Z, "s0"),
        (right, DVec3::Z, "s1"),
        (left, -DVec3::Z, "b0"),
        (right, -DVec3::Z, "b1"),
    ];
    for (centre, direction, expected) in cases {
        let atom = outermost_where(&atoms, direction, under(centre));
        assert_eq!(
            trace(&mut d, &atom_hit(fill, atom)),
            format!("fill filled\ndiff passed\n{expected} shape"),
            "atom at {atom}"
        );
    }
}

// ============================================================================
// geo_trans and the movement nodes on a Blueprint
// ============================================================================

/// Two cells of the diamond lattice along x, in Å.
fn two_cells() -> f64 {
    2.0 * atomcad_crystolecule::structure::Structure::diamond()
        .lattice_vecs
        .a
        .x
}

/// `a` (r 4 at the origin) ∪ `b` (r 4, four cells along +x) → `mover`
/// (`node_type` with `data`) → `fill`.
fn moved_spheres(node_type: &str, data: Box<dyn NodeData>) -> (StructureDesigner, u64) {
    let mut d = setup();
    let a = add_value(&mut d, blueprint(GeoNode::sphere(DVec3::ZERO, 4.0)), "a");
    let b = add_value(
        &mut d,
        blueprint(GeoNode::sphere(
            DVec3::new(2.0 * two_cells(), 0.0, 0.0),
            4.0,
        )),
        "b",
    );
    let union = add(&mut d, "union", "union");
    d.connect_nodes(a, 0, union, 0);
    d.connect_nodes(b, 0, union, 0);
    let mover = add_with(&mut d, node_type, "mover", data);
    d.connect_nodes(union, 0, mover, 0);
    let fill = add(&mut d, "materialize", "fill");
    d.connect_nodes(mover, 0, fill, 0);
    (d, fill)
}

/// After moving both spheres two cells along +x, the right-hand side of `a`
/// sits where `b` was. Only the inverse-transformed point still says `a`.
#[test]
fn a_translation_maps_the_point_back_before_the_union_decides() {
    let shift = IVec3::new(2, 0, 0);
    let movers: [(&str, Box<dyn NodeData>); 3] = [
        (
            "geo_trans",
            Box::new(GeoTransData {
                translation: shift,
                rotation: IVec3::ZERO,
                transform_only_frame: false,
            }),
        ),
        (
            "structure_move",
            Box::new(StructureMoveData {
                translation: shift,
                lattice_subdivision: IVec3::ONE,
            }),
        ),
        (
            "free_move",
            Box::new(FreeMoveData {
                translation: DVec3::new(two_cells(), 0.0, 0.0),
            }),
        ),
    ];
    for (node_type, data) in movers {
        let (mut d, fill) = moved_spheres(node_type, data);
        let atoms = output_atoms(&mut d, fill);
        // The moved `a` ends left of the moved `b`'s left side.
        let atom = outermost_where(&atoms, DVec3::X, |p| p.x < 2.0 * two_cells());
        assert!(
            (atom.x - (two_cells() + 4.0)).abs() < 1.0,
            "{node_type}: precondition, the atom is on the moved a's right side: {atom}"
        );
        assert_eq!(
            trace(&mut d, &atom_hit(fill, atom)),
            "fill filled\nmover moved\nunion passed\na shape",
            "{node_type}"
        );
    }
}

/// A rotation or an inversion about the origin moves `b` away from where it
/// was, keeping its distance from the origin; the atom farthest from the
/// origin is then on the moved `b`'s far side. Without the inverse the point
/// would land nearer `a`.
#[test]
fn a_rotation_and_an_inversion_map_the_point_back_before_the_union_decides() {
    let movers: [(&str, Box<dyn NodeData>); 3] = [
        (
            "free_rot",
            Box::new(FreeRotData {
                angle_degrees: 90.0,
                rot_axis: DVec3::Z,
                pivot_point: DVec3::ZERO,
            }),
        ),
        (
            "structure_rot",
            Box::new(StructureRotData {
                axis_index: Some(2),
                step: 1,
                pivot_point: IVec3::ZERO,
            }),
        ),
        (
            "structure_invert",
            Box::new(StructureInvertData {
                pivot_point: IVec3::ZERO,
                subdivision: 1,
            }),
        ),
    ];
    let b_centre = DVec3::new(2.0 * two_cells(), 0.0, 0.0);
    for (node_type, data) in movers {
        let (mut d, fill) = moved_spheres(node_type, data);
        let atoms = output_atoms(&mut d, fill);
        let atom = atoms
            .iter_atoms()
            .filter(|(_, a)| a.atomic_number == 6)
            .map(|(_, a)| a.position)
            .max_by(|p, q| p.length().total_cmp(&q.length()))
            .unwrap();
        assert!(
            atom.distance(b_centre) > 4.0 + 1.0,
            "{node_type}: precondition, b moved away from where it was: {atom}"
        );
        assert_eq!(
            trace(&mut d, &atom_hit(fill, atom)),
            "fill filled\nmover moved\nunion passed\nb shape",
            "{node_type}"
        );
    }
}

// ============================================================================
// Surface hits
// ============================================================================

#[test]
fn a_surface_hit_gives_the_same_rows_as_an_atom_near_that_spot() {
    let (mut d, union, fill) = sphere_union_cube();
    let atoms = output_atoms(&mut d, fill);
    let atom = outermost(&atoms, -DVec3::X, &[], 0.0);
    let from_atom = trace(&mut d, &atom_hit(fill, atom));

    let surface = TraceHit {
        node: NodeRef::top(union),
        pin: 0,
        target: TraceHitTarget::Surface {
            point: DVec3::new(-5.0, 0.0, 0.0),
        },
    };
    let from_surface = trace(&mut d, &surface);
    assert_eq!(from_surface, "union passed\nsphere shape");
    assert_eq!(
        from_atom.lines().skip(1).collect::<Vec<_>>().join("\n"),
        from_surface
    );
}

#[test]
fn a_surface_hit_reaches_the_shape_with_the_surface_delta() {
    let mut d = setup();
    let (sphere, probes) = add_probe_source(
        &mut d,
        "sphere",
        SourceKind::Blueprint(Box::new(GeoNode::sphere(DVec3::ZERO, 5.0))),
        vec![],
    );
    let cube = add_value(
        &mut d,
        blueprint(cube(
            DVec3::new(3.0, -4.0, -4.0),
            DVec3::new(11.0, 4.0, 4.0),
        )),
        "cube",
    );
    let union = add(&mut d, "union", "union");
    d.connect_nodes(sphere, 0, union, 0);
    d.connect_nodes(cube, 0, union, 0);

    let point = DVec3::new(-5.0, 0.0, 0.0);
    let hit = TraceHit {
        node: NodeRef::top(union),
        pin: 0,
        target: TraceHitTarget::Surface { point },
    };
    assert_eq!(trace(&mut d, &hit), "union passed\nsphere source");
    let probes = probes.lock().unwrap();
    assert_eq!(
        probes.as_slice(),
        [TraceProbe::Point {
            position: point,
            on_surface: true,
            delta: SURFACE_HIT_DELTA,
        }]
    );
}

// ============================================================================
// The top-of-doc example
// ============================================================================

#[test]
fn the_design_doc_example_end_to_end() {
    let mut d = setup();
    let sphere = add_value(
        &mut d,
        blueprint(GeoNode::sphere(DVec3::new(-20.0, 0.0, 0.0), 5.0)),
        "sphere",
    );
    let intersect = tetrahedron(&mut d);
    let union = add(&mut d, "union", "union");
    d.connect_nodes(sphere, 0, union, 0);
    d.connect_nodes(intersect, 0, union, 0);
    let fill = add(&mut d, "materialize", "materialize");
    d.connect_nodes(union, 0, fill, 0);
    let atoms = output_atoms(&mut d, fill);

    let faces = tetrahedron_faces();
    let others: Vec<Plane> = faces.iter().copied().filter(|f| f != &faces[3]).collect();
    let atom = outermost(&atoms, faces[3].0, &others, clear_of_other_faces());
    assert!(
        atom.distance(DVec3::new(-20.0, 0.0, 0.0)) > 5.0 + clear_of_other_faces(),
        "precondition: the atom is far from the sphere"
    );
    assert_eq!(
        trace(&mut d, &atom_hit(fill, atom)),
        "materialize filled\nunion passed\nintersect passed\nhs3 shape"
    );
}

// ============================================================================
// Phase pass-throughs (doc/design_trace_coverage.md Phase 2)
// ============================================================================

fn surface_hit(node_id: u64, point: DVec3) -> TraceHit {
    TraceHit {
        node: NodeRef::top(node_id),
        pin: 0,
        target: TraceHitTarget::Surface { point },
    }
}

fn surface_probe(point: DVec3) -> TraceProbe {
    TraceProbe::Point {
        position: point,
        on_surface: true,
        delta: SURFACE_HIT_DELTA,
    }
}

/// `shape` (a sphere) → `fill` → `exit` → `enter`: a point of the molecule's
/// and then the crystal's geometry passes every node unchanged, and
/// `materialize` reads `passed` for it, not "possibly contributed".
#[test]
fn exit_and_enter_structure_pass_a_surface_point() {
    let mut d = setup();
    let (shape, probes) = add_probe_source(
        &mut d,
        "shape",
        SourceKind::Blueprint(Box::new(GeoNode::sphere(DVec3::ZERO, 5.0))),
        vec![],
    );
    let fill = add(&mut d, "materialize", "fill");
    d.connect_nodes(shape, 0, fill, 0);
    let exit = add(&mut d, "exit_structure", "exit");
    d.connect_nodes(fill, 0, exit, 0);
    let structure = add_value(&mut d, NetworkResult::Structure(Structure::diamond()), "st");
    let enter = add(&mut d, "enter_structure", "enter");
    d.connect_nodes(exit, 0, enter, 0);
    d.connect_nodes(structure, 0, enter, 1);

    let point = DVec3::new(5.0, 0.0, 0.0);
    assert_eq!(
        trace(&mut d, &surface_hit(exit, point)),
        "exit passed
fill passed
shape source"
    );
    assert_eq!(
        trace(&mut d, &surface_hit(enter, point)),
        "enter passed
exit passed
fill passed
shape source"
    );
    let probes = probes.lock().unwrap();
    assert!(
        probes.iter().all(|p| *p == surface_probe(point)),
        "{probes:?}"
    );
}

/// `sphere ∪ cube` → `fill` → `demat` → `ws` (a new structure): a surface hit
/// on the Blueprint goes back through the crystal to the union, which keeps
/// only the sphere.
#[test]
fn with_structure_and_dematerialize_reach_the_deciding_shape() {
    let (mut d, _, fill) = sphere_union_cube();
    let demat = add(&mut d, "dematerialize", "demat");
    d.connect_nodes(fill, 0, demat, 0);
    let structure = add_value(&mut d, NetworkResult::Structure(Structure::diamond()), "st");
    let ws = add(&mut d, "with_structure", "ws");
    d.connect_nodes(demat, 0, ws, 0);
    d.connect_nodes(structure, 0, ws, 1);

    assert_eq!(
        trace(&mut d, &surface_hit(ws, DVec3::new(-5.0, 0.0, 0.0))),
        "ws passed
demat passed
fill passed
union passed
sphere shape"
    );
}

// ============================================================================
// lattice_symop
// ============================================================================

/// `a` (r 4 at the origin) ∪ `b` (r 4, four cells along +x), both probe
/// sources → `symop` (`data`). Returns the designer, the symop and `b`'s
/// probes.
fn symop_spheres(
    data: LatticeSymopData,
) -> (
    StructureDesigner,
    u64,
    std::sync::Arc<std::sync::Mutex<Vec<TraceProbe>>>,
) {
    let mut d = setup();
    let (a, _) = add_probe_source(
        &mut d,
        "a",
        SourceKind::Blueprint(Box::new(GeoNode::sphere(DVec3::ZERO, 4.0))),
        vec![],
    );
    let (b, b_probes) = add_probe_source(
        &mut d,
        "b",
        SourceKind::Blueprint(Box::new(GeoNode::sphere(
            DVec3::new(2.0 * two_cells(), 0.0, 0.0),
            4.0,
        ))),
        vec![],
    );
    let union = add(&mut d, "union", "union");
    d.connect_nodes(a, 0, union, 0);
    d.connect_nodes(b, 0, union, 0);
    let symop = add_with(&mut d, "lattice_symop", "symop", Box::new(data));
    d.connect_nodes(union, 0, symop, 0);
    (d, symop, b_probes)
}

fn quarter_turn_and_one_cell(keep_geo: bool) -> LatticeSymopData {
    LatticeSymopData {
        translation: IVec3::new(1, 0, 0),
        rotation_axis: Some(DVec3::Z),
        rotation_angle_degrees: 90.0,
        transform_only_frame: keep_geo,
    }
}

/// A quarter turn about z then one cell along +x takes `b` (off centre) to
/// (one cell, four cells, 0). A hit on its far side maps back to `b`'s +x
/// face; without the inverse the point would be nearer `a`.
#[test]
fn lattice_symop_maps_a_surface_point_back_onto_the_input_face() {
    let (mut d, symop, b_probes) = symop_spheres(quarter_turn_and_one_cell(false));
    let cell = two_cells() / 2.0;
    let b_far = 2.0 * two_cells() + 4.0;
    assert_eq!(
        trace(&mut d, &surface_hit(symop, DVec3::new(cell, b_far, 0.0))),
        "symop moved
union passed
b source"
    );
    let probes = b_probes.lock().unwrap();
    let TraceProbe::Point { position, .. } = probes[0] else {
        panic!("{probes:?}");
    };
    assert!(
        position.distance(DVec3::new(b_far, 0.0, 0.0)) < 1e-9,
        "{position}"
    );
}

/// In keep-geo mode the geometry stays where it was, so the same kind of hit
/// reads `passed` and reaches `b` at the unchanged position.
#[test]
fn lattice_symop_keep_geo_passes_the_point_unchanged() {
    let (mut d, symop, b_probes) = symop_spheres(quarter_turn_and_one_cell(true));
    let point = DVec3::new(2.0 * two_cells() + 4.0, 0.0, 0.0);
    assert_eq!(
        trace(&mut d, &surface_hit(symop, point)),
        "symop passed
union passed
b source"
    );
    assert_eq!(b_probes.lock().unwrap().as_slice(), [surface_probe(point)]);
}
