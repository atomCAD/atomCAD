//! Tests for the `free_sphere` / `free_circle` real-space (Å) geometry
//! primitives — issue #381, `doc/design_free_sphere_circle.md`.
//!
//! Phase 1 covers `free_sphere` (Rust core): SDF placement without lattice
//! quantization, roundness in real space regardless of the structure input,
//! `Aligned` alignment, wired-pin overrides (incl. `Int → Float`), materialize
//! integration with sub-cell sensitivity, and text-property roundtrips.

use glam::f64::{DVec2, DVec3};
use glam::i32::IVec3;
use rust_lib_flutter_cad::crystolecule::structure::Structure;
use rust_lib_flutter_cad::crystolecule::unit_cell_struct::UnitCellStruct;
use rust_lib_flutter_cad::geo_tree::implicit_geometry::ImplicitGeometry3D;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    Alignment, BlueprintData, CrystalData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::nodes::float::FloatData;
use rust_lib_flutter_cad::structure_designer::nodes::free_sphere::FreeSphereData;
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::nodes::vec3::Vec3Data;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

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

/// Add a `free_sphere` node to the active network and set its stored data.
fn add_free_sphere(
    designer: &mut StructureDesigner,
    network_name: &str,
    center: DVec3,
    radius: f64,
) -> u64 {
    let id = designer.add_node("free_sphere", DVec2::ZERO);
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    network.set_node_network_data(id, Box::new(FreeSphereData { center, radius }));
    id
}

/// A non-cubic lattice (distinct a/b/c lengths) with the default motif.
fn non_cubic_structure() -> Structure {
    let lattice_vecs = UnitCellStruct {
        a: DVec3::new(4.0, 0.0, 0.0),
        b: DVec3::new(0.0, 6.0, 0.0),
        c: DVec3::new(0.0, 0.0, 8.0),
        cell_length_a: 4.0,
        cell_length_b: 6.0,
        cell_length_c: 8.0,
        cell_angle_alpha: 90.0,
        cell_angle_beta: 90.0,
        cell_angle_gamma: 90.0,
    };
    Structure::from_lattice_vecs(lattice_vecs)
}

/// Sorted, rounded atom-position set — for comparing whether two carve results
/// differ (positions are floats, so we bin to 0.01 Å).
fn position_set(crystal: &CrystalData) -> Vec<(i64, i64, i64)> {
    let mut positions: Vec<(i64, i64, i64)> = crystal
        .atoms
        .iter_atoms()
        .map(|(_, atom)| {
            (
                (atom.position.x * 100.0).round() as i64,
                (atom.position.y * 100.0).round() as i64,
                (atom.position.z * 100.0).round() as i64,
            )
        })
        .collect();
    positions.sort_unstable();
    positions
}

// ============================================================================
// Tests
// ============================================================================

/// Stored fractional center/radius (not representable in whole cells) place the
/// SDF exactly there — no lattice quantization, no |a| radius scaling.
#[test]
fn free_sphere_sdf_placement_is_unquantized() {
    let mut designer = setup_designer("t");
    let center = DVec3::new(1.5, 2.25, -0.75);
    let radius = 4.2;
    let id = add_free_sphere(&mut designer, "t", center, radius);

    let bp = blueprint(evaluate_raw(&designer, "t", id));
    let geo = &bp.geo_tree_root;

    // Center: SDF ≈ -radius.
    assert!(
        (geo.implicit_eval_3d(&center) - (-radius)).abs() < 1e-6,
        "SDF at center should be -radius"
    );
    // On the surface at center + (radius, 0, 0): SDF ≈ 0. (If radius were scaled
    // by |a| like `sphere`, this would be far from zero.)
    let surface = center + DVec3::new(radius, 0.0, 0.0);
    assert!(
        geo.implicit_eval_3d(&surface).abs() < 1e-6,
        "SDF on the +x surface point should be ~0"
    );
    // Well outside: SDF > 0.
    let outside = center + DVec3::new(radius + 10.0, 0.0, 0.0);
    assert!(
        geo.implicit_eval_3d(&outside) > 0.0,
        "SDF well outside should be positive"
    );
}

/// The sphere stays round in real space regardless of the (non-cubic)
/// structure input: SDF ≈ 0 at `center + r·d` for several non-axis directions.
#[test]
fn free_sphere_is_round_on_non_cubic_lattice() {
    let mut designer = setup_designer("t");
    let center = DVec3::new(1.0, 2.0, 3.0);
    let radius = 4.0;
    let fs_id = add_free_sphere(&mut designer, "t", center, radius);

    // Inject a non-cubic structure through a `value` node into pin 2.
    let structure = non_cubic_structure();
    let value_id = {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("t")
            .unwrap();
        let value_id = network.add_node(
            "value",
            DVec2::new(-200.0, 0.0),
            0,
            Box::new(ValueData {
                value: NetworkResult::Structure(structure),
            }),
        );
        // Network-level connect is untyped — fine for a `value` injector whose
        // declared output type is `None`.
        network.connect_nodes(value_id, 0, fs_id, 2, false);
        value_id
    };
    let _ = value_id;

    let bp = blueprint(evaluate_raw(&designer, "t", fs_id));

    // The non-cubic structure flowed through unchanged.
    assert!((bp.structure.lattice_vecs.cell_length_a - 4.0).abs() < 1e-9);
    assert!((bp.structure.lattice_vecs.cell_length_b - 6.0).abs() < 1e-9);
    assert!((bp.structure.lattice_vecs.cell_length_c - 8.0).abs() < 1e-9);

    // Roundness: every direction at distance `radius` sits on the surface.
    let geo = &bp.geo_tree_root;
    for d in [
        DVec3::new(1.0, 1.0, 0.0),
        DVec3::new(1.0, 1.0, 1.0),
        DVec3::new(0.0, 1.0, 1.0),
        DVec3::new(1.0, 0.0, 1.0),
        DVec3::new(-1.0, 2.0, -1.0),
    ] {
        let p = center + radius * d.normalize();
        assert!(
            geo.implicit_eval_3d(&p).abs() < 1e-6,
            "SDF should be ~0 at center + r·{:?} (round in real space)",
            d
        );
    }
}

/// `free_sphere` emits `Aligned` alignment (per §3.5 — a fractional cutter does
/// not taint alignment).
#[test]
fn free_sphere_alignment_is_aligned() {
    let mut designer = setup_designer("t");
    let id = add_free_sphere(&mut designer, "t", DVec3::ZERO, 5.0);
    let bp = blueprint(evaluate_raw(&designer, "t", id));
    assert_eq!(bp.alignment, Alignment::Aligned);
    assert!(bp.alignment_reason.is_none());
}

/// Wired `center`/`radius` pins override the stored fields; an `Int → Float`
/// implicit conversion into `radius` works out of the box.
#[test]
fn free_sphere_wired_pins_override_stored() {
    let mut designer = setup_designer("t");
    // Stored values are deliberately different from what we wire in.
    let fs_id = add_free_sphere(&mut designer, "t", DVec3::new(9.0, 9.0, 9.0), 1.0);

    let wired_center = DVec3::new(1.0, 2.0, 3.0);
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("t")
            .unwrap();
        let vec3_id = network.add_node(
            "vec3",
            DVec2::new(-200.0, 0.0),
            0,
            Box::new(Vec3Data {
                value: wired_center,
            }),
        );
        let float_id = network.add_node(
            "float",
            DVec2::new(-200.0, 100.0),
            0,
            Box::new(FloatData { value: 4.0 }),
        );
        network.connect_nodes(vec3_id, 0, fs_id, 0, false);
        network.connect_nodes(float_id, 0, fs_id, 1, false);
    }

    let bp = blueprint(evaluate_raw(&designer, "t", fs_id));
    let geo = &bp.geo_tree_root;
    assert!(
        (geo.implicit_eval_3d(&wired_center) - (-4.0)).abs() < 1e-6,
        "wired center/radius should drive the SDF"
    );

    // Now swap the radius source for an `int` node (Int → Float conversion).
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("t")
            .unwrap();
        let int_id = network.add_node(
            "int",
            DVec2::new(-200.0, 200.0),
            0,
            Box::new(IntData { value: 3 }),
        );
        network.connect_nodes(int_id, 0, fs_id, 1, false);
    }
    let bp = blueprint(evaluate_raw(&designer, "t", fs_id));
    assert!(
        (bp.geo_tree_root.implicit_eval_3d(&wired_center) - (-3.0)).abs() < 1e-6,
        "an int node into radius should convert to 3.0 Å"
    );
}

/// `free_sphere → materialize` carves atoms, and shifting the center by a
/// sub-cell (half-lattice-vector) amount changes the resulting atom set.
#[test]
fn free_sphere_materialize_sub_cell_sensitivity() {
    // Baseline: sphere centered at the origin.
    let mut designer = setup_designer("t");
    let fs_id = add_free_sphere(&mut designer, "t", DVec3::ZERO, 5.0);
    let mat_id = designer.add_node("materialize", DVec2::new(300.0, 0.0));
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("t")
            .unwrap();
        network.connect_nodes(fs_id, 0, mat_id, 0, false);
    }
    let base = crystal(evaluate_raw(&designer, "t", mat_id));
    assert!(
        base.atoms.get_num_of_atoms() > 0,
        "materialize over a free_sphere should carve at least one atom"
    );
    let base_positions = position_set(&base);

    // Shift the center by half a diamond lattice vector (3.567 / 2 ≈ 1.7835 Å),
    // a distance not representable in whole cells.
    let mut designer2 = setup_designer("t");
    let fs_id2 = add_free_sphere(&mut designer2, "t", DVec3::new(1.7835, 0.0, 0.0), 5.0);
    let mat_id2 = designer2.add_node("materialize", DVec2::new(300.0, 0.0));
    {
        let network = designer2
            .node_type_registry
            .node_networks
            .get_mut("t")
            .unwrap();
        network.connect_nodes(fs_id2, 0, mat_id2, 0, false);
    }
    let shifted = crystal(evaluate_raw(&designer2, "t", mat_id2));
    let shifted_positions = position_set(&shifted);

    assert_ne!(
        base_positions, shifted_positions,
        "a sub-cell center shift should change the carved atom set"
    );
}

/// `get_text_properties` emits `Vec3`/`Float`; `set_text_properties` reads them
/// back (including a whole-number `IVec3` center via `as_vec3`) and roundtrips.
#[test]
fn free_sphere_text_properties_roundtrip() {
    let data = FreeSphereData {
        center: DVec3::new(1.5, 2.25, -0.75),
        radius: 4.2,
    };
    let props = data.get_text_properties();
    assert!(
        props
            .iter()
            .any(|(k, v)| k == "center" && matches!(v, TextValue::Vec3(_))),
        "center should serialize as a Vec3"
    );
    assert!(
        props
            .iter()
            .any(|(k, v)| k == "radius" && matches!(v, TextValue::Float(_))),
        "radius should serialize as a Float"
    );

    // Roundtrip through get/set.
    let map: HashMap<String, TextValue> = props.into_iter().collect();
    let mut restored = FreeSphereData {
        center: DVec3::ZERO,
        radius: 0.0,
    };
    restored.set_text_properties(&map).unwrap();
    assert_eq!(restored.center, data.center);
    assert_eq!(restored.radius, data.radius);

    // Whole-number center parses to an IVec3 in the text format; `as_vec3`
    // accepts it, so `center: (1, 2, 3)` just works.
    let mut whole = HashMap::new();
    whole.insert("center".to_string(), TextValue::IVec3(IVec3::new(1, 2, 3)));
    whole.insert("radius".to_string(), TextValue::Float(2.5));
    let mut d = FreeSphereData {
        center: DVec3::ZERO,
        radius: 0.0,
    };
    d.set_text_properties(&whole).unwrap();
    assert_eq!(d.center, DVec3::new(1.0, 2.0, 3.0));
    assert_eq!(d.radius, 2.5);
}
