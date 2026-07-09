//! Tests for the lattice-covariant `sphere` / `circle` nodes,
//! `doc/design_lattice_covariant_sphere_circle.md`.
//!
//! Phase 3 covers the `sphere` node emitting the ellipsoid image of a
//! fractional ball: cubic back-compat (byte-identical geo hash + materialize
//! counts via the constructor's spherical-basis snap), the non-positive-radius
//! guard, discrete lattice covariance on a triclinic cell, volume invariance,
//! wired-pin overrides, and alignment.
//!
//! `GeoNodeKind` is private, so the emitted primitive variant is introspected
//! through the public `Display` impl (each arm's string starts with the variant
//! name) and through `hash()` (the spherical-basis fast path snaps cubic cells
//! to `Sphere`, giving hash equality with a directly constructed sphere).

use glam::f64::{DMat3, DVec2, DVec3};
use glam::i32::IVec3;
use rust_lib_flutter_cad::crystolecule::structure::Structure;
use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::geo_tree::implicit_geometry::ImplicitGeometry3D;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    Alignment, BlueprintData, CrystalData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::ivec3::IVec3Data;
use rust_lib_flutter_cad::structure_designer::nodes::sphere::SphereData;
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// ============================================================================
// Helpers
// ============================================================================

fn setup_designer(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

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

/// Add a `sphere` node to the active network and set its stored data.
fn add_sphere(
    designer: &mut StructureDesigner,
    network_name: &str,
    center: IVec3,
    radius: i32,
) -> u64 {
    let id = designer.add_node("sphere", DVec2::ZERO);
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    network.set_node_network_data(id, Box::new(SphereData { center, radius }));
    id
}

/// Wire a `value` node carrying `structure` into the sphere's `structure` pin
/// (pin index 2). Network-level connect is untyped, which is fine for a `value`
/// injector whose declared output type is `None`.
fn inject_structure(
    designer: &mut StructureDesigner,
    network_name: &str,
    sphere_id: u64,
    structure: Structure,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let value_id = network.add_node(
        "value",
        DVec2::new(-200.0, 0.0),
        0,
        Box::new(ValueData {
            value: NetworkResult::Structure(structure),
        }),
    );
    network.connect_nodes(value_id, 0, sphere_id, 2, false);
}

/// Build a `UnitCellStruct` from three basis vectors, deriving the cell
/// length/angle bookkeeping fields (the geo SDF only uses `a`/`b`/`c`).
fn unit_cell_from_vectors(a: DVec3, b: DVec3, c: DVec3) -> UnitCellStruct {
    UnitCellStruct {
        a,
        b,
        c,
        cell_length_a: a.length(),
        cell_length_b: b.length(),
        cell_length_c: c.length(),
        cell_angle_alpha: b.angle_between(c).to_degrees(),
        cell_angle_beta: a.angle_between(c).to_degrees(),
        cell_angle_gamma: a.angle_between(b).to_degrees(),
    }
}

/// A triclinic lattice: distinct lengths, non-right angles. Default motif.
fn triclinic_structure() -> Structure {
    let lattice_vecs = unit_cell_from_vectors(
        DVec3::new(4.0, 0.0, 0.0),
        DVec3::new(1.5, 5.0, 0.0),
        DVec3::new(0.8, 1.2, 6.0),
    );
    Structure::from_lattice_vecs(lattice_vecs)
}

/// `sphere → materialize` atom count on the sphere's (default or injected)
/// structure.
fn materialize_count(
    designer: &mut StructureDesigner,
    network_name: &str,
    sphere_id: u64,
) -> usize {
    let mat_id = designer.add_node("materialize", DVec2::new(300.0, 0.0));
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        network.connect_nodes(sphere_id, 0, mat_id, 0, false);
    }
    let c = crystal(evaluate_raw(designer, network_name, mat_id));
    c.atoms.get_num_of_atoms()
}

/// Display-string variant introspection (`GeoNodeKind` is private).
fn variant_name(node: &GeoNode) -> String {
    format!("{}", node)
}

// ============================================================================
// Cubic back-compat (the promise)
// ============================================================================

/// On the default diamond (cubic) structure the emitted `geo_tree_root` is
/// **hash-equal** to a directly constructed `GeoNode::sphere(L·c₀, r·|a|)` —
/// the constructor's spherical-basis snap makes this exact, not approximate.
/// Checked for several radii; a radius-scaled center too.
#[test]
fn sphere_cubic_backcompat_geo_hash() {
    let a_len = Structure::diamond().lattice_vecs.a.length();
    for (center, radius) in [
        (IVec3::new(0, 0, 0), 1),
        (IVec3::new(0, 0, 0), 4),
        (IVec3::new(2, -1, 3), 5),
    ] {
        let mut designer = setup_designer("t");
        let id = add_sphere(&mut designer, "t", center, radius);
        let bp = blueprint(evaluate_raw(&designer, "t", id));

        let real_center = Structure::diamond()
            .lattice_vecs
            .ivec3_lattice_to_real(&center);
        let expected = GeoNode::sphere(real_center, radius as f64 * a_len);

        assert!(
            variant_name(&bp.geo_tree_root).starts_with("Sphere"),
            "cubic cell should snap to a plain Sphere, got: {}",
            variant_name(&bp.geo_tree_root)
        );
        assert_eq!(
            bp.geo_tree_root.hash(),
            expected.hash(),
            "cubic sphere (center {:?}, r {}) must be byte-identical to the legacy \
             GeoNode::sphere",
            center,
            radius
        );
    }
}

/// `sphere → materialize` atom counts on diamond are identical to the values
/// before this change (hardcoded regression anchors) for several radii. The
/// hash test above proves the geo tree is byte-identical on cubic cells, so
/// these counts are legitimately the pre-change counts.
#[test]
fn sphere_cubic_backcompat_materialize_counts() {
    // Regression anchors: current `sphere → materialize` atom counts on the
    // default diamond structure. Integer-radius spheres always have lattice
    // sites at distance exactly r·|a|, so boundary sites make every radius a
    // distinct anchor.
    for (radius, expected) in [(1, 71), (4, 2857)] {
        let mut designer = setup_designer("t");
        let id = add_sphere(&mut designer, "t", IVec3::new(0, 0, 0), radius);
        let count = materialize_count(&mut designer, "t", id);
        assert_eq!(
            count, expected,
            "sphere(r={}) → materialize on diamond should carve {} atoms",
            radius, expected
        );
    }
}

// ============================================================================
// free_sphere untouched guard
// ============================================================================

/// `free_sphere` still emits a Euclidean `GeoNodeKind::Sphere` (protects the
/// "stays Euclidean" invariant through the constructor work).
#[test]
fn free_sphere_still_emits_sphere() {
    let mut designer = setup_designer("t");
    let id = designer.add_node("free_sphere", DVec2::ZERO);
    let bp = blueprint(evaluate_raw(&designer, "t", id));
    assert!(
        variant_name(&bp.geo_tree_root).starts_with("Sphere"),
        "free_sphere must stay a Euclidean Sphere, got: {}",
        variant_name(&bp.geo_tree_root)
    );
}

// ============================================================================
// Non-positive radius guard
// ============================================================================

/// `r == 0` emits the legacy point sphere (`GeoNode::sphere(x₀, 0)`, hash-equal
/// to today); `r == -2` emits an everywhere-positive SDF (empty). Byte-
/// identical to today's behaviour on cubic *and* triclinic structures.
#[test]
fn sphere_non_positive_radius() {
    for structure in [Structure::diamond(), triclinic_structure()] {
        let a_len = structure.lattice_vecs.a.length();

        // r == 0 → point sphere, hash-equal to the legacy emission.
        {
            let mut designer = setup_designer("t");
            let center = IVec3::new(1, 2, 3);
            let id = add_sphere(&mut designer, "t", center, 0);
            inject_structure(&mut designer, "t", id, structure.clone());
            let bp = blueprint(evaluate_raw(&designer, "t", id));

            let real_center = structure.lattice_vecs.ivec3_lattice_to_real(&center);
            let expected = GeoNode::sphere(real_center, 0.0 * a_len);
            assert!(
                variant_name(&bp.geo_tree_root).starts_with("Sphere"),
                "r=0 must emit a plain Sphere"
            );
            assert_eq!(
                bp.geo_tree_root.hash(),
                expected.hash(),
                "r=0 must be byte-identical to the legacy point sphere"
            );
        }

        // r == -2 → empty (SDF positive everywhere).
        {
            let mut designer = setup_designer("t");
            let id = add_sphere(&mut designer, "t", IVec3::new(0, 0, 0), -2);
            inject_structure(&mut designer, "t", id, structure.clone());
            let bp = blueprint(evaluate_raw(&designer, "t", id));

            let expected = GeoNode::sphere(DVec3::ZERO, -2.0 * a_len);
            assert_eq!(
                bp.geo_tree_root.hash(),
                expected.hash(),
                "r<0 must be byte-identical to the legacy negative-radius sphere"
            );
            // A negative-radius sphere is empty: its SDF is positive everywhere.
            let geo = &bp.geo_tree_root;
            for p in [
                DVec3::ZERO,
                DVec3::new(1.0, 0.0, 0.0),
                DVec3::new(-3.0, 2.0, 5.0),
            ] {
                assert!(
                    geo.implicit_eval_3d(&p) > 0.0,
                    "negative-radius sphere must be empty (SDF > 0) at {:?}",
                    p
                );
            }
        }
    }
}

// ============================================================================
// Discrete covariance (the point of the feature)
// ============================================================================

/// For a triclinic structure with `c₀ = 0, r = 3`, every integer lattice point
/// `u` with `|u| ≤ 3` maps to a real position with SDF ≤ 0 (+margin), and every
/// `u` with `|u| > 3` maps to SDF > 0. The contained lattice-point set is
/// **identical** to the cubic case's set (the ellipsoid preserves the discrete
/// fractional ball regardless of the cell).
#[test]
fn sphere_discrete_covariance() {
    const R: i32 = 3;
    const MARGIN: f64 = 1e-6;

    fn contained_set(structure: Structure) -> Vec<IVec3> {
        let mut designer = setup_designer("t");
        let id = add_sphere(&mut designer, "t", IVec3::new(0, 0, 0), R);
        inject_structure(&mut designer, "t", id, structure.clone());
        let bp = blueprint(evaluate_raw(&designer, "t", id));
        let geo = &bp.geo_tree_root;
        let l = &structure.lattice_vecs;

        let mut inside = Vec::new();
        for x in -6..=6 {
            for y in -6..=6 {
                for z in -6..=6 {
                    let u = IVec3::new(x, y, z);
                    let norm = ((x * x + y * y + z * z) as f64).sqrt();
                    let p = l.ivec3_lattice_to_real(&u);
                    let sdf = geo.implicit_eval_3d(&p);
                    if norm <= R as f64 + 1e-9 {
                        assert!(
                            sdf <= MARGIN,
                            "|u|={:.3} ≤ {} must be inside (SDF {:.6} ≤ margin) at u={:?}",
                            norm,
                            R,
                            sdf,
                            u
                        );
                        inside.push(u);
                    } else {
                        assert!(
                            sdf > 0.0,
                            "|u|={:.3} > {} must be outside (SDF {:.6} > 0) at u={:?}",
                            norm,
                            R,
                            sdf,
                            u
                        );
                    }
                }
            }
        }
        inside
    }

    let triclinic_inside = contained_set(triclinic_structure());
    let cubic_inside = contained_set(Structure::diamond());
    assert_eq!(
        triclinic_inside, cubic_inside,
        "the contained lattice-point set must be lattice-invariant"
    );
    assert!(
        !triclinic_inside.is_empty(),
        "the r=3 ball should contain lattice points"
    );
}

// ============================================================================
// Volume invariance
// ============================================================================

/// `sphere(r=4) → materialize` atom counts on a cubic cell vs. a sheared cell of
/// equal cell volume and the same motif agree within a few percent (loose
/// sanity check — `vol(E) = (4/3)πr³·|det L|` is exactly `(4/3)πr³` unit cells,
/// so materialized counts are roughly cell-shape-independent at fixed `r`).
#[test]
fn sphere_volume_invariance() {
    const R: i32 = 4;
    let size = Structure::diamond().lattice_vecs.a.length();
    let motif = Structure::diamond().motif;

    // Cubic and sheared cells share |det L| = size³ (shearing b along a leaves
    // the determinant unchanged) and the same motif.
    let cubic = Structure {
        lattice_vecs: unit_cell_from_vectors(
            DVec3::new(size, 0.0, 0.0),
            DVec3::new(0.0, size, 0.0),
            DVec3::new(0.0, 0.0, size),
        ),
        motif: motif.clone(),
        motif_offset: DVec3::ZERO,
    };
    let sheared = Structure {
        lattice_vecs: unit_cell_from_vectors(
            DVec3::new(size, 0.0, 0.0),
            DVec3::new(0.5 * size, size, 0.0),
            DVec3::new(0.0, 0.0, size),
        ),
        motif,
        motif_offset: DVec3::ZERO,
    };

    // Determinants really are equal (equal cell volume).
    let det = |s: &Structure| {
        let l = &s.lattice_vecs;
        DMat3::from_cols(l.a, l.b, l.c).determinant().abs()
    };
    assert!(
        (det(&cubic) - det(&sheared)).abs() < 1e-6,
        "equal cell volume"
    );

    let count_for = |structure: Structure| -> usize {
        let mut designer = setup_designer("t");
        let id = add_sphere(&mut designer, "t", IVec3::new(0, 0, 0), R);
        inject_structure(&mut designer, "t", id, structure);
        materialize_count(&mut designer, "t", id)
    };

    let cubic_count = count_for(cubic) as f64;
    let sheared_count = count_for(sheared) as f64;
    assert!(cubic_count > 0.0 && sheared_count > 0.0);
    let rel_diff = (cubic_count - sheared_count).abs() / cubic_count;
    assert!(
        rel_diff < 0.15,
        "atom counts should be roughly cell-shape-independent: cubic {} vs sheared {} \
         (rel diff {:.3})",
        cubic_count,
        sheared_count,
        rel_diff
    );
}

// ============================================================================
// Wired-pin overrides & alignment
// ============================================================================

/// Wired `center` / `radius` / `structure` pins override the stored fields.
#[test]
fn sphere_wired_pins_override_stored() {
    let mut designer = setup_designer("t");
    // Stored values are deliberately different from what we wire in.
    let sphere_id = add_sphere(&mut designer, "t", IVec3::new(9, 9, 9), 1);

    let wired_center = IVec3::new(1, 0, 0);
    let wired_radius = 3;
    let structure = triclinic_structure();
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("t")
            .unwrap();
        let ivec3_id = network.add_node(
            "ivec3",
            DVec2::new(-200.0, 0.0),
            0,
            Box::new(IVec3Data {
                value: wired_center,
            }),
        );
        let int_id = network.add_node(
            "int",
            DVec2::new(-200.0, 100.0),
            0,
            Box::new(IntData {
                value: wired_radius,
            }),
        );
        network.connect_nodes(ivec3_id, 0, sphere_id, 0, false);
        network.connect_nodes(int_id, 0, sphere_id, 1, false);
    }
    inject_structure(&mut designer, "t", sphere_id, structure.clone());

    let bp = blueprint(evaluate_raw(&designer, "t", sphere_id));

    // The wired structure flowed through.
    assert!(
        bp.structure
            .lattice_vecs
            .is_approximately_equal(&structure.lattice_vecs),
        "wired structure should drive the blueprint"
    );

    // The wired center/radius drive the ellipsoid: the wired center maps to the
    // real position whose SDF is ~ -(radius·σ) < 0, and stepping out by one more
    // lattice vector than the radius along `a` is outside.
    let geo = &bp.geo_tree_root;
    let l = &structure.lattice_vecs;
    let real_center = l.ivec3_lattice_to_real(&wired_center);
    assert!(
        geo.implicit_eval_3d(&real_center) < 0.0,
        "wired center must be inside the shape"
    );
    let far = l.ivec3_lattice_to_real(&(wired_center + IVec3::new(wired_radius + 1, 0, 0)));
    assert!(
        geo.implicit_eval_3d(&far) > 0.0,
        "a lattice point beyond the wired radius must be outside"
    );
}

/// The sphere's Blueprint output stays `Aligned` with no reason (the ellipsoid
/// is lattice-aligned by construction).
#[test]
fn sphere_alignment_is_aligned() {
    let mut designer = setup_designer("t");
    let id = add_sphere(&mut designer, "t", IVec3::new(0, 0, 0), 3);
    inject_structure(&mut designer, "t", id, triclinic_structure());
    let bp = blueprint(evaluate_raw(&designer, "t", id));
    assert_eq!(bp.alignment, Alignment::Aligned);
    assert!(bp.alignment_reason.is_none());
}
