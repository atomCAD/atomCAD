//! Phase 2 of `doc/design_mechanosynth_tools.md` — the node layer: the
//! `feedstocks` / `tools` input pins, the `scene` output, the display default,
//! the widened `MechanosynthStep` record, the node-level phase rule, and the
//! editor's tool-aware offers and last-good-state display.
//!
//! The engine itself is covered by `atomcad-crystolecule`'s
//! `mechanosynth_tools_test.rs`; **what is tested here is only the wiring**, so
//! every structural assertion is stated as an equality against the engine or
//! against the other node rather than against hand-written coordinates.
//!
//! `.xyz` carries no tags, so the tools are tagged in code after loading — the
//! same four tags a design applies by hand.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::io::xyz_loader::load_xyz;
use atomcad_crystolecule::mechanosynth::{
    APEX_FRAME_TAG, BuildScript, HighlightTags, OpLibrary, load_build_script, load_library,
    replay_scene,
};
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::{
    NetworkResult, first_array_element_error,
};
use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::nodes::build_script::BuildScriptData;
use atomcad_structure_designer::nodes::import_xyz::ImportXYZData;
use atomcad_structure_designer::nodes::mechanosynth::{
    MS_ADDED_TAG, MS_FEEDSTOCK_TAG, MS_TOOL_TAG, MechanosynthData,
};
use atomcad_structure_designer::nodes::mechanosynth_edit::{AuthoredStep, MechanosynthEditData};
use atomcad_structure_designer::nodes::ops_library::OpsLibraryData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_test_support::{fixture_path, fixture_path_str};
use glam::f64::DVec2;

const NET: &str = "test";

/// The shared frame tag vocabulary of `tool_ops.json`.
const FRAME_TAGS: [&str; 4] = [APEX_FRAME_TAG, "a", "b", "c"];
/// The six-atom tool skeleton's order in every tool fixture: apex, the ethynyl
/// carbon, the handle carbon, then the three legs.
const LEG_INDICES: [usize; 3] = [3, 4, 5];

// ============================================================================
// Helpers
// ============================================================================

fn fixture(name: &str) -> String {
    fixture_path_str(&format!("mechanosynth/{name}"))
}

fn library() -> OpLibrary {
    load_library(&fixture_path("mechanosynth/tool_ops.json")).expect("tool_ops.json parses")
}

fn script() -> BuildScript {
    load_build_script(&fixture_path("mechanosynth/tool_build.json"))
        .expect("tool_build.json parses")
}

fn molecule(name: &str) -> AtomicStructure {
    load_xyz(&fixture(name), true).unwrap_or_else(|e| panic!("fixture {name} loads: {e}"))
}

fn ids(structure: &AtomicStructure) -> Vec<u32> {
    let mut ids: Vec<u32> = structure.iter_atoms().map(|(id, _)| *id).collect();
    ids.sort_unstable();
    ids
}

/// What a design does by hand: the type name on the whole molecule, then
/// `apex` and the three leg tags on the four frame atoms.
fn tagged(name: &str, tool_type: &str) -> AtomicStructure {
    let mut structure = molecule(name);
    let ids = ids(&structure);
    for id in &ids {
        structure
            .add_atom_tag(*id, tool_type)
            .expect("type tag fits");
    }
    structure
        .add_atom_tag(ids[0], FRAME_TAGS[0])
        .expect("apex tag fits");
    for (slot, index) in LEG_INDICES.iter().enumerate() {
        structure
            .add_atom_tag(ids[*index], FRAME_TAGS[slot + 1])
            .expect("leg tag fits");
    }
    structure
}

fn tip() -> AtomicStructure {
    tagged("tool_tip.xyz", "habst_tool")
}

fn probe() -> AtomicStructure {
    tagged("tool_probe.xyz", "probe")
}

fn setup_designer() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(NET);
    designer.set_active_node_network_name(Some(NET.to_string()));
    designer
}

fn evaluate_pin(designer: &StructureDesigner, node_id: u64, pin_index: i32) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(NET).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let stack = vec![NetworkStackElement::root(network)];
    evaluator.evaluate(&stack, node_id, pin_index, registry, false, &mut context)
}

fn with_data<T: NodeData + 'static, F: FnOnce(&mut T)>(
    designer: &mut StructureDesigner,
    node_id: u64,
    f: F,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(NET)
        .unwrap();
    let node = network.nodes.get_mut(&node_id).expect("node exists");
    let data = node
        .data
        .as_any_mut()
        .downcast_mut::<T>()
        .expect("node data type matches");
    f(data);
}

/// A Molecule source. An `import_xyz` node with its payload written directly
/// rather than a `value` node: `value` declares `DataType::None`, which no
/// array pin will accept, and the tools have to arrive **tagged** — which is
/// what a design does with `tag` nodes or `atom_edit`'s *Tag selected…* and
/// what no `.xyz` file can carry.
fn add_molecule_node(designer: &mut StructureDesigner, structure: AtomicStructure) -> u64 {
    let node_id = designer.add_node("import_xyz", DVec2::new(-600.0, 0.0));
    with_data::<ImportXYZData, _>(designer, node_id, |data| {
        data.file_name = Some(fixture("tool_scene.xyz"));
        data.atomic_structure = Some(structure);
    });
    node_id
}

fn add_ops_and_script(designer: &mut StructureDesigner) -> (u64, u64) {
    let ops_id = designer.add_node("ops_library", DVec2::new(-400.0, -100.0));
    with_data::<OpsLibraryData, _>(designer, ops_id, |data| {
        data.file = Some(fixture("tool_ops.json"));
        data.reload_missing(None);
    });
    let script_id = designer.add_node("build_script", DVec2::new(-400.0, 100.0));
    with_data::<BuildScriptData, _>(designer, script_id, |data| {
        data.file = Some(fixture("tool_build.json"));
        data.reload_missing(None);
    });
    (ops_id, script_id)
}

/// The replayer, fully wired: base, ops, steps, and optionally the two
/// participant pins. `step` is written as stored data, not wired.
fn add_replayer(
    designer: &mut StructureDesigner,
    base_id: u64,
    feedstocks: &[u64],
    tools: &[u64],
    step: i32,
) -> u64 {
    let (ops_id, script_id) = add_ops_and_script(designer);
    let node_id = designer.add_node("mechanosynth", DVec2::ZERO);
    designer.connect_nodes(base_id, 0, node_id, 0);
    designer.connect_nodes(ops_id, 0, node_id, 1);
    designer.connect_nodes(script_id, 0, node_id, 2);
    for source in feedstocks {
        designer.connect_nodes(*source, 0, node_id, 4);
    }
    for source in tools {
        designer.connect_nodes(*source, 0, node_id, 5);
    }
    with_data::<MechanosynthData, _>(designer, node_id, |data| data.step = step);
    node_id
}

fn atoms_of(result: NetworkResult) -> AtomicStructure {
    let label = result.to_display_string();
    result
        .extract_atomic()
        .unwrap_or_else(|| panic!("expected atoms, got {label}"))
}

fn field(record: &NetworkResult, name: &str) -> String {
    match record.extract_record_field(name) {
        Some(NetworkResult::String(text)) => text.clone(),
        _ => panic!("field {name} should be a string"),
    }
}

fn count_with_tag(structure: &AtomicStructure, tag: &str) -> usize {
    structure.atoms_with_tag(tag).len()
}

// ============================================================================
// The replayer node
// ============================================================================

#[test]
fn the_three_pins_split_the_scene_the_engine_builds() {
    let mut designer = setup_designer();
    let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    let dump_id = add_molecule_node(&mut designer, molecule("tool_dump.xyz"));
    let tip_id = add_molecule_node(&mut designer, tip());
    let probe_id = add_molecule_node(&mut designer, probe());
    let node_id = add_replayer(&mut designer, base_id, &[dump_id], &[tip_id, probe_id], 4);

    let result = atoms_of(evaluate_pin(&designer, node_id, 0));
    let scene = atoms_of(evaluate_pin(&designer, node_id, 2));

    // The engine's own answer, reached without the node at all.
    let engine = replay_scene(
        &molecule("tool_scene.xyz"),
        &[molecule("tool_dump.xyz")],
        &[tip(), probe()],
        &library(),
        &script(),
        4,
        HighlightTags {
            current: Some("ms_current"),
            added: Some(MS_ADDED_TAG),
            layer: Some("ms_layer"),
            tool: Some(MS_TOOL_TAG),
            feedstock: Some(MS_FEEDSTOCK_TAG),
        },
    )
    .expect("the engine replays this build");

    assert_eq!(
        ids(&result),
        ids(&engine.workpiece()),
        "`result` is the engine's workpiece, atom for atom"
    );
    assert_eq!(
        ids(&scene),
        ids(&engine.structure),
        "`scene` is the engine's whole scene"
    );

    // The participant tags: every tool atom and every reservoir atom carries
    // one, and no base atom carries either. The counts are the engine's — a
    // tool *grows* as it picks cargo up, so a literal "six plus six" would be
    // wrong the moment an abstraction lands.
    assert_eq!(
        count_with_tag(&scene, MS_TOOL_TAG),
        count_with_tag(&engine.structure, MS_TOOL_TAG)
    );
    assert!(
        count_with_tag(&scene, MS_TOOL_TAG) >= tip().iter_atoms().count(),
        "the tools are in the scene"
    );
    assert!(count_with_tag(&scene, MS_FEEDSTOCK_TAG) > 0);
    for atom_id in ids(&result) {
        assert!(
            !scene.atom_has_tag(atom_id, MS_TOOL_TAG)
                && !scene.atom_has_tag(atom_id, MS_FEEDSTOCK_TAG),
            "base atom {atom_id} must carry neither participant tag"
        );
    }
    assert_eq!(
        count_with_tag(&result, MS_TOOL_TAG),
        0,
        "`result` has no tool atoms at all"
    );
}

#[test]
fn a_scene_with_nothing_wired_to_either_pin_equals_the_result() {
    let mut designer = setup_designer();
    let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    let node_id = add_replayer(&mut designer, base_id, &[], &[], 1);

    let result = atoms_of(evaluate_pin(&designer, node_id, 0));
    let scene = atoms_of(evaluate_pin(&designer, node_id, 2));
    assert_eq!(ids(&result), ids(&scene));
    assert_eq!(count_with_tag(&scene, MS_TOOL_TAG), 0);
    assert_eq!(count_with_tag(&scene, MS_FEEDSTOCK_TAG), 0);
}

#[test]
fn a_single_structure_wired_to_an_array_pin_is_one_participant() {
    // The broadcast rule is the evaluator's; what this pins is that the node
    // reads it as a one-element list rather than refusing a non-array.
    let mut designer = setup_designer();
    let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    let dump_id = add_molecule_node(&mut designer, molecule("tool_dump.xyz"));
    let node_id = add_replayer(&mut designer, base_id, &[dump_id], &[], 0);

    let scene = atoms_of(evaluate_pin(&designer, node_id, 2));
    assert_eq!(
        count_with_tag(&scene, MS_FEEDSTOCK_TAG),
        molecule("tool_dump.xyz").iter_atoms().count()
    );
}

#[test]
fn an_evaluation_error_reaches_all_three_pins() {
    let mut designer = setup_designer();
    let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    // The tip wired *untagged*: binding fails before any step runs.
    let tip_id = add_molecule_node(&mut designer, molecule("tool_tip.xyz"));
    let node_id = add_replayer(&mut designer, base_id, &[], &[tip_id], -1);

    for pin in [0, 1, 2] {
        match evaluate_pin(&designer, node_id, pin) {
            NetworkResult::Error(message) => {
                assert!(message.contains("mechanosynth"), "pin {pin}: {message}")
            }
            other => panic!(
                "pin {pin} should carry the error, got {}",
                other.to_display_string()
            ),
        }
    }
}

#[test]
fn the_step_record_reports_the_tool_the_state_and_the_agent() {
    let mut designer = setup_designer();
    let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    let dump_id = add_molecule_node(&mut designer, molecule("tool_dump.xyz"));
    let tip_id = add_molecule_node(&mut designer, tip());
    let probe_id = add_molecule_node(&mut designer, probe());

    // Step 0: nothing has run, so every string field takes its absent value.
    let at_zero = add_replayer(&mut designer, base_id, &[dump_id], &[tip_id, probe_id], 0);
    let record = evaluate_pin(&designer, at_zero, 1);
    for name in ["method", "tool_type", "tool_state", "agent"] {
        assert_eq!(field(&record, name), "", "{name} at step 0");
    }

    // Step 1 is `habst`: a `tip` operation that leaves `habst_tool` spent.
    let mut designer = setup_designer();
    let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    let dump_id = add_molecule_node(&mut designer, molecule("tool_dump.xyz"));
    let tip_id = add_molecule_node(&mut designer, tip());
    let probe_id = add_molecule_node(&mut designer, probe());
    let after_habst = add_replayer(&mut designer, base_id, &[dump_id], &[tip_id, probe_id], 1);
    let record = evaluate_pin(&designer, after_habst, 1);
    assert_eq!(field(&record, "op"), "habst");
    assert_eq!(field(&record, "method"), "tip");
    assert_eq!(field(&record, "tool_type"), "habst_tool");
    assert_eq!(
        field(&record, "tool_state"),
        "spent",
        "the state is the one *after* the step"
    );
    assert_eq!(field(&record, "agent"), "");

    // Step 2 is `hdump`: the recharge puts it back.
    let mut designer = setup_designer();
    let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    let dump_id = add_molecule_node(&mut designer, molecule("tool_dump.xyz"));
    let tip_id = add_molecule_node(&mut designer, tip());
    let probe_id = add_molecule_node(&mut designer, probe());
    let after_dump = add_replayer(&mut designer, base_id, &[dump_id], &[tip_id, probe_id], 2);
    let record = evaluate_pin(&designer, after_dump, 1);
    assert_eq!(field(&record, "tool_state"), "charged");

    // Step 6 is `expose`: a `bulk` operation, so the agent and no tool.
    let mut designer = setup_designer();
    let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    let dump_id = add_molecule_node(&mut designer, molecule("tool_dump.xyz"));
    let tip_id = add_molecule_node(&mut designer, tip());
    let probe_id = add_molecule_node(&mut designer, probe());
    let after_expose = add_replayer(&mut designer, base_id, &[dump_id], &[tip_id, probe_id], 6);
    let record = evaluate_pin(&designer, after_expose, 1);
    assert_eq!(field(&record, "method"), "bulk");
    assert_eq!(field(&record, "agent"), "X2");
    assert_eq!(field(&record, "tool_type"), "");
    assert_eq!(field(&record, "tool_state"), "");
}

#[test]
fn a_dumped_hydrogen_is_in_the_scene_and_never_in_the_result() {
    let mut designer = setup_designer();
    let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    let dump_id = add_molecule_node(&mut designer, molecule("tool_dump.xyz"));
    let tip_id = add_molecule_node(&mut designer, tip());
    // Through the recharge: the H the tip took lands on the reservoir.
    let node_id = add_replayer(&mut designer, base_id, &[dump_id], &[tip_id], 2);

    let result = atoms_of(evaluate_pin(&designer, node_id, 0));
    let scene = atoms_of(evaluate_pin(&designer, node_id, 2));

    let reservoir_atoms = molecule("tool_dump.xyz").iter_atoms().count();
    assert_eq!(
        count_with_tag(&scene, MS_FEEDSTOCK_TAG),
        reservoir_atoms + 1,
        "the reservoir gained the dumped hydrogen"
    );
    assert_eq!(
        count_with_tag(&result, MS_FEEDSTOCK_TAG),
        0,
        "and `result` has none of it"
    );
    // `ms_added` means "what the build created on the workpiece", so the
    // dumped H — cargo, not construction — carries none of it, and the two
    // pins agree on the set.
    assert_eq!(
        count_with_tag(&scene, MS_ADDED_TAG),
        count_with_tag(&result, MS_ADDED_TAG)
    );
}

// ============================================================================
// The display default
// ============================================================================

#[test]
fn a_freshly_placed_mechanosynth_node_displays_the_scene_pin() {
    let mut designer = setup_designer();
    let replayer = designer.add_node("mechanosynth", DVec2::ZERO);
    let editor = designer.add_node("mechanosynth_edit", DVec2::new(200.0, 0.0));
    let other = designer.add_node("sphere", DVec2::new(400.0, 0.0));

    let network = designer.node_type_registry.node_networks.get(NET).unwrap();
    let pins = |node_id: u64| {
        let mut pins: Vec<i32> = network
            .get_displayed_pins(node_id)
            .expect("a freshly added node is displayed")
            .iter()
            .copied()
            .collect();
        pins.sort_unstable();
        pins
    };
    assert_eq!(pins(replayer), vec![2]);
    assert_eq!(pins(editor), vec![2]);
    assert_eq!(pins(other), vec![0], "every other node type is unaffected");
}

#[test]
fn the_display_default_round_trips_through_a_cnnd() {
    use atomcad_structure_designer::serialization::node_networks_serialization::{
        load_node_networks_from_file, save_node_networks_to_file,
    };
    use std::collections::HashMap;
    use tempfile::tempdir;

    let mut designer = setup_designer();
    let at_default = designer.add_node("mechanosynth", DVec2::ZERO);
    let explicit = designer.add_node("mechanosynth", DVec2::new(200.0, 0.0));
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(NET)
            .unwrap();
        // The second node is put explicitly back on pin 0, which is exactly the
        // state the serializer used to treat as "the default, omit it".
        network.set_pin_displayed(explicit, 0, true);
        network.set_pin_displayed(explicit, 2, false);
    }

    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("display.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .expect("save");

    let saved = std::fs::read_to_string(&path).expect("read back");
    let saved: serde_json::Value = serde_json::from_str(&saved).expect("valid json");
    // `node_networks` is a list of `[name, network]` pairs.
    let entries = saved["node_networks"][0][1]["displayed_output_pins"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let named: Vec<u64> = entries
        .iter()
        .filter_map(|entry| entry[0].as_u64())
        .collect();
    assert!(
        !named.contains(&at_default),
        "a node at its own default writes no entry"
    );
    assert!(
        named.contains(&explicit),
        "a node the user put back on {{0}} does write one"
    );

    let mut reloaded = StructureDesigner::new();
    load_node_networks_from_file(&mut reloaded.node_type_registry, path.to_str().unwrap())
        .expect("load");
    let network = reloaded.node_type_registry.node_networks.get(NET).unwrap();
    assert_eq!(
        network.get_displayed_pins(at_default).unwrap(),
        &std::collections::HashSet::from([2]),
        "the omitted entry loads back to the node's own default"
    );
    assert_eq!(
        network.get_displayed_pins(explicit).unwrap(),
        &std::collections::HashSet::from([0]),
        "an explicit {{0}} is kept"
    );
}

// ============================================================================
// The phase rule
// ============================================================================

#[test]
fn a_participant_of_the_wrong_phase_is_a_validation_error_naming_the_pin() {
    let mut designer = setup_designer();
    // A Crystal workpiece and a Molecule tool: the merge that builds `scene`
    // would have to put a Molecule's atoms inside a Crystal, which is exactly
    // what `atom_union` refuses to produce.
    let base_molecule = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    let crystal = designer.add_node("enter_structure", DVec2::new(-300.0, 0.0));
    designer.connect_nodes(base_molecule, 0, crystal, 0);
    let tip_id = add_molecule_node(&mut designer, tip());
    let node_id = add_replayer(&mut designer, crystal, &[], &[tip_id], -1);
    designer.validate_active_network();

    let network = designer.node_type_registry.node_networks.get(NET).unwrap();
    let error = network
        .validation_errors
        .iter()
        .find(|error| error.node_id == Some(node_id))
        .unwrap_or_else(|| panic!("the phase mismatch must be reported on the node"));
    assert!(error.error_text.contains("tools"), "{}", error.error_text);
    assert!(
        error.error_text.contains("enter_structure"),
        "the message names the fix: {}",
        error.error_text
    );
}

// ============================================================================
// The editor node
// ============================================================================

/// The authored block that reproduces the first two steps of `tool_build.json`.
fn authored_prefix() -> Vec<AuthoredStep> {
    script()
        .steps
        .iter()
        .take(2)
        .map(|step| AuthoredStep {
            step: step.clone(),
            residual: 0.0,
            approximate: false,
        })
        .collect()
}

fn add_editor(
    designer: &mut StructureDesigner,
    base_id: u64,
    feedstocks: &[u64],
    tools: &[u64],
    authored: Vec<AuthoredStep>,
    cursor: i32,
) -> u64 {
    let ops_id = designer.add_node("ops_library", DVec2::new(-400.0, -100.0));
    with_data::<OpsLibraryData, _>(designer, ops_id, |data| {
        data.file = Some(fixture("tool_ops.json"));
        data.reload_missing(None);
    });
    let node_id = designer.add_node("mechanosynth_edit", DVec2::ZERO);
    designer.connect_nodes(base_id, 0, node_id, 0);
    designer.connect_nodes(ops_id, 0, node_id, 1);
    for source in feedstocks {
        designer.connect_nodes(*source, 0, node_id, 3);
    }
    for source in tools {
        designer.connect_nodes(*source, 0, node_id, 4);
    }
    with_data::<MechanosynthEditData, _>(designer, node_id, |data| {
        data.authored = authored;
        data.cursor = cursor;
    });
    node_id
}

#[test]
fn same_result_both_nodes_holds_with_feedstocks_and_tools_wired() {
    let mut designer = setup_designer();
    let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    let dump_id = add_molecule_node(&mut designer, molecule("tool_dump.xyz"));
    let tip_id = add_molecule_node(&mut designer, tip());
    let replayer = add_replayer(&mut designer, base_id, &[dump_id], &[tip_id], 2);
    let editor = add_editor(
        &mut designer,
        base_id,
        &[dump_id],
        &[tip_id],
        authored_prefix(),
        -1,
    );

    for pin in [0, 2] {
        assert_eq!(
            ids(&atoms_of(evaluate_pin(&designer, editor, pin))),
            ids(&atoms_of(evaluate_pin(&designer, replayer, pin))),
            "pin {pin} must agree between the two nodes"
        );
    }
}

#[test]
fn a_recharge_lands_on_the_reservoir_and_never_in_the_result() {
    let mut designer = setup_designer();
    let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    let dump_id = add_molecule_node(&mut designer, molecule("tool_dump.xyz"));
    let tip_id = add_molecule_node(&mut designer, tip());
    let editor = add_editor(
        &mut designer,
        base_id,
        &[dump_id],
        &[tip_id],
        authored_prefix(),
        -1,
    );

    let result = atoms_of(evaluate_pin(&designer, editor, 0));
    let scene = atoms_of(evaluate_pin(&designer, editor, 2));
    assert_eq!(
        count_with_tag(&scene, MS_FEEDSTOCK_TAG),
        molecule("tool_dump.xyz").iter_atoms().count() + 1
    );
    assert_eq!(count_with_tag(&result, MS_FEEDSTOCK_TAG), 0);
}

#[test]
fn offers_on_a_tool_atom_are_refused_and_insert_nothing() {
    let mut designer = setup_designer();
    let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    let tip_id = add_molecule_node(&mut designer, tip());
    let editor = add_editor(&mut designer, base_id, &[], &[tip_id], Vec::new(), -1);

    // The scene's tool atoms are the ones carrying the type tag.
    let scene = atoms_of(evaluate_pin(&designer, editor, 2));
    let tool_atom = *scene
        .atoms_with_tag("habst_tool")
        .first()
        .expect("the tip is in the scene, tagged");

    let error = designer
        .mechanosynth_edit_offers(&[], editor, tool_atom)
        .expect_err("a tool atom is not a host");
    assert!(error.contains("tools are rewritten"), "{error}");
    let data = designer.mechanosynth_edit_data(&[], editor).unwrap();
    assert!(data.authored.is_empty());
    assert!(data.placement.offers.is_empty());
}

#[test]
fn a_spent_tip_blocks_its_abstraction_and_offers_the_recharge_instead() {
    let mut designer = setup_designer();
    let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    let dump_id = add_molecule_node(&mut designer, molecule("tool_dump.xyz"));
    let tip_id = add_molecule_node(&mut designer, tip());
    // One `habst` authored: the tip is spent at the cursor.
    let editor = add_editor(
        &mut designer,
        base_id,
        &[dump_id],
        &[tip_id],
        authored_prefix().into_iter().take(1).collect(),
        -1,
    );

    // A workpiece hydrogen of the *second* methane, which no step has touched.
    let scene = atoms_of(evaluate_pin(&designer, editor, 2));
    let hydrogen = ids(&scene)
        .into_iter()
        .find(|id| {
            let atom = scene.get_atom(*id).unwrap();
            atom.atomic_number == 1 && atom.position.x > 9.0 && atom.position.z > 1.0
        })
        .expect("methane B's +z hydrogen");

    let sweep = designer
        .mechanosynth_edit_offers(&[], editor, hydrogen)
        .expect("the sweep answers");
    let habst = sweep
        .rows
        .iter()
        .find(|row| row.op == "habst")
        .expect("habst fits this hydrogen geometrically");
    assert!(habst.fits, "the workpiece side fits");
    let tool = habst.tool.as_ref().expect("a tip row carries its tool");
    assert!(!tool.ready, "but the tool is spent");
    assert!(!habst.offerable);
    let reason = tool.reason.clone().unwrap_or_default();
    assert!(
        reason.contains("spent") && reason.contains("charged"),
        "the reason names both states: {reason}"
    );

    // …and committing it is refused, with nothing inserted.
    let before = designer
        .mechanosynth_edit_data(&[], editor)
        .unwrap()
        .authored
        .len();
    let error = designer
        .mechanosynth_edit_choose(&[], editor, "habst", 0)
        .expect_err("a tool-blocked row cannot be committed");
    assert!(error.contains("spent"), "{error}");
    assert_eq!(
        designer
            .mechanosynth_edit_data(&[], editor)
            .unwrap()
            .authored
            .len(),
        before
    );

    // `habst_probe` needs a molecule tagged `probe`, and none is wired.
    let probe_row = sweep
        .rows
        .iter()
        .find(|row| row.op == "habst_probe")
        .expect("habst_probe fits the same hydrogen");
    let probe_tool = probe_row.tool.as_ref().expect("a tip row carries its tool");
    assert!(!probe_tool.ready);
    assert!(
        probe_tool
            .reason
            .clone()
            .unwrap_or_default()
            .contains("probe"),
        "the reason names the tag"
    );
}

#[test]
fn with_tools_unwired_no_offer_row_carries_a_tool_annotation() {
    let mut designer = setup_designer();
    let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    let editor = add_editor(&mut designer, base_id, &[], &[], Vec::new(), -1);

    let result = atoms_of(evaluate_pin(&designer, editor, 0));
    let hydrogen = ids(&result)
        .into_iter()
        .find(|id| result.get_atom(*id).unwrap().atomic_number == 1)
        .expect("a hydrogen");
    let sweep = designer
        .mechanosynth_edit_offers(&[], editor, hydrogen)
        .expect("the sweep answers");
    assert!(!sweep.rows.is_empty());
    for row in &sweep.rows {
        assert!(row.tool.is_none(), "{} carries a tool annotation", row.op);
        assert_eq!(row.offerable, row.fits);
    }
}

#[test]
fn a_failing_cursor_step_errors_on_the_pins_and_shows_the_last_good_state() {
    let mut designer = setup_designer();
    let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    let tip_id = add_molecule_node(&mut designer, tip());
    // Two `habst` in a row with no recharge between them: the second is a
    // state error, because the tip starts charged and is spent after the first.
    let first = script().steps[0].clone();
    let third = script().steps[2].clone();
    let authored: Vec<AuthoredStep> = [first, third]
        .into_iter()
        .map(|step| AuthoredStep {
            step,
            residual: 0.0,
            approximate: false,
        })
        .collect();
    let editor = add_editor(&mut designer, base_id, &[], &[tip_id], authored, -1);

    // Both structure pins carry the error.
    for pin in [0, 2] {
        match evaluate_pin(&designer, editor, pin) {
            NetworkResult::Error(message) => assert!(
                message.contains("charged") || message.contains("spent"),
                "pin {pin}: {message}"
            ),
            other => panic!(
                "pin {pin} must carry the error, got {}",
                other.to_display_string()
            ),
        }
    }

    // The `steps` output is **not** a replay product: the block is stored data
    // and the prefix arrived intact, so it carries every step regardless. The
    // walk that makes a sequence tool-aware depends on it — a recharge is
    // inserted while the block is failing.
    match evaluate_pin(&designer, editor, 1) {
        NetworkResult::Array(elements) => assert_eq!(elements.len(), 2),
        other => panic!(
            "the steps pin keeps its array, got {}",
            other.to_display_string()
        ),
    }

    // …and the node kept the state after the last successful step, which is
    // what the viewport shows and what a click is resolved against.
    let data = designer.mechanosynth_edit_data(&[], editor).unwrap();
    let last = data.last_scene().expect("the last good scene is kept");
    assert!(data.last_error().is_some());

    let one_step_back = {
        let mut designer = setup_designer();
        let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
        let tip_id = add_molecule_node(&mut designer, tip());
        let editor = add_editor(
            &mut designer,
            base_id,
            &[],
            &[tip_id],
            authored_prefix().into_iter().take(1).collect(),
            -1,
        );
        ids(&atoms_of(evaluate_pin(&designer, editor, 2)))
    };
    assert_eq!(
        ids(&last.structure),
        one_step_back,
        "the last good scene is the state after the previous step"
    );
}

#[test]
fn wiring_the_tools_is_what_makes_the_second_abstraction_fail() {
    // The workflow of §"Making a sequence tool-aware": the same block replays
    // with `tools` unwired and stops at step 2 once they are wired.
    let authored: Vec<AuthoredStep> = [script().steps[0].clone(), script().steps[2].clone()]
        .into_iter()
        .map(|step| AuthoredStep {
            step,
            residual: 0.0,
            approximate: false,
        })
        .collect();

    let mut designer = setup_designer();
    let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    let without = add_editor(&mut designer, base_id, &[], &[], authored.clone(), -1);
    assert!(
        !evaluate_pin(&designer, without, 0).is_error(),
        "with tools unwired the tool side is skipped entirely"
    );
    // The `steps` output carries the whole block throughout.
    match evaluate_pin(&designer, without, 1) {
        NetworkResult::Array(elements) => {
            assert!(first_array_element_error("steps", &elements).is_none());
            assert_eq!(elements.len(), 2);
        }
        other => panic!(
            "the steps pin is an array, got {}",
            other.to_display_string()
        ),
    }

    let mut designer = setup_designer();
    let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    let tip_id = add_molecule_node(&mut designer, tip());
    let with = add_editor(&mut designer, base_id, &[], &[tip_id], authored, 1);
    assert!(
        !evaluate_pin(&designer, with, 0).is_error(),
        "cursor 1 is the first abstraction, which replays"
    );
    with_data::<MechanosynthEditData, _>(&mut designer, with, |data| data.cursor = 2);
    assert!(
        evaluate_pin(&designer, with, 0).is_error(),
        "cursor 2 is the second abstraction, and the tip is spent"
    );
}

// ============================================================================
// The text format
// ============================================================================

#[test]
fn the_two_wire_pins_round_trip_through_the_text_format() {
    use atomcad_structure_designer::text_format::{edit_network, serialize_network};

    let mut designer = setup_designer();
    let code = "\
dump = import_xyz { file_name: \"dump.xyz\" }
tipm = import_xyz { file_name: \"tip.xyz\" }
lib = ops_library { file: \"ops.json\" }
gen = build_script { file: \"build.json\" }
slab = import_xyz { file_name: \"slab.xyz\" }
build = mechanosynth { base: slab, ops: lib, steps: gen, feedstocks: [dump], tools: [tipm], step: 40 }
";
    let registry = &mut designer.node_type_registry;
    let mut network = registry.node_networks.remove(NET).expect("test network");
    let result = edit_network(&mut network, registry, code, true);
    assert!(
        result.success,
        "authoring must succeed: {:?}",
        result.errors
    );
    let text = serialize_network(&network, registry, None);
    registry.node_networks.insert(NET.to_string(), network);

    // A single wire on an array pin is spelled without the brackets, which is
    // the format's existing convention for every array pin.
    assert!(
        text.contains("feedstocks: dump"),
        "the feedstocks wire round-trips:\n{text}"
    );
    assert!(
        text.contains("tools: tipm"),
        "the tools wire round-trips:\n{text}"
    );
    // `build` still means the workpiece: a downstream consumer of pin 0 writes
    // the bare name, not `build.result`.
    assert!(
        text.contains("base: slab, ops: lib, steps: gen"),
        "the existing pins keep their spelling:\n{text}"
    );
}

#[test]
#[ignore = "generator: writes rust/tests/fixtures/mechanosynth/mechanosynth_tools.cnnd"]
fn generate_the_tools_fixture() {
    use atomcad_structure_designer::serialization::node_networks_serialization::save_node_networks_to_file;
    use atomcad_structure_designer::text_format::edit_network;
    use std::collections::HashMap;

    let mut designer = StructureDesigner::new();
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));
    let code = r#"
slab = import_xyz { file_name: "tool_scene.xyz" }
dump = import_xyz { file_name: "tool_dump.xyz" }
tip_raw = import_xyz { file_name: "tool_tip.xyz" }
r_apex = free_sphere { center: (0.0, 30.0, 0.0), radius: 0.4 }
r_a = free_sphere { center: (-0.329263, 33.470301, 0.0), radius: 0.4 }
r_b = free_sphere { center: (-2.212868, 32.382801, 1.2557), radius: 0.4 }
r_c = free_sphere { center: (-2.212868, 32.382801, -1.2557), radius: 0.4 }
tip_typed = tag { molecule: tip_raw, name: "habst_tool" }
tip_apex = tag { molecule: tip_typed, name: "apex", region: r_apex }
tip_a = tag { molecule: tip_apex, name: "a", region: r_a }
tip_b = tag { molecule: tip_a, name: "b", region: r_b }
tip = tag { molecule: tip_b, name: "c", region: r_c }
lib = ops_library { file: "tool_ops.json" }
gen = build_script { file: "tool_build.json" }
build = mechanosynth { base: slab, ops: lib, steps: gen, feedstocks: [dump], tools: [tip], step: 3, visible: [scene] }
output build
"#;
    let registry = &mut designer.node_type_registry;
    let mut network = registry.node_networks.remove("Main").expect("Main");
    let result = edit_network(&mut network, registry, code, true);
    assert!(
        result.success,
        "authoring must succeed: {:?}",
        result.errors
    );
    registry.node_networks.insert("Main".to_string(), network);

    // `import_xyz`'s `get_text_properties` is not total, so a `file_name`
    // literal on a node that has none yet is dropped with a warning
    // (`project_text_format_roundtrip`). Write the three names directly.
    for (name, file) in [
        ("slab", "tool_scene.xyz"),
        ("dump", "tool_dump.xyz"),
        ("tip_raw", "tool_tip.xyz"),
    ] {
        let node_id = designer
            .node_type_registry
            .node_networks
            .get("Main")
            .unwrap()
            .nodes
            .iter()
            .find(|(_, node)| node.custom_name.as_deref() == Some(name))
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("node {name} exists"));
        let node = designer
            .node_type_registry
            .node_networks
            .get_mut("Main")
            .unwrap()
            .nodes
            .get_mut(&node_id)
            .unwrap();
        let data = node
            .data
            .as_any_mut()
            .downcast_mut::<ImportXYZData>()
            .expect("import_xyz data");
        data.file_name = Some(file.to_string());
    }

    let path = fixture_path("mechanosynth/mechanosynth_tools.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .expect("save");
    println!("wrote {}", path.display());
}
