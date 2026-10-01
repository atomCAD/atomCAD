//! Multiple open documents, Phase 1 (`doc/design_multiple_documents.md` §9):
//! `DocumentSet`, the swap, and the document rules behind the FFI wrappers.
//!
//! A test "activates" the way the wrapper does minus the renderer:
//! `DocumentSet::activate`, then `refresh` with the pending changes. Every
//! file a test writes is in a temp dir.

use super::library_links_interaction_test::{
    a_body_resize, a_comment_edit, a_gadget_drag, a_node_drag, a_property_drag, an_atom_drag,
};
use super::library_links_refresh_test::{bump_mtime, incr, workspace};
use super::library_links_support::*;
use super::mechanosynth_edit_test::{editor_with_block, setup_designer};
use atomcad_display::gadget::GadgetPickContext;
use atomcad_structure_designer::camera_settings::CameraSettings;
use atomcad_structure_designer::canvas_viewport::CanvasViewport;
use atomcad_structure_designer::document_set::{
    DocumentId, DocumentSet, DocumentTab, OpenError, OpenOutcome, SwitchRefused, UNTITLED,
};
use atomcad_structure_designer::evaluator::eval_profiler::SelfCheckKeyMode;
use atomcad_structure_designer::evaluator::network_evaluator::PrintLogEntry;
use atomcad_structure_designer::library_refresh::RefreshReport;
use atomcad_structure_designer::mechanosynth_edit_ops::StepMetadataField;
use atomcad_structure_designer::refresh_profile::{RefreshProfile, RefreshSubPhases};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::structure_designer_changes::RefreshMode;
use glam::f64::DVec3;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// The harness: a `DocumentSet` and the active slot, as `CADInstance` has them
// ---------------------------------------------------------------------------

struct Tabs {
    set: DocumentSet,
    active: StructureDesigner,
}

impl Tabs {
    /// The state at application start: one pristine Untitled document.
    fn new() -> Self {
        let mut active = StructureDesigner::new();
        active.new_project_direct_editing();
        Self::with(active)
    }

    fn with(mut active: StructureDesigner) -> Self {
        let set = DocumentSet::new(&mut active);
        Tabs { set, active }
    }

    fn refresh(&mut self) {
        let changes = self.active.get_pending_changes();
        self.active.refresh(&changes);
    }

    fn open(&mut self, path: &Path) -> OpenOutcome {
        let outcome = self
            .set
            .open(&mut self.active, &path.to_string_lossy())
            .unwrap_or_else(|e| panic!("open {}: {}", path.display(), e));
        self.refresh();
        outcome
    }

    fn activate(&mut self, id: DocumentId) -> Option<RefreshReport> {
        let report = self.set.activate(&mut self.active, id).expect("activate");
        self.refresh();
        report
    }

    fn active_id(&self) -> DocumentId {
        self.set.active_id()
    }

    fn doc(&self, id: DocumentId) -> &StructureDesigner {
        if id == self.active_id() {
            &self.active
        } else {
            self.set.parked(id).expect("an open document")
        }
    }

    fn doc_mut(&mut self, id: DocumentId) -> &mut StructureDesigner {
        if id == self.active_id() {
            &mut self.active
        } else {
            self.set.parked_mut(id).expect("an open document")
        }
    }

    fn tabs(&self) -> Vec<DocumentTab> {
        self.set.tabs(&self.active)
    }
}

const SIMPLE: &str = "a = int { value: 1 }
b = expr { a: a, expression: \"a + 1\", parameters: [{ name: \"a\", data_type: Int }] }
output b
";

/// A saved design at `dir/name` whose `Main` is `code`.
fn design(dir: &Path, name: &str, code: &str) -> PathBuf {
    let path = dir.join(name);
    let mut d = new_design(&path);
    edit(&mut d, "Main", code);
    save(&mut d);
    path
}

/// Serialized form, dirty flag and undo state: what "unchanged" means for a
/// document.
fn state_of(d: &mut StructureDesigner) -> (String, bool, u64, bool, bool) {
    let dir = d
        .file_path
        .as_ref()
        .map(|p| Path::new(p).parent().unwrap().to_path_buf());
    (
        saved_text(d, dir.as_deref()),
        d.is_dirty,
        d.undo_stack.push_count(),
        d.undo_stack.can_undo(),
        d.undo_stack.can_redo(),
    )
}

fn node_names(d: &StructureDesigner, network: &str) -> Vec<String> {
    let mut names: Vec<String> = d.node_type_registry.node_networks[network]
        .nodes
        .values()
        .filter_map(|n| n.custom_name.clone())
        .collect();
    names.sort();
    names
}

// ---------------------------------------------------------------------------
// Isolation
// ---------------------------------------------------------------------------

#[test]
fn each_document_keeps_its_registry_dirty_flag_and_undo_history() {
    let dir = tempfile::tempdir().unwrap();
    let a_path = design(dir.path(), "a.cnnd", SIMPLE);
    let b_path = design(dir.path(), "b.cnnd", SIMPLE);
    let mut tabs = Tabs::new();
    let a = tabs.open(&a_path).id;
    let b = tabs.open(&b_path).id;

    // Both have a `Main` with the same node names; edit each differently.
    incr(&mut tabs.active, "Main", "only_b = int { value: 2 }\n");
    tabs.activate(a);
    incr(&mut tabs.active, "Main", "only_a = int { value: 3 }\n");
    incr(&mut tabs.active, "Main", "only_a2 = int { value: 4 }\n");
    tabs.activate(b);

    assert!(node_names(tabs.doc(a), "Main").contains(&"only_a".to_string()));
    assert!(!node_names(tabs.doc(a), "Main").contains(&"only_b".to_string()));
    assert!(node_names(tabs.doc(b), "Main").contains(&"only_b".to_string()));
    assert!(!node_names(tabs.doc(b), "Main").contains(&"only_a".to_string()));
    assert!(tabs.doc(a).is_dirty && tabs.doc(b).is_dirty);

    // Undo in B touches B only.
    let a_before = state_of(tabs.doc_mut(a));
    assert!(tabs.active.undo());
    assert!(!node_names(&tabs.active, "Main").contains(&"only_b".to_string()));
    assert!(!tabs.active.undo(), "B has exactly one step");
    assert_eq!(state_of(tabs.doc_mut(a)), a_before);

    // A's two steps and B's redo are where they were.
    tabs.activate(a);
    assert!(tabs.active.undo());
    assert!(tabs.active.undo());
    assert!(!tabs.active.undo());
    tabs.activate(b);
    assert!(tabs.active.undo_stack.can_redo());
    assert!(tabs.active.redo());
    assert!(node_names(&tabs.active, "Main").contains(&"only_b".to_string()));
}

#[test]
fn a_switch_changes_nothing_by_itself() {
    let dir = tempfile::tempdir().unwrap();
    let a_path = design(dir.path(), "a.cnnd", SIMPLE);
    let b_path = design(dir.path(), "b.cnnd", SIMPLE);
    let mut tabs = Tabs::new();
    let a = tabs.open(&a_path).id;
    let b = tabs.open(&b_path).id;
    // One unsaved edit in A, so "dirty" and "undoable" are both exercised.
    tabs.activate(a);
    incr(&mut tabs.active, "Main", "c = int { value: 7 }\n");
    let a_before = state_of(tabs.doc_mut(a));
    let b_before = state_of(tabs.doc_mut(b));

    assert!(tabs.activate(b).is_none(), "nothing to report");
    assert!(tabs.activate(a).is_none());
    tabs.activate(b);
    tabs.activate(a);

    assert_eq!(state_of(tabs.doc_mut(a)), a_before);
    assert_eq!(state_of(tabs.doc_mut(b)), b_before);
}

#[test]
fn activating_the_active_document_does_nothing() {
    let mut tabs = Tabs::new();
    let id = tabs.active_id();
    let pushes = tabs.active.undo_stack.push_count();
    assert_eq!(tabs.set.activate(&mut tabs.active, id), Ok(None));
    assert_eq!(tabs.active_id(), id);
    assert_eq!(tabs.active.undo_stack.push_count(), pushes);
}

#[test]
fn an_unknown_id_is_refused() {
    let mut tabs = Tabs::new();
    let before = tabs.tabs();
    assert_eq!(
        tabs.set.activate(&mut tabs.active, DocumentId(99)),
        Err(SwitchRefused::UnknownDocument(DocumentId(99)))
    );
    assert_eq!(
        tabs.set.close(&mut tabs.active, DocumentId(99)),
        Err(SwitchRefused::UnknownDocument(DocumentId(99)))
    );
    assert_eq!(tabs.tabs(), before);
}

// ---------------------------------------------------------------------------
// App state follows the session; document state stays (§6)
// ---------------------------------------------------------------------------

fn print_entry(text: &str) -> PrintLogEntry {
    PrintLogEntry {
        timestamp: std::time::SystemTime::now(),
        network_name: "Main".to_string(),
        node_id: 1,
        node_label: "p".to_string(),
        text: text.to_string(),
        from_execute: false,
    }
}

#[test]
fn app_state_follows_the_session() {
    let mut tabs = Tabs::new();
    let a = tabs.active_id();
    let b = tabs.set.new_document(&mut tabs.active, false).unwrap();
    tabs.activate(a);

    // Every app-state row of §6, set to a non-default value in A.
    let mut prefs = tabs.active.preferences.clone();
    prefs.memory_preferences.invisible_node_cache_mb += 17;
    prefs.layout_preferences.respect_hand_moved_in_reflow =
        !prefs.layout_preferences.respect_hand_moved_in_reflow;
    tabs.active.set_preferences(prefs);
    tabs.active.print_log.push(print_entry("hello"));
    tabs.active.record_refresh_profile(RefreshProfile::new(
        RefreshMode::Full,
        RefreshSubPhases::default(),
        1.0,
        2.0,
        None,
        4.0,
    ));
    let profiles = tabs.active.refresh_profiles.len();
    tabs.active.eval_profiling_enabled = true;
    tabs.active.eval_self_check_enabled = true;
    tabs.active.eval_self_check_key_mode = SelfCheckKeyMode::OmitDecorate;
    tabs.active.eval_memo_enabled = false;
    tabs.active.gadget_pick_context = GadgetPickContext {
        eye: DVec3::new(1.0, 2.0, 3.0),
        perspective_world_per_pixel: 0.25,
        ortho_world_per_pixel: 0.5,
        orthographic: true,
    };
    let pick = format!("{:?}", tabs.active.gadget_pick_context);
    let mb = tabs
        .active
        .preferences
        .memory_preferences
        .invisible_node_cache_mb;
    let hand = tabs
        .active
        .preferences
        .layout_preferences
        .respect_hand_moved_in_reflow;

    tabs.activate(b);
    let d = &tabs.active;
    assert_eq!(d.preferences.memory_preferences.invisible_node_cache_mb, mb);
    assert_eq!(
        d.preferences
            .layout_preferences
            .respect_hand_moved_in_reflow,
        hand
    );
    assert_eq!(d.print_log.len(), 1);
    assert_eq!(d.print_log[0].text, "hello");
    // The API layer records a refresh's profile; the domain refresh does not.
    assert_eq!(d.refresh_profiles.len(), profiles);
    assert!(d.eval_profiling_enabled);
    assert!(d.eval_self_check_enabled);
    assert_eq!(d.eval_self_check_key_mode, SelfCheckKeyMode::OmitDecorate);
    assert!(!d.eval_memo_enabled);
    assert_eq!(format!("{:?}", d.gadget_pick_context), pick);

    // The moved rows are gone from the parked A.
    let parked = tabs.set.parked(a).unwrap();
    assert!(parked.print_log.is_empty());
    assert!(parked.refresh_profiles.is_empty());
}

#[test]
fn document_state_stays_with_its_document() {
    let dir = tempfile::tempdir().unwrap();
    let a_path = design(dir.path(), "a.cnnd", SIMPLE);
    let b_path = design(dir.path(), "b.cnnd", SIMPLE);
    let mut tabs = Tabs::new();
    let a = tabs.open(&a_path).id;
    let b = tabs.open(&b_path).id;

    // Distinct values for the document rows of §6, B first.
    tabs.active.add_node_network("Other");
    tabs.active
        .set_active_node_network_name(Some("Other".to_string()));
    tabs.active.direct_editing_mode = true;
    tabs.active
        .cli_access_rules
        .insert("Other".to_string(), false);
    tabs.active
        .eval_error_snapshots
        .insert("Other".to_string(), Vec::new());

    tabs.activate(a);
    tabs.active
        .set_active_node_network_name(Some("Main".to_string()));
    tabs.active.direct_editing_mode = false;
    let a_node = node_id(&tabs.active, "Main", "a");
    tabs.active.select_node(a_node);
    assert!(tabs.active.copy_selection());
    incr(&mut tabs.active, "Main", "c = int { value: 7 }\n");

    for _ in 0..2 {
        tabs.activate(b);
        tabs.activate(a);
    }

    let da = tabs.doc(a);
    let db = tabs.doc(b);
    assert_eq!(da.file_path.as_deref(), Some(&*a_path.to_string_lossy()));
    assert_eq!(db.file_path.as_deref(), Some(&*b_path.to_string_lossy()));
    assert_eq!(da.active_node_network_name.as_deref(), Some("Main"));
    assert_eq!(db.active_node_network_name.as_deref(), Some("Other"));
    assert!(!da.direct_editing_mode);
    assert!(db.direct_editing_mode);
    assert!(da.cli_access_rules.is_empty());
    assert_eq!(db.cli_access_rules.get("Other"), Some(&false));
    assert!(db.eval_error_snapshots.contains_key("Other"));
    assert!(!da.eval_error_snapshots.contains_key("Other"));
    // P1: the clipboard is still document state (D9 moves it in P2).
    assert!(da.clipboard.is_some());
    assert!(db.clipboard.is_none());
    assert!(da.is_dirty);
    assert!(da.undo_stack.can_undo());
    assert_eq!(da.document_id, a);
    assert_eq!(db.document_id, b);
}

#[test]
fn per_network_view_state_survives_switches() {
    let dir = tempfile::tempdir().unwrap();
    let a_path = design(dir.path(), "a.cnnd", SIMPLE);
    let b_path = design(dir.path(), "b.cnnd", SIMPLE);
    let mut tabs = Tabs::new();
    let a = tabs.open(&a_path).id;
    let b = tabs.open(&b_path).id;

    let view = |x: f64| {
        (
            CameraSettings {
                eye: DVec3::new(x, 1.0, 2.0),
                target: DVec3::ZERO,
                up: DVec3::Z,
                orthographic: x > 5.0,
                ortho_half_height: x,
                pivot_point: DVec3::new(0.0, x, 0.0),
                nav_up: DVec3::Z,
                nav_up_label: String::new(),
            },
            CanvasViewport {
                pan_x: x,
                pan_y: -x,
                zoom_level: (x as i32) % 3,
            },
        )
    };
    let set_view = |d: &mut StructureDesigner, x: f64| {
        let (camera, canvas) = view(x);
        let net = d.node_type_registry.node_networks.get_mut("Main").unwrap();
        net.camera_settings = Some(camera);
        net.canvas_viewport = Some(canvas);
    };
    let get_view = |d: &StructureDesigner| {
        let net = &d.node_type_registry.node_networks["Main"];
        (
            net.camera_settings.clone().unwrap(),
            net.canvas_viewport.clone().unwrap(),
        )
    };
    set_view(&mut tabs.active, 3.0);
    tabs.activate(a);
    set_view(&mut tabs.active, 7.0);
    for _ in 0..2 {
        tabs.activate(b);
        tabs.activate(a);
    }
    assert_eq!(get_view(tabs.doc(a)), view(7.0));
    assert_eq!(get_view(tabs.doc(b)), view(3.0));
    assert_eq!(
        tabs.active.active_network_camera_settings(),
        Some(view(7.0).0)
    );
}

// ---------------------------------------------------------------------------
// One document per file (D5)
// ---------------------------------------------------------------------------

#[test]
fn opening_an_open_file_activates_its_tab_in_any_spelling() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("sub")).unwrap();
    let a_path = design(dir.path(), "a.cnnd", SIMPLE);
    let b_path = design(dir.path(), "b.cnnd", SIMPLE);
    let mut tabs = Tabs::new();
    let a = tabs.open(&a_path).id;
    tabs.active.set_dirty(true); // an unsaved change that a reload would lose
    let b = tabs.open(&b_path).id;

    let dotted = dir.path().join("sub").join("..").join("a.cnnd");
    let outcome = tabs.open(&dotted);
    assert!(outcome.already_open);
    assert_eq!(outcome.id, a);
    assert_eq!(tabs.active_id(), a);
    assert!(tabs.active.is_dirty, "not reloaded");
    assert_eq!(tabs.set.len(), 2);

    if cfg!(windows) {
        let upper = PathBuf::from(b_path.to_string_lossy().to_uppercase());
        let outcome = tabs.open(&upper);
        assert!(outcome.already_open);
        assert_eq!(outcome.id, b);
        assert_eq!(tabs.set.len(), 2);
    }

    // Opening the active document's own file is a no-op switch.
    let outcome = tabs.open(&b_path);
    assert!(outcome.already_open);
    assert_eq!(outcome.id, b);
}

#[test]
fn save_as_onto_a_path_open_in_another_tab_is_refused_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let a_path = design(dir.path(), "a.cnnd", SIMPLE);
    let b_path = design(dir.path(), "b.cnnd", "x = int { value: 9 }\noutput x\n");
    let mut tabs = Tabs::new();
    let a = tabs.open(&a_path).id;
    tabs.open(&b_path);
    let b_bytes = std::fs::read(&b_path).unwrap();

    tabs.activate(a);
    let target = b_path.to_string_lossy().to_string();
    assert!(
        tabs.set
            .path_open_elsewhere(&tabs.active, &target)
            .is_some()
    );
    let err = tabs.set.save_as(&mut tabs.active, &target).unwrap_err();
    assert!(
        err.contains("b.cnnd") && err.contains("open in another tab"),
        "{err}"
    );
    let err = tabs
        .set
        .save_as_with_dependencies(&mut tabs.active, &target, true, &[])
        .unwrap_err();
    assert!(err.contains("open in another tab"), "{err}");
    assert_eq!(std::fs::read(&b_path).unwrap(), b_bytes);
    assert_eq!(
        tabs.active.file_path.as_deref(),
        Some(&*a_path.to_string_lossy())
    );

    // Saving onto its own path, or a new one, is not refused.
    let own = a_path.to_string_lossy().to_string();
    assert!(tabs.set.path_open_elsewhere(&tabs.active, &own).is_none());
    tabs.set.save_as(&mut tabs.active, &own).unwrap();
    let fresh = dir.path().join("c.cnnd").to_string_lossy().to_string();
    tabs.set.save_as(&mut tabs.active, &fresh).unwrap();
    assert!(Path::new(&fresh).exists());
}

// ---------------------------------------------------------------------------
// In-place replacement (the D8 backstop)
// ---------------------------------------------------------------------------

#[test]
fn an_in_place_load_or_new_project_renumbers_the_active_document() {
    let dir = tempfile::tempdir().unwrap();
    let a_path = design(dir.path(), "a.cnnd", SIMPLE);
    let mut tabs = Tabs::new();
    let first = tabs.active_id();

    tabs.set
        .load_in_place(&mut tabs.active, &a_path.to_string_lossy())
        .unwrap();
    let loaded = tabs.active_id();
    assert_ne!(loaded, first);
    assert_eq!(tabs.active.document_id, loaded);
    assert_eq!(tabs.set.order(), &[loaded]);

    tabs.set.new_project_in_place(&mut tabs.active, false);
    let fresh = tabs.active_id();
    assert_ne!(fresh, loaded);
    assert_eq!(tabs.active.document_id, fresh);
    assert!(tabs.active.file_path.is_none());
    assert_eq!(tabs.set.order(), &[fresh]);
}

#[test]
fn an_in_place_load_of_a_file_open_in_another_tab_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let a_path = design(dir.path(), "a.cnnd", SIMPLE);
    let b_path = design(dir.path(), "b.cnnd", SIMPLE);
    let mut tabs = Tabs::new();
    tabs.open(&a_path);
    let b = tabs.open(&b_path).id;
    tabs.active.set_dirty(true);
    let before = state_of(&mut tabs.active);
    let tabs_before = tabs.tabs();

    let err = tabs
        .set
        .load_in_place(&mut tabs.active, &a_path.to_string_lossy())
        .unwrap_err();
    assert!(err.contains("open in another tab"), "{err}");
    assert_eq!(tabs.active_id(), b);
    assert_eq!(state_of(&mut tabs.active), before);
    assert_eq!(tabs.tabs(), tabs_before);
}

// ---------------------------------------------------------------------------
// A failed open changes nothing
// ---------------------------------------------------------------------------

#[test]
fn a_failed_open_leaves_the_tabs_and_the_active_document_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let a_path = design(dir.path(), "a.cnnd", SIMPLE);
    let broken = dir.path().join("broken.cnnd");
    std::fs::write(&broken, "this is not json").unwrap();
    let missing = dir.path().join("missing.cnnd");

    let mut tabs = Tabs::new();
    tabs.open(&a_path);
    incr(&mut tabs.active, "Main", "c = int { value: 7 }\n");
    let before = state_of(&mut tabs.active);
    let tabs_before = tabs.tabs();

    for path in [&broken, &missing] {
        let result = tabs.set.open(&mut tabs.active, &path.to_string_lossy());
        assert!(
            matches!(result, Err(OpenError::Load(_))),
            "{:?}",
            result.err()
        );
        assert_eq!(tabs.tabs(), tabs_before);
        assert_eq!(state_of(&mut tabs.active), before);
    }

    // A failed open from a pristine Untitled keeps the Untitled tab too.
    let mut tabs = Tabs::new();
    let tabs_before = tabs.tabs();
    assert!(
        tabs.set
            .open(&mut tabs.active, &missing.to_string_lossy())
            .is_err()
    );
    assert_eq!(tabs.tabs(), tabs_before);
    assert!(tabs.active.is_pristine());
}

// ---------------------------------------------------------------------------
// Switching is refused during an open interaction (D4)
// ---------------------------------------------------------------------------

/// Starts an interaction in the host A, tries every operation that changes
/// the active document, then ends the interaction: each is refused, nothing
/// moved, and the interaction's command lands in A's undo stack.
fn check_refused_during(
    begin: impl FnOnce(&mut StructureDesigner),
    end: impl FnOnce(&mut StructureDesigner),
    end_pushes: bool,
) {
    let ws = workspace();
    let other = design(ws.dir.path(), "other.cnnd", SIMPLE);
    let third = design(ws.dir.path(), "third.cnnd", SIMPLE);
    let mut tabs = Tabs::new();
    let a = tabs.open(&ws.host).id;
    let b = tabs.open(&other).id;
    tabs.activate(a);
    tabs.active
        .set_active_node_network_name(Some("Main".to_string()));
    begin(&mut tabs.active);
    let what = tabs
        .active
        .open_interaction()
        .expect("the interaction is open");

    let pushes = tabs.active.undo_stack.push_count();
    let b_pushes = tabs.doc(b).undo_stack.push_count();
    let tabs_before = tabs.tabs();
    let refused: Result<Option<RefreshReport>, SwitchRefused> =
        Err(SwitchRefused::OpenInteraction(what));

    assert_eq!(
        tabs.set.activate(&mut tabs.active, b).err(),
        refused.clone().err()
    );
    assert_eq!(
        tabs.set.new_document(&mut tabs.active, false),
        Err(SwitchRefused::OpenInteraction(what))
    );
    assert!(matches!(
        tabs.set.open(&mut tabs.active, &third.to_string_lossy()),
        Err(OpenError::Refused(SwitchRefused::OpenInteraction(_)))
    ));
    assert!(matches!(
        tabs.set.open(&mut tabs.active, &other.to_string_lossy()),
        Err(OpenError::Refused(SwitchRefused::OpenInteraction(_)))
    ));
    assert_eq!(tabs.set.close(&mut tabs.active, a).err(), refused.err());
    assert_eq!(tabs.tabs(), tabs_before);
    assert_eq!(tabs.active_id(), a);
    assert_eq!(tabs.active.undo_stack.push_count(), pushes);

    end(&mut tabs.active);
    assert!(tabs.active.open_interaction().is_none());
    assert_eq!(
        tabs.active.undo_stack.push_count(),
        pushes + u64::from(end_pushes),
        "the interaction's command went to A"
    );
    assert_eq!(tabs.doc(b).undo_stack.push_count(), b_pushes);
    // And now the switch goes through.
    tabs.activate(b);
    assert_eq!(tabs.active_id(), b);
}

#[test]
fn a_node_drag_refuses_a_switch() {
    let (begin, end, pushes) = a_node_drag();
    check_refused_during(begin, end, pushes);
}

#[test]
fn a_property_drag_refuses_a_switch() {
    let (begin, end, pushes) = a_property_drag();
    check_refused_during(begin, end, pushes);
}

#[test]
fn a_body_resize_refuses_a_switch() {
    let (begin, end, pushes) = a_body_resize();
    check_refused_during(begin, end, pushes);
}

#[test]
fn a_comment_edit_refuses_a_switch() {
    let (begin, end, pushes) = a_comment_edit();
    check_refused_during(begin, end, pushes);
}

#[test]
fn a_gadget_drag_refuses_a_switch() {
    let (begin, end, pushes) = a_gadget_drag();
    check_refused_during(begin, end, pushes);
}

#[test]
fn an_atom_drag_refuses_a_switch() {
    let (begin, end, pushes) = an_atom_drag();
    check_refused_during(begin, end, pushes);
}

#[test]
fn a_metadata_typing_run_does_not_refuse_and_ends_at_a_switch() {
    let mut designer = setup_designer();
    let node_id = editor_with_block(&mut designer);
    let mut tabs = Tabs::with(designer);
    let a = tabs.active_id();
    let type_phase = |d: &mut StructureDesigner, text: &str| {
        d.set_mechanosynth_edit_step_metadata(&[], node_id, 1, StepMetadataField::Phase, text, 0)
            .expect("a real step");
    };

    type_phase(&mut tabs.active, "l");
    type_phase(&mut tabs.active, "la");
    assert_eq!(tabs.active.undo_stack.history_len(), 1);
    assert!(tabs.active.pending_step_metadata_edit.is_some());

    // Not an open interaction: the switch goes through, and parking ends the
    // run.
    let b = tabs
        .set
        .new_document(&mut tabs.active, false)
        .expect("not refused");
    assert!(
        tabs.set
            .parked(a)
            .unwrap()
            .pending_step_metadata_edit
            .is_none()
    );
    tabs.activate(a);
    assert_ne!(tabs.active_id(), b);

    type_phase(&mut tabs.active, "lay");
    assert_eq!(
        tabs.active.undo_stack.history_len(),
        2,
        "the keystroke after the switch starts a new undo step"
    );
}

// ---------------------------------------------------------------------------
// Close, new, pristine replacement
// ---------------------------------------------------------------------------

#[test]
fn closing_the_active_tab_activates_its_right_neighbour_else_its_left() {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<PathBuf> = ["a", "b", "c"]
        .iter()
        .map(|n| design(dir.path(), &format!("{n}.cnnd"), SIMPLE))
        .collect();
    let mut tabs = Tabs::new();
    let ids: Vec<DocumentId> = paths.iter().map(|p| tabs.open(p).id).collect();
    assert_eq!(
        tabs.set.order(),
        &ids[..],
        "the pristine Untitled was replaced"
    );

    tabs.activate(ids[1]);
    tabs.set.close(&mut tabs.active, ids[1]).unwrap();
    assert_eq!(tabs.active_id(), ids[2], "right neighbour");
    assert_eq!(tabs.set.order(), &[ids[0], ids[2]]);

    tabs.set.close(&mut tabs.active, ids[2]).unwrap();
    assert_eq!(tabs.active_id(), ids[0], "left neighbour at the end");

    tabs.set.close(&mut tabs.active, ids[0]).unwrap();
    assert_eq!(tabs.set.len(), 1);
    assert!(
        tabs.active.is_pristine(),
        "the last tab leaves a fresh Untitled"
    );
    assert!(!ids.contains(&tabs.active_id()));
    assert_eq!(tabs.tabs()[0].display_name, UNTITLED);
}

#[test]
fn closing_a_parked_tab_just_drops_it() {
    let dir = tempfile::tempdir().unwrap();
    let a_path = design(dir.path(), "a.cnnd", SIMPLE);
    let b_path = design(dir.path(), "b.cnnd", SIMPLE);
    let mut tabs = Tabs::new();
    let a = tabs.open(&a_path).id;
    let b = tabs.open(&b_path).id;
    let before = state_of(&mut tabs.active);
    assert_eq!(tabs.set.close(&mut tabs.active, a), Ok(None));
    assert_eq!(tabs.set.order(), &[b]);
    assert_eq!(tabs.active_id(), b);
    assert_eq!(state_of(&mut tabs.active), before);
}

#[test]
fn open_replaces_a_pristine_tab_but_not_a_touched_one() {
    let dir = tempfile::tempdir().unwrap();
    let a_path = design(dir.path(), "a.cnnd", SIMPLE);
    let b_path = design(dir.path(), "b.cnnd", SIMPLE);

    let mut tabs = Tabs::new();
    let untitled = tabs.active_id();
    assert!(tabs.active.is_pristine());
    let a = tabs.open(&a_path).id;
    assert_eq!(tabs.set.order(), &[a]);
    assert!(!tabs.set.order().contains(&untitled));

    // A new Untitled that was edited is kept.
    let n = tabs.set.new_document(&mut tabs.active, false).unwrap();
    tabs.refresh();
    assert_eq!(tabs.set.order(), &[a, n], "inserted after the active tab");
    incr(&mut tabs.active, "Main", "c = int { value: 7 }\n");
    assert!(!tabs.active.is_pristine());
    let b = tabs.open(&b_path).id;
    assert_eq!(tabs.set.order(), &[a, n, b]);
}

#[test]
fn new_document_inserts_after_the_active_tab_in_the_requested_mode() {
    let dir = tempfile::tempdir().unwrap();
    let a_path = design(dir.path(), "a.cnnd", SIMPLE);
    let b_path = design(dir.path(), "b.cnnd", SIMPLE);
    let mut tabs = Tabs::new();
    let a = tabs.open(&a_path).id;
    let b = tabs.open(&b_path).id;
    tabs.activate(a);
    let n = tabs.set.new_document(&mut tabs.active, true).unwrap();
    assert_eq!(tabs.set.order(), &[a, n, b]);
    assert_eq!(tabs.active_id(), n);
    assert!(tabs.active.direct_editing_mode);
    assert!(tabs.active.is_pristine());
    let m = tabs.set.new_document(&mut tabs.active, false).unwrap();
    assert!(!tabs.active.direct_editing_mode);
    assert_eq!(tabs.set.order(), &[a, n, m, b]);
}

#[test]
fn move_to_reorders_tabs() {
    let mut tabs = Tabs::new();
    let a = tabs.active_id();
    let b = tabs.set.new_document(&mut tabs.active, false).unwrap();
    let c = tabs.set.new_document(&mut tabs.active, false).unwrap();
    assert_eq!(tabs.set.order(), &[a, b, c]);
    assert!(tabs.set.move_to(a, 2));
    assert_eq!(tabs.set.order(), &[b, c, a]);
    assert!(tabs.set.move_to(a, 99), "clamped to the end");
    assert_eq!(tabs.set.order(), &[b, c, a]);
    assert!(tabs.set.move_to(c, 0));
    assert_eq!(tabs.set.order(), &[c, b, a]);
    assert!(!tabs.set.move_to(DocumentId(42), 0));
}

#[test]
fn tabs_list_names_paths_dirty_flags_and_the_active_one() {
    let dir = tempfile::tempdir().unwrap();
    let a_path = design(dir.path(), "a.cnnd", SIMPLE);
    let mut tabs = Tabs::new();
    let a = tabs.open(&a_path).id;
    tabs.active.set_dirty(true);
    let n = tabs.set.new_document(&mut tabs.active, false).unwrap();
    assert_eq!(
        tabs.tabs(),
        vec![
            DocumentTab {
                id: a,
                display_name: "a.cnnd".to_string(),
                file_path: Some(a_path.to_string_lossy().to_string()),
                is_dirty: true,
                is_active: false,
            },
            DocumentTab {
                id: n,
                display_name: UNTITLED.to_string(),
                file_path: None,
                is_dirty: false,
                is_active: true,
            },
        ]
    );
}

// ---------------------------------------------------------------------------
// Documents communicate only through files (D6)
// ---------------------------------------------------------------------------

/// The wires of `f` in the host's `Main`, per argument, as source names.
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

#[test]
fn a_library_saved_in_another_tab_is_applied_when_the_host_is_activated() {
    let ws = workspace();
    let mut tabs = Tabs::new();
    let host = tabs.open(&ws.host).id;
    let lib = tabs.open(&ws.lib).id;
    tabs.activate(host);
    assert_eq!(wires_of_f(&tabs.active), vec![vec!["i1"], vec!["i2"]]);
    let host_pushes = tabs.active.undo_stack.push_count();
    tabs.activate(lib);

    // Edit the library in its tab. Unsaved, the host does not see it.
    incr(
        &mut tabs.active,
        "foo",
        "y = parameter { sort_order: -1 }\n",
    );
    tabs.activate(host);
    assert_eq!(wires_of_f(&tabs.active), vec![vec!["i1"], vec!["i2"]]);
    assert_eq!(tabs.active.undo_stack.push_count(), host_pushes);

    // Saved, it is applied on activation, before any evaluation.
    tabs.activate(lib);
    save(&mut tabs.active);
    bump_mtime(&ws.lib);
    let report = tabs
        .set
        .activate(&mut tabs.active, host)
        .unwrap()
        .expect("a report");
    assert_eq!(report.refreshed_mounts.len(), 1);
    assert!(report.refreshed_mounts[0].starts_with("lib "), "{report:?}");
    assert!(
        tabs.active
            .last_generated_structure_designer_scene
            .node_data
            .is_empty(),
        "activate does not evaluate: the caller's refresh is the only one"
    );
    assert_eq!(
        wires_of_f(&tabs.active),
        vec![vec!["i2"], vec!["i1"]],
        "call sites repaired by param_id"
    );
    assert_eq!(
        tabs.active.undo_stack.push_count(),
        host_pushes + 1,
        "one undo step in the host"
    );
    tabs.refresh();
    assert!(tabs.active.undo());
    assert_eq!(wires_of_f(&tabs.active), vec![vec!["i1"], vec!["i2"]]);
}

#[test]
fn a_library_change_is_held_while_the_host_has_redo_history() {
    let ws = workspace();
    let mut tabs = Tabs::new();
    let host = tabs.open(&ws.host).id;
    let lib = tabs.open(&ws.lib).id;
    tabs.activate(host);
    incr(&mut tabs.active, "Main", "i3 = int { value: 3 }\n");
    assert!(tabs.active.undo());
    let before = state_of(&mut tabs.active);

    tabs.activate(lib);
    incr(
        &mut tabs.active,
        "foo",
        "y = parameter { sort_order: -1 }\n",
    );
    save(&mut tabs.active);
    bump_mtime(&ws.lib);

    let report = tabs.activate(host).expect("a report");
    assert_eq!(report.held, vec!["lib".to_string()]);
    assert!(report.refreshed_mounts.is_empty());
    assert_eq!(wires_of_f(&tabs.active), vec![vec!["i1"], vec!["i2"]]);
    assert!(tabs.active.undo_stack.can_redo());
    assert_eq!(state_of(&mut tabs.active), before);
}

// ---------------------------------------------------------------------------
// Parking (D3)
// ---------------------------------------------------------------------------

const GEOMETRY: &str = "s = sphere { radius: 4 }
c = cuboid { extent: (3, 3, 3) }
u = union { shapes: [s, c] }
n = int { value: 5 }
output u
";

/// What the scene shows: its node refs, and each displayed top-level node's
/// evaluated value.
fn scene_summary(d: &mut StructureDesigner) -> Vec<(String, String)> {
    let mut refs: Vec<_> = d
        .last_generated_structure_designer_scene
        .node_data
        .keys()
        .cloned()
        .collect();
    refs.sort_by_key(|r| format!("{:?}", r));
    refs.iter()
        .map(|r| {
            let value = d
                .evaluate_node_output(&r.scope_path, r.node_id, 0)
                .to_display_string();
            (format!("{:?}", r), value)
        })
        .collect()
}

#[test]
fn a_parked_document_drops_its_caches_and_evaluates_the_same_on_return() {
    let dir = tempfile::tempdir().unwrap();
    let a_path = design(dir.path(), "a.cnnd", GEOMETRY);
    let mut tabs = Tabs::new();
    let a = tabs.open(&a_path).id;
    for name in ["u", "n"] {
        let id = node_id(&tabs.active, "Main", name);
        tabs.active.set_node_display(id, true);
    }
    tabs.active.mark_full_refresh();
    tabs.refresh();
    let before = scene_summary(&mut tabs.active);
    assert!(
        before.len() >= 2,
        "displayed nodes are in the scene: {before:?}"
    );
    let stats = tabs.active.network_evaluator.get_csg_cache_stats();
    assert!(
        stats.mesh_memory_bytes + stats.sketch_memory_bytes > 0,
        "the CSG cache is in use before parking"
    );

    let b = tabs.set.new_document(&mut tabs.active, false).unwrap();
    tabs.refresh();
    let parked = tabs.set.parked(a).unwrap();
    assert!(
        parked
            .last_generated_structure_designer_scene
            .node_data
            .is_empty()
    );
    let stats = parked.network_evaluator.get_csg_cache_stats();
    assert_eq!(stats.mesh_memory_bytes + stats.sketch_memory_bytes, 0);
    assert!(parked.pending_step_metadata_edit.is_none());

    tabs.activate(a);
    assert_eq!(scene_summary(&mut tabs.active), before);
    assert_ne!(tabs.active_id(), b);
}

// ---------------------------------------------------------------------------
// The CLI guard (D8)
// ---------------------------------------------------------------------------

#[test]
fn the_guard_accepts_only_the_active_document() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("sub")).unwrap();
    let a_path = design(dir.path(), "a.cnnd", SIMPLE);
    let mut tabs = Tabs::new();
    let a = tabs.open(&a_path).id;
    let n = tabs.set.new_document(&mut tabs.active, false).unwrap();

    // The active Untitled document, by id.
    assert!(tabs.set.check_guard(&tabs.active, &n.to_string()).is_ok());
    // A parked document, by path or id: refused, naming the active one.
    for spec in [a_path.to_string_lossy().to_string(), a.to_string()] {
        let err = tabs.set.check_guard(&tabs.active, &spec).unwrap_err();
        assert!(
            err.contains(UNTITLED) && err.contains(&n.to_string()),
            "{err}"
        );
    }
    assert!(tabs.set.check_guard(&tabs.active, "nowhere.cnnd").is_err());
    assert!(tabs.set.check_guard(&tabs.active, "12345").is_err());

    // The active saved document, by path in any spelling, or by id.
    tabs.activate(a);
    let dotted = dir.path().join("sub").join("..").join("a.cnnd");
    assert!(
        tabs.set
            .check_guard(&tabs.active, &a_path.to_string_lossy())
            .is_ok()
    );
    assert!(
        tabs.set
            .check_guard(&tabs.active, &dotted.to_string_lossy())
            .is_ok()
    );
    assert!(tabs.set.check_guard(&tabs.active, &a.to_string()).is_ok());
    if cfg!(windows) {
        let upper = a_path.to_string_lossy().to_uppercase();
        assert!(tabs.set.check_guard(&tabs.active, &upper).is_ok());
    }
    let err = tabs
        .set
        .check_guard(&tabs.active, &n.to_string())
        .unwrap_err();
    assert!(err.contains("a.cnnd"), "{err}");
}

#[test]
fn a_bare_designer_is_headless() {
    let d = StructureDesigner::new();
    assert_eq!(d.document_id, DocumentId::HEADLESS);
}
