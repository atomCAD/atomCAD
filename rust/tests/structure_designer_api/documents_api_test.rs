//! Multiple open documents, Phase 1 (`doc/design_multiple_documents.md` §9):
//! the Dart-facing shapes. The FFI wrappers need the GPU-backed `CADInstance`
//! and are not called here; the view builders they end in are.

use atomcad_structure_designer::clipboard::PasteRefusal;
use atomcad_structure_designer::document_set::{DocumentId, DocumentSet, OpenError, UNTITLED};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::api::structure_designer::view_builders::{
    activate_result_view, api_result_view, document_tabs_view, failed_open_document_result,
    open_document_result_view, paste_result_view, switch_result_view,
};
use std::path::{Path, PathBuf};

fn saved_design(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    let mut d = StructureDesigner::new();
    d.new_project();
    d.save_node_networks_as(&path.to_string_lossy()).unwrap();
    path
}

fn untitled() -> (DocumentSet, StructureDesigner) {
    let mut active = StructureDesigner::new();
    active.new_project_direct_editing();
    let set = DocumentSet::new(&mut active);
    (set, active)
}

#[test]
fn document_tabs_carry_name_path_dirty_and_active_in_tab_order() {
    let dir = tempfile::tempdir().unwrap();
    let a_path = saved_design(dir.path(), "a.cnnd");
    let (mut set, mut active) = untitled();
    let a = set.open(&mut active, &a_path.to_string_lossy()).unwrap().id;
    active.set_dirty(true);
    let n = set.new_document(&mut active, false).unwrap();

    let tabs = document_tabs_view(&set, &active);
    assert_eq!(tabs.len(), 2);
    assert_eq!(tabs[0].id, a.0);
    assert_eq!(tabs[0].display_name, "a.cnnd");
    assert_eq!(
        tabs[0].file_path.as_deref(),
        Some(&*a_path.to_string_lossy())
    );
    assert!(tabs[0].is_dirty);
    assert!(!tabs[0].is_active);
    assert_eq!(tabs[1].id, n.0);
    assert_eq!(tabs[1].display_name, UNTITLED);
    assert_eq!(tabs[1].file_path, None);
    assert!(!tabs[1].is_dirty);
    assert!(tabs[1].is_active);

    set.move_to(n, 0);
    let ids: Vec<u64> = document_tabs_view(&set, &active)
        .iter()
        .map(|t| t.id)
        .collect();
    assert_eq!(ids, vec![n.0, a.0]);
}

#[test]
fn open_document_result_shapes() {
    let dir = tempfile::tempdir().unwrap();
    let a_path = saved_design(dir.path(), "a.cnnd");
    let (mut set, mut active) = untitled();

    let outcome = set.open(&mut active, &a_path.to_string_lossy());
    let view = open_document_result_view(outcome, &active);
    assert!(view.result.success, "{}", view.result.error_message);
    assert_eq!(view.document_id, active.document_id.0);
    assert_eq!(view.file_path.as_deref(), Some(&*a_path.to_string_lossy()));
    assert!(!view.already_open);
    assert!(view.param_id_repairs.is_empty());
    assert!(view.library_report.is_none());

    let outcome = set.open(&mut active, &a_path.to_string_lossy());
    let view = open_document_result_view(outcome, &active);
    assert!(view.result.success);
    assert!(view.already_open);

    let missing = dir.path().join("missing.cnnd");
    let outcome = set.open(&mut active, &missing.to_string_lossy());
    assert!(matches!(outcome, Err(OpenError::Load(_))));
    let view = open_document_result_view(outcome, &active);
    assert!(!view.result.success);
    assert!(!view.result.error_message.is_empty());
    assert_eq!(view.document_id, 0);
    assert!(view.file_path.is_none());

    let view = failed_open_document_result("nope".to_string());
    assert!(!view.result.success);
    assert_eq!(view.result.error_message, "nope");
}

#[test]
fn activate_result_shapes_including_refusal() {
    let (mut set, mut active) = untitled();
    let first = active.document_id;
    let second = set.new_document(&mut active, false).unwrap();

    let view = activate_result_view(set.activate(&mut active, first), &active);
    assert!(view.result.success);
    assert!(view.library_report.is_none());

    let view = activate_result_view(set.activate(&mut active, DocumentId(77)), &active);
    assert!(!view.result.success);
    assert!(view.result.error_message.contains("77"));

    // Refused during an open interaction (D4): the message names it.
    let n = active.add_node("int", glam::f64::DVec2::ZERO);
    active.begin_node_data_drag(vec![], n);
    let view = activate_result_view(set.activate(&mut active, second), &active);
    assert!(!view.result.success);
    assert!(
        view.result.error_message.contains("property drag"),
        "{}",
        view.result.error_message
    );
    let view = switch_result_view(set.new_document(&mut active, false));
    assert!(!view.success);
    active.end_node_data_drag();

    let view = activate_result_view(set.close(&mut active, first), &active);
    assert!(view.result.success);
    assert_eq!(active.document_id, second);
}

#[test]
fn guard_result_shape() {
    let (set, active) = untitled();
    let ok = api_result_view(set.check_guard(&active, &active.document_id.to_string()));
    assert!(ok.success);
    let refused = api_result_view(set.check_guard(&active, "elsewhere.cnnd"));
    assert!(!refused.success);
    assert!(refused.error_message.contains(UNTITLED));
}

#[test]
fn a_paste_result_carries_the_ids_or_the_refusal() {
    let ok = paste_result_view(Ok(vec![3, 4]));
    assert_eq!(ok.node_ids, vec![3, 4]);
    assert_eq!(ok.error, None);

    let refused = paste_result_view(Err(PasteRefusal {
        lines: vec![
            ("a".to_string(), "`a` is gone.".to_string()),
            ("b".to_string(), "`b` is gone too.".to_string()),
        ],
    }));
    assert!(refused.node_ids.is_empty());
    assert_eq!(
        refused.error.as_deref(),
        Some(
            "Nothing was pasted:
- `a` is gone.
- `b` is gone too."
        )
    );
}
