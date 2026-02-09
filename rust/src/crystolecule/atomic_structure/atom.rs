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
    pub flags: u16, // Bit 0: selected, Bit 1: hydrogen passivation
}

const ATOM_FLAG_SELECTED: u16 = 1 << 0;
const ATOM_FLAG_HYDROGEN_PASSIVATION: u16 = 1 << 1;

impl Atom {
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
}
