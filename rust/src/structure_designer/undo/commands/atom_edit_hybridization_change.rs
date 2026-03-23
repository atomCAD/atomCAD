use std::collections::HashMap;

/// Whether a hybridization override atom ID refers to a base atom or a diff atom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HybridizationProvenance {
    Base,
    Diff,
}

/// Delta representing changes to the hybridization override maps.
#[derive(Debug, Clone, Default)]
pub struct HybridizationDelta {
    /// (provenance, atom_id, new_value) — entries added to the override maps.
    /// On undo: remove these entries. On redo: insert them.
    pub added: Vec<(HybridizationProvenance, u32, u8)>,
    /// (provenance, atom_id, old_value) — entries removed from the override maps.
    /// On undo: re-insert with old_value. On redo: remove them.
    pub removed: Vec<(HybridizationProvenance, u32, u8)>,
    /// (provenance, atom_id, old_value, new_value) — entries whose value changed.
    /// On undo: restore old_value. On redo: apply new_value.
    pub changed: Vec<(HybridizationProvenance, u32, u8, u8)>,
}

impl HybridizationDelta {
    /// Returns true if the delta contains no changes.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    /// Compute a delta by diffing before/after snapshots of the hybridization override maps.
    pub fn from_diff(
        base_before: &HashMap<u32, u8>,
        diff_before: &HashMap<u32, u8>,
        base_after: &HashMap<u32, u8>,
        diff_after: &HashMap<u32, u8>,
    ) -> Self {
        let mut delta = Self::default();

        // Diff base maps
        diff_maps(
            base_before,
            base_after,
            HybridizationProvenance::Base,
            &mut delta,
        );

        // Diff diff maps
        diff_maps(
            diff_before,
            diff_after,
            HybridizationProvenance::Diff,
            &mut delta,
        );

        delta
    }

    /// Apply this delta in the forward (redo) direction to the given maps.
    pub fn apply_redo(
        &self,
        base_map: &mut HashMap<u32, u8>,
        diff_map: &mut HashMap<u32, u8>,
    ) {
        for &(prov, atom_id, value) in &self.added {
            match prov {
                HybridizationProvenance::Base => {
                    base_map.insert(atom_id, value);
                }
                HybridizationProvenance::Diff => {
                    diff_map.insert(atom_id, value);
                }
            }
        }
        for &(prov, atom_id, _old_value) in &self.removed {
            match prov {
                HybridizationProvenance::Base => {
                    base_map.remove(&atom_id);
                }
                HybridizationProvenance::Diff => {
                    diff_map.remove(&atom_id);
                }
            }
        }
        for &(prov, atom_id, _old_value, new_value) in &self.changed {
            match prov {
                HybridizationProvenance::Base => {
                    base_map.insert(atom_id, new_value);
                }
                HybridizationProvenance::Diff => {
                    diff_map.insert(atom_id, new_value);
                }
            }
        }
    }

    /// Apply this delta in the reverse (undo) direction to the given maps.
    pub fn apply_undo(
        &self,
        base_map: &mut HashMap<u32, u8>,
        diff_map: &mut HashMap<u32, u8>,
    ) {
        // Remove what was added
        for &(prov, atom_id, _value) in &self.added {
            match prov {
                HybridizationProvenance::Base => {
                    base_map.remove(&atom_id);
                }
                HybridizationProvenance::Diff => {
                    diff_map.remove(&atom_id);
                }
            }
        }
        // Re-add what was removed
        for &(prov, atom_id, old_value) in &self.removed {
            match prov {
                HybridizationProvenance::Base => {
                    base_map.insert(atom_id, old_value);
                }
                HybridizationProvenance::Diff => {
                    diff_map.insert(atom_id, old_value);
                }
            }
        }
        // Restore old values for changed entries
        for &(prov, atom_id, old_value, _new_value) in &self.changed {
            match prov {
                HybridizationProvenance::Base => {
                    base_map.insert(atom_id, old_value);
                }
                HybridizationProvenance::Diff => {
                    diff_map.insert(atom_id, old_value);
                }
            }
        }
    }
}

/// Diff a single pair of before/after maps, appending to the delta.
fn diff_maps(
    before: &HashMap<u32, u8>,
    after: &HashMap<u32, u8>,
    prov: HybridizationProvenance,
    delta: &mut HybridizationDelta,
) {
    // Check for removed and changed entries
    for (&atom_id, &old_value) in before {
        match after.get(&atom_id) {
            None => {
                // Entry was removed
                delta.removed.push((prov, atom_id, old_value));
            }
            Some(&new_value) if new_value != old_value => {
                // Entry value changed
                delta.changed.push((prov, atom_id, old_value, new_value));
            }
            _ => {} // Unchanged
        }
    }
    // Check for added entries
    for (&atom_id, &new_value) in after {
        if !before.contains_key(&atom_id) {
            delta.added.push((prov, atom_id, new_value));
        }
    }
}
