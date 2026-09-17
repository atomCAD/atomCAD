//! P2 of `design_mechanosynth_node.md` — the `mechanosynth` panel's kernel
//! seam.
//!
//! The node itself is covered in
//! `crates/atomcad-structure-designer/tests/structure_designer/mechanosynth_test.rs`;
//! what is tested here is the API pair the property panel talks to. Two things
//! are not widget behaviour and must not hide behind the manual-walkthrough
//! rule (`feedback_manual_test_for_editor_ui`): that every accessor resolves
//! through `scope_path` rather than by bare node id (a body node and a
//! top-level node routinely share an id), and the shape of
//! `get_mechanosynth_info`, which is the only way the panel can learn the step
//! count — the parsed script never crosses the bridge.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::io::xyz_loader::load_xyz;
use atomcad_crystolecule::mechanosynth::{APEX_FRAME_TAG, ToolMotion, sweep_directions};
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::nodes::build_script::BuildScriptData;
use atomcad_structure_designer::nodes::float::FloatData;
use atomcad_structure_designer::nodes::import_xyz::ImportXYZData;
use atomcad_structure_designer::nodes::int::IntData;
use atomcad_structure_designer::nodes::mechanosynth::{
    MAX_REPORTED_CLEARANCE, MAX_REPORTED_CONTACT, MechanosynthData,
};
use atomcad_structure_designer::nodes::ops_library::OpsLibraryData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_test_support::fixture_path_str;
use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::api::structure_designer::mechanosynth_api::{
    mechanosynth_data, mechanosynth_info, set_mechanosynth_data,
};
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::APIMechanosynthData;

// ============================================================================
// Helpers
// ============================================================================

fn fixture(name: &str) -> String {
    fixture_path_str(&format!("mechanosynth/{name}"))
}

fn setup_designer() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    designer
}

/// A `mechanosynth` node in `scope_path` with both fixture files loaded.
fn add_loaded_node(designer: &mut StructureDesigner, scope_path: &[u64], step: i32) -> u64 {
    let node_id = if scope_path.is_empty() {
        designer.add_node("mechanosynth", DVec2::ZERO)
    } else {
        designer.add_node_scoped(scope_path, "mechanosynth", DVec2::ZERO, None)
    };
    let mut data = MechanosynthData {
        ops_file: Some(fixture("methylate_ops.json")),
        build_file: Some(fixture("methylate_build.json")),
        step,
        ..MechanosynthData::new()
    };
    data.reload_missing(None);
    designer.set_node_network_data_scoped(scope_path, node_id, Box::new(data));
    node_id
}

/// A `build_script` node loaded from a fixture, at the top level.
fn add_build_script_node(designer: &mut StructureDesigner, name: &str) -> u64 {
    let node_id = designer.add_node("build_script", DVec2::new(-300.0, 0.0));
    let mut data = BuildScriptData {
        file: Some(fixture(name)),
        ..BuildScriptData::new()
    };
    data.reload_missing(None);
    designer.set_node_network_data_scoped(&[], node_id, Box::new(data));
    node_id
}

// ============================================================================
// The accessors resolve through `scope_path`
// ============================================================================

#[test]
fn the_getter_reads_the_three_stored_properties() {
    let mut designer = setup_designer();
    let node_id = add_loaded_node(&mut designer, &[], 2);

    let data = mechanosynth_data(&designer, &[], node_id).expect("the node is a mechanosynth");
    assert_eq!(data.ops_file, Some(fixture("methylate_ops.json")));
    assert_eq!(data.build_file, Some(fixture("methylate_build.json")));
    assert_eq!(data.step, 2);
}

#[test]
fn the_getter_returns_none_for_a_node_of_another_type() {
    let mut designer = setup_designer();
    let other = designer.add_node("float", DVec2::ZERO);
    assert!(mechanosynth_data(&designer, &[], other).is_none());
    assert!(mechanosynth_info(&mut designer, &[], other).is_none());
}

#[test]
fn a_node_inside_a_closure_body_is_reachable_and_a_colliding_root_id_is_not_confused() {
    let mut designer = setup_designer();
    let closure_id = designer.add_node("closure", DVec2::ZERO);
    let body_id = add_loaded_node(&mut designer, &[closure_id], 1);
    let root_id = add_loaded_node(&mut designer, &[], 3);

    // Per-body `next_node_id` counters make a collision the normal case rather
    // than a coincidence; if it ever stops colliding the test below is merely
    // less interesting, not wrong.
    let scoped = mechanosynth_data(&designer, &[closure_id], body_id).expect("body node");
    assert_eq!(scoped.step, 1);
    let root = mechanosynth_data(&designer, &[], root_id).expect("root node");
    assert_eq!(root.step, 3);

    // The setter is scoped the same way: writing the body node must leave the
    // root node alone.
    set_mechanosynth_data(
        &mut designer,
        &[closure_id],
        body_id,
        &APIMechanosynthData {
            ops_file: scoped.ops_file.clone(),
            build_file: scoped.build_file.clone(),
            step: 0,
            time: 1.0,
            has_legacy_files: true,
        },
    );
    assert_eq!(
        mechanosynth_data(&designer, &[closure_id], body_id)
            .unwrap()
            .step,
        0
    );
    assert_eq!(mechanosynth_data(&designer, &[], root_id).unwrap().step, 3);
}

#[test]
fn the_setter_keeps_the_caches_on_a_no_op_write_and_reloads_on_a_real_one() {
    let mut designer = setup_designer();
    let node_id = add_loaded_node(&mut designer, &[], 3);
    let stored = mechanosynth_data(&designer, &[], node_id).expect("the node is a mechanosynth");

    // A write that only changes `step` — the shape of every slider tick — must
    // not re-import the two files.
    set_mechanosynth_data(
        &mut designer,
        &[],
        node_id,
        &APIMechanosynthData {
            ops_file: stored.ops_file.clone(),
            build_file: stored.build_file.clone(),
            step: 1,
            time: 1.0,
            has_legacy_files: true,
        },
    );
    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the script is still loaded");
    assert_eq!((info.count, info.applied), (3, 1));

    // Pointing `build_file` at another script re-reads it, so the count moves.
    set_mechanosynth_data(
        &mut designer,
        &[],
        node_id,
        &APIMechanosynthData {
            ops_file: stored.ops_file.clone(),
            build_file: Some(fixture("unmatched_build.json")),
            step: -1,
            time: 1.0,
            has_legacy_files: true,
        },
    );
    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the new script loaded");
    assert_eq!(
        info.count, 2,
        "the two-step script should have been re-read"
    );
    assert_eq!(info.applied, 2, "a negative step means every step");
}

// ============================================================================
// `get_mechanosynth_info`
// ============================================================================

#[test]
fn info_names_the_current_step() {
    let mut designer = setup_designer();
    let node_id = add_loaded_node(&mut designer, &[], 2);

    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");
    assert_eq!(info.count, 3);
    assert_eq!(info.applied, 2);
    // "The current step" is the last one applied, `steps[1]` of the fixture.
    assert_eq!(info.current_op, "gm_methylate");
    assert_eq!(info.current_note, "CH2 onto the bare carbon");
}

#[test]
fn info_at_step_zero_keeps_the_count_and_names_nothing() {
    let mut designer = setup_designer();
    let node_id = add_loaded_node(&mut designer, &[], 0);

    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");
    assert_eq!(info.count, 3);
    assert_eq!(info.applied, 0);
    assert!(info.current_op.is_empty() && info.current_note.is_empty());
}

#[test]
fn info_with_no_script_loaded_is_all_zeroes() {
    let mut designer = setup_designer();
    let node_id = designer.add_node("mechanosynth", DVec2::ZERO);

    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");
    assert_eq!((info.count, info.applied), (0, 0));
    assert!(info.current_op.is_empty() && info.current_note.is_empty());
}

// ============================================================================
// The readout follows the wired pins
// ============================================================================

/// The demo that switches between two libraries by wire feeds one
/// `mechanosynth` node from a `switch` on `string` nodes; nothing is stored on
/// the node, yet the slider must have the script's range and the readout must
/// name the current step. The steps now arrive as a `[BuildStep]` array rather
/// than as a file name, so the wire is a `build_script` node.
#[test]
fn info_follows_steps_arriving_on_the_wire() {
    let mut designer = setup_designer();
    let node_id = designer.add_node("mechanosynth", DVec2::ZERO);
    let mut data = MechanosynthData {
        ops_file: Some(fixture("methylate_ops.json")),
        build_file: None,
        step: 2,
        ..MechanosynthData::new()
    };
    data.reload_missing(None);
    designer.set_node_network_data_scoped(&[], node_id, Box::new(data));

    // Pin 2 is `steps`.
    let steps_id = add_build_script_node(&mut designer, "methylate_build.json");
    designer.connect_nodes(steps_id, 0, node_id, 2);

    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");
    assert_eq!((info.count, info.applied), (3, 2));
    assert_eq!(info.current_op, "gm_methylate");
    assert_eq!(info.current_note, "CH2 onto the bare carbon");
}

/// The wire wins over the deprecated property, as it does in `eval`.
#[test]
fn wired_steps_win_over_the_stored_build_file() {
    let mut designer = setup_designer();
    let node_id = add_loaded_node(&mut designer, &[], -1);

    let steps_id = add_build_script_node(&mut designer, "unmatched_build.json");
    designer.connect_nodes(steps_id, 0, node_id, 2);

    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");
    assert_eq!(
        (info.count, info.applied),
        (2, 2),
        "the two-step wired script, not the stored three-step one"
    );
}

#[test]
fn info_follows_a_step_arriving_on_the_wire() {
    let mut designer = setup_designer();
    let node_id = add_loaded_node(&mut designer, &[], -1);

    let step_id = designer.add_node("int", DVec2::ZERO);
    designer.set_node_network_data_scoped(&[], step_id, Box::new(IntData { value: 1 }));
    // Pin 3 is `step`.
    designer.connect_nodes(step_id, 0, node_id, 3);

    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");
    assert_eq!((info.count, info.applied), (3, 1));
    assert_eq!(info.current_op, "habst");
}

/// A wired name that does not resolve gives the all-zero readout; the error
/// itself is the result pin's business.
#[test]
fn a_wired_loader_that_fails_to_load_reads_as_no_script() {
    let mut designer = setup_designer();
    let node_id = add_loaded_node(&mut designer, &[], -1);

    let steps_id = add_build_script_node(&mut designer, "does_not_exist.json");
    designer.connect_nodes(steps_id, 0, node_id, 2);

    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");
    assert_eq!((info.count, info.applied), (0, 0));
}

// ============================================================================
// Step metadata and chapters
// (`doc/design_mechanosynth_step_metadata.md`)
// ============================================================================

/// The metadata fixtures instead of the methylate ones: five steps whose
/// `(phase, layer)` pairs form three chapters, the first of them untitled.
fn add_metadata_node(designer: &mut StructureDesigner, step: i32) -> u64 {
    let node_id = designer.add_node("mechanosynth", DVec2::ZERO);
    let mut data = MechanosynthData {
        ops_file: Some(fixture("metadata_ops.json")),
        build_file: Some(fixture("metadata_build.json")),
        step,
        ..MechanosynthData::new()
    };
    data.reload_missing(None);
    designer.set_node_network_data_scoped(&[], node_id, Box::new(data));
    node_id
}

#[test]
fn info_reports_the_current_steps_metadata() {
    let mut designer = setup_designer();
    let node_id = add_metadata_node(&mut designer, 3);

    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");
    // The method is the **operation's** kind, read from the library; every
    // operation of the metadata fixture is `tip`. The other three are the
    // step's own.
    assert_eq!(info.current_method, "tip");
    assert_eq!(info.current_phase, "layer1");
    assert_eq!(info.current_layer, 1);
    assert_eq!(info.current_site, 1);

    // A step that states nothing reports the defaults the panel omits chips for.
    let node_id = add_metadata_node(&mut designer, 1);
    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");
    assert_eq!(info.current_method, "tip");
    assert_eq!(info.current_phase, "");
    assert_eq!(info.current_layer, -1);
    assert_eq!(info.current_site, -1);

    // At step 0 nothing has been applied, so there is nothing to describe.
    let node_id = add_metadata_node(&mut designer, 0);
    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");
    assert_eq!(info.applied, 0);
    assert_eq!(info.current_phase, "");
    assert_eq!(info.current_layer, -1);
}

#[test]
fn the_chapter_list_covers_the_whole_script() {
    let mut designer = setup_designer();
    let node_id = add_metadata_node(&mut designer, -1);

    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");
    let chapters: Vec<(&str, i32, i32, i32)> = info
        .chapters
        .iter()
        .map(|c| (c.phase.as_str(), c.layer, c.first_step, c.last_step))
        .collect();

    // Step 1 names no phase and no layer, and is still a chapter, so the list
    // covers every step. Steps 2-3 and 4-5 share a layer but not a phase, so
    // the run breaks on the pair, not on the layer alone.
    assert_eq!(
        chapters,
        vec![("", -1, 1, 1), ("layer1", 1, 2, 3), ("cleanup", 1, 4, 5)]
    );

    // No gaps and no overlaps, whatever the script says.
    let mut expected_next = 1;
    for chapter in &info.chapters {
        assert_eq!(chapter.first_step, expected_next);
        assert!(chapter.last_step >= chapter.first_step);
        expected_next = chapter.last_step + 1;
    }
    assert_eq!(expected_next - 1, info.count);
}

#[test]
fn the_chapter_list_does_not_depend_on_the_step_number() {
    // The chapters describe the script, not the scrub position, so the panel
    // can draw the same tick marks wherever the slider stands.
    let mut designer = setup_designer();
    let end_id = add_metadata_node(&mut designer, -1);
    let at_end = mechanosynth_info(&mut designer, &[], end_id)
        .expect("the node is a mechanosynth")
        .chapters
        .len();
    let start_id = add_metadata_node(&mut designer, 0);
    let at_start = mechanosynth_info(&mut designer, &[], start_id)
        .expect("the node is a mechanosynth")
        .chapters
        .len();
    assert_eq!((at_end, at_start), (3, 3));
}

#[test]
fn a_script_with_no_metadata_at_all_is_one_untitled_chapter() {
    // The pre-metadata fixture: three steps, none of which says anything.
    let mut designer = setup_designer();
    let node_id = add_loaded_node(&mut designer, &[], -1);

    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");
    assert_eq!(info.chapters.len(), 1);
    assert_eq!(info.chapters[0].phase, "");
    assert_eq!(info.chapters[0].layer, -1);
    assert_eq!(
        (info.chapters[0].first_step, info.chapters[0].last_step),
        (1, 3)
    );
}

#[test]
fn no_script_means_no_chapters() {
    let mut designer = setup_designer();
    let node_id = designer.add_node("mechanosynth", DVec2::ZERO);

    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");
    assert_eq!(info.count, 0);
    assert!(info.chapters.is_empty());
    assert_eq!(info.current_layer, -1);
    assert_eq!(info.current_site, -1);
}

// ============================================================================
// The trajectory readout (`doc/design_mechanosynth_trajectory.md`)
// ============================================================================
//
// These read the **last evaluation**, the way the tool and feedstock rows
// already do, so each one wires a replayer over the trajectory fixture and
// evaluates it before asking. What is asserted is the plumbing: that the panel
// is handed the engine's own numbers. Which word each leg gets, and when, is
// the engine's own test (`mechanosynth_trajectory_test.rs`).

/// The shared frame tag vocabulary of `tool_ops.json`.
const FRAME_TAGS: [&str; 4] = [APEX_FRAME_TAG, "a", "b", "c"];
const LEG_INDICES: [usize; 3] = [3, 4, 5];

fn molecule(name: &str) -> AtomicStructure {
    load_xyz(&fixture(name), true).unwrap_or_else(|e| panic!("fixture {name} loads: {e}"))
}

fn atom_ids(structure: &AtomicStructure) -> Vec<u32> {
    let mut ids: Vec<u32> = structure.iter_atoms().map(|(id, _)| *id).collect();
    ids.sort_unstable();
    ids
}

fn tagged(name: &str, tool_type: &str) -> AtomicStructure {
    let mut structure = molecule(name);
    let ids = atom_ids(&structure);
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

fn with_data<T: NodeData + 'static, F: FnOnce(&mut T)>(
    designer: &mut StructureDesigner,
    node_id: u64,
    f: F,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    let node = network.nodes.get_mut(&node_id).expect("node exists");
    let data = node
        .data
        .as_any_mut()
        .downcast_mut::<T>()
        .expect("node data type matches");
    f(data);
}

fn add_molecule_node(designer: &mut StructureDesigner, structure: AtomicStructure) -> u64 {
    let node_id = designer.add_node("import_xyz", DVec2::new(-600.0, 0.0));
    with_data::<ImportXYZData, _>(designer, node_id, |data| {
        data.file_name = Some(fixture("tool_scene.xyz"));
        data.atomic_structure = Some(structure);
    });
    node_id
}

/// The trajectory replayer, wired and **evaluated** — the readout is read off
/// the last evaluation and never forces one, so a node nobody has evaluated
/// reports nothing, which is also the truth about what is on screen.
fn add_evaluated_replayer(
    designer: &mut StructureDesigner,
    obstacles: &[AtomicStructure],
    step: i32,
    time: f64,
) -> u64 {
    let base_id = add_molecule_node(designer, molecule("tool_scene.xyz"));
    let dump_id = add_molecule_node(designer, molecule("tool_dump.xyz"));
    let tip_id = add_molecule_node(designer, tagged("tool_tip.xyz", "habst_tool"));
    let probe_id = add_molecule_node(designer, tagged("tool_probe.xyz", "probe"));

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
    designer.connect_nodes(dump_id, 0, node_id, 4);
    for structure in obstacles {
        let source = add_molecule_node(designer, structure.clone());
        designer.connect_nodes(source, 0, node_id, 4);
    }
    designer.connect_nodes(tip_id, 0, node_id, 5);
    designer.connect_nodes(probe_id, 0, node_id, 5);
    with_data::<MechanosynthData, _>(designer, node_id, |data| {
        data.step = step;
        data.time = time;
    });

    evaluate(designer, node_id);
    node_id
}

fn evaluate(designer: &StructureDesigner, node_id: u64) {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let stack = vec![NetworkStackElement::root(network)];
    evaluator.evaluate(&stack, node_id, 0, registry, false, &mut context);
}

/// What the node's own evaluation parked, for the equalities below.
fn last_motion(designer: &StructureDesigner, node_id: u64) -> Option<ToolMotion> {
    designer
        .get_node_network_data_scoped(&[], node_id)?
        .as_any_ref()
        .downcast_ref::<MechanosynthData>()?
        .last_motion()
}

#[test]
fn the_readout_reports_the_visit_the_last_evaluation_planned() {
    let time = 0.2;
    let mut designer = setup_designer();
    let node_id = add_evaluated_replayer(&mut designer, &[], 3, time);
    let motion =
        last_motion(&designer, node_id).expect("step 3 is a tip step, so something visits");
    let landing = motion.landing().expect("a visit has a landing");

    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");
    assert_eq!(info.time, time);
    assert_eq!(info.leg, motion.leg_at(time).as_str());
    assert!(!info.leg.is_empty(), "a visiting tool is doing something");
    assert_eq!(info.tilt_degrees, landing.approach.tilt.to_degrees());
    assert_eq!(
        info.approach_clearance,
        landing.approach.clearance.min(MAX_REPORTED_CLEARANCE)
    );
    assert_eq!(
        info.contact_ratio,
        motion
            .scan()
            .and_then(|scan| scan.worst)
            .map_or(MAX_REPORTED_CONTACT, |contact| contact.ratio)
            .min(MAX_REPORTED_CONTACT)
    );
    assert!(
        info.collision.is_empty(),
        "the unobstructed fixture flight is clear: {}",
        info.collision
    );
}

#[test]
fn a_wired_time_reaches_the_readout() {
    // The readout reports what the node **evaluates**, so a wired `time` has to
    // win over the stored property here exactly as it does in `eval`.
    let mut designer = setup_designer();
    let node_id = add_evaluated_replayer(&mut designer, &[], 3, 1.0);
    let float_id = designer.add_node("float", DVec2::new(-600.0, 400.0));
    with_data::<FloatData, _>(&mut designer, float_id, |data| data.value = 0.25);
    designer.connect_nodes(float_id, 0, node_id, 6);
    evaluate(&designer, node_id);

    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");
    assert_eq!(info.time, 0.25);
    assert!(!info.leg.is_empty());
}

#[test]
fn with_every_tool_parked_the_readout_says_nothing_and_reports_both_caps() {
    // Step 6 of the fixture is `expose`, a bulk step: no visit, no sweep, no
    // scan. An empty `leg` is how the panel knows to draw no readout lines.
    let mut designer = setup_designer();
    let node_id = add_evaluated_replayer(&mut designer, &[], 6, 0.3);
    assert!(last_motion(&designer, node_id).is_none());

    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");
    assert_eq!(info.leg, "");
    assert_eq!(info.tilt_degrees, 0.0);
    assert_eq!(info.approach_clearance, MAX_REPORTED_CLEARANCE);
    assert_eq!(info.contact_ratio, MAX_REPORTED_CONTACT);
    assert_eq!(info.contact_at, 0.0);
    assert_eq!(info.collision, "");
}

#[test]
fn a_blocked_site_reports_a_negative_clearance_and_the_info_is_still_returned() {
    // The engine measures, the generator refuses, the node **reports**: a
    // blocked site is a readout line, never an error and never a missing info.
    let site = DVec3::new(0.0, 0.0, 1.09);
    let cage: Vec<AtomicStructure> = sweep_directions()
        .iter()
        .step_by(16)
        .map(|direction| {
            let mut structure = AtomicStructure::new();
            structure.add_atom(6, site + *direction * 2.6);
            structure
        })
        .collect();

    let mut designer = setup_designer();
    let node_id = add_evaluated_replayer(&mut designer, &cage, 1, 0.3);
    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");

    assert!(
        info.approach_clearance < 0.0,
        "the caged site is blocked: {}",
        info.approach_clearance
    );
    assert!(!info.leg.is_empty(), "the tool visits it anyway");
}

#[test]
fn a_tool_that_flies_past_something_reports_how_close_it_came() {
    // A loose atom parked on the probe's flight line. The scan is a report,
    // never an error: a collision on a flight is the user's layout, and the fix
    // is a re-park the user can see at once.
    let mut designer = setup_designer();
    let clear = add_evaluated_replayer(&mut designer, &[], 5, 0.1);
    let clear_ratio = mechanosynth_info(&mut designer, &[], clear)
        .expect("the node is a mechanosynth")
        .contact_ratio;

    let motion = last_motion(&designer, clear).expect("the probe visits");
    // On the probe's own inbound flight, a tenth of the way in — so the atom is
    // planted exactly where the tool will pass rather than where it might.
    let apex = motion.pose_at(0.1).t;

    let mut obstacle = AtomicStructure::new();
    obstacle.add_atom(6, apex);
    let mut designer = setup_designer();
    let node_id = add_evaluated_replayer(&mut designer, &[obstacle], 5, 0.1);
    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");

    assert!(
        info.contact_ratio < clear_ratio,
        "the planted atom is closer than anything on the clear flight: {} vs {}",
        info.contact_ratio,
        clear_ratio
    );
    assert!(
        info.collision.starts_with("path collides at "),
        "the panel sentence: {}",
        info.collision
    );
    assert!(info.collision.contains("ratio "), "{}", info.collision);
    assert!(
        (0.0..=1.0).contains(&info.contact_at),
        "the contact's step time is a step time: {}",
        info.contact_at
    );
}

#[test]
fn the_tools_block_marks_the_one_row_that_is_away_from_park() {
    // At most one tool is ever away from park, and the panel prefixes its row
    // with a marker rather than a legend
    // (`doc/design_mechanosynth_trajectory.md` §A tool leaves park once per
    // run). The flag is the binding index the motion names, so a tool that
    // failed to bind could never shift it onto its neighbour.
    let mut designer = setup_designer();
    let node_id = add_evaluated_replayer(&mut designer, &[], 3, 0.2);
    let motion = last_motion(&designer, node_id).expect("step 3 is a tip step");

    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");
    let moving: Vec<usize> = info
        .tools
        .iter()
        .enumerate()
        .filter(|(_, row)| row.moving)
        .map(|(index, _)| index)
        .collect();
    assert_eq!(moving, vec![motion.tool()], "exactly the visiting tool");
}

#[test]
fn no_tool_is_marked_when_every_tool_is_parked() {
    // A `bulk` step has no visit, so nothing in the block is moving.
    let mut designer = setup_designer();
    let node_id = add_evaluated_replayer(&mut designer, &[], 6, 0.2);
    assert!(last_motion(&designer, node_id).is_none());

    let info = mechanosynth_info(&mut designer, &[], node_id).expect("the node is a mechanosynth");
    assert!(!info.tools.is_empty(), "the fixture binds two tools");
    assert!(info.tools.iter().all(|row| !row.moving));
}
