//! The `StructureDesigner` side of node network thumbnails
//! (`doc/design_network_thumbnails.md`): which networks automatic capture may
//! write, storing a captured image, and the two user commands of D6.
//!
//! Rendering, framing and the "did the picture really change" comparison are
//! not here: the renderer sits beside this crate, not below it, so the API
//! layer captures and compares, and this file stores opaque PNG bytes.

use crate::document_set::{DocumentId, DocumentSet};
use crate::network_thumbnail::NetworkThumbnail;
use crate::structure_designer::StructureDesigner;
use crate::undo::commands::set_network_thumbnail::SetNetworkThumbnailCommand;

/// Which network a scene's content belongs to: `(document, network)` (D2).
/// The renderer records the owner of the content meshes on the GPU in this
/// shape (as a bare `u64` document id, since it cannot name `DocumentId`).
pub type ContentOwner = (DocumentId, String);

/// The D2 hand-over rule: when content owned by `incoming` is about to replace
/// content owned by `on_gpu`, the network to picture first — `on_gpu`, unless
/// it is the same network (an edit, not a switch) or there is none.
pub fn outgoing_content_owner(
    on_gpu: Option<&ContentOwner>,
    incoming: Option<&ContentOwner>,
) -> Option<ContentOwner> {
    on_gpu.filter(|owner| Some(*owner) != incoming).cloned()
}

impl DocumentSet {
    /// The open document `id` names: `active` when it is the active one,
    /// otherwise the parked one. `None` when no open document has that id —
    /// closed, or its content replaced in place (which renumbers it, D8).
    /// This is how a capture finds the outgoing network after a tab switch
    /// (`doc/design_network_thumbnails.md` D2).
    pub fn designer_mut<'a>(
        &'a mut self,
        active: &'a mut StructureDesigner,
        id: DocumentId,
    ) -> Option<&'a mut StructureDesigner> {
        if active.document_id == id {
            Some(active)
        } else {
            self.parked_mut(id)
        }
    }
}

impl StructureDesigner {
    /// The owner of the content this designer's refresh produces: this
    /// document and its active network (D2). `None` when no network is active.
    pub fn content_owner(&self) -> Option<ContentOwner> {
        self.active_node_network_name
            .clone()
            .map(|name| (self.document_id, name))
    }

    /// The stored thumbnail of `network_name`, if any.
    pub fn network_thumbnail(&self, network_name: &str) -> Option<&NetworkThumbnail> {
        self.node_type_registry
            .node_networks
            .get(network_name)
            .and_then(|n| n.thumbnail.as_ref())
    }

    /// Whether an automatic capture may write `network_name`'s thumbnail: the
    /// network exists, is local (linked networks show their library's image,
    /// D8) and its thumbnail is not user-set (D6).
    pub fn accepts_automatic_thumbnail(&self, network_name: &str) -> bool {
        let Some(network) = self.node_type_registry.node_networks.get(network_name) else {
            return false;
        };
        !self.is_linked_name(network_name)
            && !network.thumbnail.as_ref().is_some_and(|t| t.user_set)
    }

    /// Stores an automatically captured thumbnail (D2). The caller has already
    /// decided the picture changed (D3).
    ///
    /// Not an undo step and does **not** mark the document dirty — browsing
    /// networks must never produce an "unsaved changes" prompt (D8) — but sets
    /// [`Self::has_unsaved_thumbnails`], so *Save* is available. Returns
    /// whether the image was stored; refused when
    /// [`Self::accepts_automatic_thumbnail`] says no.
    pub fn store_automatic_thumbnail(&mut self, network_name: &str, png: Vec<u8>) -> bool {
        if !self.accepts_automatic_thumbnail(network_name) {
            return false;
        }
        let network = self
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .expect("accepts_automatic_thumbnail checked the network exists");
        network.set_thumbnail(Some(NetworkThumbnail {
            png,
            user_set: false,
        }));
        self.has_unsaved_thumbnails = true;
        true
    }

    /// Automatic captures stored since the last save, load or new (D8).
    /// Makes *Save* available without marking the document dirty.
    pub fn has_unsaved_thumbnails(&self) -> bool {
        self.has_unsaved_thumbnails
    }

    /// *Set current view as thumbnail* (D6): stores `png` as the network's
    /// user-set thumbnail, which automatic capture then leaves alone. One undo
    /// step; marks the document dirty. Refused on a linked network.
    pub fn set_user_thumbnail(&mut self, network_name: &str, png: Vec<u8>) -> Result<(), String> {
        self.set_thumbnail_recorded(
            network_name,
            Some(NetworkThumbnail {
                png,
                user_set: true,
            }),
        )
    }

    /// *Reset to automatic thumbnail* (D6): clears the user-set flag, storing
    /// `automatic_png` — the automatic capture the caller just made — when
    /// there is one, and otherwise keeping the current image, now automatic
    /// (an empty scene, D3). One undo step; marks the document dirty. Refused
    /// on a linked network and when the thumbnail is not user-set.
    pub fn reset_network_thumbnail(
        &mut self,
        network_name: &str,
        automatic_png: Option<Vec<u8>>,
    ) -> Result<(), String> {
        let current = self
            .node_type_registry
            .node_networks
            .get(network_name)
            .ok_or_else(|| format!("Node network '{}' does not exist", network_name))?
            .thumbnail
            .clone();
        let Some(current) = current.filter(|t| t.user_set) else {
            return Err(format!(
                "The thumbnail of '{}' is not user-set",
                network_name
            ));
        };
        let png = automatic_png.unwrap_or(current.png);
        self.set_thumbnail_recorded(
            network_name,
            Some(NetworkThumbnail {
                png,
                user_set: false,
            }),
        )
    }

    /// Replaces a thumbnail as an undoable, dirtying edit (D6).
    fn set_thumbnail_recorded(
        &mut self,
        network_name: &str,
        after: Option<NetworkThumbnail>,
    ) -> Result<(), String> {
        self.ensure_editable(network_name)?;
        let network = self
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .ok_or_else(|| format!("Node network '{}' does not exist", network_name))?;
        let before = network.thumbnail.clone();
        network.set_thumbnail(after.clone());
        self.set_dirty(true);
        self.push_command(SetNetworkThumbnailCommand {
            network_name: network_name.to_string(),
            before,
            after,
        });
        Ok(())
    }
}
