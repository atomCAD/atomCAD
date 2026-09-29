//! Library linking, Phase 4 (`doc/design_library_linking.md` §11, "Rust-side
//! interactions"): an automatic check that lands while an interaction the
//! undo coalescing treats as one open step is in progress holds the refresh
//! instead of splitting that step, and applies it once the interaction ends.
//!
//! One test per interaction Rust knows is open
//! (`StructureDesigner::open_interaction`). Each one: start the interaction,
//! change the library on disk, check → nothing happens (no command, wiring
//! and mounted content unchanged, an explicit refresh refused); end the
//! interaction → its own command is pushed; check → the refresh is applied as
//! a separate command, and undoing it leaves the interaction's command in
//! place.

use super::library_links_refresh_test::{Ws, edit_lib, incr, workspace};
use super::library_links_support::*;
use atomcad_structure_designer::library_links::mount_fingerprint;
use atomcad_structure_designer::nodes::atom_edit::atom_edit::{
    begin_atom_edit_drag, end_atom_edit_drag,
};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec2;

/// The wires of node `name` in `Main`, per argument, as source names.
fn wires_of_f(d: &StructureDesigner) -> Vec<Vec<String>> {
    let net = &d.node_type_registry.node_networks["Main"];
    let id = node_id(d, "Main", "f");
    net.nodes[&id]
        .arguments
        .iter()
        .map(|a| {
            a.incoming_wires
                .iter()
                .map(|w| {
                    net.nodes[&w.source_node_id]
                        .custom_name
                        .clone()
                        .unwrap_or_default()
                })
                .collect()
        })
        .collect()
}

/// Moves `lib.foo`'s `y` in front of `x` — a change that moves the host's
/// wires when (and only when) it is applied.
fn reorder_foo(ws: &Ws) {
    edit_lib(&ws.lib, |l| {
        incr(l, "foo", "y = parameter { sort_order: -1 }\n")
    });
}

/// Runs the hold scenario around one interaction. `begin` starts it (and
/// may change something, so that its end pushes a command); `end` ends it.
fn check_hold(
    begin: impl FnOnce(&mut StructureDesigner),
    end: impl FnOnce(&mut StructureDesigner),
    end_pushes: bool,
) {
    let ws = workspace();
    let mut d = open(&ws.host);
    d.set_active_node_network_name(Some("Main".to_string()));
    begin(&mut d);
    assert!(d.open_interaction().is_some(), "the interaction is open");

    reorder_foo(&ws);
    let pushes = d.undo_stack.push_count();
    let mount = mount_fingerprint(&d.node_type_registry, "lib");
    assert!(
        d.check_dependencies().is_none(),
        "a check during an interaction does nothing"
    );
    assert_eq!(d.undo_stack.push_count(), pushes, "no command pushed");
    assert_eq!(wires_of_f(&d), vec![vec!["i1"], vec!["i2"]]);
    assert_eq!(mount_fingerprint(&d.node_type_registry, "lib"), mount);
    assert!(
        d.refresh_library("lib").is_err(),
        "an explicit refresh is refused while the interaction is open"
    );
    assert_eq!(d.undo_stack.push_count(), pushes);

    end(&mut d);
    assert!(d.open_interaction().is_none(), "the interaction is closed");
    let after_end = d.undo_stack.push_count();
    assert_eq!(after_end, pushes + u64::from(end_pushes));
    let top = d.undo_stack.undo_description().map(str::to_string);

    let report = d
        .check_dependencies()
        .expect("applied after the interaction");
    assert!(report.held.is_empty(), "applied, not held: {:?}", report);
    assert_eq!(
        d.undo_stack.push_count(),
        after_end + 1,
        "one refresh command"
    );
    assert_eq!(wires_of_f(&d), vec![vec!["i2"], vec!["i1"]]);

    // The refresh is its own step: undoing it leaves the interaction's
    // command on the stack.
    assert!(d.undo());
    assert_eq!(wires_of_f(&d), vec![vec!["i1"], vec!["i2"]]);
    assert_eq!(d.undo_stack.undo_description().map(str::to_string), top);
}

#[test]
fn a_node_drag_holds_the_refresh() {
    check_hold(
        |d| {
            let f = node_id(d, "Main", "f");
            d.select_node(f);
            d.begin_move_nodes();
            d.move_selected_nodes(DVec2::new(40.0, 0.0));
        },
        |d| d.end_move_nodes(),
        true,
    );
}

#[test]
fn a_property_drag_holds_the_refresh() {
    check_hold(
        |d| {
            let i1 = node_id(d, "Main", "i1");
            d.begin_node_data_drag(vec![], i1);
        },
        |d| d.end_node_data_drag(),
        false,
    );
}

#[test]
fn a_body_resize_holds_the_refresh() {
    check_hold(
        |d| {
            let mp = node_id(d, "Main", "mp");
            d.begin_zone_resize(&[], mp);
        },
        |d| d.end_zone_resize(),
        false,
    );
}

#[test]
fn a_comment_edit_holds_the_refresh() {
    check_hold(
        |d| {
            let c = d.add_node("Comment", DVec2::new(0.0, 300.0));
            d.begin_comment_edit(vec![], c);
        },
        |d| d.end_comment_edit(),
        false,
    );
}

#[test]
fn a_gadget_drag_holds_the_refresh() {
    check_hold(
        |d| {
            let i1 = node_id(d, "Main", "i1");
            d.select_node(i1);
            d.begin_gadget_drag_snapshot();
        },
        |d| d.end_gadget_drag_snapshot(),
        false,
    );
}

#[test]
fn an_atom_drag_holds_the_refresh() {
    check_hold(
        |d| {
            let ae = d.add_node("atom_edit", DVec2::new(0.0, 300.0));
            d.select_node(ae);
            begin_atom_edit_drag(d);
        },
        |d| end_atom_edit_drag(d),
        false,
    );
}

// ---------------------------------------------------------------------------
// `mount_containing` — the prefix test of D3. Flutter's `mountFor`
// (`lib/structure_designer/namespace_utils.dart`) is its twin, and
// `test/library_links_test.dart` mirrors this table case for case: change
// one, change both.
// ---------------------------------------------------------------------------

use atomcad_structure_designer::library_links::{
    LibraryLinks, LibraryMount, MountStatus, UsedInterfaces,
};

fn mount(path: &str) -> LibraryMount {
    LibraryMount {
        mount_path: path.to_string(),
        alias: path.to_string(),
        rel_path: format!("{}.cnnd", path),
        abs_path: std::path::PathBuf::from(format!("{}.cnnd", path)),
        parent: None,
        status: MountStatus::Loaded,
        loaded: None,
        last_seen: None,
        stored_hash: None,
        stored_uses: UsedInterfaces::default(),
        name_only_param_ids: Default::default(),
    }
}

/// `(mounts, name, expected owner)`.
pub const MOUNT_FOR_CASES: &[(&[&str], &str, Option<&str>)] = &[
    (&[], "demolib.foo", None),
    (&["demolib"], "demolib", Some("demolib")),
    (&["demolib"], "demolib.foo", Some("demolib")),
    (&["demolib"], "demolib.shapes.slab", Some("demolib")),
    (&["demolib"], "demolibx", None),
    (&["demolib"], "demolib_extra.foo", None),
    (&["demolib"], "Main", None),
    (&["demolib"], "demo", None),
    (
        &["demolib", "demolib.common"],
        "demolib.common.slab",
        Some("demolib.common"),
    ),
    (
        &["demolib", "demolib.common"],
        "demolib.commonx",
        Some("demolib"),
    ),
    (
        &["demolib", "demolib.common"],
        "demolib.common",
        Some("demolib.common"),
    ),
    (
        &["libs.demolib", "libs.other"],
        "libs.demolib.foo",
        Some("libs.demolib"),
    ),
    (
        &["libs.demolib", "libs.other"],
        "libs.other",
        Some("libs.other"),
    ),
    (&["libs.demolib", "libs.other"], "libs", None),
    (&["libs.demolib", "libs.other"], "libs.local", None),
];

#[test]
fn mount_containing_is_the_longest_dotted_prefix() {
    for (mounts, name, expected) in MOUNT_FOR_CASES {
        let mut links = LibraryLinks::default();
        for m in *mounts {
            links.insert(mount(m));
        }
        assert_eq!(
            links.mount_containing(name).map(|m| m.mount_path.as_str()),
            *expected,
            "mounts {:?}, name {}",
            mounts,
            name
        );
    }
}
