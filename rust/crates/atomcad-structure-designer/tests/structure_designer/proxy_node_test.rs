//! Phase 3 of `doc/design_proxy_node.md` — the `proxy` node shell.
//!
//! The algorithm itself is tested in `atomcad-crystolecule`'s
//! `proxy_cut_test.rs` (Phases 1–2). What is exercised here is everything the
//! node adds on top: phase pass-through, stored-property versus wired-pin
//! precedence, the serde/text-format defaults, the localized error wording,
//! the eval cache, and the subtitle.
//!
//! The fixture is a **capped carbon chain** rather than a lattice cube: it is
//! small, its bond distances are obvious by inspection, and — because a chain
//! has no dropped heavy atom with two kept neighbours — `fill` never fires, so
//! every count below is exactly the plain cut.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use atomcad_crystolecule::proxy_cut::{ProxyOptions, proxy_cut};
use atomcad_crystolecule::structure::Structure;
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::{
    CrystalData, MoleculeData, NetworkResult,
};
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::int::IntData;
use atomcad_structure_designer::nodes::proxy::{ProxyData, ProxyEvalCache};
use atomcad_structure_designer::nodes::value::ValueData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::TextValue;
use glam::f64::{DVec2, DVec3};
use std::collections::{HashMap, HashSet};

// ============================================================================
// Fixture
// ============================================================================

const CC: f64 = 1.54;

/// `C0–C1–C2–C3–C4` along x, with three hydrogens on each end carbon so no
/// chain atom has a single bond (an atom with exactly one bond is a *rider*,
/// §4.1, and a bare chain's ends would be riders and get no distance at all).
/// `C0` carries the `focus` tag.
///
/// Bond distances from `C0` are therefore simply `0, 1, 2, 3, 4`.
fn capped_chain() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let carbons: Vec<u32> = (0..5)
        .map(|i| s.add_atom(6, DVec3::new(i as f64 * CC, 0.0, 0.0)))
        .collect();
    for pair in carbons.windows(2) {
        s.add_bond(pair[0], pair[1], BOND_SINGLE);
    }
    // Three riders on each end carbon, splayed off the chain axis.
    for (carbon, sign) in [(carbons[0], -1.0f64), (carbons[4], 1.0f64)] {
        let base = s.get_atom(carbon).unwrap().position;
        for (dy, dz) in [(1.0, 0.0), (-0.5, 0.87), (-0.5, -0.87)] {
            let h = s.add_atom(1, base + DVec3::new(sign * 0.4, dy, dz));
            s.add_bond(carbon, h, BOND_SINGLE);
        }
    }
    s.add_atom_tag(carbons[0], "focus").unwrap();
    s
}

// ============================================================================
// Harness helpers
// ============================================================================

fn setup(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn add_value_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    position: DVec2,
    value: NetworkResult,
) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    network.add_node("value", position, 0, Box::new(ValueData { value }))
}

fn molecule_value(structure: AtomicStructure) -> NetworkResult {
    NetworkResult::Molecule(MoleculeData {
        atoms: structure,
        geo_tree_root: None,
    })
}

fn crystal_value(structure: AtomicStructure, lattice: Structure) -> NetworkResult {
    NetworkResult::Crystal(CrystalData {
        structure: lattice,
        atoms: structure,
        geo_tree_root: None,
        alignment: Default::default(),
        alignment_reason: None,
    })
}

fn set_proxy_props(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    props: &[(&str, TextValue)],
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    let map: HashMap<String, TextValue> = props
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    node.data.set_text_properties(&map).unwrap();
}

fn clear_wires(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    param_index: usize,
) {
    designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap()
        .nodes
        .get_mut(&node_id)
        .unwrap()
        .arguments[param_index]
        .incoming_wires
        .clear();
}

fn evaluate(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement::root(network)];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

fn evaluate_to_atomic(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
) -> AtomicStructure {
    match evaluate(designer, network_name, node_id) {
        NetworkResult::Crystal(c) => c.atoms,
        NetworkResult::Molecule(m) => m.atoms,
        NetworkResult::Error(e) => panic!("expected an atomic result, got Error: {e}"),
        other => panic!(
            "expected an atomic result, got {:?}",
            other.infer_data_type()
        ),
    }
}

fn error_message(designer: &StructureDesigner, network_name: &str, node_id: u64) -> String {
    match evaluate(designer, network_name, node_id) {
        NetworkResult::Error(e) => e,
        other => panic!("expected an Error, got {:?}", other.infer_data_type()),
    }
}

fn atom_count(s: &AtomicStructure) -> usize {
    s.atom_ids().count()
}

fn count_element(s: &AtomicStructure, atomic_number: i16) -> usize {
    s.atoms_values()
        .filter(|a| a.atomic_number == atomic_number)
        .count()
}

/// A `value` → `proxy` pair with the given stored properties.
fn chain_network(props: &[(&str, TextValue)]) -> (StructureDesigner, &'static str, u64) {
    let net = "test";
    let mut designer = setup(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(capped_chain()),
    );
    let proxy_id = designer.add_node("proxy", DVec2::new(200.0, 0.0));
    set_proxy_props(&mut designer, net, proxy_id, props);
    designer.connect_nodes(value_id, 0, proxy_id, 0);
    (designer, net, proxy_id)
}

// ============================================================================
// Phase pass-through
// ============================================================================

/// `OutputPinDefinition::single_same_as("molecule")` plus `map_atomic`:
/// a Crystal keeps its lattice so downstream lattice-aware nodes still work,
/// and a Molecule stays a Molecule.
#[test]
fn proxy_preserves_the_input_phase_and_lattice() {
    let net = "test";
    let mut designer = setup(net);
    let lattice = Structure::diamond();
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        crystal_value(capped_chain(), lattice.clone()),
    );
    let proxy_id = designer.add_node("proxy", DVec2::new(200.0, 0.0));
    set_proxy_props(&mut designer, net, proxy_id, &[("hops", TextValue::Int(2))]);
    designer.connect_nodes(value_id, 0, proxy_id, 0);

    match evaluate(&designer, net, proxy_id) {
        NetworkResult::Crystal(c) => {
            assert_eq!(
                format!("{:?}", c.structure),
                format!("{:?}", lattice),
                "a Crystal keeps the input lattice through the cut"
            );
            assert_eq!(atom_count(&c.atoms), 7, "C0..C2 + 3 riders + 1 cap");
        }
        other => panic!("expected a Crystal, got {:?}", other.infer_data_type()),
    }

    // Same structure as a Molecule stays a Molecule.
    let (designer, net, proxy_id) = chain_network(&[("hops", TextValue::Int(2))]);
    match evaluate(&designer, net, proxy_id) {
        NetworkResult::Molecule(m) => assert_eq!(atom_count(&m.atoms), 7),
        other => panic!("expected a Molecule, got {:?}", other.infer_data_type()),
    }
}

// ============================================================================
// Defaults
// ============================================================================

/// A freshly created node evaluates as `hops 6 / free 3 / fill on / passivate
/// on / H / core off`. The chain's farthest atom is 4 hops out, so `hops: 6`
/// keeps everything and no bond is severed — and `free: 3` still freezes the
/// far end.
#[test]
fn proxy_defaults_are_hops_6_free_3_fill_on_passivate_on_hydrogen_core_off() {
    let fresh = ProxyData::default();
    assert_eq!(fresh.focus, "focus");
    assert_eq!(fresh.hops, 6);
    assert_eq!(fresh.free, 3);
    assert!(!fresh.rm_single);
    assert!(fresh.passivate);
    assert_eq!(fresh.passiv_elem, 1);
    assert_eq!(fresh.core, -1);
    assert!(fresh.fill);

    let (designer, net, proxy_id) = chain_network(&[]);
    let out = evaluate_to_atomic(&designer, net, proxy_id);

    assert_eq!(atom_count(&out), 11, "nothing is out of reach at hops: 6");
    assert_eq!(count_element(&out, 1), 6, "no cap was placed (no bond cut)");
    assert!(
        !out.tag_names().iter().any(|n| n == "high"),
        "core is off by default, so no `high` tag is interned"
    );

    // `free: 3` freezes C4 (distance 4) and its three riders, nothing else.
    let frozen = out.atoms_values().filter(|a| a.is_frozen()).count();
    assert_eq!(frozen, 4, "C4 plus its three riders");
}

// ============================================================================
// Stored property versus wired pin
// ============================================================================

/// An `int` node wired to `hops` overrides the stored value; disconnecting it
/// restores the stored value. This is the `evaluate_or_default` precedence
/// every tunable on this node follows.
#[test]
fn a_wired_hops_pin_overrides_the_stored_property() {
    let net = "test";
    let mut designer = setup(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(capped_chain()),
    );
    let proxy_id = designer.add_node("proxy", DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, proxy_id, 0);

    // Stored `hops: 6` keeps the whole chain.
    assert_eq!(
        atom_count(&evaluate_to_atomic(&designer, net, proxy_id)),
        11
    );

    let int_id = designer.add_node("int", DVec2::new(0.0, 200.0));
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(net)
            .unwrap();
        network.nodes.get_mut(&int_id).unwrap().data = Box::new(IntData { value: 2 });
    }
    designer.connect_nodes(int_id, 0, proxy_id, 2);

    assert_eq!(
        atom_count(&evaluate_to_atomic(&designer, net, proxy_id)),
        7,
        "the wired pin wins over the stored property"
    );

    clear_wires(&mut designer, net, proxy_id, 2);
    assert_eq!(
        atom_count(&evaluate_to_atomic(&designer, net, proxy_id)),
        11,
        "disconnecting restores the stored value"
    );
}

/// The remaining tunables read from their pins too — checked on the flags,
/// where a wrong pin index would otherwise pass unnoticed.
#[test]
fn the_flag_pins_override_their_stored_properties() {
    let net = "test";
    let mut designer = setup(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(capped_chain()),
    );
    let proxy_id = designer.add_node("proxy", DVec2::new(200.0, 0.0));
    set_proxy_props(&mut designer, net, proxy_id, &[("hops", TextValue::Int(2))]);
    designer.connect_nodes(value_id, 0, proxy_id, 0);

    let baseline = evaluate_to_atomic(&designer, net, proxy_id);
    assert_eq!(count_element(&baseline, 1), 4, "3 riders + 1 hydrogen cap");

    // `passivate: false` on pin 5 removes the cap.
    let off = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 200.0),
        NetworkResult::Bool(false),
    );
    designer.connect_nodes(off, 0, proxy_id, 5);
    let no_caps = evaluate_to_atomic(&designer, net, proxy_id);
    assert_eq!(atom_count(&no_caps), 6, "C0..C2 + 3 riders, no cap");
    clear_wires(&mut designer, net, proxy_id, 5);

    // `passiv_elem: 9` on pin 6 caps with fluorine instead.
    let nine = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 400.0),
        NetworkResult::Int(9),
    );
    designer.connect_nodes(nine, 0, proxy_id, 6);
    let fluorine = evaluate_to_atomic(&designer, net, proxy_id);
    assert_eq!(count_element(&fluorine, 9), 1, "the cap is now fluorine");
    assert_eq!(count_element(&fluorine, 1), 3, "only the riders are left");
    clear_wires(&mut designer, net, proxy_id, 6);

    // `core: 1` on pin 7 paints `high` on C0 and C1 (and C0's riders).
    let one = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 600.0),
        NetworkResult::Int(1),
    );
    designer.connect_nodes(one, 0, proxy_id, 7);
    let cored = evaluate_to_atomic(&designer, net, proxy_id);
    let high = cored
        .atoms_values()
        .filter(|a| cored.atom_has_tag(a.id, "high"))
        .count();
    assert_eq!(high, 5, "C0, C1 and C0's three riders");
}

// ============================================================================
// Text format
// ============================================================================

fn empty_network() -> atomcad_structure_designer::node_network::NodeNetwork {
    use atomcad_structure_designer::data_type::DataType;
    use atomcad_structure_designer::node_network::NodeNetwork;
    use atomcad_structure_designer::node_type::NodeTypeCategory;
    use atomcad_structure_designer::node_type::{NodeType, OutputPinDefinition};

    NodeNetwork::new(NodeType {
        name: "test".to_string(),
        description: "Test network".to_string(),
        summary: None,
        category: NodeTypeCategory::Custom,
        parameters: vec![],
        output_pins: OutputPinDefinition::single(DataType::Molecule),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(atomcad_structure_designer::node_data::NoData {}),
        node_data_saver: atomcad_structure_designer::node_type::no_data_saver,
        node_data_loader: atomcad_structure_designer::node_type::no_data_loader,
    })
}

/// Authors `source` into a fresh network and returns the serialized text.
fn author_and_serialize(source: &str) -> String {
    use atomcad_structure_designer::text_format::{edit_network, serialize_network};

    let registry = NodeTypeRegistry::new();
    let mut network = empty_network();
    let result = edit_network(&mut network, &registry, source, true);
    assert!(result.success, "edit should succeed: {:?}", result.errors);
    serialize_network(&network, &registry, Some("test"))
}

/// The full line of §3.3 round-trips byte for byte, and the short form —
/// every omitted property at its default — serializes to that same full line.
/// The serializer writes every text property whatever its value, so there is
/// exactly one canonical spelling.
#[test]
fn proxy_text_format_full_line_and_short_form() {
    const FULL: &str = "proxy_6 = proxy { molecule: surface, focus: \"focus\", hops: 6, \
                        free: 3, rm_single: false, passivate: true, passiv_elem: 1, core: 1, \
                        fill: true, visible: true }";

    let full_source = format!("surface = import_xyz {{ }}\n{FULL}\n");
    let serialized = author_and_serialize(&full_source);
    assert!(
        serialized.contains(FULL),
        "the full line must serialize back to itself byte for byte; got:\n{serialized}"
    );

    // …and re-authoring the serialized text is a no-op.
    let reserialized = author_and_serialize(&serialized);
    assert_eq!(serialized, reserialized, "text round-trip is stable");

    // The short form takes every omitted property at its default and expands
    // to the very same full line.
    let short_source = "surface = import_xyz { }\n\
                        proxy_6 = proxy { molecule: surface, core: 1, visible: true }\n";
    let from_short = author_and_serialize(short_source);
    assert!(
        from_short.contains(FULL),
        "the short form must expand to the full line; got:\n{from_short}"
    );
}

/// Non-default flag and element values survive the round trip — the two the
/// design calls out, and the ones a wrong `TextValue` variant would silently
/// drop.
#[test]
fn proxy_text_format_non_default_values_round_trip() {
    let source = "p = proxy { focus: \"apex\", hops: 4, free: 4, rm_single: true, \
                  passivate: false, passiv_elem: 9, core: 0, fill: false }\n";
    let serialized = author_and_serialize(source);
    assert!(
        serialized.contains(
            "p = proxy { focus: \"apex\", hops: 4, free: 4, rm_single: true, passivate: false, \
             passiv_elem: 9, core: 0, fill: false }"
        ),
        "got:\n{serialized}"
    );
    assert_eq!(serialized, author_and_serialize(&serialized));
}

// ============================================================================
// .cnnd round trip
// ============================================================================

fn proxy_data_of(network: &atomcad_structure_designer::node_network::NodeNetwork) -> ProxyData {
    let (_, node) = network
        .nodes
        .iter()
        .find(|(_, n)| n.node_type_name == "proxy")
        .expect("the proxy node should survive the round trip");
    node.data
        .as_any_ref()
        .downcast_ref::<ProxyData>()
        .expect("proxy node data")
        .clone()
}

/// A network containing the node survives a `.cnnd` save/load with every
/// persisted field intact — and a file written *before* `fill` existed (the
/// key simply absent) loads with `fill: true`, the documented serde default.
#[test]
fn proxy_cnnd_round_trip_and_missing_fill_key() {
    use atomcad_structure_designer::serialization::node_networks_serialization::{
        load_node_networks_from_file, save_node_networks_to_file,
    };
    use tempfile::tempdir;

    let net = "main";
    let mut designer = setup(net);
    let proxy_id = designer.add_node("proxy", DVec2::ZERO);
    set_proxy_props(
        &mut designer,
        net,
        proxy_id,
        &[
            ("focus", TextValue::String("apex".to_string())),
            ("hops", TextValue::Int(4)),
            ("free", TextValue::Int(2)),
            ("rm_single", TextValue::Bool(true)),
            ("passivate", TextValue::Bool(false)),
            ("passiv_elem", TextValue::Int(9)),
            ("core", TextValue::Int(1)),
            ("fill", TextValue::Bool(false)),
        ],
    );
    designer.validate_active_network();

    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("proxy.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .expect("save");

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, path.to_str().unwrap()).expect("load");
    let data = proxy_data_of(registry.node_networks.get(net).unwrap());
    assert_eq!(data.focus, "apex");
    assert_eq!(data.hops, 4);
    assert_eq!(data.free, 2);
    assert!(data.rm_single);
    assert!(!data.passivate);
    assert_eq!(data.passiv_elem, 9);
    assert_eq!(data.core, 1);
    assert!(!data.fill, "the saved `false` survives the round trip");

    // Now strip the `fill` key from the saved file, the way a project written
    // before the field existed would look, and reload.
    let mut json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let mut stripped = 0usize;
    strip_fill_key(&mut json, &mut stripped);
    assert_eq!(stripped, 1, "exactly one proxy data object to strip");
    std::fs::write(&path, serde_json::to_string(&json).unwrap()).unwrap();

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, path.to_str().unwrap()).expect("load");
    let data = proxy_data_of(registry.node_networks.get(net).unwrap());
    assert!(
        data.fill,
        "a file without the `fill` key loads at the documented default"
    );
    assert_eq!(data.hops, 4, "the other fields are untouched");
}

/// Removes `fill` from every object that looks like serialized `ProxyData`.
fn strip_fill_key(value: &mut serde_json::Value, stripped: &mut usize) {
    match value {
        serde_json::Value::Object(map) => {
            if map.contains_key("hops") && map.contains_key("passiv_elem") {
                if map.remove("fill").is_some() {
                    *stripped += 1;
                }
                return;
            }
            for v in map.values_mut() {
                strip_fill_key(v, stripped);
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                strip_fill_key(v, stripped);
            }
        }
        _ => {}
    }
}

// ============================================================================
// Localized errors
// ============================================================================

#[test]
fn proxy_reports_a_missing_focus_tag_by_name() {
    let (designer, net, proxy_id) =
        chain_network(&[("focus", TextValue::String("no-such-tag".to_string()))]);
    assert_eq!(
        error_message(&designer, net, proxy_id),
        "proxy: no atom carries the tag \"no-such-tag\""
    );
}

#[test]
fn proxy_rejects_an_empty_focus_name_and_a_negative_hops() {
    let (designer, net, proxy_id) =
        chain_network(&[("focus", TextValue::String("   ".to_string()))]);
    assert_eq!(
        error_message(&designer, net, proxy_id),
        "proxy: focus tag name is empty"
    );

    let (designer, net, proxy_id) = chain_network(&[("hops", TextValue::Int(-1))]);
    assert_eq!(
        error_message(&designer, net, proxy_id),
        "proxy: hops must be >= 0"
    );
}

/// `free: -1` is *not* an error — §4.8 says it is treated as 0, which must
/// produce exactly the `free: 0` output.
#[test]
fn proxy_clamps_a_negative_free_to_zero() {
    let (designer, net, proxy_id) =
        chain_network(&[("hops", TextValue::Int(2)), ("free", TextValue::Int(-1))]);
    let clamped = evaluate_to_atomic(&designer, net, proxy_id);

    let (designer, net, proxy_id) =
        chain_network(&[("hops", TextValue::Int(2)), ("free", TextValue::Int(0))]);
    let zero = evaluate_to_atomic(&designer, net, proxy_id);

    assert_eq!(atom_count(&clamped), atom_count(&zero));
    let frozen = |s: &AtomicStructure| s.atoms_values().filter(|a| a.is_frozen()).count();
    assert_eq!(frozen(&clamped), frozen(&zero));
    assert!(
        frozen(&zero) > 0,
        "CONTROL: `free: 0` does freeze something"
    );
}

/// A disallowed passivant is rejected with the wording `passivate` uses,
/// localized to this node's own pin name and built from `ALLOWED_PASSIVANTS`.
#[test]
fn proxy_rejects_a_disallowed_passivant() {
    use atomcad_crystolecule::atomic_constants::ALLOWED_PASSIVANTS;

    let (designer, net, proxy_id) = chain_network(&[("passiv_elem", TextValue::Int(2))]);
    let message = error_message(&designer, net, proxy_id);
    assert_eq!(
        message,
        format!(
            "proxy.passiv_elem: 2 is not an allowed passivant; expected one of {:?} (H/F/Cl/Br/I)",
            ALLOWED_PASSIVANTS
        )
    );
}

/// An upstream error passes through unchanged — never re-wrapped, never
/// replaced by a type complaint (the nodes `AGENTS.md` rule).
#[test]
fn proxy_forwards_an_upstream_error_verbatim() {
    let net = "test";
    let mut designer = setup(net);
    let bad = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        NetworkResult::Error("upstream boom".to_string()),
    );
    let proxy_id = designer.add_node("proxy", DVec2::new(200.0, 0.0));
    designer.connect_nodes(bad, 0, proxy_id, 0);

    // `evaluate_arg` is the chaining hub, so the one `error in … input` layer
    // is already on the value when the node sees it. What the node must not do
    // is add a second one, or replace the cause with a type complaint.
    let message = error_message(&designer, net, proxy_id);
    assert!(
        message.ends_with(": upstream boom"),
        "the cause survives at the end of the chain: {message}"
    );
    assert_eq!(
        message.matches("error in").count(),
        1,
        "exactly the hub's own wrap, never a second one: {message}"
    );
    assert!(
        !message.contains("proxy"),
        "the node adds nothing of its own to an upstream error: {message}"
    );
}

// ============================================================================
// Eval cache
// ============================================================================

/// After a root evaluation with the node selected, the eval cache downcasts to
/// `ProxyEvalCache` and its stats match a direct `proxy_cut` on the same input
/// — the report the property panel reads.
#[test]
fn proxy_stores_its_stats_in_the_selected_node_eval_cache() {
    let net = "test";
    let mut designer = setup(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(capped_chain()),
    );
    let proxy_id = designer.add_node("proxy", DVec2::new(200.0, 0.0));
    set_proxy_props(
        &mut designer,
        net,
        proxy_id,
        &[("hops", TextValue::Int(2)), ("free", TextValue::Int(1))],
    );
    designer.connect_nodes(value_id, 0, proxy_id, 0);
    // Deliberately no `validate_active_network()`: the `value` node's declared
    // output type is `None`, so the validator would mark the wire bad and
    // poison the node before it ever evaluates.
    designer.select_node(proxy_id);
    designer.set_node_display(proxy_id, true);
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);

    let cache = designer
        .get_selected_node_eval_cache()
        .expect("a root evaluation of the selected proxy node stores a cache");
    let stats = &cache
        .downcast_ref::<ProxyEvalCache>()
        .expect("the cache downcasts to ProxyEvalCache")
        .stats;

    let mut direct = capped_chain();
    let expected = proxy_cut(
        &mut direct,
        "focus",
        &ProxyOptions {
            hops: 2,
            free: 1,
            ..Default::default()
        },
    )
    .expect("the direct cut succeeds");

    assert_eq!(
        *stats, expected,
        "the node's report is exactly the module's"
    );
    assert_eq!(stats.heavy, 3);
    assert_eq!(stats.riders, 3);
    assert_eq!(stats.caps, 1);
    assert_eq!(stats.min_rim, 1);
}

/// A nested evaluation stores nothing: the cache is per *root* evaluation of
/// the selected node, and a subnetwork's node state is shared across call
/// sites.
#[test]
fn a_nested_evaluation_stores_no_eval_cache() {
    let net = "test";
    let mut designer = setup(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(capped_chain()),
    );
    let proxy_id = designer.add_node("proxy", DVec2::new(200.0, 0.0));
    set_proxy_props(&mut designer, net, proxy_id, &[("hops", TextValue::Int(2))]);
    designer.connect_nodes(value_id, 0, proxy_id, 0);

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(net).unwrap();
    let evaluator = NetworkEvaluator::new();

    // Depth 1: the cache is written.
    let mut context = NetworkEvaluationContext::new();
    let root = vec![NetworkStackElement::root(network)];
    evaluator.evaluate(&root, proxy_id, 0, registry, false, &mut context);
    assert!(
        context.selected_node_eval_cache.is_some(),
        "CONTROL: a root evaluation does store the cache"
    );

    // Depth 2 — a custom-network instance frame. Nothing is stored.
    let mut context = NetworkEvaluationContext::new();
    let nested = vec![
        NetworkStackElement::root(network),
        NetworkStackElement::instance(network, proxy_id),
    ];
    evaluator.evaluate(&nested, proxy_id, 0, registry, false, &mut context);
    assert!(
        context.selected_node_eval_cache.is_none(),
        "a nested evaluation must not write the selected-node cache"
    );
}

// ============================================================================
// Subtitle
// ============================================================================

#[test]
fn proxy_subtitle_shows_focus_hops_and_free_until_one_of_them_is_wired() {
    use atomcad_structure_designer::node_data::NodeData;

    let data = ProxyData::default();
    let none: HashSet<String> = HashSet::new();
    assert_eq!(data.get_subtitle(&none), Some("focus · 6 / 3".to_string()));

    for pin in ["focus", "hops", "free"] {
        let wired: HashSet<String> = [pin.to_string()].into_iter().collect();
        assert_eq!(
            data.get_subtitle(&wired),
            None,
            "a wired `{pin}` pin overrides the stored value, so the subtitle \
             must not show a stale one"
        );
    }

    // A pin the subtitle does not show leaves it alone.
    let wired: HashSet<String> = ["fill".to_string()].into_iter().collect();
    assert_eq!(data.get_subtitle(&wired), Some("focus · 6 / 3".to_string()));
}

// ============================================================================
// The corpus fixture
// ============================================================================

/// The corpus gains a network holding the node
/// (`rust/tests/fixtures/proxy/proxy_node.cnnd`, round-tripped through
/// `query` → `--replace` by `text_format_roundtrip_corpus_test.rs`). Here it is
/// checked for the thing the text cannot show: that it still *evaluates*, with
/// a `hops` series that nests the way §4.3's monotonicity claim says it must.
#[test]
fn the_proxy_fixture_loads_and_evaluates_a_nested_hops_series() {
    let mut designer = StructureDesigner::new();
    designer
        .load_node_networks(&atomcad_test_support::fixture_path_str(
            "proxy/proxy_node.cnnd",
        ))
        .expect("the proxy fixture loads");
    let net = "proxy_demo";
    designer.set_active_node_network_name(Some(net.to_string()));

    let id_of = |name: &str| -> u64 {
        let network = designer.node_type_registry.node_networks.get(net).unwrap();
        *network
            .nodes
            .iter()
            .find(|(_, n)| n.custom_name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("the fixture should hold a node named `{name}`"))
            .0
    };

    let six = evaluate_to_atomic(&designer, net, id_of("proxy_6"));
    let seven = evaluate_to_atomic(&designer, net, id_of("proxy_7"));

    assert!(atom_count(&six) > 0, "the cut keeps something");
    assert!(
        atom_count(&seven) > atom_count(&six),
        "one more hop is strictly more atoms: {} vs {}",
        atom_count(&six),
        atom_count(&seven)
    );
    assert!(
        six.atoms_values().any(|a| six.atom_has_tag(a.id, "high")),
        "`core: 1` paints the high layer"
    );
    assert!(
        six.atoms_values().any(|a| a.is_frozen()),
        "`free: 3` freezes the rim"
    );
}
