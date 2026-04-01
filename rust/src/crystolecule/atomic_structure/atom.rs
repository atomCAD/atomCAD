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
    pub flags: u16, // Bit 0: selected, Bit 1: hydrogen passivation, Bit 2: frozen, Bits 3-4: hybridization override
}

const ATOM_FLAG_SELECTED: u16 = 1 << 0;
const ATOM_FLAG_HYDROGEN_PASSIVATION: u16 = 1 << 1;
const ATOM_FLAG_FROZEN: u16 = 1 << 2;
const ATOM_FLAG_HYBRIDIZATION_MASK: u16 = 0b11 << 3;
const ATOM_FLAG_HYBRIDIZATION_SHIFT: u16 = 3;
const ATOM_FLAG_GHOST: u16 = 1 << 5;

pub const HYBRIDIZATION_AUTO: u8 = 0;
pub const HYBRIDIZATION_SP3: u8 = 1;
pub const HYBRIDIZATION_SP2: u8 = 2;
pub const HYBRIDIZATION_SP1: u8 = 3;

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

    /// Returns the hybridization override (0=Auto, 1=Sp3, 2=Sp2, 3=Sp1).
    #[inline]
    pub fn hybridization_override(&self) -> u8 {
        ((self.flags & ATOM_FLAG_HYBRIDIZATION_MASK) >> ATOM_FLAG_HYBRIDIZATION_SHIFT) as u8
    }

    #[inline]
    pub fn set_hybridization_override(&mut self, hybridization: u8) {
        self.flags = (self.flags & !ATOM_FLAG_HYBRIDIZATION_MASK)
            | (((hybridization as u16) & 0b11) << ATOM_FLAG_HYBRIDIZATION_SHIFT);
    }

    /// Returns true if this atom is a ghost copy from a neighboring unit cell.
    /// Ghost atoms are generated for display only in motif_edit mode.
    #[inline]
    pub fn is_ghost(&self) -> bool {
        (self.flags & ATOM_FLAG_GHOST) != 0
    }

    #[inline]
    pub fn set_ghost(&mut self, ghost: bool) {
        if ghost {
            self.flags |= ATOM_FLAG_GHOST;
        } else {
            self.flags &= !ATOM_FLAG_GHOST;
        }
    }
}
