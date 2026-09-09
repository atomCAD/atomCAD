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

use atomcad_structure_designer::nodes::mechanosynth::MechanosynthData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_test_support::fixture_path_str;
use glam::f64::DVec2;
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
    assert!(mechanosynth_info(&designer, &[], other).is_none());
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
        },
    );
    let info = mechanosynth_info(&designer, &[], node_id).expect("the script is still loaded");
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
        },
    );
    let info = mechanosynth_info(&designer, &[], node_id).expect("the new script loaded");
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

    let info = mechanosynth_info(&designer, &[], node_id).expect("the node is a mechanosynth");
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

    let info = mechanosynth_info(&designer, &[], node_id).expect("the node is a mechanosynth");
    assert_eq!(info.count, 3);
    assert_eq!(info.applied, 0);
    assert!(info.current_op.is_empty() && info.current_note.is_empty());
}

#[test]
fn info_with_no_script_loaded_is_all_zeroes() {
    let mut designer = setup_designer();
    let node_id = designer.add_node("mechanosynth", DVec2::ZERO);

    let info = mechanosynth_info(&designer, &[], node_id).expect("the node is a mechanosynth");
    assert_eq!((info.count, info.applied), (0, 0));
    assert!(info.current_op.is_empty() && info.current_note.is_empty());
}
