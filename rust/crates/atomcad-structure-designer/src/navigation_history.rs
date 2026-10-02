//! Back/forward navigation between node networks, like back/forward in a code
//! IDE.
//!
//! The history is the **session's**, not a document's
//! (`doc/design_multiple_documents.md` §6): every entry names the document it
//! was recorded in, and *Back* can lead into another tab. The active
//! `StructureDesigner` holds it and `hand_over_app_state` passes it on at
//! every switch, so rename/delete upkeep — which only ever happens in the
//! active document — names that document's id.
//!
//! Entries are never validated when recorded. Whether one can still be
//! visited (its tab is open, its network exists) is the caller's question,
//! asked through the `usable` predicate at the moment of navigating: an entry
//! that fails it is stepped over, not removed, since an undo can make it
//! valid again.
use crate::document_set::DocumentId;

/// One visited place: a network (or no network) in a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationEntry {
    pub document: DocumentId,
    pub network: Option<String>,
}

impl NavigationEntry {
    pub fn new(document: DocumentId, network: Option<String>) -> Self {
        Self { document, network }
    }
}

#[derive(Clone, Debug, Default)]
pub struct NavigationHistory {
    /// The visited places, oldest first. Empty until the first visit.
    entries: Vec<NavigationEntry>,
    /// The current place: an index into `entries` (0 while it is empty).
    index: usize,
}

impl NavigationHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a visit. Drops the forward history; a visit to the current
    /// place records nothing.
    pub fn navigate_to(&mut self, entry: NavigationEntry) {
        if self.current() == Some(&entry) {
            return;
        }
        if !self.entries.is_empty() {
            self.entries.truncate(self.index + 1);
        }
        self.entries.push(entry);
        self.index = self.entries.len() - 1;
    }

    /// The current place, `None` before the first visit.
    pub fn current(&self) -> Option<&NavigationEntry> {
        self.entries.get(self.index)
    }

    /// Every entry, oldest first (tests and diagnostics).
    pub fn entries(&self) -> &[NavigationEntry] {
        &self.entries
    }

    /// The index *Back* would move to: the nearest earlier entry that is
    /// `usable` and differs from the current one.
    pub fn back_target(&self, usable: impl Fn(&NavigationEntry) -> bool) -> Option<usize> {
        let current = self.current()?;
        (0..self.index)
            .rev()
            .find(|&i| self.entries[i] != *current && usable(&self.entries[i]))
    }

    /// The index *Forward* would move to (see [`back_target`](Self::back_target)).
    pub fn forward_target(&self, usable: impl Fn(&NavigationEntry) -> bool) -> Option<usize> {
        let current = self.current()?;
        (self.index + 1..self.entries.len())
            .find(|&i| self.entries[i] != *current && usable(&self.entries[i]))
    }

    pub fn can_navigate_back(&self, usable: impl Fn(&NavigationEntry) -> bool) -> bool {
        self.back_target(usable).is_some()
    }

    pub fn can_navigate_forward(&self, usable: impl Fn(&NavigationEntry) -> bool) -> bool {
        self.forward_target(usable).is_some()
    }

    /// Makes entry `index` the current place, without recording anything.
    /// Returns it, or `None` (and changes nothing) for an index out of range.
    pub fn move_to(&mut self, index: usize) -> Option<&NavigationEntry> {
        if index >= self.entries.len() {
            return None;
        }
        self.index = index;
        self.entries.get(index)
    }

    /// Moves back to [`back_target`](Self::back_target) and returns it.
    pub fn navigate_back(
        &mut self,
        usable: impl Fn(&NavigationEntry) -> bool,
    ) -> Option<NavigationEntry> {
        let target = self.back_target(usable)?;
        self.move_to(target).cloned()
    }

    /// Moves forward to [`forward_target`](Self::forward_target) and returns it.
    pub fn navigate_forward(
        &mut self,
        usable: impl Fn(&NavigationEntry) -> bool,
    ) -> Option<NavigationEntry> {
        let target = self.forward_target(usable)?;
        self.move_to(target).cloned()
    }

    /// Forgets everything.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.index = 0;
    }

    /// Follows a rename of network `old_name` of `document`.
    pub fn rename_network(&mut self, document: DocumentId, old_name: &str, new_name: &str) {
        for entry in &mut self.entries {
            if entry.document == document && entry.network.as_deref() == Some(old_name) {
                entry.network = Some(new_name.to_string());
            }
        }
    }

    /// Forgets the visits to network `network_name` of `document`, which was
    /// deleted.
    pub fn remove_network(&mut self, document: DocumentId, network_name: &str) {
        self.retain(|e| !(e.document == document && e.network.as_deref() == Some(network_name)));
    }

    /// Forgets every visit to `document`: its tab was closed, or its content
    /// replaced in place.
    pub fn remove_document(&mut self, document: DocumentId) {
        self.retain(|e| e.document != document);
    }

    /// Keeps the entries `keep` accepts. The current place stays current when
    /// kept; otherwise the nearest kept entry before it becomes current (the
    /// first one when there is none). Neighbours that became equal are merged,
    /// so *Back* never steps to where it already is.
    fn retain(&mut self, keep: impl Fn(&NavigationEntry) -> bool) {
        let mut kept: Vec<NavigationEntry> = Vec::with_capacity(self.entries.len());
        let mut new_index = 0;
        for (i, entry) in self.entries.drain(..).enumerate() {
            if keep(&entry) && kept.last() != Some(&entry) {
                kept.push(entry);
            }
            if i == self.index {
                new_index = kept.len().saturating_sub(1);
            }
        }
        self.entries = kept;
        self.index = new_index;
    }
}
