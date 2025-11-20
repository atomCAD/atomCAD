use indexmap::IndexMap;
use rustc_hash::FxBuildHasher;
use glam::i32::IVec3;

type FxIndexMap<K, V> = IndexMap<K, V, FxBuildHasher>;

#[derive(Debug, Clone)]
pub struct PlacedAtomTracker {
  // Primary storage: maps (motif_space_pos, site_index) -> atom_id
  atom_map: FxIndexMap<(IVec3, usize), u32>,
}

impl PlacedAtomTracker {
  pub fn new() -> Self {
    PlacedAtomTracker {
      atom_map: FxIndexMap::default(),
    }
  }
  
  /// Records that an atom was placed at the given motif space position and site index
  pub fn record_atom(&mut self, motif_space_pos: IVec3, site_index: usize, atom_id: u32) {
    self.atom_map.insert((motif_space_pos, site_index), atom_id);
  }
  
  /// Looks up the atom ID for a given motif space position and site index
  pub fn get_atom_id(&self, motif_space_pos: IVec3, site_index: usize) -> Option<u32> {
    self.atom_map.get(&(motif_space_pos, site_index)).copied()
  }
  
  /// Gets atom ID for a site specifier (handles relative cell offsets)
  pub fn get_atom_id_for_specifier(
    &self, 
    base_motif_space_pos: IVec3, 
    site_specifier: &crate::structure_designer::evaluator::motif::SiteSpecifier
  ) -> Option<u32> {
    let target_motif_space_pos = base_motif_space_pos + site_specifier.relative_cell;
    self.get_atom_id(target_motif_space_pos, site_specifier.site_index)
  }
  
  /// Returns an iterator over all placed atoms: (lattice_pos, site_index, atom_id)
  pub fn iter_atoms(&self) -> impl Iterator<Item = (IVec3, usize, u32)> + '_ {
    self.atom_map.iter().map(|((motif_space_pos, site_index), &atom_id)| (*motif_space_pos, *site_index, atom_id))
  }
}
