/// Ultra-compact inline bond representation - 4 bytes total
/// Stores bond information directly in the atom's SmallVec for maximum cache efficiency
/// 
/// Memory layout: 29 bits atom_id (max 536M atoms) + 3 bits bond_order (8 types)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlineBond {
  /// Packed data: lower 29 bits = other_atom_id, upper 3 bits = bond_order
  packed: u32,
}

pub const BOND_SINGLE: u8 = 1;
pub const BOND_DOUBLE: u8 = 2;
pub const BOND_TRIPLE: u8 = 3;
pub const BOND_QUADRUPLE: u8 = 4;
pub const BOND_AROMATIC: u8 = 5;
pub const BOND_DATIVE: u8 = 6;
pub const BOND_METALLIC: u8 = 7;

impl InlineBond {
  const ATOM_ID_MASK: u32 = 0x1FFFFFFF;      // 29 bits
  const BOND_ORDER_SHIFT: u32 = 29;
  const BOND_ORDER_MASK: u32 = 0x7;
  
  #[inline]
  pub fn new(other_atom_id: u32, bond_order: u8) -> Self {
    debug_assert!(other_atom_id <= Self::ATOM_ID_MASK, 
      "Atom ID {} exceeds maximum of {}", other_atom_id, Self::ATOM_ID_MASK);
    debug_assert!(bond_order <= Self::BOND_ORDER_MASK as u8,
      "Bond order {} exceeds maximum of {}", bond_order, Self::BOND_ORDER_MASK);
    
    Self {
      packed: other_atom_id | ((bond_order as u32) << Self::BOND_ORDER_SHIFT)
    }
  }
  
  #[inline]
  pub fn other_atom_id(&self) -> u32 {
    self.packed & Self::ATOM_ID_MASK
  }
  
  #[inline]
  pub fn bond_order(&self) -> u8 {
    ((self.packed >> Self::BOND_ORDER_SHIFT) & Self::BOND_ORDER_MASK) as u8
  }
  
  #[inline]
  pub fn set_bond_order(&mut self, bond_order: u8) {
    debug_assert!(bond_order <= Self::BOND_ORDER_MASK as u8,
      "Bond order {} exceeds maximum of {}", bond_order, Self::BOND_ORDER_MASK);
    self.packed = (self.packed & Self::ATOM_ID_MASK) | ((bond_order as u32) << Self::BOND_ORDER_SHIFT);
  }
}
















