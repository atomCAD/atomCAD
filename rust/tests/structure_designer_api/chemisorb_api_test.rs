//! Phase 2 of the chemisorption search design — the `chemisorb` panel's kernel
//! seam. The CLI's `run` is generic now and tested in `node_jobs_api_test.rs`.
//!
//! The node is covered in
//! `crates/atomcad-structure-designer/tests/structure_designer/chemisorb_node_test.rs`.
//! What is tested here is what the panel reads: the report travels
//! through the selected node's eval cache, the settings setter is scoped,
//! undoable and keeps the stored search.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use atomcad_structure_designer::evaluator::network_result::{MoleculeData, NetworkResult};
use atomcad_structure_designer::nodes::chemisorb::ChemisorbData;
use atomcad_structure_designer::nodes::value::add_value_node;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::api::structure_designer::chemisorb_api::{
    chemisorb_node_data, chemisorb_node_report, set_chemisorb_node_data,
};
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::APIChemisorbData;

/// •OH over two silyl radicals whose hydrogens are frozen.
pub(crate) fn fixture() -> (AtomicStructure, AtomicStructure) {
    let mut ads = AtomicStructure::new();
    let o = ads.add_atom(8, DVec3::new(0.3, 0.2, 2.2));
    let h = ads.add_atom(1, DVec3::new(0.3, 0.2, 3.17));
    ads.add_bond(o, h, BOND_SINGLE);

    let mut sub = AtomicStructure::new();
    let (sin, cos) = ((8.0f64 / 9.0).sqrt(), -1.0 / 3.0);
    for p in [DVec3::ZERO, DVec3::new(2.6, 0.0, 0.0)] {
        let si = sub.add_atom(14, p);
        for deg in [0.0f64, 120.0, 240.0] {
            let phi = deg.to_radians();
            let d = DVec3::new(sin * phi.cos(), sin * phi.sin(), cos);
            let h = sub.add_atom(1, p + d * 1.48);
            sub.add_bond(si, h, BOND_SINGLE);
            sub.set_atom_frozen(h, true);
        }
    }
    (ads, sub)
}

pub(crate) fn network() -> (StructureDesigner, u64) {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let (ads, sub) = fixture();
    let mut value = |atoms: AtomicStructure| {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        add_value_node(
            network,
            DVec2::ZERO,
            NetworkResult::Molecule(MoleculeData {
                atoms,
                geo_tree_root: None,
            }),
        )
    };
    let a = value(ads);
    let s = value(sub);
    let node = designer.add_node("chemisorb", DVec2::new(200.0, 0.0));
    designer.connect_nodes(a, 0, node, 0);
    designer.connect_nodes(s, 0, node, 1);
    (designer, node)
}

fn refresh_selected(designer: &mut StructureDesigner, node: u64) {
    designer.select_node(node);
    designer.set_node_display(node, true);
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
}

#[test]
fn the_report_is_the_plan_before_run_and_the_result_after() {
    let (mut designer, node) = network();
    refresh_selected(&mut designer, node);
    let report = chemisorb_node_report(&designer).expect("report");
    assert!(!report.stats.searched);
    assert!(!report.stats.stale);
    assert_eq!(report.stats.to_relax, 2);
    assert!(report.rows.is_empty());

    designer.run_node_job_blocking(&[], node).unwrap();
    refresh_selected(&mut designer, node);
    let report = chemisorb_node_report(&designer).expect("report");
    assert!(report.stats.searched);
    assert_eq!(report.stats.relaxed, 2);
    assert_eq!(report.rows.len(), 2);
    assert_eq!(report.rows[0].rank, 1);
    assert_eq!(report.rows[0].bonds, "formed 1× O–Si");

    // A node of another type selected: no report.
    designer.select_node(1);
    assert!(chemisorb_node_report(&designer).is_none());
}

#[test]
fn the_setter_is_undoable_and_keeps_the_stored_search() {
    let (mut designer, node) = network();
    designer.run_node_job_blocking(&[], node).unwrap();

    let mut data = chemisorb_node_data(&designer, &[], node).unwrap();
    assert_eq!(data.reach, 3.5);
    data.top_n = 1;
    set_chemisorb_node_data(&mut designer, &[], node, &data);
    let stored = |d: &StructureDesigner| {
        d.get_node_network_data_scoped(&[], node)
            .unwrap()
            .as_any_ref()
            .downcast_ref::<ChemisorbData>()
            .unwrap()
            .stored
            .is_some()
    };
    assert_eq!(chemisorb_node_data(&designer, &[], node).unwrap().top_n, 1);
    assert!(stored(&designer), "a settings write keeps the search");

    refresh_selected(&mut designer, node);
    let report = chemisorb_node_report(&designer).unwrap();
    assert!(
        report.stats.stale && !report.stats.searched,
        "top_n is a search setting: the result goes stale"
    );

    assert!(designer.undo());
    assert_eq!(chemisorb_node_data(&designer, &[], node).unwrap().top_n, 10);
    assert!(stored(&designer));
    refresh_selected(&mut designer, node);
    let report = chemisorb_node_report(&designer).unwrap();
    assert!(report.stats.searched, "undo brings the stored result back");
    assert_eq!(report.rows.len(), 2);

    // A write addressed to a node of another type does nothing.
    let other = APIChemisorbData { reach: 9.0, ..data };
    set_chemisorb_node_data(&mut designer, &[], 1, &other);
    assert!(chemisorb_node_data(&designer, &[], 1).is_none());
}
