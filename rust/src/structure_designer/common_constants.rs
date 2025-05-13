// TODO: these will not be constant, will be set by the user
use glam::i32::IVec3;
use std::collections::HashMap;
use lazy_static::lazy_static;

pub const IMPLICIT_VOLUME_MIN: IVec3 = IVec3::new(-10, -10, -10);
pub const IMPLICIT_VOLUME_MAX: IVec3 = IVec3::new(10, 10, 10);

pub const DIAMOND_UNIT_CELL_SIZE_ANGSTROM: f64 = 3.567;  // Size of one complete unit cell in Ångströms

// Map of known unit cell sizes for various element pairs (in Ångströms)
// Key is a tuple of (primary_atomic_number, secondary_atomic_number)
// Values are measured unit cell sizes for zinc blende crystal structures
lazy_static! {
pub static ref UNIT_CELL_SIZES: HashMap<(i32, i32), f64> = {
    let mut map = HashMap::new();
    
    // Diamond structures
    map.insert((6, 6), 3.567);  // C-C (Diamond)
    map.insert((14, 14), 5.431020511);  // Si-Si (Silicon)
    map.insert((32, 32), 5.658);  // Ge-Ge (Germanium)
    
    // Common zinc blende structures
    map.insert((13, 33), 5.6605);  // AlAs (Aluminum Arsenide)
    map.insert((13, 15), 5.451);  // AlP (Aluminum Phosphide)
    map.insert((13, 51), 6.1355);  // AlSb (Aluminum Antimonide)
    map.insert((5, 7), 3.6150);  // BN (Boron Nitride)
    map.insert((5, 15), 4.5380);  // BP (Boron Phosphide)
    map.insert((48, 16), 5.8320);  // CdS (Cadmium Sulfide)
    map.insert((48, 34), 5.8320);  // CdSe (Cadmium Selenide)
    map.insert((48, 52), 5.8320);  // CdTe (Cadmium Telluride)
    map.insert((31, 33), 5.653);  // GaAs (Gallium Arsenide)
    map.insert((31, 15), 5.4505);  // GaP (Gallium Phosphide)
    map.insert((31, 51), 6.0959);  // GaSb (Gallium Antimonide)
    map.insert((49, 33), 6.0583);  // InAs (Indium Arsenide)
    map.insert((49, 15), 5.869);  // InP (Indium Phosphide)
    map.insert((49, 51), 6.479);  // InSb (Indium Antimonide)
    map.insert((30, 16), 5.420);  // ZnS (Zinc Sulfide - actual zinc blende)

    map
};
}
