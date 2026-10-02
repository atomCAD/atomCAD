//! Back/forward across tabs: the session's navigation history driven through
//! `DocumentSet`, the way the FFI wrappers drive it (minus the renderer).
//! The history list on its own is in `navigation_history_test.rs`.

use super::library_links_refresh_test::{bump_mtime, incr, workspace};
use super::library_links_support::*;
use atomcad_structure_designer::document_set::{
    DocumentId, DocumentSet, NavigateOutcome, SwitchRefused,
};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec2;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct Tabs {
    set: DocumentSet,
    active: StructureDesigner,
}

type Place = (DocumentId, Option<String>);

impl Tabs {
    /// The state at application start: one pristine Untitled document.
    fn new() -> Self {
        let mut active = StructureDesigner::new();
        active.new_project_direct_editing();
        let set = DocumentSet::new(&mut active);
        Tabs { set, active }
    }

    fn open(&mut self, path: &Path) -> DocumentId {
        self.set
            .open(&mut self.active, &path.to_string_lossy())
            .unwrap_or_else(|e| panic!("open {}: {}", path.display(), e))
            .id
    }

    fn activate(&mut self, id: DocumentId) {
        self.set.activate(&mut self.active, id).expect("activate");
    }

    /// Clicks network `name` in the active document's panel.
    fn go(&mut self, name: &str) {
        self.active
            .set_active_node_network_name(Some(name.to_string()));
    }

    fn back(&mut self) -> NavigateOutcome {
        self.set.navigate_back(&mut self.active).expect("back")
    }

    fn forward(&mut self) -> NavigateOutcome {
        self.set
            .navigate_forward(&mut self.active)
            .expect("forward")
    }

    fn can_back(&self) -> bool {
        self.set.can_navigate_back(&self.active)
    }

    fn can_forward(&self) -> bool {
        self.set.can_navigate_forward(&self.active)
    }

    /// Where the user is: the active tab and its network.
    fn place(&self) -> Place {
        (
            self.set.active_id(),
            self.active.active_node_network_name.clone(),
        )
    }

    /// The history, as `id:network` strings.
    fn history(&self) -> Vec<String> {
        self.active
            .navigation_history()
            .entries()
            .iter()
            .map(|e| format!("{}:{}", e.document, e.network.as_deref().unwrap_or("-")))
            .collect()
    }

    fn has_visits_to(&self, id: DocumentId) -> bool {
        self.active
            .navigation_history()
            .entries()
            .iter()
            .any(|e| e.document == id)
    }
}

fn place(id: DocumentId, network: &str) -> Place {
    (id, Some(network.to_string()))
}

const SIMPLE: &str = "a = int { value: 1 }
b = expr { a: a, expression: \"a + 1\", parameters: [{ name: \"a\", data_type: Int }] }
output b
";

/// A saved design at `dir/name` with a `Main` and the networks `extra`, all
/// with the same small content, and `Main` active.
fn design(dir: &Path, name: &str, extra: &[&str]) -> PathBuf {
    let path = dir.join(name);
    let mut d = new_design(&path);
    edit(&mut d, "Main", SIMPLE);
    for network in extra {
        edit(&mut d, network, SIMPLE);
    }
    d.set_active_node_network_name(Some("Main".to_string()));
    save(&mut d);
    path
}

/// Two open documents, `a` (Main, Na) and `b` (Main, Nb), visited
/// a:Main → a:Na → b:Main → b:Nb. Returns (tabs, a, b, dir).
fn two_tabs() -> (Tabs, DocumentId, DocumentId, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let a_path = design(dir.path(), "a.cnnd", &["Na"]);
    let b_path = design(dir.path(), "b.cnnd", &["Nb"]);
    let mut tabs = Tabs::new();
    let a = tabs.open(&a_path);
    tabs.go("Main");
    tabs.go("Na");
    let b = tabs.open(&b_path);
    tabs.go("Main");
    tabs.go("Nb");
    (tabs, a, b, dir)
}

// ---------------------------------------------------------------------------
// Back and forward cross tabs
// ---------------------------------------------------------------------------

#[test]
fn at_start_there_is_nowhere_to_go() {
    let mut tabs = Tabs::new();
    assert_eq!(tabs.history().len(), 1, "the start tab is recorded");
    assert!(!tabs.can_back());
    assert!(!tabs.can_forward());
    let outcome = tabs.back();
    assert!(!outcome.moved);
    assert!(outcome.activation_report.is_none());
}

#[test]
fn back_and_forward_lead_through_tabs() {
    let (mut tabs, a, b, _dir) = two_tabs();
    assert_eq!(tabs.place(), place(b, "Nb"));

    assert!(tabs.back().moved);
    assert_eq!(tabs.place(), place(b, "Main"));
    assert!(tabs.back().moved);
    assert_eq!(tabs.place(), place(a, "Na"), "into the other tab");
    assert!(tabs.back().moved);
    assert_eq!(tabs.place(), place(a, "Main"));
    assert!(
        !tabs.can_back(),
        "the pristine start tab was replaced, and forgotten"
    );

    assert!(tabs.forward().moved);
    assert!(tabs.forward().moved);
    assert_eq!(tabs.place(), place(b, "Main"), "back into the first tab");
    assert!(tabs.forward().moved);
    assert_eq!(tabs.place(), place(b, "Nb"));
    assert!(!tabs.can_forward());
}

#[test]
fn moving_through_the_history_records_nothing() {
    let (mut tabs, _a, _b, _dir) = two_tabs();
    let before = tabs.history();
    tabs.back();
    tabs.back();
    tabs.forward();
    assert_eq!(tabs.history(), before);
}

#[test]
fn the_history_follows_the_session_and_is_not_copied() {
    let (mut tabs, a, b, _dir) = two_tabs();
    tabs.back();
    tabs.back(); // now in a
    assert_eq!(tabs.set.active_id(), a);
    assert!(
        tabs.set
            .parked(b)
            .unwrap()
            .navigation_history()
            .entries()
            .is_empty(),
        "the parked document handed its history over"
    );
    assert!(tabs.has_visits_to(b));
}

#[test]
fn a_tab_switch_is_a_step() {
    let (mut tabs, a, b, _dir) = two_tabs();
    tabs.activate(a);
    assert_eq!(tabs.place(), place(a, "Na"), "a remembers its network");
    tabs.back();
    assert_eq!(tabs.place(), place(b, "Nb"));
}

#[test]
fn a_visit_after_going_back_drops_the_forward_history_in_every_tab() {
    let (mut tabs, a, _b, _dir) = two_tabs();
    tabs.back();
    tabs.back();
    tabs.back(); // a:Main
    tabs.go("Na");
    assert!(!tabs.can_forward());
    assert_eq!(tabs.history(), [format!("{a}:Main"), format!("{a}:Na")]);
}

#[test]
fn a_step_into_another_tab_clears_its_record_def_and_shows_the_network() {
    let (mut tabs, a, b, _dir) = two_tabs();
    tabs.activate(a);
    tabs.active
        .add_record_type_def(int_record("R", &["x"]))
        .unwrap();
    tabs.active
        .set_active_record_def_name(Some("R".to_string()));
    tabs.activate(b);
    tabs.back(); // into a, at the network it showed when left
    assert_eq!(tabs.place(), place(a, "Na"));
    assert_eq!(tabs.active.get_active_record_def_name(), None);
}

// ---------------------------------------------------------------------------
// Upkeep
// ---------------------------------------------------------------------------

#[test]
fn closing_a_parked_tab_forgets_its_visits() {
    let (mut tabs, a, b, _dir) = two_tabs();
    tabs.activate(a);
    tabs.set.close(&mut tabs.active, b).unwrap();
    assert!(!tabs.has_visits_to(b));
    tabs.back();
    assert_eq!(tabs.place(), place(a, "Main"), "b's visits are gone");
    assert!(!tabs.can_back());
}

#[test]
fn closing_the_active_tab_records_the_neighbour_and_forgets_the_closed_one() {
    let (mut tabs, a, b, _dir) = two_tabs();
    tabs.set.close(&mut tabs.active, b).unwrap();
    assert_eq!(tabs.place(), place(a, "Na"));
    assert!(!tabs.has_visits_to(b));
    assert_eq!(tabs.history(), [format!("{a}:Main"), format!("{a}:Na")]);
}

#[test]
fn an_in_place_load_forgets_the_old_content_and_records_the_new() {
    let (mut tabs, a, b, dir) = two_tabs();
    let c_path = design(dir.path(), "c.cnnd", &[]);
    tabs.set
        .load_in_place(&mut tabs.active, &c_path.to_string_lossy())
        .unwrap();
    let c = tabs.set.active_id();
    assert_ne!(c, b);
    assert!(!tabs.has_visits_to(b));
    assert_eq!(
        tabs.active
            .navigation_history()
            .current()
            .map(|e| e.document),
        Some(c)
    );
    tabs.back();
    assert_eq!(tabs.place(), place(a, "Na"));
}

#[test]
fn a_rename_is_followed_in_its_own_document_only() {
    let (mut tabs, a, b, _dir) = two_tabs();
    tabs.back();
    tabs.back(); // a:Na
    assert!(tabs.active.rename_node_network("Main", "Start"));
    assert_eq!(
        tabs.history(),
        [
            format!("{a}:Start"),
            format!("{a}:Na"),
            format!("{b}:Main"),
            format!("{b}:Nb"),
        ],
        "b's Main is another network"
    );
    tabs.back();
    assert_eq!(tabs.place(), place(a, "Start"));
}

#[test]
fn a_deleted_network_is_forgotten() {
    let (mut tabs, a, b, _dir) = two_tabs();
    tabs.activate(a);
    tabs.go("Main");
    tabs.active.delete_node_network("Na").unwrap();
    assert!(!tabs.history().contains(&format!("{a}:Na")));
    tabs.back();
    assert_eq!(tabs.place(), place(b, "Nb"));
}

#[test]
fn a_stale_entry_is_stepped_over() {
    // Undo puts a renamed network's old name back, but cannot reach the
    // history: the entry names a network that no longer exists.
    let (mut tabs, a, b, _dir) = two_tabs();
    tabs.activate(a);
    assert!(tabs.active.rename_node_network("Na", "Nc"));
    tabs.go("Main");
    assert!(tabs.active.undo());
    assert!(
        !tabs
            .active
            .node_type_registry
            .node_networks
            .contains_key("Nc")
    );
    assert!(tabs.history().contains(&format!("{a}:Nc")));

    tabs.activate(b);
    tabs.back();
    assert_eq!(tabs.place(), place(a, "Main"));
    tabs.back();
    assert_eq!(tabs.place(), place(b, "Nb"), "a:Nc was stepped over");
}

// ---------------------------------------------------------------------------
// Open in library file, refusals, and the activation report
// ---------------------------------------------------------------------------

#[test]
fn opening_at_a_network_is_one_step() {
    let ws = workspace();
    let mut tabs = Tabs::new();
    let host = tabs.open(&ws.host);
    tabs.go("Main");
    let lib = tabs
        .set
        .open_at(&mut tabs.active, &ws.lib.to_string_lossy(), Some("split"))
        .unwrap()
        .id;
    assert_eq!(tabs.place(), place(lib, "split"));
    assert_eq!(
        tabs.history(),
        [format!("{host}:Main"), format!("{lib}:split")]
    );
    tabs.back();
    assert_eq!(tabs.place(), place(host, "Main"));

    // Already open: the same, through `activate_at`.
    tabs.set
        .open_at(&mut tabs.active, &ws.lib.to_string_lossy(), Some("bar"))
        .unwrap();
    assert_eq!(tabs.place(), place(lib, "bar"));
    tabs.back();
    assert_eq!(tabs.place(), place(host, "Main"));
}

#[test]
fn opening_at_a_missing_network_shows_the_documents_own() {
    let ws = workspace();
    let mut tabs = Tabs::new();
    tabs.open(&ws.host);
    let lib = tabs
        .set
        .open_at(&mut tabs.active, &ws.lib.to_string_lossy(), Some("nope"))
        .unwrap()
        .id;
    let shown = tabs.active.active_node_network_name.clone();
    assert_ne!(shown.as_deref(), Some("nope"));
    assert_eq!(
        tabs.active.navigation_history().current().unwrap().network,
        shown
    );
    assert_eq!(tabs.set.active_id(), lib);
}

#[test]
fn a_switching_step_is_refused_during_an_interaction_and_changes_nothing() {
    let (mut tabs, a, b, _dir) = two_tabs();
    tabs.back(); // b:Main, where the nodes are
    assert_eq!(tabs.place(), place(b, "Main"));
    let node = node_id(&tabs.active, "Main", "a");
    assert!(tabs.active.select_node(node));
    tabs.active.begin_move_nodes();
    tabs.active.move_selected_nodes(DVec2::new(40.0, 0.0));
    let what = tabs.active.open_interaction().expect("a drag is open");
    let before = (
        tabs.place(),
        tabs.active.navigation_history().current().cloned(),
    );

    let refused = tabs.set.navigate_back(&mut tabs.active);
    assert_eq!(refused.err(), Some(SwitchRefused::OpenInteraction(what)));
    assert_eq!(
        (
            tabs.place(),
            tabs.active.navigation_history().current().cloned()
        ),
        before,
        "neither the tab nor the history moved"
    );

    tabs.active.end_move_nodes();
    tabs.back();
    assert_eq!(tabs.place(), place(a, "Na"));
}

#[test]
fn a_step_into_a_host_applies_a_library_saved_meanwhile() {
    let ws = workspace();
    let mut tabs = Tabs::new();
    let host = tabs.open(&ws.host);
    tabs.go("Main");
    let lib = tabs
        .set
        .open_at(&mut tabs.active, &ws.lib.to_string_lossy(), Some("foo"))
        .unwrap()
        .id;
    let host_pushes = tabs.set.parked(host).unwrap().undo_stack.push_count();

    incr(
        &mut tabs.active,
        "foo",
        "y = parameter { sort_order: -1 }\n",
    );
    save(&mut tabs.active);
    bump_mtime(&ws.lib);
    assert_eq!(tabs.place(), place(lib, "foo"));

    let outcome = tabs.back();
    assert!(outcome.moved);
    assert_eq!(tabs.place(), place(host, "Main"));
    let report = outcome.activation_report.expect("the activation's report");
    assert_eq!(report.refreshed_mounts.len(), 1, "{report:?}");
    assert_eq!(
        tabs.active.undo_stack.push_count(),
        host_pushes + 1,
        "applied as one undo step in the host"
    );
}

// ---------------------------------------------------------------------------
// Without a DocumentSet (headless)
// ---------------------------------------------------------------------------

#[test]
fn a_bare_designer_navigates_within_itself() {
    let mut d = StructureDesigner::new();
    d.new_project();
    d.add_node_network("N2");
    d.set_active_node_network_name(Some("Main".to_string()));
    d.set_active_node_network_name(Some("N2".to_string()));
    d.add_record_type_def(int_record("R", &["x"])).unwrap();
    d.set_active_record_def_name(Some("R".to_string()));

    assert!(d.can_navigate_back());
    let (moved, _) = d.navigate_back();
    assert!(moved);
    assert_eq!(d.active_node_network_name.as_deref(), Some("Main"));
    assert_eq!(d.get_active_record_def_name(), None);
    assert!(d.can_navigate_forward());
    let (moved, _) = d.navigate_forward();
    assert!(moved);
    assert_eq!(d.active_node_network_name.as_deref(), Some("N2"));

    // A deleted network's visits are gone.
    d.delete_node_network("Main").unwrap();
    assert!(!d.can_navigate_back());
    assert_eq!(d.navigate_back(), (false, None));
}
