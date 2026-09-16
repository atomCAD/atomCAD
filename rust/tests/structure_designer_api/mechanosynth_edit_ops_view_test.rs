//! The `mechanosynth_edit` panel's **operation list** —
//! `doc/design_mechanosynth_op_muting.md` Phase 2.
//!
//! This is not a thin wrapper, which is why it is tested rather than left to
//! the manual walkthrough: `mechanosynth_edit_data` joins the wired library
//! against the node's stored mute set, and it has to account for the names on
//! each side that the other does not have. A muted name the library does not
//! define is the interesting case — it is kept on purpose (the `ops` pin may be
//! rewired back), so it must reach the panel, because a mute the user cannot
//! see is a mute they cannot undo.

use atomcad_crystolecule::io::xyz_loader::load_xyz;
use atomcad_structure_designer::nodes::import_xyz::ImportXYZData;
use atomcad_structure_designer::nodes::ops_library::OpsLibraryData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_test_support::fixture_path_str;
use glam::f64::DVec2;
use rust_lib_flutter_cad::api::structure_designer::mechanosynth_edit_api::mechanosynth_edit_data;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::APIMechanosynthOp;

const NET: &str = "test";

fn fixture(name: &str) -> String {
    fixture_path_str(&format!("mechanosynth/{name}"))
}

fn with_data<T: 'static, F: FnOnce(&mut T)>(designer: &mut StructureDesigner, node_id: u64, f: F) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(NET)
        .unwrap();
    let data = network
        .nodes
        .get_mut(&node_id)
        .unwrap()
        .data
        .as_any_mut()
        .downcast_mut::<T>()
        .expect("node carries the expected data type");
    f(data);
}

/// An editor wired to the placement fixtures — fourteen operations, one of
/// every shape the engine distinguishes.
fn setup() -> (StructureDesigner, u64) {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(NET);
    designer.set_active_node_network_name(Some(NET.to_string()));

    let base_id = designer.add_node("import_xyz", DVec2::new(-600.0, 0.0));
    with_data::<ImportXYZData, _>(&mut designer, base_id, |data| {
        data.file_name = Some(fixture("place_workpiece.xyz"));
        data.atomic_structure = load_xyz(&fixture("place_workpiece.xyz"), true).ok();
    });
    let ops_id = designer.add_node("ops_library", DVec2::new(-600.0, 200.0));
    with_data::<OpsLibraryData, _>(&mut designer, ops_id, |data| {
        data.file = Some(fixture("place_ops.json"));
        data.reload_missing(None);
    });

    let node_id = designer.add_node("mechanosynth_edit", DVec2::new(0.0, 0.0));
    designer.connect_nodes(base_id, 0, node_id, 0);
    designer.connect_nodes(ops_id, 0, node_id, 1);
    (designer, node_id)
}

fn ops(designer: &mut StructureDesigner, node_id: u64) -> Vec<APIMechanosynthOp> {
    mechanosynth_edit_data(designer, &[], node_id)
        .expect("a mechanosynth_edit node")
        .ops
}

fn row<'a>(ops: &'a [APIMechanosynthOp], name: &str) -> &'a APIMechanosynthOp {
    ops.iter()
        .find(|op| op.name == name)
        .unwrap_or_else(|| panic!("no row for {name}"))
}

#[test]
fn every_library_operation_is_a_row_carrying_what_the_palette_groups_by() {
    let (mut designer, node_id) = setup();
    let ops = ops(&mut designer, node_id);

    assert_eq!(ops.len(), 14, "one row per library operation");
    assert!(
        ops.iter().all(|op| !op.muted),
        "nothing is muted on a fresh node"
    );
    // The instrument fields are the operation's own, and they are what the
    // panel's group toggles are derived from — there is deliberately no
    // `family` key to group by instead.
    let habst = row(&ops, "habst");
    assert_eq!(habst.method, "tip");
    assert!(!habst.tool_type.is_empty(), "a tip op names its instrument");
    assert!(habst.agent.is_empty(), "and carries no bulk agent");
}

#[test]
fn muting_shows_on_the_row_rather_than_removing_it() {
    // The palette is where a mute is undone, so a muted operation has to stay
    // in the list. Hiding it there would be a one-way door.
    let (mut designer, node_id) = setup();
    designer
        .set_mechanosynth_edit_muted(&[], node_id, &["habst".to_string()], true)
        .expect("a mechanosynth_edit node");

    let ops = ops(&mut designer, node_id);
    assert_eq!(ops.len(), 14, "the list keeps its length");
    assert!(row(&ops, "habst").muted);
    assert!(!row(&ops, "dimerize").muted);
}

#[test]
fn a_muted_name_the_library_does_not_define_is_appended_with_no_method() {
    let (mut designer, node_id) = setup();
    designer
        .set_mechanosynth_edit_muted(&[], node_id, &["from_another_library".to_string()], true)
        .expect("a mechanosynth_edit node");

    let ops = ops(&mut designer, node_id);
    assert_eq!(ops.len(), 15);
    let stranger = &ops[14];
    assert_eq!(
        stranger.name, "from_another_library",
        "after the library's own, not mixed into them"
    );
    assert!(stranger.muted);
    assert!(
        stranger.method.is_empty(),
        "an empty method is what the panel greys the row on"
    );
}

#[test]
fn with_no_library_wired_the_list_is_the_mute_set_alone() {
    // An editor whose `ops` pin is empty has no operations to show, but it may
    // still carry a mute set — the pin gets rewired, and the working set has to
    // survive the gap.
    let mut designer = StructureDesigner::new();
    designer.add_node_network(NET);
    designer.set_active_node_network_name(Some(NET.to_string()));
    let node_id = designer.add_node("mechanosynth_edit", DVec2::ZERO);
    designer
        .set_mechanosynth_edit_muted(&[], node_id, &["habst".to_string()], true)
        .expect("a mechanosynth_edit node");

    let ops = ops(&mut designer, node_id);
    assert_eq!(
        ops.iter().map(|op| op.name.as_str()).collect::<Vec<_>>(),
        vec!["habst"],
        "an unwired `ops` pin must not silently drop the working set"
    );
    assert!(ops[0].method.is_empty());
}
