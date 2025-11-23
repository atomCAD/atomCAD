use serde::{Serialize, Deserialize};
use std::hash::{Hash, Hasher};

// BondReference can be used to refer to a bond globally:
// without being in the context of an atom.
// The order of the atoms is irrelevant: two bond references between the same two atoms are equal. 
#[derive(Clone,Debug, Serialize, Deserialize)]
pub struct BondReference {
  pub atom_id1: u32,
  pub atom_id2: u32,
}

impl PartialEq for BondReference {
  fn eq(&self, other: &Self) -> bool {
    // Order doesn't matter for bonds: (1,2) == (2,1)
    (self.atom_id1 == other.atom_id1 && self.atom_id2 == other.atom_id2) ||
    (self.atom_id1 == other.atom_id2 && self.atom_id2 == other.atom_id1)
  }
}

impl Eq for BondReference {}

impl Hash for BondReference {
  fn hash<H: Hasher>(&self, state: &mut H) {
    // Consistent hash regardless of atom order
    let (smaller, larger) = if self.atom_id1 < self.atom_id2 {
      (self.atom_id1, self.atom_id2)
    } else {
      (self.atom_id2, self.atom_id1)
    };
    smaller.hash(state);
    larger.hash(state);
  }
}




