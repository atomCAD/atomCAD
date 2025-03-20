use std::collections::HashMap;
use glam::f32::Vec3;
use lazy_static::lazy_static;

#[derive(Clone)]
pub struct AtomInfo {
    pub radius: f64,
    pub color: Vec3,
}

lazy_static! {
    /// HashMap containing chemical elements (as uppercase symbols) and their atomic numbers
    pub static ref CHEMICAL_ELEMENTS: HashMap<String, i32> = {
        let mut elements = HashMap::new();
        elements.insert("H".to_string(), 1);   // Hydrogen
        elements.insert("C".to_string(), 6);   // Carbon
        elements.insert("N".to_string(), 7);   // Nitrogen
        elements.insert("O".to_string(), 8);   // Oxygen
        elements
    };

    pub static ref DEFAULT_ATOM_INFO: AtomInfo = AtomInfo {
        radius: 0.7,
        color: Vec3::new(0.5, 0.5, 0.5)  // Default gray for unknown atoms
    };

    /// HashMap containing atomic numbers and their corresponding AtomInfo
    /// Source: https://periodictable.com/Properties/A/CovalentRadius.v.log.html
    pub static ref ATOM_INFO: HashMap<i32, AtomInfo> = {
        let mut m = HashMap::new();
        // Values based on common atomic radii (in Angstroms) and typical visualization colors
        m.insert(1, AtomInfo { radius: 0.31, color: Vec3::new(1.0, 1.0, 1.0) });  // Hydrogen - white
        m.insert(6, AtomInfo { radius: 0.76, color: Vec3::new(0.1, 1.0, 0.1) });  // Carbon - dark grey
        m.insert(7, AtomInfo { radius: 0.71, color: Vec3::new(0.2, 0.2, 1.0) });  // Nitrogen - blue
        m.insert(8, AtomInfo { radius: 0.66, color: Vec3::new(1.0, 0.0, 0.0) });  // Oxygen - red
        m
    };
}
