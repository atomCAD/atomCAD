use indexmap::IndexMap;
use rustc_hash::FxBuildHasher;
use glam::i32::IVec3;

type FxIndexMap<K, V> = IndexMap<K, V, FxBuildHasher>;

/// A crystallographic address uniquely identifying an atom site in the crystal structure.
/// 
/// In crystallography, an address consists of:
/// - Unit cell coordinates (here in motif space, which may be offset from lattice space in atomCAD)
/// - Basis index (which site/atom within the unit cell)
/// 
/// Note: In atomCAD, motif space coordinates may be slightly offset from pure lattice space
/// coordinates if the motif has been offset. However, the concept is the same - these are
/// the integer coordinates identifying which unit cell we're in, plus which site within that cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CrystallographicAddress {
  /// Position in motif space (unit cell coordinates)
  pub motif_space_pos: IVec3,
  /// Site index within the unit cell (basis index)
  pub site_index: usize,
}

impl CrystallographicAddress {
  pub fn new(motif_space_pos: IVec3, site_index: usize) -> Self {
    CrystallographicAddress {
      motif_space_pos,
      site_index,
    }
  }
}

#[derive(Debug, Clone)]
pub struct PlacedAtomTracker {
  // Primary storage: maps crystallographic address -> atom_id
  atom_map: FxIndexMap<CrystallographicAddress, u32>,
}

impl PlacedAtomTracker {
  pub fn new() -> Self {
    PlacedAtomTracker {
      atom_map: FxIndexMap::default(),
    }
  }
  
  /// Records that an atom was placed at the given motif space position and site index
  pub fn record_atom(&mut self, motif_space_pos: IVec3, site_index: usize, atom_id: u32) {
    let address = CrystallographicAddress::new(motif_space_pos, site_index);
    self.atom_map.insert(address, atom_id);
  }
  
  /// Looks up the atom ID for a given motif space position and site index
  pub fn get_atom_id(&self, motif_space_pos: IVec3, site_index: usize) -> Option<u32> {
    let address = CrystallographicAddress::new(motif_space_pos, site_index);
    self.atom_map.get(&address).copied()
  }
  
  /// Looks up the atom ID for a given crystallographic address
  pub fn get_atom_id_by_address(&self, address: &CrystallographicAddress) -> Option<u32> {
    self.atom_map.get(address).copied()
  }
  
  /// Gets atom ID for a site specifier (handles relative cell offsets)
  pub fn get_atom_id_for_specifier(
    &self, 
    base_motif_space_pos: IVec3, 
    site_specifier: &crate::crystolecule::motif::SiteSpecifier
  ) -> Option<u32> {
    let target_motif_space_pos = base_motif_space_pos + site_specifier.relative_cell;
    self.get_atom_id(target_motif_space_pos, site_specifier.site_index)
  }
  
  /// Returns an iterator over all placed atoms: (crystallographic_address, atom_id)
  pub fn iter_atoms(&self) -> impl Iterator<Item = (CrystallographicAddress, u32)> + '_ {
    self.atom_map.iter().map(|(address, &atom_id)| (*address, atom_id))
  }
}
















