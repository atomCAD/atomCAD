//! Multiple open documents (`doc/design_multiple_documents.md`): which
//! documents are open, in which tab order, and which one is active.
//!
//! A document is a whole [`StructureDesigner`] (D1). The **active** one is not
//! stored here: it stays in the caller's slot (`CADInstance.structure_designer`
//! in the GUI), so the ~600 API functions that reach "the" designer keep
//! working on the active document unchanged. Every other document is
//! **parked** in this set. The operations that change the active document take
//! that slot as `&mut` and swap a parked designer into it.
//!
//! Every rule about documents lives here, not in the FFI wrappers: those need
//! the GPU-backed `CADInstance` and so cannot be tested (§5.1, "every rule
//! lives here"). This module knows nothing about the renderer; the wrappers
//! add the camera and the scene refresh around each call.

use crate::camera_settings::CameraSettings;
use crate::file_dependencies::{canonical_or_lexical, path_key};
use crate::library_links::RealFs;
use crate::library_refresh::RefreshReport;
use crate::navigation_history::NavigationEntry;
use crate::structure_designer::StructureDesigner;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

/// One open document, for the whole of its life: never reused within a
/// session, and replaced when a designer's content is replaced in place (D8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocumentId(pub u64);

impl DocumentId {
    /// The id of a designer that belongs to no `DocumentSet` (headless mode,
    /// tests that build a bare `StructureDesigner`).
    pub const HEADLESS: DocumentId = DocumentId(0);

    /// The source of a clipboard copied in content that was since replaced in
    /// place (`StructureDesigner::detach_own_clipboard`). No document has it,
    /// so a paste always takes the cross-document path.
    pub const REPLACED: DocumentId = DocumentId(u64::MAX);
}

impl fmt::Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Why the active document was not changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitchRefused {
    /// An interaction the undo coalescing treats as one open step is in
    /// progress (D4); the value is `open_interaction()`'s description.
    OpenInteraction(&'static str),
    /// No open document has this id.
    UnknownDocument(DocumentId),
}

impl fmt::Display for SwitchRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SwitchRefused::OpenInteraction(what) => {
                write!(f, "Cannot switch documents during {}", what)
            }
            SwitchRefused::UnknownDocument(id) => write!(f, "No open document has id {}", id),
        }
    }
}

/// Why `open` did not open a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenError {
    Refused(SwitchRefused),
    /// The file could not be loaded. No tab was added and the active document
    /// was not touched.
    Load(String),
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenError::Refused(r) => r.fmt(f),
            OpenError::Load(e) => f.write_str(e),
        }
    }
}

impl From<SwitchRefused> for OpenError {
    fn from(r: SwitchRefused) -> Self {
        OpenError::Refused(r)
    }
}

/// What `open` did.
#[derive(Debug, Default)]
pub struct OpenOutcome {
    /// The document now active: the newly opened one, or the tab that
    /// already had the file open.
    pub id: DocumentId,
    /// The file was open already, and its tab was activated (D5). Nothing was
    /// read from disk, and the post-load reports below are empty.
    pub already_open: bool,
    /// `load_node_networks`'s parameter-id repair messages.
    pub param_id_repairs: Vec<String>,
    /// `load_node_networks`'s library report (the open report of library
    /// linking D7/D13).
    pub load_report: Option<RefreshReport>,
    /// What the activation's dependency check did (§5.2 step 6).
    pub activation_report: Option<RefreshReport>,
}

/// What a *Back* / *Forward* step did.
#[derive(Debug, Default)]
pub struct NavigateOutcome {
    /// False when there was nowhere to go; nothing changed then.
    pub moved: bool,
    /// The step led into another tab, and this is what that activation's
    /// dependency check did (as for [`DocumentSet::activate`]).
    pub activation_report: Option<RefreshReport>,
}

/// One row of the tab list: the domain twin of `APIDocumentTab`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentTab {
    pub id: DocumentId,
    /// The file name, or *Untitled* for a design that was never saved.
    pub display_name: String,
    pub file_path: Option<String>,
    pub is_dirty: bool,
    pub is_active: bool,
}

/// The display name of an Untitled document.
pub const UNTITLED: &str = "Untitled";

/// The identity of a document file (D5): its canonical absolute path,
/// case-insensitive on Windows. Two spellings of one file (`..`, case,
/// separators) give the same key.
pub fn document_path_key(path: &str) -> String {
    path_key(&canonical_or_lexical(&RealFs, Path::new(path)))
}

/// The open documents (§5.1). See the module doc for how the active one is
/// held.
pub struct DocumentSet {
    active: DocumentId,
    /// Tab order; includes the active id.
    order: Vec<DocumentId>,
    parked: HashMap<DocumentId, StructureDesigner>,
    next_id: u64,
}

impl DocumentSet {
    /// A set whose only document is `active`, which becomes document 1.
    pub fn new(active: &mut StructureDesigner) -> Self {
        let first = DocumentId(1);
        active.document_id = first;
        // Anything visited before the set existed was recorded under the
        // headless id.
        active.navigation_history.clear();
        active.record_navigation();
        DocumentSet {
            active: first,
            order: vec![first],
            parked: HashMap::new(),
            next_id: 2,
        }
    }

    pub fn active_id(&self) -> DocumentId {
        self.active
    }

    /// The tab order, active document included.
    pub fn order(&self) -> &[DocumentId] {
        &self.order
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// A parked document, for inspection.
    pub fn parked(&self, id: DocumentId) -> Option<&StructureDesigner> {
        self.parked.get(&id)
    }

    /// A parked document, for tests and tools that edit it in place. Editing a
    /// parked document is not something the application does: every API call
    /// acts on the active one.
    pub fn parked_mut(&mut self, id: DocumentId) -> Option<&mut StructureDesigner> {
        self.parked.get_mut(&id)
    }

    fn fresh_id(&mut self) -> DocumentId {
        let id = DocumentId(self.next_id);
        self.next_id += 1;
        id
    }

    fn refuse_during_interaction(active: &StructureDesigner) -> Result<(), SwitchRefused> {
        match active.open_interaction() {
            Some(what) => Err(SwitchRefused::OpenInteraction(what)),
            None => Ok(()),
        }
    }

    /// Makes `target` the active document (§5.2, steps 1–4, 6 and 7a): the
    /// designer in `active` is parked and `target` takes its place.
    ///
    /// Refused while an interaction is open (D4) or for an unknown id; after
    /// those checks nothing can fail, so a switch happens completely or not at
    /// all. Activating the active document does nothing. The returned report
    /// is what the dependency check (library linking D7) did on activation —
    /// a library saved in another tab is applied here, *before* the caller's
    /// refresh, so the document is evaluated once.
    ///
    /// **Does not evaluate.** The caller refreshes (`refresh` with the pending
    /// changes, which are marked full here), and applies the incoming active
    /// network's camera to the renderer.
    ///
    /// **The swap moves the designers' inline bytes.** Their heap data does
    /// not move, and the two `&mut` borrows guarantee nothing observes either
    /// value during the swap, so this is an ordinary Rust move. It rests on
    /// one assumption: **no code keeps a raw pointer into the active
    /// `StructureDesigner` across API calls.** Such a pointer would point at
    /// the other document after a switch. The raw pointers that exist today
    /// split a borrow of `node_type_registry` inside one block, and the
    /// evaluation memo's address-based keys point into the heap and are
    /// rebuilt every pass.
    ///
    /// The incoming document's active network is recorded as a visit in the
    /// session's back/forward history.
    pub fn activate(
        &mut self,
        active: &mut StructureDesigner,
        target: DocumentId,
    ) -> Result<Option<RefreshReport>, SwitchRefused> {
        self.activate_at(active, target, None)
    }

    /// [`activate`](Self::activate), then shows `network` in `target` when
    /// given and it exists there — recorded as **one** visit, so *Back* from
    /// it leads to where the user came from, not to the network `target`
    /// happened to have open (*Open in library file*).
    pub fn activate_at(
        &mut self,
        active: &mut StructureDesigner,
        target: DocumentId,
        network: Option<&str>,
    ) -> Result<Option<RefreshReport>, SwitchRefused> {
        let report = if target == self.active {
            None
        } else {
            self.swap_in(active, target)?
        };
        if let Some(name) = network
            && active.can_show_network(Some(name))
        {
            active.show_navigated_network(Some(name.to_string()));
        }
        active.record_navigation();
        Ok(report)
    }

    /// The swap of [`activate`](Self::activate), recording nothing: a
    /// back/forward step moves the history itself.
    fn swap_in(
        &mut self,
        active: &mut StructureDesigner,
        target: DocumentId,
    ) -> Result<Option<RefreshReport>, SwitchRefused> {
        Self::refuse_during_interaction(active)?;
        let mut incoming = self
            .parked
            .remove(&target)
            .ok_or(SwitchRefused::UnknownDocument(target))?;

        active.hand_over_app_state(&mut incoming);
        std::mem::swap(active, &mut incoming);
        // `incoming` now holds the outgoing document.
        incoming.park();
        self.parked.insert(incoming.document_id, incoming);
        self.active = target;

        let report = active.check_dependencies();
        active.mark_full_refresh();
        Ok(report)
    }

    /// Adds `designer` as a parked document right after `after` in the tab
    /// order (at the end when `after` is not open) and assigns its id.
    pub fn insert(&mut self, mut designer: StructureDesigner, after: DocumentId) -> DocumentId {
        let id = self.fresh_id();
        designer.document_id = id;
        let index = self
            .order
            .iter()
            .position(|d| *d == after)
            .map_or(self.order.len(), |i| i + 1);
        self.order.insert(index, id);
        self.parked.insert(id, designer);
        id
    }

    /// A fresh designer set up as `new_project_direct_editing` /
    /// `new_project` does, not yet in the set.
    fn new_designer(direct_editing: bool) -> StructureDesigner {
        let mut designer = StructureDesigner::new();
        if direct_editing {
            designer.new_project_direct_editing();
        } else {
            designer.new_project();
        }
        designer
    }

    /// *File > New*: a fresh Untitled document, inserted after the active tab
    /// and activated. Refused during an open interaction (D4).
    pub fn new_document(
        &mut self,
        active: &mut StructureDesigner,
        direct_editing: bool,
    ) -> Result<DocumentId, SwitchRefused> {
        Self::refuse_during_interaction(active)?;
        let id = self.insert(Self::new_designer(direct_editing), self.active);
        self.activate(active, id)?;
        Ok(id)
    }

    /// *File > Open*: switches to the tab that has `path` open (D5), or loads
    /// the file into a fresh designer **while it is detached** and only on
    /// success adds it after the active tab and activates it. A failed load
    /// drops the fresh designer; nothing else was touched. When the active
    /// document is pristine, the opened one takes its place in the tab order
    /// and the pristine one is dropped (§3).
    pub fn open(
        &mut self,
        active: &mut StructureDesigner,
        path: &str,
    ) -> Result<OpenOutcome, OpenError> {
        self.open_at(active, path, None)
    }

    /// [`open`](Self::open), showing `network` in the opened document as
    /// [`activate_at`](Self::activate_at) does.
    pub fn open_at(
        &mut self,
        active: &mut StructureDesigner,
        path: &str,
        network: Option<&str>,
    ) -> Result<OpenOutcome, OpenError> {
        Self::refuse_during_interaction(active)?;
        let key = document_path_key(path);
        if let Some(id) = self.find_by_path(active, &key) {
            let activation_report = self.activate_at(active, id, network)?;
            return Ok(OpenOutcome {
                id,
                already_open: true,
                activation_report,
                ..Default::default()
            });
        }

        let mut designer = StructureDesigner::new();
        designer
            .load_node_networks(path)
            .map_err(|e| OpenError::Load(e.to_string()))?;
        let param_id_repairs = designer.take_load_param_id_repairs();
        let load_report = designer.take_load_library_report();

        let replaced = active.is_pristine().then_some(self.active);
        let id = self.insert(designer, self.active);
        let activation_report = self.activate_at(active, id, network)?;
        if let Some(pristine) = replaced {
            self.drop_parked(active, pristine);
        }
        Ok(OpenOutcome {
            id,
            already_open: false,
            param_id_repairs,
            load_report,
            activation_report,
        })
    }

    /// Drops a parked document, and its visits from the history `active`
    /// holds.
    fn drop_parked(
        &mut self,
        active: &mut StructureDesigner,
        id: DocumentId,
    ) -> Option<StructureDesigner> {
        let designer = self.parked.remove(&id)?;
        self.order.retain(|d| *d != id);
        active.navigation_history.remove_document(id);
        Some(designer)
    }

    /// Closes document `id`. A parked document is simply dropped. The active
    /// one first activates its neighbour (the tab to the right, else the one
    /// to the left) and returns that activation's report; it is refused
    /// during an open interaction (D4). Closing the last tab leaves a fresh
    /// Untitled document, set up as at application start, so there is always
    /// an active document. There is no dirty check: the UI asks first.
    pub fn close(
        &mut self,
        active: &mut StructureDesigner,
        id: DocumentId,
    ) -> Result<Option<RefreshReport>, SwitchRefused> {
        if id != self.active {
            return match self.drop_parked(active, id) {
                Some(_) => Ok(None),
                None => Err(SwitchRefused::UnknownDocument(id)),
            };
        }
        Self::refuse_during_interaction(active)?;
        let index = self
            .order
            .iter()
            .position(|d| *d == id)
            .expect("the active document is in the tab order");
        let neighbour = match self.order.get(index + 1).copied() {
            Some(right) => right,
            None if index > 0 => self.order[index - 1],
            None => self.insert(Self::new_designer(true), id),
        };
        let report = self.activate(active, neighbour)?;
        self.drop_parked(active, id);
        Ok(report)
    }

    /// Gives the active designer a new id after its content was replaced in
    /// place (D8), so an id never names two different documents.
    ///
    /// The old id's visits are forgotten (they name content that is gone) and
    /// the new content is recorded as a visit.
    pub fn renumber_active(&mut self, active: &mut StructureDesigner) {
        let new_id = self.fresh_id();
        if let Some(slot) = self.order.iter_mut().find(|d| **d == self.active) {
            *slot = new_id;
        }
        active.navigation_history.remove_document(self.active);
        active.document_id = new_id;
        self.active = new_id;
        active.record_navigation();
    }

    // ===== Back / forward across tabs =====
    //
    // The history is the session's (`navigation_history.rs`) and lives in the
    // active designer. An entry can be visited while its document is open
    // and its network exists there; others are stepped over.

    /// Whether the history entry `entry` can be visited now.
    fn navigable(&self, active: &StructureDesigner, entry: &NavigationEntry) -> bool {
        self.designer(active, entry.document)
            .is_some_and(|d| d.can_show_network(entry.network.as_deref()))
    }

    pub fn can_navigate_back(&self, active: &StructureDesigner) -> bool {
        active
            .navigation_history
            .can_navigate_back(|e| self.navigable(active, e))
    }

    pub fn can_navigate_forward(&self, active: &StructureDesigner) -> bool {
        active
            .navigation_history
            .can_navigate_forward(|e| self.navigable(active, e))
    }

    /// *Back*: to the previous visited network, switching tabs when it is in
    /// another document. Refused during an open interaction (D4) only when it
    /// would switch. Like [`activate`](Self::activate), it does not evaluate:
    /// the caller refreshes and applies the active network's camera.
    pub fn navigate_back(
        &mut self,
        active: &mut StructureDesigner,
    ) -> Result<NavigateOutcome, SwitchRefused> {
        let target = active
            .navigation_history
            .back_target(|e| self.navigable(active, e));
        self.navigate_to_index(active, target)
    }

    /// *Forward* (see [`navigate_back`](Self::navigate_back)).
    pub fn navigate_forward(
        &mut self,
        active: &mut StructureDesigner,
    ) -> Result<NavigateOutcome, SwitchRefused> {
        let target = active
            .navigation_history
            .forward_target(|e| self.navigable(active, e));
        self.navigate_to_index(active, target)
    }

    fn navigate_to_index(
        &mut self,
        active: &mut StructureDesigner,
        target: Option<usize>,
    ) -> Result<NavigateOutcome, SwitchRefused> {
        let Some(index) = target else {
            return Ok(NavigateOutcome::default());
        };
        let document = active.navigation_history.entries()[index].document;
        let mut activation_report = None;
        if document != self.active {
            // Checked before the history moves, so a refusal changes nothing.
            Self::refuse_during_interaction(active)?;
            active.navigation_history.move_to(index);
            // The history travels with the swap (`hand_over_app_state`).
            activation_report = self.swap_in(active, document)?;
        } else {
            active.navigation_history.move_to(index);
        }
        // The activation's dependency check may have removed the network
        // since `navigable` said yes; the document's own network stays then.
        if let Some(entry) = active.navigation_history.current().cloned()
            && entry.document == self.active
            && active.can_show_network(entry.network.as_deref())
        {
            active.show_navigated_network(entry.network);
        }
        Ok(NavigateOutcome {
            moved: true,
            activation_report,
        })
    }

    /// The parked document that has `path` open, if any — the D5 refusal of
    /// Save As and of the in-place load. The active document itself is never
    /// "elsewhere".
    pub fn path_open_elsewhere(
        &self,
        active: &StructureDesigner,
        path: &str,
    ) -> Option<DocumentId> {
        let key = document_path_key(path);
        self.find_by_path(active, &key)
            .filter(|id| *id != self.active)
    }

    fn refuse_path_open_elsewhere(
        &self,
        active: &StructureDesigner,
        path: &str,
    ) -> Result<(), String> {
        match self.path_open_elsewhere(active, path) {
            Some(_) => Err(format!(
                "{} is open in another tab — close it first",
                file_name_of(path)
            )),
            None => Ok(()),
        }
    }

    /// *Save As*, refused onto a path open in another tab (D5): two in-memory
    /// copies of one file would make "which one did I save last" a question
    /// with a wrong answer. Nothing is written when refused.
    pub fn save_as(&self, active: &mut StructureDesigner, path: &str) -> Result<(), String> {
        self.refuse_path_open_elsewhere(active, path)?;
        active
            .save_node_networks_as(path)
            .map_err(|e| e.to_string())
    }

    /// *Save As* with the dependency copy (library linking D11), with the same
    /// refusal as [`save_as`](Self::save_as).
    pub fn save_as_with_dependencies(
        &self,
        active: &mut StructureDesigner,
        path: &str,
        copy: bool,
        overwrite: &[String],
    ) -> Result<crate::file_dependencies::CopyOutcome, String> {
        self.refuse_path_open_elsewhere(active, path)?;
        active.save_as_with_dependencies(path, copy, overwrite)
    }

    /// The in-place load that `load_node_networks` has always been (the D8
    /// backstop for callers other than *File > Open*): refused for a path
    /// open in another tab, and afterwards the active designer gets a fresh
    /// id, because its content is no longer the document the old id named.
    /// Renumbered even when the load fails, since a failed load may already
    /// have changed the registry.
    pub fn load_in_place(
        &mut self,
        active: &mut StructureDesigner,
        path: &str,
    ) -> Result<Option<CameraSettings>, String> {
        self.refuse_path_open_elsewhere(active, path)?;
        let result = active.load_node_networks(path).map_err(|e| e.to_string());
        self.renumber_active(active);
        result
    }

    /// `new_project` / `new_project_direct_editing` on the active designer,
    /// which then gets a fresh id (D8 backstop).
    pub fn new_project_in_place(&mut self, active: &mut StructureDesigner, direct_editing: bool) {
        if direct_editing {
            active.new_project_direct_editing();
        } else {
            active.new_project();
        }
        self.renumber_active(active);
    }

    /// The CLI's `--document <path-or-id>` guard (D8): `Ok` when `spec` names
    /// the active document — by its id, or by its path in any spelling of it
    /// — and otherwise an error naming the active document.
    pub fn check_guard(&self, active: &StructureDesigner, spec: &str) -> Result<(), String> {
        let spec = spec.trim();
        if let Ok(n) = spec.parse::<u64>()
            && DocumentId(n) == self.active
        {
            return Ok(());
        }
        if let Some(path) = &active.file_path
            && document_path_key(path) == document_path_key(spec)
        {
            return Ok(());
        }
        let active_name = match &active.file_path {
            Some(path) => path.clone(),
            None => format!("{} (document id {})", UNTITLED, self.active),
        };
        Err(format!(
            "The request names document '{}', but the active document is {}. \
             Switch to that tab in atomCAD, or pass --document for the active one.",
            spec, active_name
        ))
    }

    fn designer<'a>(
        &'a self,
        active: &'a StructureDesigner,
        id: DocumentId,
    ) -> Option<&'a StructureDesigner> {
        if id == self.active {
            Some(active)
        } else {
            self.parked.get(&id)
        }
    }

    /// The tab list, in tab order.
    pub fn tabs(&self, active: &StructureDesigner) -> Vec<DocumentTab> {
        self.order
            .iter()
            .filter_map(|id| {
                let d = self.designer(active, *id)?;
                Some(DocumentTab {
                    id: *id,
                    display_name: d
                        .file_path
                        .as_deref()
                        .map(file_name_of)
                        .unwrap_or_else(|| UNTITLED.to_string()),
                    file_path: d.file_path.clone(),
                    is_dirty: d.is_dirty,
                    is_active: *id == self.active,
                })
            })
            .collect()
    }

    /// Moves tab `id` to position `index` of the tab order (clamped). Returns
    /// whether `id` is open.
    pub fn move_to(&mut self, id: DocumentId, index: usize) -> bool {
        let Some(from) = self.order.iter().position(|d| *d == id) else {
            return false;
        };
        self.order.remove(from);
        let index = index.min(self.order.len());
        self.order.insert(index, id);
        true
    }

    /// The document whose file has the identity `key` (a
    /// [`document_path_key`]), active or parked.
    pub fn find_by_path(&self, active: &StructureDesigner, key: &str) -> Option<DocumentId> {
        self.order.iter().copied().find(|id| {
            self.designer(active, *id)
                .and_then(|d| d.file_path.as_deref())
                .is_some_and(|p| document_path_key(p) == key)
        })
    }
}

fn file_name_of(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}
