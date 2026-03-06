use crate::crystolecule::atomic_structure::inline_bond::InlineBond;
use glam::f64::DVec3;
use smallvec::SmallVec;

#[derive(Debug, Clone)]
pub struct Atom {
    pub position: DVec3,
    pub bonds: SmallVec<[InlineBond; 4]>,
    pub id: u32,
    pub in_crystal_depth: f32,
    pub atomic_number: i16,
    pub flags: u16, // Bit 0: selected, Bit 1: hydrogen passivation, Bit 2: frozen
}

const ATOM_FLAG_SELECTED: u16 = 1 << 0;
const ATOM_FLAG_HYDROGEN_PASSIVATION: u16 = 1 << 1;
const ATOM_FLAG_FROZEN: u16 = 1 << 2;

impl Atom {
    /// Returns true if this atom is a delete marker in a diff structure.
    /// Delete markers have atomic_number == 0.
    #[inline]
    pub fn is_delete_marker(&self) -> bool {
        self.atomic_number == super::DELETED_SITE_ATOMIC_NUMBER
    }

    /// Returns true if this atom is an unchanged marker in a diff structure.
    /// Unchanged markers have atomic_number == -1 and represent bond endpoint
    /// references to base atoms that are not being modified.
    #[inline]
    pub fn is_unchanged_marker(&self) -> bool {
        self.atomic_number == super::UNCHANGED_ATOMIC_NUMBER
    }

    /// Returns true if this atom is any special marker (delete or unchanged).
    #[inline]
    pub fn is_special_marker(&self) -> bool {
        self.is_delete_marker() || self.is_unchanged_marker()
    }

    #[inline]
    pub fn is_selected(&self) -> bool {
        (self.flags & ATOM_FLAG_SELECTED) != 0
    }

    #[inline]
    pub fn set_selected(&mut self, selected: bool) {
        if selected {
            self.flags |= ATOM_FLAG_SELECTED;
        } else {
            self.flags &= !ATOM_FLAG_SELECTED;
        }
    }

    #[inline]
    pub fn is_hydrogen_passivation(&self) -> bool {
        (self.flags & ATOM_FLAG_HYDROGEN_PASSIVATION) != 0
    }

    #[inline]
    pub fn set_hydrogen_passivation(&mut self, is_passivation: bool) {
        if is_passivation {
            self.flags |= ATOM_FLAG_HYDROGEN_PASSIVATION;
        } else {
            self.flags &= !ATOM_FLAG_HYDROGEN_PASSIVATION;
        }
    }

    #[inline]
    pub fn is_frozen(&self) -> bool {
        (self.flags & ATOM_FLAG_FROZEN) != 0
    }

    #[inline]
    pub fn set_frozen(&mut self, frozen: bool) {
        if frozen {
            self.flags |= ATOM_FLAG_FROZEN;
        } else {
            self.flags &= !ATOM_FLAG_FROZEN;
        }
    }
}
