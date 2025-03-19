use std::collections::HashMap;
use lazy_static::lazy_static;

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
}