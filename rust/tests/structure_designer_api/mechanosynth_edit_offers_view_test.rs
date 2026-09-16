//! The **offer popup's** view of a sweep — the projection
//! `mechanosynth_edit_api::offers_view` makes, and the one thing in it that is
//! not a field copy: the refusals of
//! `doc/design_mechanosynth_pattern_checks.md` §7.
//!
//! A refusal crosses the bridge as a **string, empty for none**, at two levels
//! that mean different things: on a candidate it says *this way of placing it*
//! is refused, and on a row it says *every* way is. Getting the second one
//! wrong would put a perfectly placeable row below the rule, or leave an
//! unplaceable one above it, so it is worth a test of its own rather than the
//! manual walkthrough.

use atomcad_crystolecule::io::xyz_loader::load_xyz;
use atomcad_structure_designer::nodes::import_xyz::ImportXYZData;
use atomcad_structure_designer::nodes::ops_library::OpsLibraryData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_test_support::fixture_path_str;
use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::api::structure_designer::mechanosynth_edit_api::offers_view;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::{
    APIMechanosynthOffer, APIMechanosynthOffers,
};

const NET: &str = "test";

/// The planar host of `place_workpiece_blocked.xyz` with **one** side of its
/// plane occupied, and the one with **both** sides occupied.
const MIXED: DVec3 = DVec3::new(20.0, 20.0, 0.0);
const FULLY_BLOCKED: DVec3 = DVec3::new(40.0, 20.0, 0.0);

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

/// An editor wired to the occupied-site placement fixtures.
fn setup() -> (StructureDesigner, u64) {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(NET);
    designer.set_active_node_network_name(Some(NET.to_string()));

    let path = fixture("place_workpiece_blocked.xyz");
    let base_id = designer.add_node("import_xyz", DVec2::new(-600.0, 0.0));
    with_data::<ImportXYZData, _>(&mut designer, base_id, |data| {
        data.file_name = Some(path.clone());
        data.atomic_structure = load_xyz(&path, true).ok();
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

/// The view of the sweep at the atom sitting at `pos`.
fn sweep_at(designer: &mut StructureDesigner, node_id: u64, pos: DVec3) -> APIMechanosynthOffers {
    let structure = load_xyz(&fixture("place_workpiece_blocked.xyz"), true).expect("it loads");
    let found = structure.get_atoms_in_radius(&pos, 1e-4);
    assert_eq!(found.len(), 1, "expected exactly one atom at {pos:?}");
    let sweep = designer
        .mechanosynth_edit_offers(&[], node_id, found[0])
        .expect("the atom exists");
    offers_view(&sweep)
}

fn row<'a>(offers: &'a APIMechanosynthOffers, op: &str) -> &'a APIMechanosynthOffer {
    offers
        .rows
        .iter()
        .find(|row| row.op == op)
        .unwrap_or_else(|| panic!("no {op} row"))
}

#[test]
fn a_mixed_row_carries_its_refusal_on_the_candidate_and_not_on_the_row() {
    let (mut designer, node_id) = setup();
    let offers = sweep_at(&mut designer, node_id, MIXED);
    let row = row(&offers, "land4");

    assert!(row.offerable, "one good way of placing it is enough");
    assert!(
        row.blocked.is_empty(),
        "the row stays above the rule: {}",
        row.blocked
    );
    let refused: Vec<&_> = row
        .candidates
        .iter()
        .filter(|candidate| !candidate.blocked.is_empty())
        .collect();
    assert_eq!(refused.len(), 1, "the occupied side, and only it");
    assert!(
        refused[0].blocked.contains("would put"),
        "{}",
        refused[0].blocked
    );
    assert!(
        row.candidates[0].blocked.is_empty(),
        "index 0 of an offerable row is always placeable"
    );
}

#[test]
fn a_row_whose_every_candidate_is_refused_carries_the_reason_itself() {
    let (mut designer, node_id) = setup();
    let offers = sweep_at(&mut designer, node_id, FULLY_BLOCKED);
    let row = row(&offers, "land4");

    assert!(row.fits, "the fit is real; the site is not");
    assert!(!row.offerable);
    assert!(
        row.blocked.contains("would put"),
        "the row says why, where its residual would be: {}",
        row.blocked
    );
    assert!(
        row.candidates
            .iter()
            .all(|candidate| !candidate.blocked.is_empty()),
        "and every way of placing it says so too"
    );
}

#[test]
fn a_row_nothing_refuses_carries_no_reason_at_any_level() {
    // The empty string is the "nothing to say" state at both levels, and a
    // clean sweep has to reach the popup with both of them empty — a
    // non-empty `blocked` is what dims a row.
    let (mut designer, node_id) = setup();
    let offers = sweep_at(&mut designer, node_id, DVec3::ZERO);
    assert!(!offers.rows.is_empty(), "the A cluster hosts several ops");
    for row in &offers.rows {
        assert!(row.blocked.is_empty(), "{} says {}", row.op, row.blocked);
        for candidate in &row.candidates {
            assert!(candidate.blocked.is_empty(), "{}", row.op);
        }
    }
}
