//! Phases 2 and 3 of the chemisorption search design — the `chemisorb` node
//! shell, and its `transfers` pin.
//!
//! The search itself is tested in `atomcad-crystolecule`'s
//! `chemisorption_test.rs`. What is exercised here is what the node adds: the
//! run model (evaluation shows the plan and never searches; Run stores a result
//! keyed by an input fingerprint; a mismatch falls back to the plan and says
//! `stale`), the three outputs and their records, the eval cache the panel
//! reads, the text format and the `.cnnd` round trip.
//!
//! The fixture is the phase-1 •OH over three silyl radicals: three single-bond
//! hypotheses, small enough that a debug-build Run takes well under a second.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use atomcad_crystolecule::chemisorption::CHANGED_TAG;
use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::evaluator::network_evaluator::NetworkStackElement;
use atomcad_structure_designer::evaluator::network_result::{MoleculeData, NetworkResult};
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::chemisorb::{
    ChemisorbData, ChemisorbEvalCache, StoredSearch,
};
use atomcad_structure_designer::nodes::parameter::ParameterData;
use atomcad_structure_designer::nodes::value::ValueData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::TextValue;
use glam::f64::{DQuat, DVec2, DVec3};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

const H: i16 = 1;
const O: i16 = 8;
const SI: i16 = 14;

// ============================================================================
// Fixture
// ============================================================================

/// A silyl radical at `p`: SiH3, its one dangling bond pointing up.
fn add_silyl(s: &mut AtomicStructure, p: DVec3) -> u32 {
    let si = s.add_atom(SI, p);
    let (sin, cos) = ((8.0f64 / 9.0).sqrt(), -1.0 / 3.0);
    for deg in [0.0f64, 120.0, 240.0] {
        let phi = deg.to_radians();
        let d = DQuat::IDENTITY * DVec3::new(sin * phi.cos(), sin * phi.sin(), cos);
        let h = s.add_atom(H, p + d * 1.48);
        s.add_bond(si, h, BOND_SINGLE);
    }
    si
}

/// •OH above the origin.
fn hydroxyl(offset: DVec3) -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let o = s.add_atom(O, DVec3::new(0.3, 0.2, 2.2) + offset);
    let h = s.add_atom(H, DVec3::new(0.3, 0.2, 3.17) + offset);
    s.add_bond(o, h, BOND_SINGLE);
    s
}

/// Three silyl radicals, everything but the three Si frozen.
fn three_silyls() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let sites: Vec<u32> = [
        DVec3::ZERO,
        DVec3::new(2.6, 0.0, 0.0),
        DVec3::new(0.0, 2.9, 0.0),
    ]
    .into_iter()
    .map(|p| add_silyl(&mut s, p))
    .collect();
    for id in s.atom_ids().copied().collect::<Vec<_>>() {
        if !sites.contains(&id) {
            s.set_atom_frozen(id, true);
        }
    }
    s
}

fn molecule(atoms: AtomicStructure) -> NetworkResult {
    NetworkResult::Molecule(MoleculeData {
        atoms,
        geo_tree_root: None,
    })
}

// ============================================================================
// Harness
// ============================================================================

struct Net {
    designer: StructureDesigner,
    name: &'static str,
    adsorbate: u64,
    node: u64,
}

fn add_value(designer: &mut StructureDesigner, network: &str, value: NetworkResult) -> u64 {
    designer
        .node_type_registry
        .node_networks
        .get_mut(network)
        .unwrap()
        .add_node("value", DVec2::ZERO, 0, Box::new(ValueData { value }))
}

/// `value(•OH)` and `value(3 × SiH3)` wired into a `chemisorb`.
///
/// Not validated: a `value` node declares its output `None`, which the
/// validator rejects on a typed pin, while the evaluator flows the payload
/// through unchanged.
fn network() -> Net {
    let name = "main";
    let mut designer = StructureDesigner::new();
    designer.add_node_network(name);
    designer.set_active_node_network_name(Some(name.to_string()));
    let adsorbate = add_value(&mut designer, name, molecule(hydroxyl(DVec3::ZERO)));
    let substrate = add_value(&mut designer, name, molecule(three_silyls()));
    let node = designer.add_node("chemisorb", DVec2::new(200.0, 0.0));
    designer.connect_nodes(adsorbate, 0, node, 0);
    designer.connect_nodes(substrate, 0, node, 1);
    Net {
        designer,
        name,
        adsorbate,
        node,
    }
}

/// All three outputs, evaluated the way a refresh does — through the
/// designer's own context, so the van der Waals preference matches Run's.
fn outputs(designer: &mut StructureDesigner, network: &str, node: u64) -> Vec<NetworkResult> {
    let network = network.to_string();
    designer.with_eval_context(false, |evaluator, registry, _prefs, context| {
        let net = registry.node_networks.get(&network).unwrap();
        let stack = vec![NetworkStackElement::root(net)];
        (0..3)
            .map(|pin| evaluator.evaluate(&stack, node, pin, registry, false, context))
            .collect()
    })
}

fn fields(record: &NetworkResult) -> BTreeMap<String, NetworkResult> {
    match record {
        NetworkResult::Record(r) => r.iter().cloned().collect(),
        NetworkResult::Error(e) => panic!("expected a record, got Error: {e}"),
        other => panic!("expected a record, got {:?}", other.infer_data_type()),
    }
}

fn int(f: &BTreeMap<String, NetworkResult>, key: &str) -> i32 {
    match &f[key] {
        NetworkResult::Int(n) => *n,
        other => panic!("{key}: {:?}", other.infer_data_type()),
    }
}

fn float(f: &BTreeMap<String, NetworkResult>, key: &str) -> f64 {
    match &f[key] {
        NetworkResult::Float(x) => *x,
        other => panic!("{key}: {:?}", other.infer_data_type()),
    }
}

fn boolean(f: &BTreeMap<String, NetworkResult>, key: &str) -> bool {
    match &f[key] {
        NetworkResult::Bool(b) => *b,
        other => panic!("{key}: {:?}", other.infer_data_type()),
    }
}

fn string(f: &BTreeMap<String, NetworkResult>, key: &str) -> String {
    match &f[key] {
        NetworkResult::String(s) => s.clone(),
        other => panic!("{key}: {:?}", other.infer_data_type()),
    }
}

fn atoms(v: &NetworkResult) -> &AtomicStructure {
    match v {
        NetworkResult::Molecule(m) => &m.atoms,
        NetworkResult::Error(e) => panic!("expected a Molecule, got Error: {e}"),
        other => panic!("expected a Molecule, got {:?}", other.infer_data_type()),
    }
}

fn array(v: &NetworkResult) -> &[NetworkResult] {
    match v {
        NetworkResult::Array(items) => items,
        other => panic!("expected an array, got {:?}", other.infer_data_type()),
    }
}

fn positions(s: &AtomicStructure) -> Vec<(u32, [u64; 3])> {
    s.atoms_values()
        .map(|a| (a.id, a.position.to_array().map(f64::to_bits)))
        .collect()
}

fn data(designer: &StructureDesigner, network: &str, node: u64) -> ChemisorbData {
    designer.node_type_registry.node_networks[network].nodes[&node]
        .data
        .as_any_ref()
        .downcast_ref::<ChemisorbData>()
        .unwrap()
        .clone()
}

fn stored(designer: &StructureDesigner, network: &str, node: u64) -> Option<Arc<StoredSearch>> {
    data(designer, network, node).stored
}

fn set_props(
    designer: &mut StructureDesigner,
    network: &str,
    node: u64,
    props: &[(&str, TextValue)],
) {
    let map: HashMap<String, TextValue> = props
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    designer
        .node_type_registry
        .node_networks
        .get_mut(network)
        .unwrap()
        .nodes
        .get_mut(&node)
        .unwrap()
        .data
        .set_text_properties(&map)
        .unwrap();
}

fn set_value(designer: &mut StructureDesigner, network: &str, node: u64, value: NetworkResult) {
    designer
        .node_type_registry
        .node_networks
        .get_mut(network)
        .unwrap()
        .nodes
        .get_mut(&node)
        .unwrap()
        .data = Box::new(ValueData { value });
}

// ============================================================================
// Before Run: the plan
// ============================================================================

#[test]
fn before_run_the_node_outputs_the_plan_and_relaxes_nothing() {
    let Net {
        mut designer,
        name,
        node,
        ..
    } = network();
    let out = outputs(&mut designer, name, node);

    // `best` is the pose as wired: adsorbate then substrate, unrelaxed.
    let best = atoms(&out[0]);
    let mut expected = AtomicStructure::new();
    expected
        .add_atomic_structure(&hydroxyl(DVec3::ZERO))
        .unwrap();
    expected.add_atomic_structure(&three_silyls()).unwrap();
    assert_eq!(positions(best), positions(&expected));
    assert!(best.atoms_with_tag(CHANGED_TAG).is_empty());

    assert!(array(&out[1]).is_empty());

    let s = fields(&out[2]);
    assert_eq!(int(&s, "to_relax"), 3);
    assert_eq!(int(&s, "relaxed"), 0);
    assert_eq!(int(&s, "listed"), 0);
    assert_eq!(int(&s, "unconverged"), 0);
    assert_eq!(int(&s, "feet"), 1);
    assert_eq!(int(&s, "sites_in_reach"), 3);
    assert!(!boolean(&s, "searched"));
    assert!(!boolean(&s, "stale"));
    assert!(!boolean(&s, "truncated"));
    assert_eq!(string(&s, "estimated_pairs"), "");
    assert_eq!(
        int(&s, "considered"),
        int(&s, "pruned_valence")
            + int(&s, "pruned_pair_tolerance")
            + int(&s, "duplicates")
            + int(&s, "to_relax")
    );

    // However often it is evaluated, nothing is stored and nothing relaxed.
    for _ in 0..3 {
        let again = outputs(&mut designer, name, node);
        assert_eq!(positions(atoms(&again[0])), positions(&expected));
    }
    assert!(stored(&designer, name, node).is_none());
}

// ============================================================================
// Run
// ============================================================================

#[test]
fn after_run_the_result_is_output_and_evaluations_never_search_again() {
    let Net {
        mut designer,
        name,
        node,
        ..
    } = network();
    let summary = designer.run_chemisorb(&[], node).expect("run");
    assert_eq!(summary.relaxed, 3);
    assert_eq!(summary.listed, 3);
    assert_eq!(summary.best_bonds, "formed 1× O–Si");
    assert!(!summary.truncated);

    let first = stored(&designer, name, node).expect("stored");
    let out = outputs(&mut designer, name, node);

    let s = fields(&out[2]);
    assert!(boolean(&s, "searched"));
    assert!(!boolean(&s, "stale"));
    assert_eq!(int(&s, "relaxed"), 3);
    assert_eq!(int(&s, "to_relax"), 3);
    assert_eq!(int(&s, "listed"), 3);
    assert_eq!(float(&s, "seconds"), first.report.stats.seconds);

    // Candidates in rank order, every field filled, changed atoms tagged.
    let rows = array(&out[1]);
    assert_eq!(rows.len(), 3);
    let mut last_score = f64::NEG_INFINITY;
    for (i, row) in rows.iter().enumerate() {
        let f = fields(row);
        assert_eq!(int(&f, "rank"), i as i32 + 1);
        let score = float(&f, "score");
        assert!(score >= last_score);
        last_score = score;
        assert!((score - (float(&f, "strain") + float(&f, "bond_energy"))).abs() < 1e-9);
        assert!((float(&f, "bond_energy") + 452.0 / 4.184).abs() < 1e-9);
        assert!(!boolean(&f, "estimated"));
        assert_eq!(string(&f, "bonds"), "formed 1× O–Si");
        assert!(
            string(&f, "sites").starts_with('O'),
            "{}",
            string(&f, "sites")
        );
        assert!(string(&f, "sites").contains("–Si"));
        assert_eq!(int(&f, "formed_bonds"), 1);
        assert_eq!(int(&f, "transfers"), 0);
        assert!(float(&f, "worst_bond_ratio") > 0.0);
        let terms = fields(&f["terms"]);
        let total: f64 = ["stretch", "bend", "torsion", "inversion", "vdw"]
            .iter()
            .map(|k| float(&terms, k))
            .sum();
        assert!((total - float(&f, "strain")).abs() < 1e-6);
        let structure = atoms(&f["structure"]);
        assert_eq!(structure.atoms_with_tag(CHANGED_TAG).len(), 2);
        let c = &first.report.candidates[i];
        assert_eq!(positions(structure), positions(&c.structure));
    }

    // `best` is rank 1.
    assert_eq!(
        positions(atoms(&out[0])),
        positions(&first.report.candidates[0].structure)
    );

    // Evaluating again — as refreshes, selection changes and downstream edits
    // do — reads the same stored report and never replaces it.
    for _ in 0..3 {
        let again = outputs(&mut designer, name, node);
        assert_eq!(
            float(&fields(&again[2]), "seconds"),
            first.report.stats.seconds
        );
    }
    let still = stored(&designer, name, node).unwrap();
    assert!(Arc::ptr_eq(&first, &still));
}

#[test]
fn run_is_refused_for_other_nodes_and_broken_inputs() {
    let Net {
        mut designer,
        name,
        adsorbate,
        node,
    } = network();
    assert!(designer.run_chemisorb(&[], adsorbate).is_err());
    assert!(designer.run_chemisorb(&[7], node).is_err());

    // A mistyped tag is reported, and nothing is stored.
    set_props(
        &mut designer,
        name,
        node,
        &[("adsorbate_tag", TextValue::String("feet".to_string()))],
    );
    let err = designer.run_chemisorb(&[], node).unwrap_err();
    assert!(
        err.contains("no adsorbate atom carries the tag 'feet'"),
        "{err}"
    );
    assert!(stored(&designer, name, node).is_none());
    // …and evaluation reports the same on every pin.
    for v in outputs(&mut designer, name, node) {
        match v {
            NetworkResult::Error(e) => assert!(e.contains("'feet'"), "{e}"),
            other => panic!("expected an error, got {:?}", other.infer_data_type()),
        }
    }
}

// ============================================================================
// Staleness
// ============================================================================

#[test]
fn moving_the_adsorbate_makes_the_result_stale_and_moving_it_back_restores_it() {
    let Net {
        mut designer,
        name,
        adsorbate,
        node,
    } = network();
    designer.run_chemisorb(&[], node).unwrap();

    set_value(
        &mut designer,
        name,
        adsorbate,
        molecule(hydroxyl(DVec3::new(0.1, 0.0, 0.0))),
    );
    let out = outputs(&mut designer, name, node);
    let s = fields(&out[2]);
    assert!(!boolean(&s, "searched"));
    assert!(boolean(&s, "stale"));
    assert_eq!(int(&s, "relaxed"), 0);
    assert!(int(&s, "to_relax") > 0, "the plan of the moved pose");
    assert!(array(&out[1]).is_empty(), "a stale result is never output");
    assert!(atoms(&out[0]).atoms_with_tag(CHANGED_TAG).is_empty());

    set_value(
        &mut designer,
        name,
        adsorbate,
        molecule(hydroxyl(DVec3::ZERO)),
    );
    let s = fields(&outputs(&mut designer, name, node)[2]);
    assert!(boolean(&s, "searched"));
    assert!(!boolean(&s, "stale"));
}

#[test]
fn a_settings_edit_makes_the_result_stale_and_its_undo_restores_it() {
    let Net {
        mut designer,
        name,
        node,
        ..
    } = network();
    designer.run_chemisorb(&[], node).unwrap();

    let edited = ChemisorbData {
        reach: 3.0,
        ..data(&designer, name, node)
    };
    designer.set_chemisorb_data(&[], node, edited);
    let s = fields(&outputs(&mut designer, name, node)[2]);
    assert!(boolean(&s, "stale"), "reach is part of the fingerprint");
    assert!(!boolean(&s, "searched"));

    // Undo rebuilds the data from its JSON snapshot; the stored search is
    // inherited across that replacement and matches again.
    assert!(designer.undo());
    assert_eq!(data(&designer, name, node).reach, 3.5);
    let s = fields(&outputs(&mut designer, name, node)[2]);
    assert!(boolean(&s, "searched"));
    assert!(!boolean(&s, "stale"));
}

#[test]
fn top_n_and_energy_window_relist_without_making_the_result_stale() {
    let Net {
        mut designer,
        name,
        node,
        ..
    } = network();
    designer.run_chemisorb(&[], node).unwrap();

    set_props(&mut designer, name, node, &[("top_n", TextValue::Int(2))]);
    let out = outputs(&mut designer, name, node);
    assert_eq!(array(&out[1]).len(), 2);
    let s = fields(&out[2]);
    assert!(boolean(&s, "searched"));
    assert_eq!(int(&s, "listed"), 2);

    set_props(
        &mut designer,
        name,
        node,
        &[
            ("top_n", TextValue::Int(10)),
            ("energy_window", TextValue::Float(0.0)),
        ],
    );
    let out = outputs(&mut designer, name, node);
    assert_eq!(
        array(&out[1]).len(),
        1,
        "only the best is within a zero window"
    );
    assert!(boolean(&fields(&out[2]), "searched"));
}

// ============================================================================
// Subnetwork call sites
// ============================================================================

fn configure_parameter(
    designer: &mut StructureDesigner,
    node: u64,
    name: &str,
    index: usize,
    data_type: DataType,
) {
    designer.set_node_network_data(
        node,
        Box::new(ParameterData {
            param_id: None,
            param_index: index,
            param_name: name.to_string(),
            data_type,
            sort_order: index as i32,
            data_type_str: None,
            error: None,
        }),
    );
}

/// A node inside a custom network has one data shared by every call site. The
/// call site whose inputs match the run's outputs the result; another gets the
/// plan, marked stale.
#[test]
fn only_the_call_site_matching_the_run_outputs_the_result() {
    let mut designer = StructureDesigner::new();

    // "Inner": two Molecule parameters (defaults = the fixture) → chemisorb.
    designer.add_node_network("Inner");
    designer.set_active_node_network_name(Some("Inner".to_string()));
    let pa = designer.add_node("parameter", DVec2::ZERO);
    configure_parameter(&mut designer, pa, "adsorbate", 0, DataType::Molecule);
    let ps = designer.add_node("parameter", DVec2::new(0.0, 100.0));
    configure_parameter(&mut designer, ps, "substrate", 1, DataType::Molecule);
    let cs = designer.add_node("chemisorb", DVec2::new(200.0, 0.0));
    designer.connect_nodes(pa, 0, cs, 0);
    designer.connect_nodes(ps, 0, cs, 1);
    designer.set_return_node_id(Some(cs));
    designer.validate_active_network();
    // Wired after validating: a `value` node's declared output is `None`, which
    // the validator would reject on the typed `default` pin.
    let da = add_value(&mut designer, "Inner", molecule(hydroxyl(DVec3::ZERO)));
    let ds = add_value(&mut designer, "Inner", molecule(three_silyls()));
    designer.connect_nodes(da, 0, pa, 0);
    designer.connect_nodes(ds, 0, ps, 0);

    // Run inside Inner: the inputs are the parameters' defaults.
    designer.run_chemisorb(&[], cs).expect("run in Inner");

    // "main": the same pose at call site A, a moved one at call site B.
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let a_ads = add_value(&mut designer, "main", molecule(hydroxyl(DVec3::ZERO)));
    let b_ads = add_value(
        &mut designer,
        "main",
        molecule(hydroxyl(DVec3::new(0.0, 0.2, 0.0))),
    );
    let sub = add_value(&mut designer, "main", molecule(three_silyls()));
    let call_a = designer.add_node("Inner", DVec2::new(300.0, 0.0));
    let call_b = designer.add_node("Inner", DVec2::new(300.0, 200.0));
    designer.connect_nodes(a_ads, 0, call_a, 0);
    designer.connect_nodes(sub, 0, call_a, 1);
    designer.connect_nodes(b_ads, 0, call_b, 0);
    designer.connect_nodes(sub, 0, call_b, 1);
    assert!(designer.node_type_registry.node_networks["Inner"].valid);

    let a = outputs(&mut designer, "main", call_a);
    let s = fields(&a[2]);
    assert!(boolean(&s, "searched"), "call site A matches the run");
    assert_eq!(array(&a[1]).len(), 3);

    let b = outputs(&mut designer, "main", call_b);
    let s = fields(&b[2]);
    assert!(!boolean(&s, "searched"));
    assert!(
        boolean(&s, "stale"),
        "call site B gets the plan, marked stale"
    );
    assert!(array(&b[1]).is_empty());
}

// ============================================================================
// The panel's data
// ============================================================================

#[test]
fn the_selected_node_eval_cache_carries_the_stats_and_the_rows() {
    let Net {
        mut designer, node, ..
    } = network();
    let refresh = |designer: &mut StructureDesigner| {
        designer.select_node(node);
        designer.set_node_display(node, true);
        designer.mark_full_refresh();
        let changes = designer.get_pending_changes();
        designer.refresh(&changes);
        designer
            .get_selected_node_eval_cache()
            .and_then(|c| c.downcast_ref::<ChemisorbEvalCache>())
            .cloned()
            .expect("chemisorb eval cache")
    };

    let before = refresh(&mut designer);
    assert!(!before.stats.searched);
    assert_eq!(before.stats.to_relax, 3);
    assert!(before.rows.is_empty());

    designer.run_chemisorb(&[], node).unwrap();
    designer.mark_node_data_changed(node);
    let after = refresh(&mut designer);
    assert!(after.stats.searched);
    assert_eq!(after.rows.len(), 3);
    assert_eq!(after.rows[0].rank, 1);
    assert_eq!(after.rows[0].bonds, "formed 1× O–Si");
    assert!(after.rows.windows(2).all(|w| w[0].score <= w[1].score));
}

#[test]
fn an_estimated_pair_is_named_before_run() {
    // •NH2 over the silyls: N–Si is not in the enthalpy table.
    let mut ads = AtomicStructure::new();
    let n = ads.add_atom(7, DVec3::new(0.3, 0.2, 2.2));
    for d in [DVec3::new(0.9, 0.0, 0.4), DVec3::new(-0.45, 0.8, 0.4)] {
        let h = ads.add_atom(H, DVec3::new(0.3, 0.2, 2.2) + d);
        ads.add_bond(n, h, BOND_SINGLE);
    }
    let Net {
        mut designer,
        name,
        adsorbate,
        node,
    } = network();
    set_value(&mut designer, name, adsorbate, molecule(ads));
    let s = fields(&outputs(&mut designer, name, node)[2]);
    assert_eq!(string(&s, "estimated_pairs"), "N–Si");
}

// ============================================================================
// Text format and .cnnd
// ============================================================================

fn author_and_serialize(source: &str) -> String {
    use atomcad_structure_designer::node_network::NodeNetwork;
    use atomcad_structure_designer::node_type::{NodeType, NodeTypeCategory, OutputPinDefinition};
    use atomcad_structure_designer::text_format::{edit_network, serialize_network};

    let registry = NodeTypeRegistry::new();
    let mut network = NodeNetwork::new(NodeType {
        name: "test".to_string(),
        description: String::new(),
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
    });
    let result = edit_network(&mut network, &registry, source, true);
    assert!(result.success, "edit should succeed: {:?}", result.errors);
    serialize_network(&network, &registry, Some("test"))
}

#[test]
fn every_property_round_trips_through_the_text_format() {
    const FULL: &str = "c = chemisorb { adsorbate_tag: \"feet\", substrate_tag: \"top\", \
                        reach: 4.5, pair_tolerance: 0.0, max_formed_bonds: 3, max_transfers: 2, top_n: 5, \
                        energy_window: 12.5, budget: 500, max_iterations: 800 }";
    let serialized = author_and_serialize(&format!("{FULL}\n"));
    assert!(serialized.contains(FULL), "got:\n{serialized}");
    assert_eq!(serialized, author_and_serialize(&serialized));

    // The short form expands to the documented defaults.
    let short = author_and_serialize("c = chemisorb { }\n");
    assert!(
        short.contains(
            "c = chemisorb { adsorbate_tag: \"\", substrate_tag: \"\", reach: 3.5, \
             pair_tolerance: 3.0, max_formed_bonds: 0, max_transfers: 1, top_n: 10, \
             energy_window: 30.0, \
             budget: 10000, max_iterations: 2000 }"
        ),
        "got:\n{short}"
    );
}

#[test]
fn a_saved_and_reloaded_node_keeps_its_settings_and_not_its_result() {
    use atomcad_structure_designer::serialization::node_networks_serialization::{
        load_node_networks_from_file, save_node_networks_to_file,
    };
    use tempfile::tempdir;

    let Net {
        mut designer,
        name,
        node,
        ..
    } = network();
    set_props(
        &mut designer,
        name,
        node,
        &[("reach", TextValue::Float(4.0))],
    );
    set_props(
        &mut designer,
        name,
        node,
        &[("reach", TextValue::Float(3.5))],
    );
    set_props(
        &mut designer,
        name,
        node,
        &[("max_formed_bonds", TextValue::Int(2))],
    );
    designer.run_chemisorb(&[], node).unwrap();
    assert!(stored(&designer, name, node).is_some());

    let tmp = tempdir().unwrap();
    let path = tmp.path().join("chemisorb.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .expect("save");
    let json = std::fs::read_to_string(&path).unwrap();
    assert!(!json.contains("stored"), "the result is never written");

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, path.to_str().unwrap()).expect("load");
    let (_, loaded) = registry.node_networks[name]
        .nodes
        .iter()
        .find(|(_, n)| n.node_type_name == "chemisorb")
        .unwrap();
    let loaded = loaded
        .data
        .as_any_ref()
        .downcast_ref::<ChemisorbData>()
        .unwrap();
    assert_eq!(loaded.max_formed_bonds, 2);
    assert_eq!(loaded.reach, 3.5);
    assert!(
        loaded.stored.is_none(),
        "a reloaded node is in the before-Run state"
    );
}

#[test]
fn invalid_settings_are_reported_in_the_nodes_words() {
    let Net {
        mut designer,
        name,
        node,
        ..
    } = network();
    for (key, value, needle) in [
        (
            "reach",
            TextValue::Float(0.0),
            "reach must be a positive distance",
        ),
        (
            "max_formed_bonds",
            TextValue::Int(-1),
            "max_formed_bonds must be >= 0",
        ),
        ("top_n", TextValue::Int(0), "top_n must be at least 1"),
        ("budget", TextValue::Int(0), "budget must be at least 1"),
    ] {
        let restore = data(&designer, name, node);
        set_props(&mut designer, name, node, &[(key, value)]);
        match &outputs(&mut designer, name, node)[2] {
            NetworkResult::Error(e) => assert!(e.contains(needle), "{key}: {e}"),
            other => panic!(
                "{key}: expected an error, got {:?}",
                other.infer_data_type()
            ),
        }
        designer
            .node_type_registry
            .node_networks
            .get_mut(name)
            .unwrap()
            .nodes
            .get_mut(&node)
            .unwrap()
            .data = Box::new(restore);
    }
}

// ============================================================================
// Transfers (phase 3)
// ============================================================================

fn transfer_record(element: i32, direction: &str) -> NetworkResult {
    NetworkResult::record(vec![
        ("element".to_string(), NetworkResult::Int(element)),
        (
            "direction".to_string(),
            NetworkResult::String(direction.to_string()),
        ),
    ])
}

/// The phase-2 fixture with a `value` node on the `transfers` pin. Returns the
/// net and that value node.
fn network_with_transfers(records: Vec<NetworkResult>) -> (Net, u64) {
    let mut net = network();
    let transfers = add_value(&mut net.designer, net.name, NetworkResult::Array(records));
    net.designer.connect_nodes(transfers, 0, net.node, 2);
    (net, transfers)
}

#[test]
fn the_transfers_pin_is_appended_optional_and_typed() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("chemisorb").unwrap();
    let names: Vec<&str> = node_type
        .parameters
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, ["adsorbate", "substrate", "transfers"]);
    assert_eq!(
        node_type.parameters[2].data_type,
        DataType::Array(Box::new(DataType::Record(
            atomcad_structure_designer::data_type::RecordType::Named(
                "ChemisorbTransfer".to_string()
            )
        )))
    );
    let def = registry
        .lookup_record_type_def("ChemisorbTransfer")
        .expect("built-in record");
    let fields: Vec<&str> = def.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(fields, ["element", "direction"]);

    // Disconnected, the node is the phase-2 node: bond forming only.
    let Net {
        mut designer,
        name,
        node,
        ..
    } = network();
    let s = fields_of_stats(&mut designer, name, node);
    assert_eq!(int(&s, "transfer_candidates"), 0);
    assert_eq!(int(&s, "to_relax"), 3);
}

fn fields_of_stats(
    designer: &mut StructureDesigner,
    network: &str,
    node: u64,
) -> BTreeMap<String, NetworkResult> {
    fields(&outputs(designer, network, node)[2])
}

#[test]
fn a_wired_transfer_record_is_planned_run_and_listed() {
    // The •OH's H reaches the first silyl only; moving it there frees a second
    // valence on the O, which then bonds to either of the other two.
    let (
        Net {
            mut designer,
            name,
            node,
            ..
        },
        _,
    ) = network_with_transfers(vec![transfer_record(1, "to_substrate")]);
    // A transfer pattern breaks O–H for a weaker Si–H: ~35 kcal/mol above the
    // plain O–Si bond, past the default 30 kcal/mol window. List everything.
    set_props(
        &mut designer,
        name,
        node,
        &[("energy_window", TextValue::Float(1000.0))],
    );
    let s = fields_of_stats(&mut designer, name, node);
    assert_eq!(int(&s, "transfer_candidates"), 1);
    assert_eq!(
        int(&s, "to_relax"),
        6,
        "3 bond-forming + 3 with the transfer"
    );
    assert!(!boolean(&s, "searched"));

    let summary = designer.run_chemisorb(&[], node).unwrap();
    assert_eq!(summary.relaxed, 6);
    let out = outputs(&mut designer, name, node);
    let s = fields(&out[2]);
    assert!(boolean(&s, "searched"));
    assert_eq!(int(&s, "transfer_candidates"), 1);
    let rows: Vec<BTreeMap<String, NetworkResult>> = array(&out[1]).iter().map(fields).collect();
    let with_transfer: Vec<&BTreeMap<String, NetworkResult>> =
        rows.iter().filter(|r| int(r, "transfers") == 1).collect();
    assert!(!with_transfer.is_empty());
    for row in &with_transfer {
        assert!(
            string(row, "sites").contains('→'),
            "{}",
            string(row, "sites")
        );
        assert!(string(row, "bonds").contains("broken 1× H–O"));
    }
    // The panel's rows say the same.
    let cache = designer_eval_cache(&mut designer, name, node);
    assert_eq!(cache.stats.transfer_candidates, 1);
    assert!(cache.rows.iter().any(|r| r.transfers == 1));
}

/// Refreshes with the node selected and displayed, and returns the panel's
/// data, as `the_selected_node_eval_cache_carries_the_stats_and_the_rows` does.
fn designer_eval_cache(
    designer: &mut StructureDesigner,
    _network: &str,
    node: u64,
) -> ChemisorbEvalCache {
    designer.select_node(node);
    designer.set_node_display(node, true);
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
    designer
        .get_selected_node_eval_cache()
        .and_then(|c| c.downcast_ref::<ChemisorbEvalCache>())
        .cloned()
        .expect("chemisorb eval cache")
}

#[test]
fn a_transfer_record_edit_makes_the_result_stale_and_max_transfers_counts_only_when_wired() {
    let (
        Net {
            mut designer,
            name,
            node,
            ..
        },
        transfers,
    ) = network_with_transfers(vec![transfer_record(1, "to_substrate")]);
    designer.run_chemisorb(&[], node).unwrap();
    assert!(boolean(
        &fields_of_stats(&mut designer, name, node),
        "searched"
    ));

    // max_transfers is read while a record is wired: stale.
    set_props(
        &mut designer,
        name,
        node,
        &[("max_transfers", TextValue::Int(2))],
    );
    assert!(boolean(
        &fields_of_stats(&mut designer, name, node),
        "stale"
    ));
    set_props(
        &mut designer,
        name,
        node,
        &[("max_transfers", TextValue::Int(1))],
    );
    assert!(boolean(
        &fields_of_stats(&mut designer, name, node),
        "searched"
    ));

    // Another direction is another search.
    set_value(
        &mut designer,
        name,
        transfers,
        NetworkResult::Array(vec![transfer_record(1, "to_adsorbate")]),
    );
    let s = fields_of_stats(&mut designer, name, node);
    assert!(boolean(&s, "stale"));
    assert_eq!(int(&s, "transfer_candidates"), 0);

    // With no record, max_transfers is unread and makes nothing stale.
    set_value(&mut designer, name, transfers, NetworkResult::Array(vec![]));
    designer.run_chemisorb(&[], node).unwrap();
    set_props(
        &mut designer,
        name,
        node,
        &[("max_transfers", TextValue::Int(3))],
    );
    assert!(boolean(
        &fields_of_stats(&mut designer, name, node),
        "searched"
    ));
}

#[test]
fn a_bad_transfer_record_is_reported_in_the_nodes_words() {
    for (record, needle) in [
        (transfer_record(8, "to_substrate"), "is not H or a halogen"),
        (
            transfer_record(1, "sideways"),
            "neither 'to_substrate' nor 'to_adsorbate'",
        ),
        (
            NetworkResult::record(vec![("element".to_string(), NetworkResult::Int(1))]),
            "direction: expected String",
        ),
    ] {
        let (
            Net {
                mut designer,
                name,
                node,
                ..
            },
            _,
        ) = network_with_transfers(vec![record]);
        match &outputs(&mut designer, name, node)[2] {
            NetworkResult::Error(e) => assert!(e.contains(needle), "{e}"),
            other => panic!("expected an error, got {:?}", other.infer_data_type()),
        }
        assert!(
            designer
                .run_chemisorb(&[], node)
                .unwrap_err()
                .contains(needle)
        );
    }

    // An upstream error in an element is forwarded, not re-described.
    let (
        Net {
            mut designer,
            name,
            node,
            ..
        },
        _,
    ) = network_with_transfers(vec![NetworkResult::Error("boom".to_string())]);
    match &outputs(&mut designer, name, node)[0] {
        NetworkResult::Error(e) => assert!(e.contains("boom") && e.contains("transfers")),
        other => panic!("expected an error, got {:?}", other.infer_data_type()),
    }
}

#[test]
fn the_transfers_wire_round_trips_through_the_text_format_and_cnnd() {
    use atomcad_structure_designer::data_type::RecordType;
    use atomcad_structure_designer::nodes::array::ArrayData;
    use atomcad_structure_designer::serialization::node_networks_serialization::{
        load_node_networks_from_file, save_node_networks_to_file,
    };
    use tempfile::tempdir;

    let source = "t = array { element_type: Record(ChemisorbTransfer), elements: \
                  [{ element: 1, direction: \"to_substrate\" }] }\n\
                  c = chemisorb { max_transfers: 2, transfers: t }\n";
    let serialized = author_and_serialize(source);
    assert!(serialized.contains("transfers: t"), "got:\n{serialized}");
    assert!(
        serialized.contains("max_transfers: 2"),
        "got:\n{serialized}"
    );
    assert!(
        serialized.contains("Record(ChemisorbTransfer)"),
        "got:\n{serialized}"
    );
    assert_eq!(serialized, author_and_serialize(&serialized));

    // .cnnd: the wire into pin 2 and the record literal survive a save/load.
    let Net {
        mut designer,
        name,
        node,
        ..
    } = network();
    let array = designer
        .node_type_registry
        .node_networks
        .get_mut(name)
        .unwrap()
        .add_node(
            "array",
            DVec2::ZERO,
            0,
            Box::new(ArrayData {
                element_type: DataType::Record(RecordType::Named("ChemisorbTransfer".to_string())),
                elements: vec![TextValue::Object(vec![
                    ("element".to_string(), TextValue::Int(1)),
                    (
                        "direction".to_string(),
                        TextValue::String("to_substrate".to_string()),
                    ),
                ])],
            }),
        );
    designer.connect_nodes(array, 0, node, 2);
    assert_eq!(
        int(
            &fields_of_stats(&mut designer, name, node),
            "transfer_candidates"
        ),
        1
    );

    let tmp = tempdir().unwrap();
    let path = tmp.path().join("transfers.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .expect("save");
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, path.to_str().unwrap()).expect("load");
    let loaded = &registry.node_networks[name];
    let (_, chemisorb) = loaded
        .nodes
        .iter()
        .find(|(_, n)| n.node_type_name == "chemisorb")
        .unwrap();
    let source = chemisorb.arguments[2]
        .get_node_id()
        .expect("the transfers wire survives");
    assert_eq!(loaded.nodes[&source].node_type_name, "array");
}
