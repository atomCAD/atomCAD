// TODO: these will not be constant, will be set by the user
use glam::i32::IVec3;
use glam::f32::Vec3;
use std::collections::HashMap;
use lazy_static::lazy_static;

pub const IMPLICIT_VOLUME_MIN: IVec3 = IVec3::new(-200, -200, -200);
pub const IMPLICIT_VOLUME_MAX: IVec3 = IVec3::new(200, 200, 200);

pub const IMPLICIT_VOLUME_UNIT_CELLS_MIN: IVec3 = IVec3::new(-50, -50, -50);
pub const IMPLICIT_VOLUME_UNIT_CELLS_MAX: IVec3 = IVec3::new(50, 50, 50);

pub const MAX_EVAL_CACHE_SIZE: i32 = 200000;

pub const DIAMOND_UNIT_CELL_SIZE_ANGSTROM: f64 = 3.567;  // Size of one complete unit cell in Ångströms

// Gadget constants
pub const HANDLE_TRIANGLE_SIDE_LENGTH: f64 = 1.0;
pub const HANDLE_RADIUS: f64 = 0.4;
pub const HANDLE_DIVISIONS: u32 = 16;
pub const HANDLE_HEIGHT: f64 = 0.6;
pub const HANDLE_COLOR: Vec3 = Vec3::new(0.0, 0.0, 0.8); // Dark blue for handles
pub const SELECTED_HANDLE_COLOR: Vec3 = Vec3::new(1.0, 0.6, 0.0); // Orange for selected handle
pub const LINE_COLOR: Vec3 = Vec3::new(0.0, 0.0, 0.6);
pub const LINE_DIVISIONS: u32 = 16;
pub const LINE_RADIUS: f64 = 0.15;

#[derive(Clone, Debug)]
pub struct CrystalTypeInfo {
    pub primary_atomic_number: i32,
    pub secondary_atomic_number: i32,
    pub unit_cell_size: f64,
    pub name: String,
}

// Function to create a CrystalTypeInfo object
fn create_crystal_info(primary: i32, secondary: i32, size: f64, name: &str) -> CrystalTypeInfo {
    CrystalTypeInfo {
        primary_atomic_number: primary,
        secondary_atomic_number: secondary,
        unit_cell_size: size,
        name: name.to_string(),
    }
}

// Non-public function that returns a Vec of all crystal types
fn get_all_crystal_types() -> Vec<CrystalTypeInfo> {
    vec![
        // Diamond structures
        create_crystal_info(6, 6, 3.567, "Diamond (Carbon)"),
        create_crystal_info(14, 14, 5.431020511, "Silicon"),
        create_crystal_info(32, 32, 5.658, "Germanium"),
        
        // Common zinc blende structures
        create_crystal_info(13, 33, 5.6605, "AlAs (Aluminum Arsenide)"),
        create_crystal_info(13, 15, 5.451, "AlP (Aluminum Phosphide)"),
        create_crystal_info(13, 51, 6.1355, "AlSb (Aluminum Antimonide)"),
        create_crystal_info(5, 7, 3.6150, "BN (Boron Nitride)"),
        create_crystal_info(5, 15, 4.5380, "BP (Boron Phosphide)"),
        create_crystal_info(48, 16, 5.8320, "CdS (Cadmium Sulfide)"),
        create_crystal_info(48, 34, 6.050, "CdSe (Cadmium Selenide)"),
        create_crystal_info(48, 52, 6.482, "CdTe (Cadmium Telluride)"),
        create_crystal_info(31, 33, 5.653, "GaAs (Gallium Arsenide)"),
        create_crystal_info(31, 15, 5.4505, "GaP (Gallium Phosphide)"),
        create_crystal_info(31, 51, 6.0959, "GaSb (Gallium Antimonide)"),
        create_crystal_info(49, 33, 6.0583, "InAs (Indium Arsenide)"),
        create_crystal_info(49, 15, 5.869, "InP (Indium Phosphide)"),
        create_crystal_info(49, 51, 6.479, "InSb (Indium Antimonide)"),
        create_crystal_info(30, 16, 5.420, "ZnS (Zinc Sulfide)"),
    ]
}

// Initialize both the map and vector from the same source data
lazy_static! {
    // Vector of crystal information in the defined order, with reversed entries for different elements
    pub static ref CRYSTAL_INFO_VEC: Vec<CrystalTypeInfo> = {
        let mut vec = Vec::new();
        
        // Add all original crystal types
        for crystal in get_all_crystal_types() {
            // Add the original crystal type
            vec.push(crystal.clone());
            
            // If primary and secondary elements are different, add a reversed entry
            if crystal.primary_atomic_number != crystal.secondary_atomic_number {
                let reversed = CrystalTypeInfo {
                    primary_atomic_number: crystal.secondary_atomic_number,
                    secondary_atomic_number: crystal.primary_atomic_number,
                    unit_cell_size: crystal.unit_cell_size,
                    name: format!("{}*", crystal.name),
                };
                vec.push(reversed);
            }
        }
        
        vec
    };
    
    // Map of crystal information for quick lookup
    pub static ref CRYSTAL_INFO_MAP: HashMap<(i32, i32), CrystalTypeInfo> = {
        let mut map = HashMap::new();
        for crystal in get_all_crystal_types() {
            map.insert(
                (crystal.primary_atomic_number, crystal.secondary_atomic_number),
                crystal
                );
        }
        map
    };
}
