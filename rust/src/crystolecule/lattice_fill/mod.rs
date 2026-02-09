pub mod config;
pub mod fill_algorithm;
pub mod hydrogen_passivation;
pub mod placed_atom_tracker;
pub mod surface_reconstruction;

// Re-export main API
pub use config::{LatticeFillConfig, LatticeFillOptions, LatticeFillResult, LatticeFillStatistics};
pub use fill_algorithm::fill_lattice;
pub use placed_atom_tracker::{CrystallographicAddress, PlacedAtomTracker};
pub use surface_reconstruction::reconstruct_surface;
