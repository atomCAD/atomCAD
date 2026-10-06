//! Node network thumbnails, Phase 1 (`doc/design_network_thumbnails.md`): the
//! domain side — storage and serialization (D7), the dirty flag, undo and
//! linked networks (D8), the user commands (D6), and the hand-over rule that
//! decides which network a capture lands on (D2).
//!
//! The render itself needs a GPU and lives in the API layer. These tests
//! drive the same decisions with a fake picture: [`Gpu::upload`] is what
//! `refresh_structure_designer` does around a content upload, minus the draw.

use super::library_links_support::{edit, new_design, open, save, saved_text};
use atomcad_structure_designer::document_set::{DocumentId, DocumentSet};
use atomcad_structure_designer::network_thumbnail::NetworkThumbnail;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::thumbnail_ops::{ContentOwner, outgoing_content_owner};
use std::collections::HashSet;

const SIMPLE: &str = "a = int { value: 1 }
b = expr { a: a, expression: \"a + 1\", parameters: [{ name: \"a\", data_type: Int }] }
output b
";

/// A fake PNG: the store is opaque bytes, so any bytes will do.
fn png(tag: u8) -> Vec<u8> {
    vec![0x89, b'P', b'N', b'G', tag]
}

fn thumbnail(d: &StructureDesigner, network: &str) -> Option<NetworkThumbnail> {
    d.node_type_registry.node_networks[network]
        .thumbnail
        .clone()
}

fn revision(d: &StructureDesigner, network: &str) -> u64 {
    d.node_type_registry.node_networks[network].thumbnail_revision
}

/// A designer with a `Main` network, unsaved.
fn designer() -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.new_project();
    d
}

// ---------------------------------------------------------------------------
// The capture hand-over (D2), without the GPU
// ---------------------------------------------------------------------------

/// The renderer's side of D2: who the content on the GPU belongs to. Each
/// `upload` is a content refresh: when the incoming owner differs, the
/// outgoing network is pictured first (here: given picture `tag`).
#[derive(Default)]
struct Gpu {
    owner: Option<ContentOwner>,
}

impl Gpu {
    /// Returns the owner a capture was attempted for, and whether it stored.
    fn upload(
        &mut self,
        set: &mut DocumentSet,
        active: &mut StructureDesigner,
        tag: u8,
    ) -> Option<(ContentOwner, bool)> {
        let incoming = active.content_owner();
        let attempt = outgoing_content_owner(self.owner.as_ref(), incoming.as_ref()).map(|out| {
            let stored = set
                .designer_mut(active, out.0)
                .is_some_and(|d| d.store_automatic_thumbnail(&out.1, png(tag)));
            (out, stored)
        });
        self.owner = incoming;
        attempt
    }
}

struct Session {
    set: DocumentSet,
    active: StructureDesigner,
    gpu: Gpu,
}

impl Session {
    fn new() -> Self {
        let mut active = designer();
        let set = DocumentSet::new(&mut active);
        let mut s = Session {
            set,
            active,
            gpu: Gpu::default(),
        };
        s.upload(0);
        s
    }

    fn upload(&mut self, tag: u8) -> Option<(ContentOwner, bool)> {
        self.gpu.upload(&mut self.set, &mut self.active, tag)
    }

    /// The network list's selection, as the `set_active_node_network` wrapper
    /// does it.
    fn select(&mut self, network: &str) {
        self.active
            .set_active_node_network_name(Some(network.to_string()));
        self.active.record_navigation();
    }
}

#[test]
fn an_edit_of_the_shown_network_captures_nothing() {
    let mut s = Session::new();
    edit(&mut s.active, "Main", SIMPLE);
    assert_eq!(s.upload(1), None);
    assert_eq!(thumbnail(&s.active, "Main"), None);
}

#[test]
fn selecting_another_network_pictures_the_outgoing_one() {
    let mut s = Session::new();
    s.active.add_node_network("Other");
    s.select("Other");
    let doc = s.active.document_id;
    assert_eq!(s.upload(7), Some(((doc, "Main".to_string()), true)));
    assert_eq!(thumbnail(&s.active, "Main").unwrap().png, png(7));
    assert_eq!(thumbnail(&s.active, "Other"), None);
}

#[test]
fn back_and_forward_picture_the_outgoing_network() {
    let mut s = Session::new();
    s.active.add_node_network("Other");
    s.select("Other");
    s.upload(1);
    let outcome = s.set.navigate_back(&mut s.active).unwrap();
    assert!(outcome.moved);
    assert_eq!(s.active.active_node_network_name.as_deref(), Some("Main"));
    s.upload(2);
    assert_eq!(thumbnail(&s.active, "Other").unwrap().png, png(2));
    s.set.navigate_forward(&mut s.active).unwrap();
    s.upload(3);
    assert_eq!(thumbnail(&s.active, "Main").unwrap().png, png(3));
}

#[test]
fn adding_and_duplicating_a_network_picture_the_outgoing_one() {
    let mut s = Session::new();
    // *Add network*: the wrapper activates the new one.
    let added = s.active.add_new_node_network();
    s.select(&added);
    s.upload(1);
    assert_eq!(thumbnail(&s.active, "Main").unwrap().png, png(1));

    // *Duplicate*: the copy starts with the source's picture and is
    // activated; the outgoing network is pictured.
    let copy = s.active.duplicate_node_network("Main").unwrap();
    assert_eq!(thumbnail(&s.active, &copy).unwrap().png, png(1));
    s.select(&copy);
    s.upload(2);
    assert_eq!(thumbnail(&s.active, &added).unwrap().png, png(2));
}

#[test]
fn a_tab_switch_pictures_the_outgoing_network_in_its_now_parked_document() {
    let mut s = Session::new();
    let first = s.set.active_id();
    let second = s.set.new_document(&mut s.active, false).unwrap();
    assert_eq!(s.set.active_id(), second);
    s.upload(4);
    // The first document is parked; the capture still found it.
    let parked = s.set.parked(first).unwrap();
    assert_eq!(thumbnail(parked, "Main").unwrap().png, png(4));
    assert!(parked.has_unsaved_thumbnails());
    assert_eq!(thumbnail(&s.active, "Main"), None);

    // And back: the second document's Main is pictured, now parked.
    s.set.activate(&mut s.active, first).unwrap();
    s.upload(5);
    assert_eq!(
        thumbnail(s.set.parked(second).unwrap(), "Main")
            .unwrap()
            .png,
        png(5)
    );
    assert_eq!(thumbnail(&s.active, "Main").unwrap().png, png(4));
}

#[test]
fn no_capture_for_a_deleted_or_renamed_outgoing_network() {
    let mut s = Session::new();
    s.active.add_node_network("Doomed");
    s.select("Doomed");
    s.upload(1);
    s.active.delete_node_network("Doomed").unwrap();
    s.select("Main");
    let (owner, stored) = s.upload(2).unwrap();
    assert_eq!(owner.1, "Doomed");
    assert!(!stored);

    s.active.add_node_network("Old");
    s.select("Old");
    s.upload(3);
    assert!(s.active.rename_node_network("Old", "New"));
    s.select("Main");
    let (owner, stored) = s.upload(4).unwrap();
    assert_eq!(owner.1, "Old");
    assert!(!stored);
    // Captured under its new name at the next switch (D2), not now.
    assert_eq!(thumbnail(&s.active, "New"), None);
}

#[test]
fn no_capture_into_a_document_replaced_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("other.cnnd");
    let mut other = new_design(&path);
    save(&mut other);

    let mut s = Session::new();
    s.set
        .load_in_place(&mut s.active, &path.to_string_lossy())
        .unwrap();
    // The old content's owner names a document id nobody has any more.
    let (owner, stored) = s.upload(1).unwrap();
    assert_ne!(owner.0, s.active.document_id);
    assert!(!stored);
    assert_eq!(thumbnail(&s.active, "Main"), None);
}

// ---------------------------------------------------------------------------
// Dirty flag and Save (D8)
// ---------------------------------------------------------------------------

#[test]
fn automatic_capture_leaves_the_dirty_flag_alone_but_makes_save_available() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("design.cnnd");
    let mut d = new_design(&path);
    assert!(!d.is_dirty());
    assert!(!d.has_unsaved_thumbnails());
    let undo_before = d.undo_stack.push_count();

    assert!(d.store_automatic_thumbnail("Main", png(1)));
    assert!(!d.is_dirty());
    assert!(d.has_unsaved_thumbnails());
    assert_eq!(d.undo_stack.push_count(), undo_before, "not an undo step");

    save(&mut d);
    assert!(!d.has_unsaved_thumbnails());
    let reopened = open(&path);
    assert_eq!(thumbnail(&reopened, "Main").unwrap().png, png(1));
}

#[test]
fn load_and_new_clear_the_unsaved_thumbnails_flag() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("design.cnnd");
    let mut d = new_design(&path);
    d.store_automatic_thumbnail("Main", png(1));
    d.load_node_networks(&path.to_string_lossy()).unwrap();
    assert!(!d.has_unsaved_thumbnails());

    d.store_automatic_thumbnail("Main", png(2));
    d.new_project();
    assert!(!d.has_unsaved_thumbnails());
    d.store_automatic_thumbnail("Main", png(3));
    d.new_project_direct_editing();
    assert!(!d.has_unsaved_thumbnails());
}

// ---------------------------------------------------------------------------
// Serialization (D7)
// ---------------------------------------------------------------------------

#[test]
fn a_thumbnail_round_trips_through_the_file_and_sits_last() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("design.cnnd");
    let mut d = new_design(&path);
    edit(&mut d, "Main", SIMPLE);
    d.store_automatic_thumbnail("Main", png(9));

    let text = saved_text(&mut d, Some(dir.path()));
    let at = text.find("\"thumbnail\"").expect("written");
    for earlier in ["\"nodes\"", "\"displayed_node_ids\"", "\"next_node_id\""] {
        assert!(
            text.find(earlier).unwrap() < at,
            "{earlier} before thumbnail"
        );
    }
    // Base64 on one line, and `user_set` omitted when false.
    let line = text[at..].lines().nth(1).unwrap();
    assert!(line.trim_start().starts_with("\"png\": \""), "{line}");
    assert!(!text.contains("user_set"));

    save(&mut d);
    let reopened = open(&path);
    assert_eq!(
        thumbnail(&reopened, "Main"),
        Some(NetworkThumbnail {
            png: png(9),
            user_set: false
        })
    );
}

#[test]
fn a_user_set_thumbnail_round_trips_with_its_flag() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("design.cnnd");
    let mut d = new_design(&path);
    d.set_user_thumbnail("Main", png(3)).unwrap();
    assert!(saved_text(&mut d, Some(dir.path())).contains("\"user_set\": true"));
    save(&mut d);
    assert!(thumbnail(&open(&path), "Main").unwrap().user_set);
}

#[test]
fn files_without_thumbnails_load_and_save_without_the_field() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("design.cnnd");
    let mut d = new_design(&path);
    edit(&mut d, "Main", SIMPLE);
    save(&mut d);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("thumbnail"));
    let mut reopened = open(&path);
    assert_eq!(thumbnail(&reopened, "Main"), None);
    assert_eq!(saved_text(&mut reopened, Some(dir.path())), text);
}

#[test]
fn a_malformed_thumbnail_is_dropped_not_fatal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("design.cnnd");
    let mut d = new_design(&path);
    d.store_automatic_thumbnail("Main", png(1));
    save(&mut d);
    let text = std::fs::read_to_string(&path).unwrap();
    let start = text.find("\"png\": \"").unwrap() + "\"png\": \"".len();
    let end = start + text[start..].find('"').unwrap();
    let broken = format!("{}!!not base64!!{}", &text[..start], &text[end..]);
    std::fs::write(&path, broken).unwrap();
    let reopened = open(&path);
    assert_eq!(thumbnail(&reopened, "Main"), None);
}

#[test]
fn duplicate_carries_the_thumbnail_through_undo_and_redo() {
    let mut d = designer();
    d.set_user_thumbnail("Main", png(5)).unwrap();
    let copy = d.duplicate_node_network("Main").unwrap();
    assert_eq!(thumbnail(&d, &copy), thumbnail(&d, "Main"));
    assert!(d.undo());
    assert!(!d.node_type_registry.node_networks.contains_key(&copy));
    assert!(d.redo());
    assert_eq!(thumbnail(&d, &copy).unwrap().png, png(5));
}

// ---------------------------------------------------------------------------
// User-set thumbnails (D6)
// ---------------------------------------------------------------------------

#[test]
fn automatic_capture_never_replaces_a_user_set_thumbnail() {
    let mut s = Session::new();
    s.active.set_user_thumbnail("Main", png(1)).unwrap();
    assert!(!s.active.accepts_automatic_thumbnail("Main"));
    assert!(!s.active.store_automatic_thumbnail("Main", png(2)));

    s.active.add_node_network("Other");
    s.select("Other");
    let (_, stored) = s.upload(3).unwrap();
    assert!(!stored);
    assert_eq!(
        thumbnail(&s.active, "Main"),
        Some(NetworkThumbnail {
            png: png(1),
            user_set: true
        })
    );
}

#[test]
fn set_and_reset_are_undoable_and_mark_the_document_dirty() {
    let mut d = designer();
    d.store_automatic_thumbnail("Main", png(1));
    d.set_dirty(false);

    d.set_user_thumbnail("Main", png(2)).unwrap();
    assert!(d.is_dirty());
    assert_eq!(
        thumbnail(&d, "Main"),
        Some(NetworkThumbnail {
            png: png(2),
            user_set: true
        })
    );

    // Reset with a fresh automatic capture.
    d.set_dirty(false);
    d.reset_network_thumbnail("Main", Some(png(3))).unwrap();
    assert!(d.is_dirty());
    assert_eq!(
        thumbnail(&d, "Main"),
        Some(NetworkThumbnail {
            png: png(3),
            user_set: false
        })
    );
    // Reset is offered only for a user-set thumbnail.
    assert!(d.reset_network_thumbnail("Main", None).is_err());

    assert!(d.undo());
    assert_eq!(
        thumbnail(&d, "Main").unwrap(),
        NetworkThumbnail {
            png: png(2),
            user_set: true
        }
    );
    assert!(d.undo());
    assert_eq!(
        thumbnail(&d, "Main").unwrap(),
        NetworkThumbnail {
            png: png(1),
            user_set: false
        }
    );
    assert!(d.redo());
    assert!(d.redo());
    assert_eq!(thumbnail(&d, "Main").unwrap().png, png(3));
}

#[test]
fn reset_without_a_capture_keeps_the_image_now_automatic() {
    let mut d = designer();
    d.set_user_thumbnail("Main", png(2)).unwrap();
    d.reset_network_thumbnail("Main", None).unwrap();
    assert_eq!(
        thumbnail(&d, "Main"),
        Some(NetworkThumbnail {
            png: png(2),
            user_set: false
        })
    );
}

// ---------------------------------------------------------------------------
// Undo snapshots (D8)
// ---------------------------------------------------------------------------

#[test]
fn undoing_a_text_edit_keeps_the_current_thumbnail() {
    let mut d = designer();
    edit(&mut d, "Main", SIMPLE);
    d.store_automatic_thumbnail("Main", png(1));
    edit(&mut d, "Main", "a = int { value: 7 }\noutput a\n");
    d.store_automatic_thumbnail("Main", png(2));
    let rev = revision(&d, "Main");

    assert!(d.undo());
    assert_eq!(thumbnail(&d, "Main").unwrap().png, png(2));
    assert_eq!(revision(&d, "Main"), rev, "same image, same revision");
    assert!(d.redo());
    assert_eq!(thumbnail(&d, "Main").unwrap().png, png(2));
}

#[test]
fn undoing_a_delete_brings_the_image_back() {
    let mut d = designer();
    d.add_node_network("Doomed");
    d.set_user_thumbnail("Doomed", png(4)).unwrap();
    d.delete_node_network("Doomed").unwrap();
    assert!(d.undo());
    assert_eq!(
        thumbnail(&d, "Doomed"),
        Some(NetworkThumbnail {
            png: png(4),
            user_set: true
        })
    );
}

#[test]
fn undoing_a_namespace_delete_brings_the_images_back() {
    let mut d = designer();
    d.add_node_network("ns.a");
    d.add_node_network("ns.b");
    d.store_automatic_thumbnail("ns.a", png(1));
    d.store_automatic_thumbnail("ns.b", png(2));
    d.delete_namespace("ns").unwrap();
    assert!(d.undo());
    assert_eq!(thumbnail(&d, "ns.a").unwrap().png, png(1));
    assert_eq!(thumbnail(&d, "ns.b").unwrap().png, png(2));
}

#[test]
fn revisions_never_repeat_across_restores_and_reloads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("design.cnnd");
    let mut d = new_design(&path);
    let mut seen = HashSet::new();
    let mut check = |d: &StructureDesigner, what: &str| {
        let r = revision(d, "Main");
        assert!(seen.insert(r), "revision {r} repeated at {what}");
    };
    check(&d, "new");
    d.store_automatic_thumbnail("Main", png(1));
    check(&d, "capture");
    d.set_user_thumbnail("Main", png(2)).unwrap();
    check(&d, "set");
    d.undo();
    check(&d, "undo of set");
    d.redo();
    check(&d, "redo of set");
    save(&mut d);
    d.load_node_networks(&path.to_string_lossy()).unwrap();
    check(&d, "reload");
    // A text edit's undo restores a rebuilt network; its revision is the
    // live one's, which is still unique.
    edit(&mut d, "Main", SIMPLE);
    d.undo();
    assert_eq!(thumbnail(&d, "Main").unwrap().png, png(2));
}

// ---------------------------------------------------------------------------
// Linked networks (D8)
// ---------------------------------------------------------------------------

#[test]
fn linked_networks_are_never_captured_and_keep_their_library_image() {
    let dir = tempfile::tempdir().unwrap();
    let lib_path = dir.path().join("lib.cnnd");
    let mut lib = new_design(&lib_path);
    edit(&mut lib, "shape", SIMPLE);
    lib.store_automatic_thumbnail("shape", png(8));
    save(&mut lib);

    let host_path = dir.path().join("host.cnnd");
    let mut host = new_design(&host_path);
    host.link_library("lib.cnnd", "a").unwrap();
    let linked = "a.shape";
    assert_eq!(thumbnail(&host, linked).unwrap().png, png(8));
    assert!(!host.accepts_automatic_thumbnail(linked));
    assert!(!host.store_automatic_thumbnail(linked, png(1)));
    assert!(host.set_user_thumbnail(linked, png(1)).is_err());
    assert_eq!(thumbnail(&host, linked).unwrap().png, png(8));

    // The host file never writes the library's picture.
    host.store_automatic_thumbnail("Main", png(2));
    let text = saved_text(&mut host, Some(dir.path()));
    assert_eq!(text.matches("\"thumbnail\"").count(), 1);
}

#[test]
fn the_document_id_of_a_parked_capture_is_resolved_through_the_set() {
    let mut active = designer();
    let mut set = DocumentSet::new(&mut active);
    let first = set.active_id();
    assert!(std::ptr::eq(
        set.designer_mut(&mut active, first).unwrap() as *const _,
        &active as *const _
    ));
    let second = set.new_document(&mut active, false).unwrap();
    assert_eq!(
        set.designer_mut(&mut active, first).unwrap().document_id,
        first
    );
    assert_eq!(
        set.designer_mut(&mut active, second).unwrap().document_id,
        second
    );
    assert!(set.designer_mut(&mut active, DocumentId(999)).is_none());
}

#[test]
fn the_hand_over_rule() {
    let a = (DocumentId(1), "A".to_string());
    let b = (DocumentId(1), "B".to_string());
    let a2 = (DocumentId(2), "A".to_string());
    assert_eq!(outgoing_content_owner(None, Some(&a)), None);
    assert_eq!(outgoing_content_owner(Some(&a), Some(&a)), None);
    assert_eq!(outgoing_content_owner(Some(&a), Some(&b)), Some(a.clone()));
    assert_eq!(outgoing_content_owner(Some(&a), Some(&a2)), Some(a.clone()));
    assert_eq!(outgoing_content_owner(Some(&a), None), Some(a.clone()));
}
