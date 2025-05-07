use std::collections::HashMap;
use glam::f32::Vec3;
use lazy_static::lazy_static;

#[derive(Clone)]
pub struct AtomInfo {
    pub symbol: String,
    pub atomic_number: i32,
    pub element_name: String,
    pub radius: f64,
    pub color: Vec3,
}

lazy_static! {
    /// HashMap containing chemical elements (as uppercase symbols) and their atomic numbers
    pub static ref CHEMICAL_ELEMENTS: HashMap<String, i32> = {
        let mut map = HashMap::new();
        for atom_info in get_all_elements() {
            map.insert(atom_info.symbol.clone(), atom_info.atomic_number);
        }
        map
    };

    pub static ref DEFAULT_ATOM_INFO: AtomInfo = AtomInfo {
        symbol: "X".to_string(),
        atomic_number: 0,
        element_name: "Unknown".to_string(),
        radius: 0.7,
        color: Vec3::new(0.5, 0.5, 0.5)  // Default gray for unknown atoms
    };

    /// HashMap containing atomic numbers and their corresponding AtomInfo
    /// Source: https://periodictable.com/Properties/A/CovalentRadius.v.log.html
    pub static ref ATOM_INFO: HashMap<i32, AtomInfo> = {
        let mut map = HashMap::new();
        for atom_info in get_all_elements() {
            map.insert(atom_info.atomic_number, atom_info);
        }
        map
    };
}

/// Creates a new AtomInfo with all the necessary information
pub fn create_atom_info(atomic_number: i32, symbol: &str, element_name: &str, radius: f64, color: Vec3) -> AtomInfo {
    AtomInfo {
        symbol: symbol.to_string(),
        atomic_number,
        element_name: element_name.to_string(),
        radius,
        color,
    }
}

/// Contains the registry of all chemical elements in the system
fn get_all_elements() -> Vec<AtomInfo> {
    vec![
        // Elements are ordered by atomic number

        create_atom_info(1,"H", "Hydrogen", 0.31, Vec3::new(1.0, 1.0, 1.0)),
        create_atom_info(2, "He", "Helium", 0.28, Vec3::new(0.85, 1.00, 1.00)),
        create_atom_info(3, "Li", "Lithium", 1.28, Vec3::new(0.80, 0.50, 1.00)),
        create_atom_info(4, "Be", "Beryllium", 0.96, Vec3::new(0.76, 1.00, 0.00)),
        create_atom_info(5, "B", "Boron", 0.85, Vec3::new(1.00, 0.71, 0.71)),
        create_atom_info(6, "C", "Carbon", 0.76, Vec3::new(0.18, 0.18, 0.18)),
        create_atom_info(7,"N", "Nitrogen", 0.71, Vec3::new(0.187, 0.375, 0.97)),
        create_atom_info(8, "O", "Oxygen", 0.66, Vec3::new(1.0, 0.05, 0.05)),
        create_atom_info(9, "F", "Fluorine", 0.57, Vec3::new(0.56, 0.88, 0.31)),
        create_atom_info(10, "Ne", "Neon", 0.58, Vec3::new(0.70, 0.89, 0.96)),
        create_atom_info(11, "Na", "Sodium", 1.66, Vec3::new(0.67, 0.36, 0.95)),
        create_atom_info(12, "Mg", "Magnesium", 1.41, Vec3::new(0.54, 1.00, 0.00)),
        create_atom_info(13, "Al", "Aluminium", 1.21, Vec3::new(0.75, 0.65, 0.65)),
        create_atom_info(14, "Si", "Silicon", 1.11, Vec3::new(0.94, 0.78, 0.63)),
        create_atom_info(15, "P", "Phosphorus", 1.07, Vec3::new(1.00, 0.50, 0.00)),
        create_atom_info(16, "S", "Sulfur", 1.05, Vec3::new(1.00, 1.00, 0.19)),
        create_atom_info(17, "Cl", "Chlorine", 1.02, Vec3::new(0.12, 0.94, 0.12)),
        create_atom_info(18, "Ar", "Argon", 1.06, Vec3::new(0.50, 0.82, 0.89)),
        create_atom_info(19, "K", "Potassium", 2.03, Vec3::new(0.56, 0.25, 0.83)),
        create_atom_info(20, "Ca", "Calcium", 1.76, Vec3::new(0.24, 1.00, 0.00)),
        create_atom_info(21, "Sc", "Scandium", 1.7, Vec3::new(0.90, 0.90, 0.90)),
        create_atom_info(22, "Ti", "Titanium", 1.6, Vec3::new(0.75, 0.76, 0.78)),
        create_atom_info(23, "V", "Vanadium", 1.53, Vec3::new(0.65, 0.65, 0.67)),
        create_atom_info(24, "Cr", "Chromium", 1.39, Vec3::new(0.54, 0.60, 0.78)),
        create_atom_info(25, "Mn", "Manganese", 1.39, Vec3::new(0.61, 0.48, 0.78)),
        create_atom_info(26, "Fe", "Iron", 1.32, Vec3::new(0.88, 0.40, 0.20)),
        create_atom_info(27, "Co", "Cobalt", 1.26, Vec3::new(0.94, 0.56, 0.63)),
        create_atom_info(28, "Ni", "Nickel", 1.24, Vec3::new(0.31, 0.82, 0.31)),
        create_atom_info(29, "Cu", "Copper", 1.32, Vec3::new(0.78, 0.50, 0.20)),
        create_atom_info(30, "Zn", "Zinc", 1.22, Vec3::new(0.49, 0.50, 0.69)),
        create_atom_info(31, "Ga", "Gallium", 1.22, Vec3::new(0.76, 0.56, 0.56)),
        create_atom_info(32, "Ge", "Germanium", 1.2, Vec3::new(0.4, 0.56, 0.56)),
        create_atom_info(33, "As", "Arsenic", 1.19, Vec3::new(0.74, 0.50, 0.89)),
        create_atom_info(34, "Se", "Selenium", 1.2, Vec3::new(1.00, 0.63, 0.00)),
        create_atom_info(35, "Br", "Bromine", 1.2, Vec3::new(0.65, 0.16, 0.16)),
        create_atom_info(36, "Kr", "Krypton", 1.16, Vec3::new(0.36, 0.72, 0.82)),
        create_atom_info(37, "Rb", "Rubidium", 2.2, Vec3::new(0.44, 0.18, 0.69)),
        create_atom_info(38, "Sr", "Strontium", 1.95, Vec3::new(0.00, 1.00, 0.00)),
        create_atom_info(39, "Y", "Yttrium", 1.9, Vec3::new(0.58, 1.00, 1.00)),
        create_atom_info(40, "Zr", "Zirconium", 1.75, Vec3::new(0.58, 0.88, 0.88)),
        create_atom_info(41, "Nb", "Niobium", 1.64, Vec3::new(0.45, 0.76, 0.79)),
        create_atom_info(42, "Mo", "Molybdenum", 1.54, Vec3::new(0.33, 0.71, 0.71)),
        create_atom_info(43, "Tc", "Technetium", 1.47, Vec3::new(0.23, 0.62, 0.62)),
        create_atom_info(44, "Ru", "Ruthenium", 1.46, Vec3::new(0.14, 0.56, 0.56)),
        create_atom_info(45, "Rh", "Rhodium", 1.42, Vec3::new(0.04, 0.49, 0.55)),
        create_atom_info(46, "Pd", "Palladium", 1.39, Vec3::new(0.00, 0.41, 0.52)),
        create_atom_info(47, "Ag", "Silver", 1.45, Vec3::new(0.75, 0.75, 0.75)),
        create_atom_info(48, "Cd", "Cadmium", 1.44, Vec3::new(1.00, 0.85, 0.56)),
        create_atom_info(49, "In", "Indium", 1.42, Vec3::new(0.65, 0.46, 0.45)),
        create_atom_info(50, "Sn", "Tin", 1.39, Vec3::new(0.40, 0.50, 0.50)),
        create_atom_info(51, "Sb", "Antimony", 1.39, Vec3::new(0.62, 0.39, 0.71)),
        create_atom_info(52, "Te", "Tellurium", 1.38, Vec3::new(0.83, 0.48, 0.00)),
        create_atom_info(53, "I", "Iodine", 1.39, Vec3::new(0.58, 0.00, 0.58)),
        create_atom_info(54, "Xe", "Xenon", 1.4, Vec3::new(0.26, 0.62, 0.69)),
        create_atom_info(55, "Cs", "Cesium", 2.44, Vec3::new(0.34, 0.09, 0.56)),
        create_atom_info(56, "Ba", "Barium", 2.15, Vec3::new(0.00, 0.79, 0.00)),
        create_atom_info(57, "La", "Lanthanum", 2.07, Vec3::new(0.44, 0.83, 1.00)),
        create_atom_info(58, "Ce", "Cerium", 2.04, Vec3::new(1.00, 1.00, 0.78)),
        create_atom_info(59, "Pr", "Praseodymium", 2.03, Vec3::new(0.85, 1.00, 0.78)),
        create_atom_info(60, "Nd", "Neodymium", 2.01, Vec3::new(0.78, 1.00, 0.78)),
        create_atom_info(61, "Pm", "Promethium", 1.99, Vec3::new(0.64, 1.00, 0.78)),
        create_atom_info(62, "Sm", "Samarium", 1.98, Vec3::new(0.56, 1.00, 0.78)),
        create_atom_info(63, "Eu", "Europium", 1.98, Vec3::new(0.38, 1.00, 0.78)),
        create_atom_info(64, "Gd", "Gadolinium", 1.96, Vec3::new(0.27, 1.00, 0.78)),
        create_atom_info(65, "Tb", "Terbium", 1.94, Vec3::new(0.19, 1.00, 0.78)),
        create_atom_info(66, "Dy", "Dysprosium", 1.92, Vec3::new(0.12, 1.00, 0.78)),
        create_atom_info(67, "Ho", "Holmium", 1.92, Vec3::new(0.00, 1.00, 0.61)),
        create_atom_info(68, "Er", "Erbium", 1.89, Vec3::new(0.00, 0.90, 0.46)),
        create_atom_info(69, "Tm", "Thulium", 1.9, Vec3::new(0.00, 0.83, 0.32)),
        create_atom_info(70, "Yb", "Ytterbium", 1.87, Vec3::new(0.00, 0.75, 0.22)),
        create_atom_info(71, "Lu", "Lutetium", 1.87, Vec3::new(0.00, 0.67, 0.14)),
        create_atom_info(72, "Hf", "Hafnium", 1.75, Vec3::new(0.30, 0.76, 1.00)),
        create_atom_info(73, "Ta", "Tantalum", 1.7, Vec3::new(0.30, 0.65, 1.00)),
        create_atom_info(74, "W", "Tungsten", 1.62, Vec3::new(0.13, 0.58, 0.84)),
        create_atom_info(75, "Re", "Rhenium", 1.51, Vec3::new(0.15, 0.49, 0.67)),
        create_atom_info(76, "Os", "Osmium", 1.44, Vec3::new(0.15, 0.40, 0.59)),
        create_atom_info(77, "Ir", "Iridium", 1.41, Vec3::new(0.09, 0.33, 0.53)),
        create_atom_info(78, "Pt", "Platinum", 1.36, Vec3::new(0.82, 0.82, 0.88)),
        create_atom_info(79, "Au", "Gold", 1.36, Vec3::new(1.00, 0.82, 0.14)),
        create_atom_info(80, "Hg", "Mercury", 1.32, Vec3::new(0.72, 0.72, 0.82)),
        create_atom_info(81, "Tl", "Thallium", 1.45, Vec3::new(0.65, 0.33, 0.30)),
        create_atom_info(82, "Pb", "Lead", 1.46, Vec3::new(0.34, 0.35, 0.38)),
        create_atom_info(83, "Bi", "Bismuth", 1.48, Vec3::new(0.62, 0.31, 0.71)),
        create_atom_info(84, "Po", "Polonium", 1.4, Vec3::new(0.67, 0.36, 0.00)),
        create_atom_info(85, "At", "Astatine", 1.5, Vec3::new(0.46, 0.31, 0.27)),
        create_atom_info(86, "Rn", "Radon", 1.5, Vec3::new(0.26, 0.51, 0.59)),
        create_atom_info(87, "Fr", "Francium", 2.6, Vec3::new(0.26, 0.00, 0.40)),
        create_atom_info(88, "Ra", "Radium", 2.21, Vec3::new(0.00, 0.49, 0.00)),
        create_atom_info(89, "Ac", "Actinium", 2.15, Vec3::new(0.44, 0.67, 0.98)),
        create_atom_info(90, "Th", "Thorium", 2.06, Vec3::new(0.00, 0.73, 1.00)),
        create_atom_info(91, "Pa", "Protactinium", 2.0, Vec3::new(0.00, 0.63, 1.00)),
        create_atom_info(92, "U", "Uranium", 1.96, Vec3::new(0.00, 0.56, 1.00)),
        create_atom_info(93, "Np", "Neptunium", 1.9, Vec3::new(0.00, 0.50, 1.00)),
        create_atom_info(94, "Pu", "Plutonium", 1.87, Vec3::new(0.00, 0.42, 1.00)),
        create_atom_info(95, "Am", "Americium", 1.8, Vec3::new(0.33, 0.36, 0.95)),
        create_atom_info(96, "Cm", "Curium", 1.69, Vec3::new(0.47, 0.36, 0.89)),
    ]
}

/// Initializes the CHEMICAL_ELEMENTS map from the element registry
fn initialize_chemical_elements() -> HashMap<String, i32> {
    let mut elements = HashMap::new();
    for atom_info in get_all_elements() {
        elements.insert(atom_info.symbol.clone(), atom_info.atomic_number);
    }
    elements
}

/// Initializes the ATOM_INFO map from the element registry
fn initialize_atom_info() -> HashMap<i32, AtomInfo> {
    let mut info_map = HashMap::new();
    for atom_info in get_all_elements() {
        info_map.insert(atom_info.atomic_number, atom_info);
    }
    info_map
}
