use glam::f64::DVec3;

/// State of an atom in the diff at a point in time.
#[derive(Debug, Clone)]
pub struct AtomState {
    pub atomic_number: i16,
    pub position: DVec3,
    pub anchor: Option<DVec3>,
    pub flags: u16,
}

/// A change to a single atom in the diff.
///
/// Three cases:
/// - `before=None, after=Some`: Atom was **added**
/// - `before=Some, after=None`: Atom was **removed**
/// - `before=Some, after=Some`: Atom was **modified**
#[derive(Debug, Clone)]
pub struct AtomDelta {
    pub atom_id: u32,
    /// None if atom didn't exist before (was added).
    pub before: Option<AtomState>,
    /// None if atom doesn't exist after (was removed).
    pub after: Option<AtomState>,
}

/// A change to a bond in the diff.
#[derive(Debug, Clone)]
pub struct BondDelta {
    pub atom_id1: u32,
    pub atom_id2: u32,
    /// None if bond didn't exist before.
    pub old_order: Option<u8>,
    /// None if bond doesn't exist after.
    pub new_order: Option<u8>,
}

/// Captures diff deltas during a recording session.
///
/// Hybridization overrides are now stored as atom flags and captured in AtomDelta.
#[derive(Debug, Default)]
pub struct DiffRecorder {
    pub atom_deltas: Vec<AtomDelta>,
    pub bond_deltas: Vec<BondDelta>,
}

impl DiffRecorder {
    /// Coalesce consecutive deltas for the same atom into a single delta.
    ///
    /// Coalescing rules:
    /// - Two consecutive Modified deltas for same atom → merge (before from first, after from second)
    /// - Added followed by Modified for same atom → single Added with final state
    /// - Modified followed by Removed for same atom → single Removed with original before-state
    pub fn coalesce(&mut self) {
        if self.atom_deltas.len() < 2 {
            return;
        }

        let mut result: Vec<AtomDelta> = Vec::with_capacity(self.atom_deltas.len());

        for delta in self.atom_deltas.drain(..) {
            // Try to merge with the last delta in result if same atom_id
            let merged = if let Some(last) = result.last_mut() {
                if last.atom_id == delta.atom_id {
                    match (&last.before, &last.after, &delta.before, &delta.after) {
                        // Added + Modified → Added with final state
                        (None, Some(_), Some(_), Some(new_after)) => {
                            last.after = Some(new_after.clone());
                            true
                        }
                        // Modified + Modified → Modified (first.before, second.after)
                        (Some(_), Some(_), Some(_), Some(new_after)) => {
                            last.after = Some(new_after.clone());
                            true
                        }
                        // Modified + Removed → Removed with original before
                        (Some(_), Some(_), Some(_), None) => {
                            last.after = None;
                            true
                        }
                        // Added + Removed → cancel out (remove the last entry)
                        (None, Some(_), Some(_), None) => {
                            result.pop();
                            true
                        }
                        _ => false,
                    }
                } else {
                    false
                }
            } else {
                false
            };

            if !merged {
                result.push(delta);
            }
        }

        self.atom_deltas = result;
    }
}
