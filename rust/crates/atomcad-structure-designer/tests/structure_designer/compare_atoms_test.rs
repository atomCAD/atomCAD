//! The `compare_atoms` node (`doc/design_structure_comparison.md` Phase 3):
//! only what the node adds over `structure_compare`, whose verdicts are tested
//! in the crystolecule crate — the pins and the report, the cluster `tag`
//! cuts, the errors, the text format, and agreement with `chemisorb`'s
//! `unique`.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use atomcad_crystolecule::chemisorption::{DistinctSettings, LOCAL_TAG, distinct};
use atomcad_crystolecule::structure_compare::{Cluster, Motion, compare};
use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::evaluator::network_evaluator::NetworkStackElement;
use atomcad_structure_designer::evaluator::network_result::{MoleculeData, NetworkResult};
use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::chemisorb::ChemisorbData;
use atomcad_structure_designer::nodes::compare_atoms::{CompareAtomsData, MOTIONS, parse_motion};
use atomcad_structure_designer::nodes::value::{self, ValueData, add_value_node};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::TextValue;
use glam::f64::{DQuat, DVec2, DVec3};
use std::collections::{BTreeMap, HashMap, HashSet};

const H: i16 = 1;
const C: i16 = 6;
const N: i16 = 7;
const O: i16 = 8;
const SI: i16 = 14;

// ============================================================================
// Fixtures
// ============================================================================

/// Nine atoms of four elements in the plane z = 0, irregularly placed so the
/// cluster has no symmetry, every two at least 1.4 Å apart. Unbonded: the
/// bond comparison is exercised by the crystolecule tests.
const PLANAR: [(i16, [f64; 2]); 9] = [
    (C, [0.0, 0.0]),
    (C, [1.5, 0.0]),
    (N, [3.0, 0.2]),
    (O, [0.1, 1.6]),
    (C, [1.6, 1.5]),
    (SI, [3.1, 1.9]),
    (C, [0.2, 3.1]),
    (N, [1.7, 3.2]),
    (C, [3.3, 3.4]),
];

/// The atom [`lifted`] moves: the middle one.
const LIFTED: usize = 4;

/// [`PLANAR`], with the atom `lift` (if any) raised out of the plane by `dz`.
fn planar(lift: Option<(usize, f64)>) -> AtomicStructure {
    let mut s = AtomicStructure::new();
    for (i, (element, [x, y])) in PLANAR.into_iter().enumerate() {
        let dz = lift.filter(|&(k, _)| k == i).map_or(0.0, |(_, dz)| dz);
        s.add_atom(element, DVec3::new(x, y, dz));
    }
    s
}

/// The motion every `b` below is put under, B → world.
fn motion() -> (DQuat, DVec3) {
    let r = DQuat::from_axis_angle(DVec3::new(1.0, 2.0, -0.5).normalize(), 1.1);
    (r, DVec3::new(4.0, -2.5, 7.0))
}

/// `s` under [`motion`], its atoms added in reverse order so that atom ids
/// tell nothing about the matching.
fn moved(s: &AtomicStructure) -> AtomicStructure {
    let (r, t) = motion();
    let mut out = AtomicStructure::new();
    let atoms: Vec<_> = s.atoms_values().cloned().collect();
    for atom in atoms.iter().rev() {
        out.add_atom(atom.atomic_number, r * atom.position + t);
    }
    out
}

fn molecule(atoms: AtomicStructure) -> NetworkResult {
    NetworkResult::Molecule(MoleculeData {
        atoms,
        geo_tree_root: None,
    })
}

// ============================================================================
// Harness: value(a), value(b) → compare_atoms
// ============================================================================

struct Net {
    designer: StructureDesigner,
    a: u64,
    b: u64,
    node: u64,
}

const NAME: &str = "main";

/// Not validated: a `value` node declares its output `None`, which the
/// validator rejects on a typed pin, while the evaluator flows the payload
/// through unchanged.
fn network(a: AtomicStructure, b: AtomicStructure) -> Net {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(NAME);
    designer.set_active_node_network_name(Some(NAME.to_string()));
    let net = designer
        .node_type_registry
        .node_networks
        .get_mut(NAME)
        .unwrap();
    let a = add_value_node(net, DVec2::ZERO, molecule(a));
    let b = add_value_node(net, DVec2::new(0.0, 100.0), molecule(b));
    let node = designer.add_node("compare_atoms", DVec2::new(200.0, 0.0));
    designer.connect_nodes(a, 0, node, 0);
    designer.connect_nodes(b, 0, node, 1);
    Net {
        designer,
        a,
        b,
        node,
    }
}

fn set_value(designer: &mut StructureDesigner, node: u64, value: NetworkResult) {
    let node = designer
        .node_type_registry
        .node_networks
        .get_mut(NAME)
        .unwrap()
        .nodes
        .get_mut(&node)
        .unwrap();
    let data = ValueData {
        value,
        declared_type: None,
    };
    node.set_custom_node_type(
        data.calculate_custom_node_type(&value::get_node_type()),
        false,
    );
    node.data = Box::new(data);
}

fn set_props(designer: &mut StructureDesigner, node: u64, props: &[(&str, TextValue)]) {
    let map: HashMap<String, TextValue> = props
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    designer
        .node_type_registry
        .node_networks
        .get_mut(NAME)
        .unwrap()
        .nodes
        .get_mut(&node)
        .unwrap()
        .data
        .set_text_properties(&map)
        .unwrap();
}

/// The three outputs: `equal`, `aligned`, `report`.
fn outputs(net: &mut Net) -> Vec<NetworkResult> {
    let node = net.node;
    net.designer
        .with_eval_context(false, |evaluator, registry, _prefs, context| {
            let network = registry.node_networks.get(NAME).unwrap();
            let stack = vec![NetworkStackElement::root(network)];
            (0..3)
                .map(|pin| evaluator.evaluate(&stack, node, pin, registry, false, context))
                .collect()
        })
}

/// The comparison's three outputs, unpacked.
struct Out {
    equal: bool,
    aligned: AtomicStructure,
    report: BTreeMap<String, NetworkResult>,
}

impl Out {
    fn float(&self, key: &str) -> f64 {
        match &self.report[key] {
            NetworkResult::Float(x) => *x,
            other => panic!("{key}: {:?}", other.infer_data_type()),
        }
    }
    fn boolean(&self, key: &str) -> bool {
        match &self.report[key] {
            NetworkResult::Bool(b) => *b,
            other => panic!("{key}: {:?}", other.infer_data_type()),
        }
    }
    fn string(&self, key: &str) -> String {
        match &self.report[key] {
            NetworkResult::String(s) => s.clone(),
            other => panic!("{key}: {:?}", other.infer_data_type()),
        }
    }
    fn mismatches(&self) -> Vec<String> {
        match &self.report["mismatches"] {
            NetworkResult::Array(items) => items
                .iter()
                .map(|m| match m {
                    NetworkResult::String(s) => s.clone(),
                    other => panic!("mismatch: {:?}", other.infer_data_type()),
                })
                .collect(),
            other => panic!("mismatches: {:?}", other.infer_data_type()),
        }
    }
}

fn evaluate(net: &mut Net) -> Out {
    let out = outputs(net);
    let equal = match &out[0] {
        NetworkResult::Bool(b) => *b,
        NetworkResult::Error(e) => panic!("equal: Error: {e}"),
        other => panic!("equal: {:?}", other.infer_data_type()),
    };
    let aligned = match &out[1] {
        NetworkResult::Molecule(m) => m.atoms.clone(),
        other => panic!("aligned: {:?}", other.infer_data_type()),
    };
    let report = match &out[2] {
        NetworkResult::Record(r) => r.iter().cloned().collect(),
        other => panic!("report: {:?}", other.infer_data_type()),
    };
    Out {
        equal,
        aligned,
        report,
    }
}

/// The error every pin carries.
fn error(net: &mut Net) -> String {
    let out = outputs(net);
    let messages: Vec<String> = out
        .iter()
        .map(|v| match v {
            NetworkResult::Error(e) => e.clone(),
            other => panic!("expected an error, got {:?}", other.infer_data_type()),
        })
        .collect();
    assert!(messages.iter().all(|m| m == &messages[0]), "{messages:?}");
    messages[0].clone()
}

/// Each atom of `a` has an atom of `b` of its element within `within`.
fn coincide(a: &AtomicStructure, b: &AtomicStructure, within: f64) -> bool {
    a.get_num_of_atoms() == b.get_num_of_atoms()
        && a.atoms_values().all(|x| {
            b.atoms_values().any(|y| {
                y.atomic_number == x.atomic_number && y.position.distance(x.position) < within
            })
        })
}

// ============================================================================
// Pins and report
// ============================================================================

#[test]
fn a_moved_copy_is_equal_and_aligned_onto_a() {
    let a = planar(None);
    let b = moved(&a);
    let mut net = network(a.clone(), b.clone());
    let out = evaluate(&mut net);

    assert!(out.equal);
    assert!(coincide(&a, &out.aligned, 1e-6), "aligned is b on a");
    assert!(!coincide(&a, &b, 0.15), "b itself is elsewhere");
    assert!(out.float("max_deviation") < 1e-6);
    assert!(out.float("rmsd") < 1e-6);
    assert!(!out.boolean("undecided"));
    assert_eq!(out.string("rejected_by"), "");
    assert!(out.mismatches().is_empty());

    // `none` does not move it, so the same pair is not equal in place.
    set_props(
        &mut net.designer,
        net.node,
        &[("motion", TextValue::String("none".into()))],
    );
    let out = evaluate(&mut net);
    assert!(!out.equal);
    assert!(coincide(&b, &out.aligned, 1e-9), "no fit: b unmoved");
}

#[test]
fn a_near_miss_names_the_displaced_atom_against_the_best_fit() {
    // One atom lifted out of the plane by 3·tolerance: the distances barely
    // change, so the pre-checks pass and the motion search runs.
    let a = planar(None);
    let b = moved(&planar(Some((LIFTED, 0.45))));
    let mut net = network(a.clone(), b);
    let out = evaluate(&mut net);

    assert!(!out.equal);
    assert_eq!(out.string("rejected_by"), "");
    assert!(!out.boolean("undecided"));
    // The best fit is the copy's motion: every atom but the lifted one lands
    // on its original.
    let lifted = a.atoms_values().nth(LIFTED).unwrap().position;
    let mut on_a = 0;
    for atom in out.aligned.atoms_values() {
        let nearest = a
            .atoms_values()
            .map(|x| x.position.distance(atom.position))
            .fold(f64::INFINITY, f64::min);
        if nearest < 0.15 {
            on_a += 1;
        } else {
            assert!(atom.position.distance(lifted) > 0.15);
        }
    }
    assert_eq!(on_a, PLANAR.len() - 1);
    assert!(out.float("max_deviation") > 0.15);
    // The displaced atom, named from both sides and nothing else.
    let mismatches = out.mismatches();
    assert_eq!(mismatches.len(), 2, "{mismatches:?}");
}

#[test]
fn a_pre_check_rejection_names_the_check_and_leaves_b_unmoved() {
    let a = planar(None);
    let mut b = moved(&a);
    b.add_atom(H, DVec3::new(20.0, 0.0, 0.0));
    let mut net = network(a, b.clone());
    let out = evaluate(&mut net);

    assert!(!out.equal);
    let reason = out.string("rejected_by");
    assert!(reason.contains("element counts"), "{reason}");
    assert!(coincide(&b, &out.aligned, 1e-9), "no fit: b unmoved");
    assert!(out.float("max_deviation").is_nan());
    assert!(out.float("rmsd").is_nan());
    assert!(out.mismatches().is_empty());
}

#[test]
fn the_tag_compares_only_the_tagged_atoms_and_aligned_is_all_of_b() {
    let tag = |mut s: AtomicStructure| {
        for id in s.atom_ids().copied().collect::<Vec<_>>() {
            s.add_atom_tag(id, "core").unwrap();
        }
        s
    };
    let a = tag(planar(None));
    let mut b = tag(moved(&planar(None)));
    // An untagged extra atom, bonded into the tagged ones.
    let stray = b.add_atom(O, DVec3::new(9.0, 9.0, 9.0));
    let first = *b.atom_ids().next().unwrap();
    b.add_bond(first, stray, BOND_SINGLE);
    let mut net = network(a.clone(), b.clone());

    assert!(
        !evaluate(&mut net).equal,
        "every atom: element counts differ"
    );
    set_props(
        &mut net.designer,
        net.node,
        &[("tag", TextValue::String("core".into()))],
    );
    let out = evaluate(&mut net);
    assert!(out.equal, "{:?}", out.string("rejected_by"));
    assert_eq!(out.aligned.get_num_of_atoms(), b.get_num_of_atoms());

    // The panel's suggestions come from both inputs.
    let data = net.designer.node_type_registry.node_networks[NAME].nodes[&net.node]
        .data
        .as_any_ref()
        .downcast_ref::<CompareAtomsData>()
        .unwrap()
        .available_tags
        .borrow()
        .clone();
    assert_eq!(data, ["core"]);
}

#[test]
fn a_crystal_input_is_compared_and_aligned_as_a_molecule() {
    use atomcad_crystolecule::structure::Structure;
    use atomcad_structure_designer::evaluator::network_result::CrystalData;
    let a = planar(None);
    let mut net = network(a.clone(), AtomicStructure::new());
    let crystal = NetworkResult::Crystal(CrystalData {
        structure: Structure::diamond(),
        atoms: moved(&a),
        geo_tree_root: None,
        alignment: Default::default(),
        alignment_reason: None,
    });
    set_value(&mut net.designer, net.b, crystal);
    let out = evaluate(&mut net);
    assert!(out.equal);
    assert!(coincide(&a, &out.aligned, 1e-6));
}

// ============================================================================
// Errors
// ============================================================================

#[test]
fn invalid_settings_and_missing_tags_are_node_errors() {
    let a = planar(None);
    let mut net = network(a.clone(), moved(&a));
    for tolerance in [0.0, -0.1, 0.31] {
        set_props(
            &mut net.designer,
            net.node,
            &[("tolerance", TextValue::Float(tolerance))],
        );
        let e = error(&mut net);
        assert!(e.contains("tolerance") && e.contains("0.3"), "{e}");
    }
    set_props(
        &mut net.designer,
        net.node,
        &[
            ("tolerance", TextValue::Float(0.3)),
            ("tag", TextValue::String("site".into())),
        ],
    );
    let e = error(&mut net);
    assert!(e.contains("no atom of a carries the tag \"site\""), "{e}");

    // The tag on `a` only: now `b` is the one missing it.
    let mut tagged = a.clone();
    let first = *tagged.atom_ids().next().unwrap();
    tagged.add_atom_tag(first, "site").unwrap();
    set_value(&mut net.designer, net.a, molecule(tagged));
    let e = error(&mut net);
    assert!(e.contains("no atom of b carries the tag \"site\""), "{e}");

    // Every atom of an empty structure is no atom.
    set_props(
        &mut net.designer,
        net.node,
        &[("tag", TextValue::String(String::new()))],
    );
    set_value(&mut net.designer, net.b, molecule(AtomicStructure::new()));
    let e = error(&mut net);
    assert!(e.contains("b has no atoms"), "{e}");

    // An input that is not atomic.
    set_value(&mut net.designer, net.b, NetworkResult::Float(1.0));
    let e = error(&mut net);
    assert!(e.contains("b must be an atomic structure"), "{e}");
}

#[test]
fn an_upstream_error_is_forwarded_on_every_pin_verbatim() {
    let a = planar(None);
    let mut net = network(a.clone(), moved(&a));
    set_value(
        &mut net.designer,
        net.b,
        NetworkResult::Error("upstream broke".to_string()),
    );
    let e = error(&mut net);
    assert!(e.contains("upstream broke"), "{e}");
    assert!(!e.contains("compare_atoms"), "not re-wrapped: {e}");
}

// ============================================================================
// Node type, text format, persistence
// ============================================================================

#[test]
fn compare_atoms_pin_layout_snapshot() {
    let registry = NodeTypeRegistry::new();
    let nt = registry.built_in_node_types.get("compare_atoms").unwrap();
    let mut s = String::new();
    s.push_str("inputs:\n");
    for p in &nt.parameters {
        s.push_str(&format!("  {}: {}\n", p.name, p.data_type));
    }
    s.push_str("outputs:\n");
    for pin in &nt.output_pins {
        let ty = pin
            .fixed_type()
            .map(|t| t.to_string())
            .unwrap_or_else(|| "<polymorphic>".to_string());
        s.push_str(&format!("  {}: {}\n", pin.name, ty));
    }
    s.push_str("CompareReport:\n");
    let def = &registry.built_in_record_type_defs["CompareReport"];
    for field in &def.fields {
        s.push_str(&format!("  {}: {}\n", field.name, field.data_type));
    }
    insta::assert_snapshot!(s);
}

#[test]
fn a_new_node_shows_aligned_and_has_unique_s_defaults() {
    let a = planar(None);
    let net = network(a.clone(), moved(&a));
    let data = net.designer.node_type_registry.node_networks[NAME].nodes[&net.node]
        .data
        .as_any_ref()
        .downcast_ref::<CompareAtomsData>()
        .unwrap()
        .clone();
    let unique = DistinctSettings::default().compare_settings().unwrap();
    assert_eq!(data.tag, "");
    assert_eq!(data.compare_settings(), unique);
    assert_eq!(
        data.default_displayed_output_pins(),
        Some(HashSet::from([1]))
    );
}

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
    const FULL: &str = "c = compare_atoms { tag: \"cs_local\", motion: \"rigid\", \
                        tolerance: 0.25, compare_bonds: false }";
    let serialized = author_and_serialize(&format!("{FULL}\n"));
    assert!(serialized.contains(FULL), "got:\n{serialized}");
    assert_eq!(serialized, author_and_serialize(&serialized));

    let short = author_and_serialize("c = compare_atoms { }\n");
    assert!(
        short.contains(
            "c = compare_atoms { tag: \"\", motion: \"rigid_mirror\", tolerance: 0.15, \
             compare_bonds: true }"
        ),
        "got:\n{short}"
    );

    for motion in MOTIONS {
        let name = motion.name();
        let serialized =
            author_and_serialize(&format!("c = compare_atoms {{ motion: \"{name}\" }}\n"));
        assert!(
            serialized.contains(&format!("motion: \"{name}\"")),
            "got:\n{serialized}"
        );
        assert_eq!(parse_motion(name), Some(motion));
    }
    let mut data = CompareAtomsData::default();
    let err = data
        .set_text_properties(&HashMap::from([(
            "motion".to_string(),
            TextValue::String("mirror".into()),
        )]))
        .unwrap_err();
    assert!(
        err.contains("none, translation, rigid, rigid_mirror"),
        "{err}"
    );
}

#[test]
fn a_saved_node_reloads_its_settings_and_an_old_one_its_defaults() {
    let data = CompareAtomsData {
        tag: "cs_local".to_string(),
        motion: Motion::Translation,
        tolerance: 0.2,
        compare_bonds: false,
        ..CompareAtomsData::default()
    };
    let json = serde_json::to_value(&data).unwrap();
    assert!(json.get("available_tags").is_none(), "{json}");
    let back: CompareAtomsData = serde_json::from_value(json).unwrap();
    assert_eq!(
        (back.tag.as_str(), back.compare_settings()),
        ("cs_local", data.compare_settings())
    );

    let empty: CompareAtomsData = serde_json::from_str("{}").unwrap();
    assert_eq!(
        empty.compare_settings(),
        CompareAtomsData::default().compare_settings()
    );
}

// ============================================================================
// Agrees with `chemisorb`'s `unique`
// ============================================================================

/// A silyl radical at `p`: SiH3, its one dangling bond pointing up.
fn add_silyl(s: &mut AtomicStructure, p: DVec3) -> u32 {
    let si = s.add_atom(SI, p);
    let (sin, cos) = ((8.0f64 / 9.0).sqrt(), -1.0 / 3.0);
    for deg in [0.0f64, 120.0, 240.0] {
        let phi = deg.to_radians();
        let d = DVec3::new(sin * phi.cos(), sin * phi.sin(), cos);
        let h = s.add_atom(H, p + d * 1.48);
        s.add_bond(si, h, BOND_SINGLE);
    }
    si
}

/// Silyls at `sites`, everything but the Si frozen.
fn silyls(sites: &[DVec3]) -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let si: Vec<u32> = sites.iter().map(|&p| add_silyl(&mut s, p)).collect();
    for id in s.atom_ids().copied().collect::<Vec<_>>() {
        if !si.contains(&id) {
            s.set_atom_frozen(id, true);
        }
    }
    s
}

/// Runs `chemisorb` on `adsorbate` over `substrate` with `unique` off and
/// returns every kept candidate's structure, plus `unique`'s own answer on the
/// stored result.
fn kept_candidates(
    adsorbate: AtomicStructure,
    substrate: AtomicStructure,
) -> (Vec<AtomicStructure>, Vec<Option<usize>>) {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(NAME);
    designer.set_active_node_network_name(Some(NAME.to_string()));
    let net = designer
        .node_type_registry
        .node_networks
        .get_mut(NAME)
        .unwrap();
    let ads = add_value_node(net, DVec2::ZERO, molecule(adsorbate));
    let sub = add_value_node(net, DVec2::ZERO, molecule(substrate));
    let node = designer.add_node("chemisorb", DVec2::new(200.0, 0.0));
    designer.connect_nodes(ads, 0, node, 0);
    designer.connect_nodes(sub, 0, node, 1);
    set_props(
        &mut designer,
        node,
        &[
            ("anchor_reach", TextValue::Float(13.0)),
            ("unique", TextValue::String("off".into())),
        ],
    );
    designer.run_chemisorb(&[], node).unwrap();

    let candidates = designer.with_eval_context(false, |evaluator, registry, _prefs, context| {
        let network = registry.node_networks.get(NAME).unwrap();
        let stack = vec![NetworkStackElement::root(network)];
        evaluator.evaluate(&stack, node, 0, registry, false, context)
    });
    let structures: Vec<AtomicStructure> = match candidates {
        NetworkResult::Array(rows) => rows
            .iter()
            .map(|row| match row.extract_record_field("structure") {
                Some(NetworkResult::Molecule(m)) => m.atoms.clone(),
                _ => panic!("candidate without a structure"),
            })
            .collect(),
        other => panic!("candidates: {:?}", other.infer_data_type()),
    };

    let data = designer.node_type_registry.node_networks[NAME].nodes[&node]
        .data
        .as_any_ref()
        .downcast_ref::<ChemisorbData>()
        .unwrap()
        .clone();
    let stored = data.stored.expect("a stored result");
    let answer = distinct(
        &stored.plan.setup,
        &stored.report,
        &DistinctSettings::default(),
    )
    .unwrap();
    assert_eq!(answer.candidates.len(), structures.len());
    let copy_of = answer.candidates.iter().map(|c| c.copy_of).collect();
    (structures, copy_of)
}

/// With only `tag` set to `cs_local`, `equal` is `unique`'s verdict: on every
/// pair of kept candidates it is the core's comparison at `unique`'s settings,
/// every copy equals the candidate it was merged into, and no two listed
/// candidates are equal.
#[test]
fn equal_agrees_with_unique_on_every_pair_of_kept_candidates() {
    let mut h = AtomicStructure::new();
    h.add_atom(H, DVec3::new(0.3, 0.2, 2.2));
    let mut oh = AtomicStructure::new();
    let o = oh.add_atom(O, DVec3::new(0.3, 0.2, 2.2));
    let oh_h = oh.add_atom(H, DVec3::new(0.3, 0.2, 3.17));
    oh.add_bond(o, oh_h, BOND_SINGLE);
    let fixtures = [
        // Three silyls 12 Å apart: one binding, three copies.
        (
            h,
            silyls(&[
                DVec3::ZERO,
                DVec3::new(12.0, 0.0, 0.0),
                DVec3::new(0.0, 12.0, 0.0),
            ]),
        ),
        // 2.6 Å apart: each Si relaxes towards its own neighbours, so the
        // bindings differ.
        (
            oh,
            silyls(&[
                DVec3::ZERO,
                DVec3::new(2.6, 0.0, 0.0),
                DVec3::new(0.0, 2.9, 0.0),
            ]),
        ),
    ];

    let unique = DistinctSettings::default().compare_settings().unwrap();
    let (mut equal_pairs, mut unequal_pairs) = (0, 0);
    for (adsorbate, substrate) in fixtures {
        let (structures, copy_of) = kept_candidates(adsorbate, substrate);
        assert!(structures.len() >= 2, "at least one pair");
        let mut net = network(AtomicStructure::new(), AtomicStructure::new());
        set_props(
            &mut net.designer,
            net.node,
            &[("tag", TextValue::String(LOCAL_TAG.into()))],
        );
        for i in 0..structures.len() {
            for j in i + 1..structures.len() {
                set_value(&mut net.designer, net.a, molecule(structures[i].clone()));
                set_value(&mut net.designer, net.b, molecule(structures[j].clone()));
                let node_says = evaluate(&mut net).equal;

                let ci = Cluster::from_tag(&structures[i], LOCAL_TAG).unwrap();
                let cj = Cluster::from_tag(&structures[j], LOCAL_TAG).unwrap();
                let core_says = compare(&ci, &cj, &unique).unwrap().equal;
                assert_eq!(node_says, core_says, "pair ({i}, {j})");

                if copy_of[j] == Some(i) {
                    assert!(node_says, "{j} was merged into {i}");
                }
                if copy_of[i].is_none() && copy_of[j].is_none() {
                    assert!(!node_says, "{i} and {j} are both listed");
                }
                if node_says {
                    equal_pairs += 1;
                } else {
                    unequal_pairs += 1;
                }
            }
        }
    }
    assert!(
        equal_pairs >= 3 && unequal_pairs >= 3,
        "{equal_pairs} / {unequal_pairs}"
    );
}
