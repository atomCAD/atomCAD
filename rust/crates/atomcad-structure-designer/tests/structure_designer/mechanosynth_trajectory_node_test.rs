//! Phase 2 of `doc/design_mechanosynth_trajectory.md` — the node layer: the
//! `time` property and its appended pin, the five appended `MechanosynthStep`
//! fields, and the persistence of the property through a `.cnnd` and the text
//! format.
//!
//! The trajectory engine itself is covered by `atomcad-crystolecule`'s
//! `mechanosynth_trajectory_test.rs`; **what is tested here is only the
//! wiring**, so every structural assertion is stated as an equality against the
//! engine rather than against hand-written coordinates. A pose is right when it
//! is the one `replay_scene_at` computed, not when it matches a number somebody
//! wrote down.
//!
//! `.xyz` carries no tags, so the tools are tagged in code after loading — the
//! same four tags a design applies by hand.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::io::xyz_loader::load_xyz;
use atomcad_crystolecule::mechanosynth::{
    APEX_FRAME_TAG, BuildScript, CAGE_MERIDIANS, HighlightTags, OpLibrary, Pose, REACTION, Scene,
    ToolMotion, apply_tool_pose, load_build_script, load_library, replay_scene_at,
    sweep_directions, tool_envelope_cages,
};
use atomcad_display::preferences::{
    AtomicRenderingMethod as DisplayAtomicRenderingMethod,
    AtomicStructureVisualization as DisplayAtomicVisualization,
    AtomicStructureVisualizationPreferences as DisplayAtomicPreferences,
    BackgroundPreferences as DisplayBackgroundPreferences, DisplayPreferences,
    GeometryVisualizationPreferences, MeshSmoothing,
};
use atomcad_renderer::camera::Camera;
use atomcad_renderer::line_mesh::LineMesh;
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::node_network::NodeRef;
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::build_script::BuildScriptData;
use atomcad_structure_designer::nodes::float::FloatData;
use atomcad_structure_designer::nodes::import_xyz::ImportXYZData;
use atomcad_structure_designer::nodes::mechanosynth::{
    MAX_REPORTED_CLEARANCE, MAX_REPORTED_CONTACT, MECHANOSYNTH_STEP_RECORD, MS_ADDED_TAG,
    MS_CURRENT_TAG, MS_FEEDSTOCK_TAG, MS_LAYER_TAG, MS_TOOL_TAG, MechanosynthData, TIME_PIN,
    default_time,
};
use atomcad_structure_designer::nodes::ops_library::OpsLibraryData;
use atomcad_structure_designer::overlay::{Overlay, OverlayKind};
use atomcad_structure_designer::scene_tessellator::tessellate_scene_content;
use atomcad_structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::structure_designer_scene::{
    NodeOutput, NodeSceneData, StructureDesignerScene,
};
use atomcad_structure_designer::text_format::serialize_network;
use atomcad_test_support::{fixture_path, fixture_path_str};
use glam::f64::{DMat3, DVec2, DVec3};
use tempfile::tempdir;

const NET: &str = "test";

/// The shared frame tag vocabulary of `tool_ops.json`.
const FRAME_TAGS: [&str; 4] = [APEX_FRAME_TAG, "a", "b", "c"];
/// The six-atom tool skeleton's order in every tool fixture: apex, the ethynyl
/// carbon, the handle carbon, then the three legs.
const LEG_INDICES: [usize; 3] = [3, 4, 5];

/// The step of `trajectory_build.json` the tests scrub: `hdump`, the second
/// visit of `habst_tool`'s run, so the tool is **chained** — already hovering at
/// its standoff at `u = 0` and flying on to its next site at `u = 1`.
const CHAINED_VISIT: i32 = 3;

// ============================================================================
// Helpers — the same cast `mechanosynth_tools_node_test.rs` uses
// ============================================================================

fn fixture(name: &str) -> String {
    fixture_path_str(&format!("mechanosynth/{name}"))
}

fn library() -> OpLibrary {
    load_library(&fixture_path("mechanosynth/tool_ops.json")).expect("tool_ops.json parses")
}

fn script() -> BuildScript {
    load_build_script(&fixture_path("mechanosynth/trajectory_build.json"))
        .expect("trajectory_build.json parses")
}

fn molecule(name: &str) -> AtomicStructure {
    load_xyz(&fixture(name), true).unwrap_or_else(|e| panic!("fixture {name} loads: {e}"))
}

fn ids(structure: &AtomicStructure) -> Vec<u32> {
    let mut ids: Vec<u32> = structure.iter_atoms().map(|(id, _)| *id).collect();
    ids.sort_unstable();
    ids
}

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

/// A single carbon hung at `position`: its own one-atom reservoir, so the
/// scene's per-participant input checks have nothing to compare it against.
fn atom_at(position: DVec3) -> AtomicStructure {
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, position);
    structure
}

/// A shell of loose carbons around a site, so that no direction out of it is
/// free and the sweep has to report a blocked landing. The engine test boxes a
/// site in exactly this way.
fn cage_around(centre: DVec3, radius: f64) -> Vec<AtomicStructure> {
    sweep_directions()
        .iter()
        .step_by(16)
        .map(|direction| atom_at(centre + *direction * radius))
        .collect()
}

fn tags() -> HighlightTags<'static> {
    HighlightTags {
        current: Some(MS_CURRENT_TAG),
        added: Some(MS_ADDED_TAG),
        layer: Some(MS_LAYER_TAG),
        tool: Some(MS_TOOL_TAG),
        feedstock: Some(MS_FEEDSTOCK_TAG),
    }
}

/// The engine's own answer for the trajectory fixture, reached without the node
/// at all: the scene, and the structure a viewer is shown.
fn engine_at(
    feedstocks: &[AtomicStructure],
    step: i32,
    time: f64,
) -> (Scene, Option<ToolMotion>, AtomicStructure) {
    let mut cast: Vec<AtomicStructure> = vec![molecule("tool_dump.xyz")];
    cast.extend(feedstocks.iter().cloned());
    let (scene, motion) = replay_scene_at(
        &molecule("tool_scene.xyz"),
        &cast,
        &[tip(), probe()],
        &library(),
        &script(),
        step,
        time,
        tags(),
    )
    .expect("the fixture replays");

    let mut shown = scene.structure.clone();
    if let Some(motion) = &motion {
        apply_tool_pose(&mut shown, &scene, motion.tool(), &motion.pose_at(time));
    }
    (scene, motion, shown)
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

/// A Molecule source: an `import_xyz` node with its payload written directly,
/// because the tools have to arrive **tagged** and no `.xyz` file carries tags.
fn add_molecule_node(designer: &mut StructureDesigner, structure: AtomicStructure) -> u64 {
    let node_id = designer.add_node("import_xyz", DVec2::new(-600.0, 0.0));
    with_data::<ImportXYZData, _>(designer, node_id, |data| {
        data.file_name = Some(fixture("tool_scene.xyz"));
        data.atomic_structure = Some(structure);
    });
    node_id
}

/// The replayer over the trajectory fixture, fully wired, with `step` and
/// `time` written as stored data rather than wired.
fn add_replayer(
    designer: &mut StructureDesigner,
    feedstocks: &[AtomicStructure],
    step: i32,
    time: f64,
) -> u64 {
    let base_id = add_molecule_node(designer, molecule("tool_scene.xyz"));
    let tip_id = add_molecule_node(designer, tip());
    let probe_id = add_molecule_node(designer, probe());

    let ops_id = designer.add_node("ops_library", DVec2::new(-400.0, -100.0));
    with_data::<OpsLibraryData, _>(designer, ops_id, |data| {
        data.file = Some(fixture("tool_ops.json"));
        data.reload_missing(None);
    });
    let script_id = designer.add_node("build_script", DVec2::new(-400.0, 100.0));
    with_data::<BuildScriptData, _>(designer, script_id, |data| {
        data.file = Some(fixture("trajectory_build.json"));
        data.reload_missing(None);
    });

    let node_id = designer.add_node("mechanosynth", DVec2::ZERO);
    designer.connect_nodes(base_id, 0, node_id, 0);
    designer.connect_nodes(ops_id, 0, node_id, 1);
    designer.connect_nodes(script_id, 0, node_id, 2);

    // The reservoir first, then whatever obstacles the test brought — the same
    // order `engine_at` feeds them to the engine.
    let dump_id = add_molecule_node(designer, molecule("tool_dump.xyz"));
    designer.connect_nodes(dump_id, 0, node_id, 4);
    for structure in feedstocks {
        let source = add_molecule_node(designer, structure.clone());
        designer.connect_nodes(source, 0, node_id, 4);
    }

    designer.connect_nodes(tip_id, 0, node_id, 5);
    designer.connect_nodes(probe_id, 0, node_id, 5);

    with_data::<MechanosynthData, _>(designer, node_id, |data| {
        data.step = step;
        data.time = time;
    });
    node_id
}

fn atoms_of(result: NetworkResult) -> AtomicStructure {
    let label = result.to_display_string();
    result
        .extract_atomic()
        .unwrap_or_else(|| panic!("expected atoms, got {label}"))
}

fn float_field(record: &NetworkResult, name: &str) -> f64 {
    match record.extract_record_field(name) {
        Some(NetworkResult::Float(value)) => *value,
        other => panic!("field {name} should be a float, got {:?}", other.is_some()),
    }
}

fn vec3_field(record: &NetworkResult, name: &str) -> DVec3 {
    match record.extract_record_field(name) {
        Some(NetworkResult::Vec3(value)) => *value,
        other => panic!("field {name} should be a Vec3, got {:?}", other.is_some()),
    }
}

fn mat3_field(record: &NetworkResult, name: &str) -> DMat3 {
    match record.extract_record_field(name) {
        Some(NetworkResult::Mat3(value)) => *value,
        other => panic!("field {name} should be a Mat3, got {:?}", other.is_some()),
    }
}

/// Positions are carried through a quaternion slerp on both sides, so they
/// agree to float noise rather than bitwise.
#[track_caller]
fn assert_positions_equal(actual: &AtomicStructure, expected: &AtomicStructure, what: &str) {
    assert_eq!(ids(actual), ids(expected), "{what}: the same atom ids");
    for atom_id in ids(expected) {
        let a = actual.get_atom(atom_id).expect("live").position;
        let b = expected.get_atom(atom_id).expect("live").position;
        assert!(
            (a - b).length() < 1e-9,
            "{what}: atom {atom_id} at {a:?}, engine says {b:?}"
        );
    }
}

// ============================================================================
// The `time` pin and property
// ============================================================================

#[test]
fn the_time_pin_is_appended_last_and_is_optional() {
    // **Appended**, never inserted: a saved project's wires are stored by pin
    // index, so pin 6 is the only place a new input pin may go.
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("mechanosynth").unwrap();
    assert_eq!(node_type.parameters.len(), 7);
    let names: Vec<&str> = node_type
        .parameters
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "base",
            "ops",
            "steps",
            "step",
            "feedstocks",
            "tools",
            "time"
        ]
    );
    assert_eq!(names[TIME_PIN], "time");
    assert_eq!(
        node_type.parameters[TIME_PIN].data_type,
        atomcad_structure_designer::data_type::DataType::Float
    );

    // Only `base` is required; a node with nothing on `time` reads the
    // property, which is what every existing project does.
    let metadata = MechanosynthData::new().get_parameter_metadata();
    assert_eq!(metadata.get("time").map(|(required, _)| *required), None);
    assert_eq!(MechanosynthData::new().time, default_time());
    assert_eq!(default_time(), 1.0);
}

#[test]
fn the_default_time_reproduces_the_scene_before_trajectories_existed() {
    // The compatibility assertion the whole design rests on: at `time = 1.0`
    // the outputs are the ones milestone 1 produced, tools included, because
    // step 4 ends `habst_tool`'s run and the tool is home again.
    let mut designer = setup_designer();
    let node_id = add_replayer(&mut designer, &[], 4, default_time());

    let (scene, motion, shown) = engine_at(&[], 4, 1.0);
    assert!(
        motion.is_some(),
        "step 4 is a tip step, so something visits"
    );

    assert_positions_equal(
        &atoms_of(evaluate_pin(&designer, node_id, 0)),
        &scene.workpiece(),
        "`result` at the default time",
    );
    assert_positions_equal(
        &atoms_of(evaluate_pin(&designer, node_id, 2)),
        &shown,
        "`scene` at the default time",
    );
    // …and the tool really is back at its bound pose, which is what makes the
    // claim about milestone 1 more than a tautology.
    assert_positions_equal(&shown, &scene.structure, "the run's last visit ends home");
}

#[test]
fn the_outputs_at_a_step_time_are_the_engines() {
    // Every structural assertion in this file is this equality; the rest of the
    // tests only vary what is fed in.
    for time in [0.0, 0.3, 0.49, 0.5, 0.8, 1.0] {
        let mut designer = setup_designer();
        let node_id = add_replayer(&mut designer, &[], CHAINED_VISIT, time);
        let (scene, _, shown) = engine_at(&[], CHAINED_VISIT, time);

        assert_positions_equal(
            &atoms_of(evaluate_pin(&designer, node_id, 0)),
            &scene.workpiece(),
            &format!("`result` at time {time}"),
        );
        assert_positions_equal(
            &atoms_of(evaluate_pin(&designer, node_id, 2)),
            &shown,
            &format!("`scene` at time {time}"),
        );
    }
}

#[test]
fn the_workpiece_switches_at_the_middle_of_the_dwell() {
    // `time` gates the *workpiece*, not only the tool: before the reaction the
    // node emits the scene after `k - 1`, from it the scene after `k`.
    let mut before = setup_designer();
    let before_id = add_replayer(&mut before, &[], CHAINED_VISIT, REACTION - 0.01);
    let mut after = setup_designer();
    let after_id = add_replayer(&mut after, &[], CHAINED_VISIT, REACTION);

    let (previous, _, _) = engine_at(&[], CHAINED_VISIT - 1, 1.0);
    let (current, _, _) = engine_at(&[], CHAINED_VISIT, 1.0);

    assert_positions_equal(
        &atoms_of(evaluate_pin(&before, before_id, 0)),
        &previous.workpiece(),
        "just before the reaction",
    );
    assert_positions_equal(
        &atoms_of(evaluate_pin(&after, after_id, 0)),
        &current.workpiece(),
        "at the reaction",
    );
}

#[test]
fn a_wired_time_overrides_the_property_and_both_are_clamped() {
    let mut designer = setup_designer();
    let node_id = add_replayer(&mut designer, &[], CHAINED_VISIT, 1.0);
    let float_id = designer.add_node("float", DVec2::new(-600.0, 400.0));
    with_data::<FloatData, _>(&mut designer, float_id, |data| data.value = 0.3);
    designer.connect_nodes(float_id, 0, node_id, TIME_PIN);

    let record = evaluate_pin(&designer, node_id, 1);
    assert_eq!(
        float_field(&record, "time"),
        0.3,
        "the wire wins over the stored 1.0"
    );

    // Out of range on the wire…
    with_data::<FloatData, _>(&mut designer, float_id, |data| data.value = 4.2);
    assert_eq!(
        float_field(&evaluate_pin(&designer, node_id, 1), "time"),
        1.0,
        "a wired time above the range is clamped"
    );
    with_data::<FloatData, _>(&mut designer, float_id, |data| data.value = -2.0);
    assert_eq!(
        float_field(&evaluate_pin(&designer, node_id, 1), "time"),
        0.0,
        "a wired time below the range is clamped"
    );

    // …and on the property, with the wire gone.
    let mut designer = setup_designer();
    let node_id = add_replayer(&mut designer, &[], CHAINED_VISIT, 9.0);
    assert_eq!(
        float_field(&evaluate_pin(&designer, node_id, 1), "time"),
        1.0,
        "a stored time above the range is clamped"
    );
}

// ============================================================================
// The five appended record fields
// ============================================================================

#[test]
fn the_record_carries_the_motions_pose_and_the_visits_two_measurements() {
    let time = 0.3;
    let mut designer = setup_designer();
    let node_id = add_replayer(&mut designer, &[], CHAINED_VISIT, time);
    let record = evaluate_pin(&designer, node_id, 1);

    let (_, motion, _) = engine_at(&[], CHAINED_VISIT, time);
    let motion = motion.expect("a tip step with a bound tool visits");
    let pose: Pose = motion.pose_at(time);

    assert_eq!(float_field(&record, "time"), time);
    assert!(
        (vec3_field(&record, "tool_t") - pose.t).length() < 1e-9,
        "`tool_t` is the motion's translation"
    );
    for column in 0..3 {
        assert!(
            (mat3_field(&record, "tool_r").col(column) - pose.r.col(column)).length() < 1e-9,
            "`tool_r` column {column} is the motion's"
        );
    }

    let landing = motion.landing().expect("a visit has a landing");
    assert_eq!(
        float_field(&record, "approach"),
        landing.approach.clearance.min(MAX_REPORTED_CLEARANCE),
        "`approach` is the sweep's clearance"
    );
    assert!(
        landing.reachable(),
        "the unobstructed fixture site is reachable, so the clearance is positive"
    );

    let scan = motion.scan().expect("a visit is scanned");
    assert_eq!(
        float_field(&record, "contact"),
        scan.worst
            .map_or(MAX_REPORTED_CONTACT, |contact| contact.ratio)
            .min(MAX_REPORTED_CONTACT),
        "`contact` is the scan's worst ratio"
    );
}

#[test]
fn with_every_tool_parked_the_pose_is_the_identity_and_both_caps_are_reported() {
    // Step 6 of the fixture is `expose`, a bulk step: no visit, no sweep, no
    // scan. The record has to say so in numbers a downstream `expr` can read.
    let mut designer = setup_designer();
    let node_id = add_replayer(&mut designer, &[], 6, 0.3);
    let record = evaluate_pin(&designer, node_id, 1);

    let (_, motion, _) = engine_at(&[], 6, 0.3);
    assert!(motion.is_none(), "a bulk step moves nothing");

    assert_eq!(mat3_field(&record, "tool_r"), DMat3::IDENTITY);
    assert_eq!(vec3_field(&record, "tool_t"), DVec3::ZERO);
    assert_eq!(float_field(&record, "approach"), MAX_REPORTED_CLEARANCE);
    assert_eq!(float_field(&record, "contact"), MAX_REPORTED_CONTACT);
}

#[test]
fn a_hovering_tool_reports_its_pose_but_no_landing() {
    // Step 2 is the `settle` *inside* `habst_tool`'s run: the tool waits over
    // its next site, so there is a pose to report and nothing was swept for it.
    let mut designer = setup_designer();
    let node_id = add_replayer(&mut designer, &[], 2, 0.2);
    let record = evaluate_pin(&designer, node_id, 1);

    let (_, motion, _) = engine_at(&[], 2, 0.2);
    let motion = motion.expect("a settle inside a run is a hover");
    assert!(matches!(motion, ToolMotion::Hover { .. }));

    assert!((vec3_field(&record, "tool_t") - motion.pose_at(0.2).t).length() < 1e-9);
    assert_eq!(
        float_field(&record, "approach"),
        MAX_REPORTED_CLEARANCE,
        "a hover is not a visit, so there is no clearance to report"
    );
}

#[test]
fn a_blocked_site_reports_a_negative_approach_and_the_replay_does_not_fail() {
    // The engine measures, the generator refuses, the node **reports**: a site
    // no direction reaches still visits, and the record carries the verdict.
    let (_, _, _) = engine_at(&[], 1, 0.0);
    let site = DVec3::new(0.0, 0.0, 1.09); // step 1's `habst`, on methane A
    let cage = cage_around(site, 2.6);

    let mut designer = setup_designer();
    let node_id = add_replayer(&mut designer, &cage, 1, 0.3);
    let record = evaluate_pin(&designer, node_id, 1);

    let (_, motion, shown) = engine_at(&cage, 1, 0.3);
    let landing = motion
        .as_ref()
        .and_then(ToolMotion::landing)
        .expect("the step still visits");
    assert!(
        !landing.reachable(),
        "the cage is supposed to block every sampled direction"
    );
    assert!(
        float_field(&record, "approach") < 0.0,
        "a blocked site reports a negative clearance, not an error"
    );
    assert_positions_equal(
        &atoms_of(evaluate_pin(&designer, node_id, 2)),
        &shown,
        "the blocked visit is still drawn",
    );
}

#[test]
fn the_scene_pin_moves_the_tool_while_the_parked_scene_stays_bound() {
    // The engine's `Scene` is never moved. The node poses the tool into its own
    // copy for the `scene` pin, and what it parks in `last_scene` for the panel
    // keeps every tool at its binding.
    let time = 0.2;
    let mut designer = setup_designer();
    let node_id = add_replayer(&mut designer, &[], CHAINED_VISIT, time);
    let shown = atoms_of(evaluate_pin(&designer, node_id, 2));

    let network = designer.node_type_registry.node_networks.get(NET).unwrap();
    let data = network.nodes[&node_id]
        .data
        .as_any_ref()
        .downcast_ref::<MechanosynthData>()
        .expect("mechanosynth data");
    let parked = data.last_scene().expect("the evaluation parked its scene");
    let motion = data.last_motion().expect("…and its motion");

    assert_eq!(
        ids(&shown),
        ids(&parked.structure),
        "the two differ in positions only"
    );
    let apex = parked
        .structure
        .atoms_with_tag(APEX_FRAME_TAG)
        .into_iter()
        .find(|atom_id| parked.structure.get_atom(*atom_id).is_some())
        .expect("a bound tool has a tagged apex");
    let bound = parked.structure.get_atom(apex).unwrap().position;
    let flown = shown.get_atom(apex).unwrap().position;
    assert!(
        (bound - flown).length() > 1.0,
        "at {time} the tool is off its park by more than an ångström: {bound:?} vs {flown:?}"
    );
    assert!(matches!(motion, ToolMotion::Visit(_)));
}

// ============================================================================
// Persistence
// ============================================================================

#[test]
fn time_round_trips_through_a_cnnd_and_a_node_saved_without_it_loads_at_one() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("trajectory.cnnd");

    let mut designer = setup_designer();
    let node_id = designer.add_node("mechanosynth", DVec2::ZERO);
    with_data::<MechanosynthData, _>(&mut designer, node_id, |data| data.time = 0.3);
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &std::collections::HashMap::new(),
    )
    .expect("save succeeds");

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, path.to_str().unwrap()).expect("load succeeds");
    let loaded = registry
        .node_networks
        .get(NET)
        .expect("the network survives")
        .nodes[&node_id]
        .data
        .as_any_ref()
        .downcast_ref::<MechanosynthData>()
        .expect("mechanosynth data");
    assert_eq!(loaded.time, 0.3);

    // A project saved before the property existed has no `time` key at all; the
    // serde default is what makes it load showing what it showed before.
    let text = std::fs::read_to_string(&path).expect("readable");
    let stripped = text
        .replace("\"time\":0.3", "\"step\":-1")
        .replace("\"time\": 0.3", "\"step\": -1");
    let legacy = dir.path().join("legacy.cnnd");
    std::fs::write(&legacy, stripped).expect("writable");
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, legacy.to_str().unwrap()).expect("load succeeds");
    let loaded = registry
        .node_networks
        .get(NET)
        .expect("the network survives")
        .nodes[&node_id]
        .data
        .as_any_ref()
        .downcast_ref::<MechanosynthData>()
        .expect("mechanosynth data");
    assert_eq!(
        loaded.time,
        default_time(),
        "a node with no stored `time` loads at 1.0"
    );
}

#[test]
fn the_text_format_writes_time_as_a_property() {
    // Every stored property is written, at its default or not — a property the
    // text omits reads back as wire-only and can then never be *set* from the
    // text (`a_structure_rot_axis_survives_a_replace` is the corpus pin for
    // that rule).
    let mut designer = setup_designer();
    let node_id = designer.add_node("mechanosynth", DVec2::ZERO);
    with_data::<MechanosynthData, _>(&mut designer, node_id, |data| data.time = 0.3);

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(NET).unwrap();
    let text = serialize_network(network, registry, None);
    assert!(text.contains("time: 0.3"), "{text}");

    with_data::<MechanosynthData, _>(&mut designer, node_id, |data| data.time = default_time());
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(NET).unwrap();
    let text = serialize_network(network, registry, None);
    assert!(text.contains("time: 1"), "{text}");
}

#[test]
fn the_record_type_is_still_the_registered_built_in() {
    // The five appended fields must not have turned the pin's type into an
    // anonymous record, or a `record_destructure` downstream would lose its
    // schema.
    let registry = NodeTypeRegistry::new();
    let def = registry
        .lookup_record_type_def(MECHANOSYNTH_STEP_RECORD)
        .expect("MechanosynthStep is a built-in record type");
    let tail: Vec<&str> = def
        .fields
        .iter()
        .rev()
        .take(5)
        .map(|field| field.name.as_str())
        .rev()
        .collect();
    assert_eq!(
        tail,
        vec!["time", "tool_r", "tool_t", "approach", "contact"],
        "the five are appended after `agent`"
    );
}

// ============================================================================
// Phase 3 — the envelope cage overlay
// ============================================================================

/// The overlays the node's evaluation emitted, in pin order of the bindings.
fn evaluate_overlays(designer: &StructureDesigner, node_id: u64) -> Vec<Overlay> {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(NET).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let stack = vec![NetworkStackElement::root(network)];
    evaluator
        .evaluate_all_outputs(&stack, node_id, registry, false, &mut context)
        .overlays
}

/// What the engine says the cages are at `(step, time)` — the equality every
/// structural assertion in this file is stated against.
fn engine_cages(step: i32, time: f64) -> Vec<Vec<(DVec3, DVec3)>> {
    let (scene, motion, _) = engine_at(&[], step, time);
    tool_envelope_cages(&scene, &library(), &script(), step, motion.as_ref(), time)
}

#[test]
fn a_bound_tool_gets_one_envelope_overlay_per_tool() {
    let mut designer = setup_designer();
    let node_id = add_replayer(&mut designer, &[], CHAINED_VISIT, 0.3);

    let overlays = evaluate_overlays(&designer, node_id);
    assert_eq!(overlays.len(), 2, "one cage per bound tool, in pin order");
    for overlay in &overlays {
        assert_eq!(overlay.kind, OverlayKind::ToolEnvelope);
        // CAGE_MERIDIANS slants, the same number of cylinder lines, and three
        // rings of CAGE_MERIDIANS segments each.
        assert_eq!(overlay.segments.len(), 5 * CAGE_MERIDIANS);
    }
}

#[test]
fn the_cage_segments_are_the_engines() {
    let mut designer = setup_designer();
    let node_id = add_replayer(&mut designer, &[], CHAINED_VISIT, 0.3);

    let overlays = evaluate_overlays(&designer, node_id);
    let expected = engine_cages(CHAINED_VISIT, 0.3);
    assert_eq!(overlays.len(), expected.len());
    for (overlay, cage) in overlays.iter().zip(&expected) {
        assert_eq!(overlay.segments.len(), cage.len());
        for ((from, to), (want_from, want_to)) in overlay.segments.iter().zip(cage) {
            assert!(
                (*from - *want_from).length() < 1e-9 && (*to - *want_to).length() < 1e-9,
                "the node poses the cage exactly as the engine does"
            );
        }
    }
}

#[test]
fn the_cage_follows_the_flying_tool_and_stands_still_for_a_parked_one() {
    // `CHAINED_VISIT` is a `habst_tool` visit, so binding 0 moves through the
    // step and binding 1 (the probe) stays at park the whole way.
    let mut designer = setup_designer();
    let node_id = add_replayer(&mut designer, &[], CHAINED_VISIT, 0.0);
    let early = evaluate_overlays(&designer, node_id);

    with_data::<MechanosynthData, _>(&mut designer, node_id, |data| data.time = REACTION);
    let at_reaction = evaluate_overlays(&designer, node_id);

    let apex = |overlay: &Overlay| overlay.segments[0].0;
    assert!(
        (apex(&early[0]) - apex(&at_reaction[0])).length() > 1.0,
        "the visiting tool's cage descends with it"
    );
    assert_eq!(
        early[1].segments, at_reaction[1].segments,
        "the parked tool's cage does not move"
    );
}

#[test]
fn the_cage_sits_on_the_site_at_the_reaction() {
    // The cone's apex is the tool-side reaction point, and the reaction pose
    // brings that onto the target's — so at the reaction the apex *is* the
    // landing's reaction point, which is what makes the cage readable as "this
    // is the volume that had to be clear".
    let mut designer = setup_designer();
    let node_id = add_replayer(&mut designer, &[], CHAINED_VISIT, REACTION);

    let overlays = evaluate_overlays(&designer, node_id);
    let (_, motion, _) = engine_at(&[], CHAINED_VISIT, REACTION);
    let motion = motion.expect("a tip step with a bound tool moves");
    let landing = motion.landing().expect("a tip step lands");
    assert!(
        (overlays[motion.tool()].segments[0].0 - landing.reaction_point).length() < 1e-9,
        "the apex coincides with the reaction point at the reaction"
    );
}

#[test]
fn the_cage_is_emitted_whether_or_not_the_preference_is_on() {
    // The invariant that keeps a preference out of an evaluation: the node
    // emits its segments unconditionally, and the tessellator does the asking.
    // A preferences read inside `eval` would break the memoised evaluator's
    // independence from them, and toggling the box would then cost a
    // re-evaluation rather than a re-tessellation.
    let mut designer = setup_designer();
    let node_id = add_replayer(&mut designer, &[], CHAINED_VISIT, 0.3);
    let off = evaluate_overlays(&designer, node_id);

    let mut preferences = designer.preferences.clone();
    preferences
        .atomic_structure_visualization_preferences
        .show_tool_envelopes = true;
    designer.set_preferences(preferences);

    let on = evaluate_overlays(&designer, node_id);
    assert_eq!(off.len(), on.len());
    for (a, b) in off.iter().zip(&on) {
        assert_eq!(a.segments, b.segments);
    }
}

#[test]
fn a_node_with_no_bound_tool_emits_no_cage() {
    // Tools unwired is the workpiece-only replay — the way a library is
    // developed before its instruments exist — and there is no envelope to draw.
    let mut designer = setup_designer();
    let base_id = add_molecule_node(&mut designer, molecule("tool_scene.xyz"));
    let ops_id = designer.add_node("ops_library", DVec2::new(-400.0, -100.0));
    with_data::<OpsLibraryData, _>(&mut designer, ops_id, |data| {
        data.file = Some(fixture("tool_ops.json"));
        data.reload_missing(None);
    });
    let node_id = designer.add_node("mechanosynth", DVec2::ZERO);
    designer.connect_nodes(base_id, 0, node_id, 0);
    designer.connect_nodes(ops_id, 0, node_id, 1);

    assert!(evaluate_overlays(&designer, node_id).is_empty());
}

// ============================================================================
// …and the tessellation seam it reaches the screen through
// ============================================================================

/// A one-node scene whose node carries one two-segment `ToolEnvelope` overlay.
fn scene_with_overlay() -> StructureDesignerScene {
    let mut node_data = NodeSceneData::new(NodeOutput::None);
    node_data.overlays = vec![Overlay::new(
        OverlayKind::ToolEnvelope,
        vec![
            (DVec3::ZERO, DVec3::new(1.0, 0.0, 0.0)),
            (DVec3::ZERO, DVec3::new(0.0, 1.0, 0.0)),
        ],
    )];
    let mut scene = StructureDesignerScene::new();
    scene.node_data.insert(NodeRef::top(0), node_data);
    scene
}

fn cage_display_prefs(show: bool, color: [u8; 3]) -> DisplayPreferences {
    DisplayPreferences {
        geometry_visualization: GeometryVisualizationPreferences {
            wireframe_geometry: false,
            mesh_smoothing: MeshSmoothing::Smooth,
            display_camera_target: false,
            wireframe_active_color: [1.0, 1.0, 1.0],
            wireframe_inactive_color: [0.5, 0.5, 0.5],
            hide_coplanar_edges: false,
        },
        atomic_structure_visualization: DisplayAtomicPreferences {
            visualization: DisplayAtomicVisualization::BallAndStick,
            rendering_method: DisplayAtomicRenderingMethod::Impostors,
            ball_and_stick_cull_depth: None,
            space_filling_cull_depth: None,
            scene_transparency_enabled: false,
            scene_alpha: 1.0,
            label_scale: 0.7,
            show_tool_envelopes: show,
            tool_envelope_color: color,
        },
        background: DisplayBackgroundPreferences {
            show_axes: false,
            show_grid: false,
            grid_size: 10,
            grid_color: [0, 0, 0],
            grid_strong_color: [0, 0, 0],
            show_lattice_axes: false,
            show_lattice_grid: false,
            lattice_grid_color: [0, 0, 0],
            lattice_grid_strong_color: [0, 0, 0],
            drawing_plane_grid_color: [0, 0, 0],
            drawing_plane_grid_strong_color: [0, 0, 0],
            unit_cell_wireframe_color: [0, 0, 0],
        },
    }
}

fn cage_camera() -> Camera {
    Camera {
        eye: DVec3::new(0.0, -30.0, 10.0),
        target: DVec3::ZERO,
        up: DVec3::new(0.0, 0.32, 0.95),
        aspect: 1.0,
        fovy: std::f64::consts::PI * 0.15,
        znear: 1.5,
        zfar: 2400.0,
        orthographic: false,
        ortho_half_height: 10.0,
        pivot_point: DVec3::ZERO,
        nav_up: DVec3::Z,
        nav_up_label: "Z".to_string(),
    }
}

/// The wireframe mesh a scene tessellates to — the pass the cage shares with the
/// unit-cell wireframe and the drawing-plane grid.
fn wireframe_of(scene: &StructureDesignerScene, preferences: &DisplayPreferences) -> LineMesh {
    let (_, _, _, wireframe, _, _, _, _, _, _, _) =
        tessellate_scene_content(scene, &cage_camera(), false, preferences);
    wireframe
}

#[test]
fn the_cage_reaches_the_wireframe_pass_in_the_preference_colour() {
    let scene = scene_with_overlay();
    let mesh = wireframe_of(&scene, &cage_display_prefs(true, [255, 160, 0]));

    assert_eq!(mesh.indices.len(), 4, "two segments, two endpoints each");
    for vertex in &mesh.vertices {
        assert!((vertex.color[0] - 1.0).abs() < 1e-6);
        assert!((vertex.color[1] - 160.0 / 255.0).abs() < 1e-6);
        assert_eq!(vertex.color[2], 0.0);
    }
}

#[test]
fn the_cage_contributes_nothing_with_the_preference_off() {
    let scene = scene_with_overlay();
    let mesh = wireframe_of(&scene, &cage_display_prefs(false, [255, 160, 0]));
    assert!(mesh.vertices.is_empty() && mesh.indices.is_empty());
}
