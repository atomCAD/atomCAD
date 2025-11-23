pub mod fill_algorithm;
pub mod surface_reconstruction;
pub mod placed_atom_tracker;

// Re-export main API
pub use fill_algorithm::{fill_lattice, LatticeFillConfig, LatticeFillOptions, LatticeFillResult};
pub use placed_atom_tracker::{PlacedAtomTracker, CrystallographicAddress};
pub use surface_reconstruction::reconstruct_surface;