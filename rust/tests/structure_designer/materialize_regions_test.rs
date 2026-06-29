//! Phase B2 tests for `materialize.regions`. See
//! `doc/design_blueprint_region_atom_edits.md` §B2 / §B6. Phase B2 adds:
//!   - the built-in `MaterializeRegion` record def (reserved name), and
//!   - the optional `regions: Array[Record(Named("MaterializeRegion"))]` input
//!     pin on `materialize`, parsed into the `RegionSpec` list that
//!     `fill_lattice` layers on top of the root settings (Phase B1 engine).
//!
//! What we verify here:
//!   - `MaterializeRegion` resolves through `lookup_record_type_def` and is a
//!     reserved built-in (add/delete/rename/update guards + namespace).
//!   - Pin signature: `materialize` gains an optional `regions` pin.
//!   - Eval semantics: disconnected ≡ empty array ≡ today's output; a region
//!     covering the whole structure with `passivate: false` strips all H; a
//!     region disjoint from the structure is a no-op; per-field inheritance
//!     (a region setting only `passivate` leaves the root's other settings).
//!   - Parse errors: missing `volume` / wrong-typed `volume` → indexed Error.

use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::structure::Structure;
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::structure_designer::data_type::{DataType, RecordType};
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    BlueprintData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    NodeTypeRegistry, RecordTypeDef, RecordTypeDefError,
};
use rust_lib_flutter_cad::structure_designer::nodes::materialize::MaterializeData;
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn evaluate(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
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

/// Build a cuboid → materialize chain in a fresh network. Returns
/// `(designer, network_name, materialize_id)`. The materialize node keeps its
/// default root settings (passivate on, rm_unbonded on, the rest off).
fn cuboid_materialize() -> (StructureDesigner, String, u64) {
    let net = "test_mat_regions".to_string();
    let mut designer = StructureDesigner::new();
    designer.add_node_network(&net);
    designer.set_active_node_network_name(Some(net.clone()));

    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let mat_id = designer.add_node("materialize", DVec2::new(300.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, mat_id, 0);
    (designer, net, mat_id)
}

fn count_hydrogens(result: &NetworkResult) -> usize {
    match result {
        NetworkResult::Crystal(c) => c
            .atoms
            .iter_atoms()
            .filter(|(_, a)| a.atomic_number == 1)
            .count(),
        NetworkResult::Error(e) => panic!("materialize returned error: {}", e),
        other => panic!("expected Crystal, got {:?}", other.infer_data_type()),
    }
}

/// A `MaterializeRegion` record value with the given volume SDF and (optional)
/// `passivate` override; all other settings fields left unset (inherit).
fn region_record(volume: GeoNode, passivate: Option<bool>) -> NetworkResult {
    let opt_bool = |b: Option<bool>| b.map_or(NetworkResult::None, NetworkResult::Bool);
    NetworkResult::record(vec![
        (
            "volume".to_string(),
            NetworkResult::Blueprint(BlueprintData {
                structure: Structure::diamond(),
                geo_tree_root: volume,
                alignment: Default::default(),
                alignment_reason: None,
            }),
        ),
        ("margin".to_string(), NetworkResult::None),
        ("passivate".to_string(), opt_bool(passivate)),
        ("rm_single".to_string(), NetworkResult::None),
        ("surf_recon".to_string(), NetworkResult::None),
        ("invert_phase".to_string(), NetworkResult::None),
        ("rm_unbonded".to_string(), NetworkResult::None),
    ])
}

/// Half-space `{ p : p.z <= z0 }` (membership = SDF ≤ margin). With a large
/// positive `z0` it covers the whole structure; with a large negative `z0` it
/// covers nothing.
fn z_below(z0: f64) -> GeoNode {
    GeoNode::half_space(DVec3::new(0.0, 0.0, 1.0), DVec3::new(0.0, 0.0, z0))
}

// ---------------------------------------------------------------------------
// Built-in def: lookup + reserved-name guards (mirrors ElementMapping Phase A)
// ---------------------------------------------------------------------------

#[test]
fn materialize_region_resolves_via_lookup() {
    let registry = NodeTypeRegistry::new();
    let def = registry
        .lookup_record_type_def("MaterializeRegion")
        .expect("MaterializeRegion should resolve via built_in_record_type_defs");
    assert_eq!(def.name, "MaterializeRegion");
    // Authored field order drives the record_construct pin layout.
    let names: Vec<&str> = def.fields.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "volume",
            "margin",
            "passivate",
            "rm_single",
            "surf_recon",
            "invert_phase",
            "rm_unbonded"
        ]
    );
    // volume is a plain Blueprint; the six settings are Optional[..].
    assert_eq!(def.fields[0].1, DataType::Blueprint);
    assert_eq!(
        def.fields[1].1,
        DataType::Optional(Box::new(DataType::Float))
    );
    for field in &def.fields[2..] {
        assert_eq!(field.1, DataType::Optional(Box::new(DataType::Bool)));
    }
}

#[test]
fn materialize_region_is_reserved_built_in() {
    let mut registry = NodeTypeRegistry::new();
    assert!(registry.is_built_in_record_type_def("MaterializeRegion"));
    assert!(registry.name_is_taken("MaterializeRegion"));

    // add / rename / update rejected; delete is a no-op.
    let add_err = registry
        .add_record_type_def(RecordTypeDef {
            name: "MaterializeRegion".to_string(),
            fields: vec![],
        })
        .unwrap_err();
    assert!(matches!(add_err, RecordTypeDefError::BuiltIn(_)));

    assert!(
        registry
            .delete_record_type_def("MaterializeRegion")
            .is_none()
    );

    let rename_err = registry
        .rename_record_type_def("MaterializeRegion", "MyRegion")
        .unwrap_err();
    assert!(matches!(rename_err, RecordTypeDefError::BuiltIn(_)));

    let update_err = registry
        .update_record_type_def("MaterializeRegion", vec![])
        .unwrap_err();
    assert!(matches!(update_err, RecordTypeDefError::BuiltIn(_)));

    // Still resolvable and unchanged after the rejected mutations.
    assert!(
        registry
            .lookup_record_type_def("MaterializeRegion")
            .is_some()
    );
}

#[test]
fn network_named_materialize_region_collides() {
    let designer = StructureDesigner::default();
    // The API layer's `add_node_network_with_name` consults `name_is_taken`.
    assert!(
        designer
            .node_type_registry
            .name_is_taken("MaterializeRegion")
    );
}

// ---------------------------------------------------------------------------
// Pin signature
// ---------------------------------------------------------------------------

#[test]
fn materialize_regions_pin_signature() {
    let registry = NodeTypeRegistry::new();
    let nt = registry.get_node_type("materialize").unwrap();
    let regions = nt
        .parameters
        .iter()
        .find(|p| p.name == "regions")
        .expect("materialize should have a `regions` pin");
    assert_eq!(
        regions.data_type,
        DataType::Array(Box::new(DataType::Record(RecordType::Named(
            "MaterializeRegion".to_string()
        ))))
    );
    // `regions` is optional; `shape` stays required.
    let data = MaterializeData {
        parameter_element_value_definition: String::new(),
        hydrogen_passivation: true,
        remove_unbonded_atoms: true,
        remove_single_bond_atoms_before_passivation: false,
        surface_reconstruction: false,
        invert_phase: false,
        error: None,
        parameter_element_values: Default::default(),
        available_parameters: Default::default(),
    };
    let meta = data.get_parameter_metadata();
    assert_eq!(meta.get("shape"), Some(&(true, None)));
    assert_eq!(meta.get("regions"), Some(&(false, None)));
}

// ---------------------------------------------------------------------------
// Eval semantics
// ---------------------------------------------------------------------------

/// Disconnected `regions` pin and an explicitly empty array both reproduce the
/// no-regions baseline output exactly.
#[test]
fn materialize_disconnected_and_empty_match_baseline() {
    let (mut designer, net, mat_id) = cuboid_materialize();
    let baseline = evaluate(&designer, &net, mat_id);
    let h_baseline = count_hydrogens(&baseline);
    assert!(h_baseline > 0, "default passivation should add hydrogens");

    let empty_id = add_value_node(&mut designer, &net, NetworkResult::Array(vec![]));
    designer.connect_nodes(empty_id, 0, mat_id, 6);
    let empty = evaluate(&designer, &net, mat_id);
    assert_eq!(
        count_hydrogens(&empty),
        h_baseline,
        "empty regions array ≡ disconnected pin"
    );
}

/// A region covering the whole structure with `passivate: false` strips every
/// passivating hydrogen — proving per-position settings reach the fill pass
/// through the node's parsing path.
#[test]
fn materialize_whole_structure_region_disables_passivation() {
    let (mut designer, net, mat_id) = cuboid_materialize();
    let h_baseline = count_hydrogens(&evaluate(&designer, &net, mat_id));

    let all = region_record(z_below(1.0e6), Some(false));
    let regions_id = add_value_node(&mut designer, &net, NetworkResult::Array(vec![all]));
    designer.connect_nodes(regions_id, 0, mat_id, 6);

    let result = evaluate(&designer, &net, mat_id);
    assert!(h_baseline > 0);
    assert_eq!(
        count_hydrogens(&result),
        0,
        "passivate:false over the whole structure → no hydrogens"
    );
}

/// A region disjoint from the structure is a no-op even with `passivate: false`.
#[test]
fn materialize_disjoint_region_is_noop() {
    let (mut designer, net, mat_id) = cuboid_materialize();
    let h_baseline = count_hydrogens(&evaluate(&designer, &net, mat_id));

    let nowhere = region_record(z_below(-1.0e6), Some(false));
    let regions_id = add_value_node(&mut designer, &net, NetworkResult::Array(vec![nowhere]));
    designer.connect_nodes(regions_id, 0, mat_id, 6);

    let result = evaluate(&designer, &net, mat_id);
    assert_eq!(
        count_hydrogens(&result),
        h_baseline,
        "a region containing no atoms changes nothing"
    );
}

/// Per-field inheritance: a region that sets only `passivate` does not disturb
/// the other (inherited-from-root) settings. We split the structure at the
/// midpoint of its actual z-range and disable passivation only in the upper
/// half, so the hydrogen count lands strictly between the all-on baseline and
/// zero.
#[test]
fn materialize_region_partial_passivation() {
    let (mut designer, net, mat_id) = cuboid_materialize();
    let baseline = evaluate(&designer, &net, mat_id);
    let h_baseline = count_hydrogens(&baseline);

    // Find the z-midpoint of the carved (non-hydrogen) atoms so the splitting
    // plane genuinely cuts the structure rather than missing it entirely.
    let NetworkResult::Crystal(c) = &baseline else {
        panic!("expected Crystal baseline");
    };
    let zs: Vec<f64> = c.atoms.iter_atoms().map(|(_, a)| a.position.z).collect();
    let z_min = zs.iter().cloned().fold(f64::INFINITY, f64::min);
    let z_max = zs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let z_mid = 0.5 * (z_min + z_max);
    assert!(z_max - z_min > 0.5, "structure should span a real z-range");

    // Upper half: { p : p.z >= z_mid } = half-space normal (0,0,-1) through z_mid.
    let upper = GeoNode::half_space(DVec3::new(0.0, 0.0, -1.0), DVec3::new(0.0, 0.0, z_mid));
    let region = region_record(upper, Some(false));
    let regions_id = add_value_node(&mut designer, &net, NetworkResult::Array(vec![region]));
    designer.connect_nodes(regions_id, 0, mat_id, 6);

    let h_partial = count_hydrogens(&evaluate(&designer, &net, mat_id));
    assert!(
        h_partial > 0 && h_partial < h_baseline,
        "partial-region passivation off should drop H below baseline but not to zero (baseline={}, partial={})",
        h_baseline,
        h_partial
    );
}

// ---------------------------------------------------------------------------
// Parse errors (indexed)
// ---------------------------------------------------------------------------

#[test]
fn materialize_region_missing_volume_errors() {
    let (mut designer, net, mat_id) = cuboid_materialize();
    // A record with no `volume` field at all.
    let bad = NetworkResult::record(vec![("passivate".to_string(), NetworkResult::Bool(false))]);
    let regions_id = add_value_node(&mut designer, &net, NetworkResult::Array(vec![bad]));
    designer.connect_nodes(regions_id, 0, mat_id, 6);

    let result = evaluate(&designer, &net, mat_id);
    let NetworkResult::Error(msg) = result else {
        panic!("expected Error, got {:?}", result.infer_data_type());
    };
    assert!(
        msg.contains("regions[0]") && msg.contains("volume"),
        "error should name the item index and the missing field: {}",
        msg
    );
}

#[test]
fn materialize_region_wrong_typed_volume_errors() {
    let (mut designer, net, mat_id) = cuboid_materialize();
    // `volume` present but not a Blueprint.
    let bad = NetworkResult::record(vec![("volume".to_string(), NetworkResult::Int(3))]);
    let regions_id = add_value_node(&mut designer, &net, NetworkResult::Array(vec![bad]));
    designer.connect_nodes(regions_id, 0, mat_id, 6);

    let result = evaluate(&designer, &net, mat_id);
    let NetworkResult::Error(msg) = result else {
        panic!("expected Error, got {:?}", result.infer_data_type());
    };
    assert!(
        msg.contains("regions[0]") && msg.contains("volume"),
        "error should name the item index and the volume field: {}",
        msg
    );
}
