// Lattice-space refactoring .cnnd migration tests (v2 → v3).
//
// Per-fixture tests for each migration change-class. Each phase of
// `doc/design_cnnd_migration_v2_to_v3.md` adds new fixtures here.

use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::network_validator::validate_network;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use tempfile::tempdir;

const FIXTURE_DIR: &str = "tests/fixtures/lattice_space_migration";

// ---------------------------------------------------------------------------
// Phase 1: version dispatch — v3 file is a no-op through the new load path.
// ---------------------------------------------------------------------------

#[test]
fn test_load_trivial_v3_no_op() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/trivial_v3.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load trivial_v3.cnnd");

    assert_eq!(load_result.first_network_name, "Main");

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // A v3 file with a single sphere node loads exactly as written.
    assert_eq!(network.nodes.len(), 1);
    let only = network.nodes.values().next().unwrap();
    assert_eq!(only.node_type_name, "sphere");
    assert_eq!(network.return_node_id, Some(1));
}

// ---------------------------------------------------------------------------
// Phase 2: string renames (DataType + node type).
// ---------------------------------------------------------------------------

/// Fixture 1 — `pure_rename.cnnd`:
/// a Main network using `unit_cell`, `atom_move`, and a `parameter` node whose
/// `data.data_type` is `"UnitCell"`. No structural changes needed post-v3 beyond
/// the string rewrites. Verifies both rename tables (DataType + node type) and
/// that the `data_type` dispatch tag is rewritten in lock-step with
/// `node_type_name`.
#[test]
fn test_load_pure_rename() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/pure_rename.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load pure_rename.cnnd");

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // Node-type rename: `unit_cell` → `lattice_vecs`, `atom_move` → `free_move`.
    let type_names: std::collections::HashSet<&str> = network
        .nodes
        .values()
        .map(|n| n.node_type_name.as_str())
        .collect();
    assert!(
        type_names.contains("lattice_vecs"),
        "expected unit_cell → lattice_vecs; got {:?}",
        type_names
    );
    assert!(
        type_names.contains("free_move"),
        "expected atom_move → free_move; got {:?}",
        type_names
    );
    assert!(
        !type_names.contains("unit_cell") && !type_names.contains("atom_move"),
        "old node type names must not survive the rename; got {:?}",
        type_names
    );

    // DataType rename inside ParameterData: `"UnitCell"` → `"LatticeVecs"`. The parameter
    // node's custom type is computed from its data_type so the pin type shows through.
    let param_node = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "parameter")
        .expect("parameter node missing");
    let custom_type = param_node
        .custom_node_type
        .as_ref()
        .expect("parameter node should have a resolved custom node type");
    assert_eq!(
        *custom_type.output_type(),
        DataType::LatticeVecs,
        "parameter node's output should be LatticeVecs after v2→v3 rename"
    );
}

#[test]
fn test_roundtrip_pure_rename() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/pure_rename.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load");

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().join("roundtrip.cnnd");
    save_node_networks_to_file(
        &mut registry,
        &temp_path,
        load_result.direct_editing_mode,
        &load_result.cli_access_rules,
    )
    .expect("Failed to save");

    // Re-save is v3: second load goes through the v3 no-op path.
    let saved = std::fs::read_to_string(&temp_path).expect("read saved file");
    assert!(
        saved.contains("\"version\": 3"),
        "saved file should be tagged version: 3"
    );
    assert!(
        !saved.contains("\"unit_cell\""),
        "saved file must not contain the old `unit_cell` node type name"
    );
    assert!(
        !saved.contains("\"atom_move\""),
        "saved file must not contain the old `atom_move` node type name"
    );

    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_path.to_str().unwrap())
        .expect("Failed to reload");

    let net1 = registry.node_networks.get("Main").unwrap();
    let net2 = registry2.node_networks.get("Main").unwrap();
    assert_eq!(net1.nodes.len(), net2.nodes.len());
    assert_eq!(net1.return_node_id, net2.return_node_id);
}

/// Fixture 7 — `custom_network.cnnd`:
/// the rename walker must reach the parameter/output-pin type strings inside
/// each custom network's embedded `node_type`, not just the top-level Main
/// network. Also verifies the `[Atomic]` (bracketed) Display form rewrites to
/// `[Molecule]` rather than the abstract `HasAtoms`.
#[test]
fn test_load_custom_network_rename() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/custom_network.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load custom_network.cnnd");

    let my_custom = registry
        .node_networks
        .get("my_custom")
        .expect("my_custom network missing");

    // Custom-network output_type: `Geometry` → `Blueprint`.
    assert_eq!(
        *my_custom.node_type.output_type(),
        DataType::Blueprint,
        "custom-network output_type should be Blueprint after rename"
    );

    // Custom-network parameter types: `UnitCell` → `LatticeVecs`, `[Atomic]` → `[Molecule]`.
    let param_types: Vec<(String, DataType)> = my_custom
        .node_type
        .parameters
        .iter()
        .map(|p| (p.name.clone(), p.data_type.clone()))
        .collect();
    let expected: Vec<(String, DataType)> = vec![
        ("uc".to_string(), DataType::LatticeVecs),
        (
            "motifs".to_string(),
            DataType::Array(Box::new(DataType::Molecule)),
        ),
    ];
    assert_eq!(param_types, expected);

    // The registry must expose the custom type with the renamed output.
    let custom_type = registry
        .get_node_type("my_custom")
        .expect("my_custom should be registered as a node type");
    assert_eq!(*custom_type.output_type(), DataType::Blueprint);
}

#[test]
fn test_roundtrip_custom_network_rename() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/custom_network.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load");

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().join("roundtrip.cnnd");
    save_node_networks_to_file(
        &mut registry,
        &temp_path,
        load_result.direct_editing_mode,
        &load_result.cli_access_rules,
    )
    .expect("Failed to save");

    let saved = std::fs::read_to_string(&temp_path).expect("read saved file");
    assert!(
        !saved.contains("\"UnitCell\""),
        "saved file must not contain v2 `UnitCell` DataType string"
    );
    assert!(
        !saved.contains("\"Geometry\""),
        "saved file must not contain v2 `Geometry` DataType string"
    );
    assert!(
        !saved.contains("\"[Atomic]\""),
        "saved file must not contain v2 `[Atomic]` DataType string"
    );

    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_path.to_str().unwrap())
        .expect("Failed to reload");

    let c1 = registry.node_networks.get("my_custom").unwrap();
    let c2 = registry2.node_networks.get("my_custom").unwrap();
    assert_eq!(c1.node_type.output_type(), c2.node_type.output_type());
    assert_eq!(
        c1.node_type.parameters.len(),
        c2.node_type.parameters.len()
    );
}

// ---------------------------------------------------------------------------
// Phase 3: deleted-node drop (`atom_trans`, `lattice_symop`).
// ---------------------------------------------------------------------------

/// Fixture 6 — `atom_trans_present.cnnd`:
/// two parallel chains covering both deleted v2 node types:
/// 1. `parameter(Atomic) → atom_trans → atom_move` (tests atom_trans drop);
/// 2. `cuboid → lattice_symop → lattice_move` (tests lattice_symop drop).
///
/// In each chain the deleted node sits between an upstream source and a
/// downstream consumer with a polymorphic output (so the disconnected input
/// surfaces as an unresolved-output validation error). The downstream
/// consumers are also both subject to node renames (`atom_move → free_move`,
/// `lattice_move → structure_move`), exercising the rename pass's
/// interaction with the deleted-node drop.
///
/// Expected post-migration state:
/// - the two deleted nodes disappear;
/// - each downstream node keeps its argument slot but with an empty
///   `argument_output_pins` map (wire to the deleted node removed);
/// - `displayed_nodes` no longer references the deleted ids;
/// - `validate_network` flags both downstream consumers as invalid.
#[test]
fn test_load_atom_trans_and_lattice_symop_dropped() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/atom_trans_present.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load atom_trans_present.cnnd");

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // Deleted node types must be gone.
    let type_names: std::collections::HashSet<&str> = network
        .nodes
        .values()
        .map(|n| n.node_type_name.as_str())
        .collect();
    assert!(
        !type_names.contains("atom_trans"),
        "atom_trans must be dropped; got {:?}",
        type_names
    );
    assert!(
        !type_names.contains("lattice_symop"),
        "lattice_symop must be dropped; got {:?}",
        type_names
    );
    // The surviving nodes are the two upstream sources, the two downstream
    // consumers (renamed), and nothing else.
    assert_eq!(
        network.nodes.len(),
        4,
        "expected 4 surviving nodes (parameter, free_move, cuboid, structure_move); got {}",
        network.nodes.len()
    );

    // Downstream wire disconnection: free_move's arg 0 (previously wired to
    // atom_trans=id 2) must be empty now.
    let free_move = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "free_move")
        .expect("free_move node missing");
    assert!(
        free_move.arguments[0].argument_output_pins.is_empty(),
        "free_move's input wire to atom_trans should be disconnected; got {:?}",
        free_move.arguments[0].argument_output_pins
    );

    // Same for structure_move: its input (previously wired to lattice_symop=id 5)
    // must be empty.
    let structure_move = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "structure_move")
        .expect("structure_move node missing");
    assert!(
        structure_move.arguments[0].argument_output_pins.is_empty(),
        "structure_move's input wire to lattice_symop should be disconnected; got {:?}",
        structure_move.arguments[0].argument_output_pins
    );

    // lattice_symop's id (5) was in `displayed_node_ids`; the deleted-node drop
    // must strip it so the display map doesn't reference a missing node.
    assert!(
        !network.displayed_nodes.contains_key(&5),
        "displayed_nodes should not reference deleted lattice_symop (id 5)"
    );

    // Validate the loaded network and assert the resulting dangling inputs are
    // surfaced as errors. This is the user-visible signal the design calls for:
    // "this old node was removed, please replace it."
    let free_move_id = free_move.id;
    let structure_move_id = structure_move.id;
    let mut network = registry.node_networks.remove("Main").unwrap();
    validate_network(&mut network, &mut registry, None);
    assert!(
        !network.valid,
        "network should be invalid after the migration dropped nodes with polymorphic downstream consumers"
    );
    let error_nodes: std::collections::HashSet<u64> = network
        .validation_errors
        .iter()
        .filter_map(|e| e.node_id)
        .collect();
    // Either consumer can short-circuit the validator (it returns on the first
    // failing node); assert that at least one of them produced the error, and
    // that neither of the deleted node ids somehow survived as an error source.
    assert!(
        error_nodes.contains(&free_move_id) || error_nodes.contains(&structure_move_id),
        "at least one dangling consumer (free_move={}, structure_move={}) should have a validation error; got errors on {:?}",
        free_move_id,
        structure_move_id,
        error_nodes
    );
    assert!(
        !error_nodes.contains(&2) && !error_nodes.contains(&5),
        "deleted node ids must not appear in validation errors; got {:?}",
        error_nodes
    );
}

#[test]
fn test_roundtrip_atom_trans_and_lattice_symop_dropped() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/atom_trans_present.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load");

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().join("roundtrip.cnnd");
    save_node_networks_to_file(
        &mut registry,
        &temp_path,
        load_result.direct_editing_mode,
        &load_result.cli_access_rules,
    )
    .expect("Failed to save");

    let saved = std::fs::read_to_string(&temp_path).expect("read saved file");
    assert!(
        saved.contains("\"version\": 3"),
        "saved file should be tagged version: 3"
    );
    assert!(
        !saved.contains("\"atom_trans\""),
        "saved file must not contain the deleted `atom_trans` node type"
    );
    assert!(
        !saved.contains("\"lattice_symop\""),
        "saved file must not contain the deleted `lattice_symop` node type"
    );

    // Reload-after-save is byte-idempotent and stays on the v3 no-op path.
    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_path.to_str().unwrap())
        .expect("Failed to reload");
    let n1 = registry.node_networks.get("Main").unwrap();
    let n2 = registry2.node_networks.get("Main").unwrap();
    assert_eq!(n1.nodes.len(), n2.nodes.len());

    // And the parameter node kept its renamed Molecule type through the trip.
    let param = n2
        .nodes
        .values()
        .find(|n| n.node_type_name == "parameter")
        .expect("parameter missing after reload");
    let custom_type = param
        .custom_node_type
        .as_ref()
        .expect("parameter custom_node_type missing");
    assert_eq!(*custom_type.output_type(), DataType::Molecule);
}

// ---------------------------------------------------------------------------
// Phase 4: primitive input adaptation (`LatticeVecs` wire → `structure` adapter).
// ---------------------------------------------------------------------------

/// Fixture 3 — `primitive_with_lattice.cnnd`:
/// a Main network with two primitives covering both branches of the Phase 4
/// decision tree:
/// 1. `unit_cell → cuboid.unit_cell` (wired pin) — verifies adapter synthesis;
/// 2. `sphere` with its `unit_cell` pin unwired — verifies no-op, new
///    `structure` input falls back to the diamond default.
///
/// Expected post-migration state:
/// - `unit_cell` is renamed to `lattice_vecs`;
/// - a synthesized `structure` node sits between `lattice_vecs` and `cuboid`,
///   with the old wire routed to its `lattice_vecs` input (arg 1);
/// - `cuboid`'s `structure` input (arg 2) points at the adapter's output 0;
/// - `sphere` is untouched (no adapter synthesized);
/// - `next_node_id` advances to cover the new adapter's id;
/// - the network validates cleanly (Blueprint flows to the return node).
#[test]
fn test_load_primitive_with_lattice_adapter_synthesised() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/primitive_with_lattice.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load primitive_with_lattice.cnnd");

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // Original 3 + 1 synthesized adapter = 4 nodes.
    assert_eq!(
        network.nodes.len(),
        4,
        "expected 3 original + 1 synthesized structure adapter; got {}",
        network.nodes.len()
    );

    // `unit_cell` renamed; exactly one synthesized `structure` adapter.
    let structure_adapters: Vec<&rust_lib_flutter_cad::structure_designer::node_network::Node> =
        network
            .nodes
            .values()
            .filter(|n| n.node_type_name == "structure")
            .collect();
    assert_eq!(
        structure_adapters.len(),
        1,
        "expected exactly one synthesized structure adapter; got {}",
        structure_adapters.len()
    );
    let adapter = structure_adapters[0];

    // Adapter must be the id allocated from next_node_id (10 → allocated to adapter).
    assert_eq!(
        adapter.id, 10,
        "adapter should use the id pulled from next_node_id"
    );
    // The adapter's 4 arguments: only arg 1 (lattice_vecs) is wired — to the
    // renamed `lattice_vecs` node (id 1). The other three are unwired so the
    // primitive still gets diamond defaults for motif / motif_offset / base.
    assert_eq!(adapter.arguments.len(), 4);
    assert!(adapter.arguments[0].argument_output_pins.is_empty());
    assert_eq!(
        adapter.arguments[1].argument_output_pins.get(&1),
        Some(&0),
        "adapter's lattice_vecs input (arg 1) must be wired to the renamed unit_cell node (id 1, pin 0)"
    );
    assert!(adapter.arguments[2].argument_output_pins.is_empty());
    assert!(adapter.arguments[3].argument_output_pins.is_empty());

    // The cuboid's structure input (arg 2) now points at the adapter's output 0.
    let cuboid = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "cuboid")
        .expect("cuboid missing");
    assert_eq!(
        cuboid.arguments[2].argument_output_pins.get(&adapter.id),
        Some(&0),
        "cuboid's structure input (arg 2) should be wired to adapter's output 0"
    );
    assert_eq!(
        cuboid.arguments[2].argument_output_pins.len(),
        1,
        "cuboid's structure pin must have the adapter as its only source"
    );

    // The sphere — whose unit_cell pin was unwired in v2 — has no adapter.
    // Its structure input (arg 2) stays empty; diamond defaults apply.
    let sphere = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "sphere")
        .expect("sphere missing");
    assert!(
        sphere.arguments[2].argument_output_pins.is_empty(),
        "sphere had no v2 unit_cell wire; its structure pin must stay unwired (no adapter synthesized)"
    );

    // next_node_id advanced past the allocated id so future nodes don't clash.
    assert!(
        network.next_node_id > adapter.id,
        "next_node_id ({}) should be greater than the adapter's id ({})",
        network.next_node_id,
        adapter.id
    );

    // The migrated network validates cleanly: cuboid outputs Blueprint to the
    // return node and no pin is left dangling.
    let mut network = registry.node_networks.remove("Main").unwrap();
    validate_network(&mut network, &mut registry, None);
    let error_texts: Vec<String> = network
        .validation_errors
        .iter()
        .map(|e| e.error_text.clone())
        .collect();
    assert!(
        network.valid,
        "migrated network should validate cleanly; errors: {:?}",
        error_texts
    );
}

#[test]
fn test_roundtrip_primitive_with_lattice() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/primitive_with_lattice.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load");

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().join("roundtrip.cnnd");
    save_node_networks_to_file(
        &mut registry,
        &temp_path,
        load_result.direct_editing_mode,
        &load_result.cli_access_rules,
    )
    .expect("Failed to save");

    let saved = std::fs::read_to_string(&temp_path).expect("read saved file");
    assert!(
        saved.contains("\"version\": 3"),
        "saved file should be tagged version: 3"
    );
    assert!(
        !saved.contains("\"unit_cell\""),
        "saved file must not contain the renamed-away `unit_cell` node type"
    );
    // The adapter round-trips as a real `structure` node.
    assert!(
        saved.contains("\"structure\""),
        "saved file should contain the synthesized `structure` node type"
    );

    // Reload-after-save stays on the v3 no-op path and preserves the adapter.
    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_path.to_str().unwrap())
        .expect("Failed to reload");
    let n1 = registry.node_networks.get("Main").unwrap();
    let n2 = registry2.node_networks.get("Main").unwrap();
    assert_eq!(n1.nodes.len(), n2.nodes.len());

    // Exactly one structure adapter survives the round trip — no second pass
    // can sneak an extra one in on reload.
    let adapter_count = n2
        .nodes
        .values()
        .filter(|n| n.node_type_name == "structure")
        .count();
    assert_eq!(
        adapter_count, 1,
        "second load must not synthesize a second adapter; got {}",
        adapter_count
    );
}

// ---------------------------------------------------------------------------
// Phase 5: `atom_fill` split (renames to `materialize`, synthesises a
// `structure` source node holding the motif / motif_offset wires).
// ---------------------------------------------------------------------------

/// Fixture 2 — `atom_fill_split.cnnd`:
/// a `unit_cell → cuboid → atom_fill` pipeline with motif and motif_offset
/// sources wired in, every Bool flag pin (passivate, rm_single, surf_recon,
/// invert_phase) wired to its own `bool` source, and non-default
/// `AtomFillData` field values. Verifies the full algorithm in one fixture:
/// node-rename + data-tag rename, argument re-indexing, NodeData translation
/// (drop `motif_offset`, keep the rest), and synthesis of the new `structure`
/// node carrying the motif / motif_offset wires.
///
/// Expected post-migration state:
/// - the v2 `atom_fill` is renamed to `materialize` (both `node_type_name`
///   and the `data_type` dispatch tag);
/// - `materialize.arguments` has 5 slots in v3 order: shape (from cuboid),
///   then the four Bool flag wires;
/// - a synthesized `structure` node receives the motif wire on arg 2 and the
///   motif_offset wire on arg 3, with arg 0 (structure) and arg 1
///   (lattice_vecs) left unwired per the design;
/// - `MaterializeData` carries `parameter_element_value_definition`,
///   `hydrogen_passivation`, `remove_single_bond_atoms_before_passivation`,
///   `surface_reconstruction`, `invert_phase` verbatim from `AtomFillData`;
/// - `next_node_id` advances past every newly-allocated id (one for the
///   primitive-adaptation adapter on cuboid, one for the atom_fill split's
///   structure source).
#[test]
fn test_load_atom_fill_split() {
    use rust_lib_flutter_cad::structure_designer::nodes::materialize::MaterializeData;

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/atom_fill_split.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load atom_fill_split.cnnd");

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // No surviving atom_fill; exactly one materialize.
    let type_names: std::collections::HashSet<&str> = network
        .nodes
        .values()
        .map(|n| n.node_type_name.as_str())
        .collect();
    assert!(
        !type_names.contains("atom_fill"),
        "atom_fill must be renamed; got {:?}",
        type_names
    );
    let materialize_nodes: Vec<&rust_lib_flutter_cad::structure_designer::node_network::Node> =
        network
            .nodes
            .values()
            .filter(|n| n.node_type_name == "materialize")
            .collect();
    assert_eq!(materialize_nodes.len(), 1, "expected one materialize");
    let materialize = materialize_nodes[0];
    // The renamed node keeps its original id (9).
    assert_eq!(materialize.id, 9);

    // The split synthesises one structure source node; the primitive
    // adaptation pass synthesises another for the cuboid's old unit_cell wire,
    // so we expect exactly two `structure` nodes.
    let structure_nodes: Vec<&rust_lib_flutter_cad::structure_designer::node_network::Node> =
        network
            .nodes
            .values()
            .filter(|n| n.node_type_name == "structure")
            .collect();
    assert_eq!(
        structure_nodes.len(),
        2,
        "expected one structure adapter (from primitive adaptation) plus one \
         structure source (from atom_fill split); got {}",
        structure_nodes.len()
    );

    // The split's structure source is the one with motif (arg 2) and
    // motif_offset (arg 3) wired and structure (arg 0) + lattice_vecs (arg 1)
    // empty. The primitive adapter is the inverse: lattice_vecs (arg 1) wired,
    // others empty.
    let split_source = structure_nodes
        .iter()
        .find(|n| {
            !n.arguments[2].argument_output_pins.is_empty()
                && !n.arguments[3].argument_output_pins.is_empty()
        })
        .expect("structure source for atom_fill split should have motif + motif_offset wired");
    assert_eq!(split_source.arguments.len(), 4);
    assert!(
        split_source.arguments[0].argument_output_pins.is_empty(),
        "split source's `structure` input (arg 0) must be unwired"
    );
    assert!(
        split_source.arguments[1].argument_output_pins.is_empty(),
        "split source's `lattice_vecs` input (arg 1) must be unwired"
    );
    assert_eq!(
        split_source.arguments[2].argument_output_pins.get(&3),
        Some(&0),
        "split source's `motif` input (arg 2) must be wired to motif source (id 3, pin 0)"
    );
    assert_eq!(
        split_source.arguments[3].argument_output_pins.get(&4),
        Some(&0),
        "split source's `motif_offset` input (arg 3) must be wired to vec3 source (id 4, pin 0)"
    );

    // Materialize args, in v3 order: shape (cuboid id 2), passivate (id 5),
    // rm_single (id 6), surf_recon (id 7), invert_phase (id 8). The cuboid's
    // shape wire still points at the cuboid (id 2) — the primitive-adapter
    // pass rewrote the cuboid's own `structure` input, not its downstream wire.
    assert_eq!(materialize.arguments.len(), 5);
    assert_eq!(
        materialize.arguments[0].argument_output_pins.get(&2),
        Some(&0)
    );
    assert_eq!(
        materialize.arguments[1].argument_output_pins.get(&5),
        Some(&0),
        "passivate wire must move from v2 arg 3 to v3 arg 1"
    );
    assert_eq!(
        materialize.arguments[2].argument_output_pins.get(&6),
        Some(&0),
        "rm_single wire must move from v2 arg 4 to v3 arg 2"
    );
    assert_eq!(
        materialize.arguments[3].argument_output_pins.get(&7),
        Some(&0),
        "surf_recon wire must move from v2 arg 5 to v3 arg 3"
    );
    assert_eq!(
        materialize.arguments[4].argument_output_pins.get(&8),
        Some(&0),
        "invert_phase wire must move from v2 arg 6 to v3 arg 4"
    );

    // NodeData translation: AtomFillData → MaterializeData. `motif_offset` is
    // dropped; everything else carries over verbatim from the v2 fixture.
    let mat_data = materialize
        .data
        .as_any_ref()
        .downcast_ref::<MaterializeData>()
        .expect("materialize node should hold MaterializeData");
    assert_eq!(mat_data.parameter_element_value_definition, "X = C");
    assert!(!mat_data.hydrogen_passivation);
    assert!(mat_data.remove_single_bond_atoms_before_passivation);
    assert!(!mat_data.surface_reconstruction);
    assert!(mat_data.invert_phase);

    // next_node_id advances past every synthesised id — primitive adapter +
    // atom_fill split each pull one. With v2 next_node_id=10, post-migration
    // must be ≥ 12.
    assert!(
        network.next_node_id >= 12,
        "next_node_id should advance past both the cuboid adapter and the \
         atom_fill split's structure source; got {}",
        network.next_node_id
    );
}

#[test]
fn test_roundtrip_atom_fill_split() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/atom_fill_split.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load");

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().join("roundtrip.cnnd");
    save_node_networks_to_file(
        &mut registry,
        &temp_path,
        load_result.direct_editing_mode,
        &load_result.cli_access_rules,
    )
    .expect("Failed to save");

    let saved = std::fs::read_to_string(&temp_path).expect("read saved file");
    assert!(
        saved.contains("\"version\": 3"),
        "saved file should be tagged version: 3"
    );
    assert!(
        !saved.contains("\"atom_fill\""),
        "saved file must not contain the v2 `atom_fill` node type name"
    );
    assert!(
        saved.contains("\"materialize\""),
        "saved file should contain the v3 `materialize` node type"
    );

    // Reload-after-save stays on the v3 no-op path; the split runs once and
    // doesn't multiply on subsequent loads.
    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_path.to_str().unwrap())
        .expect("Failed to reload");
    let n1 = registry.node_networks.get("Main").unwrap();
    let n2 = registry2.node_networks.get("Main").unwrap();
    assert_eq!(n1.nodes.len(), n2.nodes.len());

    let materialize_count = n2
        .nodes
        .values()
        .filter(|n| n.node_type_name == "materialize")
        .count();
    let structure_count = n2
        .nodes
        .values()
        .filter(|n| n.node_type_name == "structure")
        .count();
    assert_eq!(materialize_count, 1, "second load must not duplicate the materialize node");
    assert_eq!(
        structure_count, 2,
        "second load must not synthesise a third structure node"
    );
}

/// Fixture 4 — `shared_unit_cell.cnnd`:
/// one `unit_cell` (id 1) feeding two primitives — a `cuboid` (id 2) which
/// then feeds `atom_fill.shape` (id 4), and a parallel `sphere` (id 3) used
/// directly. Verifies that the primitive-adaptation pass and the atom_fill
/// split compose cleanly: the unit_cell's two consumers each get their own
/// independent `structure` adapter (fresh-per-consumer is correct under
/// evaluator semantics; the design treats deduplication as a polish item),
/// and the atom_fill split adds a third `structure` for its motif/motif_offset
/// holder — total three `structure` nodes synthesised.
#[test]
fn test_load_shared_unit_cell_composes_passes() {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(
        &mut registry,
        &format!("{}/shared_unit_cell.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load shared_unit_cell.cnnd");

    let network = registry
        .node_networks
        .get("Main")
        .expect("Main network missing");

    // Original 4 + 2 primitive adapters + 1 atom_fill-split source = 7 nodes.
    assert_eq!(
        network.nodes.len(),
        7,
        "expected 4 originals + 2 primitive adapters + 1 atom_fill split source; got {}",
        network.nodes.len()
    );

    let structure_nodes: Vec<&rust_lib_flutter_cad::structure_designer::node_network::Node> =
        network
            .nodes
            .values()
            .filter(|n| n.node_type_name == "structure")
            .collect();
    assert_eq!(
        structure_nodes.len(),
        3,
        "expected 3 synthesised structure nodes (one adapter per primitive + \
         one source for the atom_fill split); got {}",
        structure_nodes.len()
    );

    // The two primitive adapters both read from the renamed lattice_vecs (id 1).
    // Each adapter's lattice_vecs input (arg 1) must be wired to id 1, with
    // structure / motif / motif_offset (args 0, 2, 3) all unwired.
    let primitive_adapters: Vec<&&rust_lib_flutter_cad::structure_designer::node_network::Node> =
        structure_nodes
            .iter()
            .filter(|n| {
                !n.arguments[1].argument_output_pins.is_empty()
                    && n.arguments[2].argument_output_pins.is_empty()
                    && n.arguments[3].argument_output_pins.is_empty()
            })
            .collect();
    assert_eq!(
        primitive_adapters.len(),
        2,
        "expected exactly two primitive adapters; got {}",
        primitive_adapters.len()
    );
    for adapter in &primitive_adapters {
        assert_eq!(
            adapter.arguments[1].argument_output_pins.get(&1),
            Some(&0),
            "primitive adapter's lattice_vecs (arg 1) must be wired to id 1, pin 0"
        );
    }

    // The third structure is the atom_fill split's source — motif/motif_offset
    // empty (since the v2 atom_fill had nothing wired there) and
    // lattice_vecs/structure also empty.
    let split_source = structure_nodes
        .iter()
        .find(|n| n.arguments[1].argument_output_pins.is_empty())
        .expect("expected one structure node with no lattice_vecs wire (the split source)");
    assert!(
        split_source
            .arguments
            .iter()
            .all(|a| a.argument_output_pins.is_empty()),
        "atom_fill split's structure source should have all four inputs unwired \
         when the v2 atom_fill had nothing wired to motif / motif_offset"
    );

    // The materialize node (renamed atom_fill, id 4) has its shape wire
    // pointing at the cuboid (id 2), and the four Bool flag args are unwired
    // (no flag wires in this fixture).
    let materialize = network
        .nodes
        .values()
        .find(|n| n.node_type_name == "materialize")
        .expect("materialize node missing");
    assert_eq!(materialize.id, 4);
    assert_eq!(materialize.arguments.len(), 5);
    assert_eq!(
        materialize.arguments[0].argument_output_pins.get(&2),
        Some(&0),
        "materialize.shape (arg 0) must still point at cuboid (id 2, pin 0)"
    );
    for i in 1..5 {
        assert!(
            materialize.arguments[i].argument_output_pins.is_empty(),
            "materialize Bool arg {} should be unwired in this fixture",
            i
        );
    }

    // Each cuboid / sphere has its `structure` input (arg 2) rewired to its
    // own adapter — never to id 1 directly.
    for primitive_name in ["cuboid", "sphere"] {
        let prim = network
            .nodes
            .values()
            .find(|n| n.node_type_name == primitive_name)
            .unwrap_or_else(|| panic!("{} node missing", primitive_name));
        let wires = &prim.arguments[2].argument_output_pins;
        assert_eq!(wires.len(), 1, "{} should have exactly one structure-input source", primitive_name);
        assert!(
            !wires.contains_key(&1),
            "{}'s structure input must point at its adapter, not directly at id 1",
            primitive_name
        );
    }
}

#[test]
fn test_roundtrip_shared_unit_cell() {
    let mut registry = NodeTypeRegistry::new();
    let load_result = load_node_networks_from_file(
        &mut registry,
        &format!("{}/shared_unit_cell.cnnd", FIXTURE_DIR),
    )
    .expect("Failed to load");

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().join("roundtrip.cnnd");
    save_node_networks_to_file(
        &mut registry,
        &temp_path,
        load_result.direct_editing_mode,
        &load_result.cli_access_rules,
    )
    .expect("Failed to save");

    let saved = std::fs::read_to_string(&temp_path).expect("read saved file");
    assert!(
        saved.contains("\"version\": 3"),
        "saved file should be tagged version: 3"
    );
    assert!(
        !saved.contains("\"atom_fill\""),
        "saved file must not contain the v2 `atom_fill` node type name"
    );

    // Reload-after-save: structure-node count must be stable (no new
    // synthesis on top of v3 input).
    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_path.to_str().unwrap())
        .expect("Failed to reload");
    let n1 = registry.node_networks.get("Main").unwrap();
    let n2 = registry2.node_networks.get("Main").unwrap();
    assert_eq!(n1.nodes.len(), n2.nodes.len());
    let s1 = n1.nodes.values().filter(|n| n.node_type_name == "structure").count();
    let s2 = n2.nodes.values().filter(|n| n.node_type_name == "structure").count();
    assert_eq!(s1, s2, "structure-node count must survive a v3 round trip");
}
