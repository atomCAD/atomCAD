use crate::structure_designer::nodes::geo_to_atom;
use crate::api::structure_designer::structure_designer_api_types::APICrystalTypeInfo;

/// Returns the unit cell size for a given element pair
/// If the pair has a known measured unit cell size, returns that
/// Otherwise, estimates based on covalent radii
#[flutter_rust_bridge::frb(sync)]
pub fn get_unit_cell_size(primary_atomic_number: i32, secondary_atomic_number: i32) -> f64 {
  geo_to_atom::get_unit_cell_size(primary_atomic_number, secondary_atomic_number)
}

/// Returns whether the unit cell size for a given element pair is estimated or measured
#[flutter_rust_bridge::frb(sync)]
pub fn is_unit_cell_size_estimated(primary_atomic_number: i32, secondary_atomic_number: i32) -> bool {
  geo_to_atom::is_unit_cell_size_estimated(primary_atomic_number, secondary_atomic_number)
}

/// Returns a list of all available crystal types
#[flutter_rust_bridge::frb(sync)]
pub fn get_crystal_types() -> Vec<APICrystalTypeInfo> {
  geo_to_atom::get_crystal_types()
    .into_iter()
    .map(|info| APICrystalTypeInfo {
      primary_atomic_number: info.primary_atomic_number,
      secondary_atomic_number: info.secondary_atomic_number,
      unit_cell_size: info.unit_cell_size,
      name: info.name,
    })
    .collect()
}
