// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::mem;

use bevy::math::Vec3;
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use static_assertions::const_assert_eq;

#[allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[repr(u8)] // Oganesson == 118
pub enum Element {
    Empty = 0,
    Hydrogen,
    Helium,
    Lithium,
    Beryllium,
    Boron,
    Carbon,
    Nitrogen,
    Oxygen,
    Fluorine,
    Neon,
    Sodium,
    Magnesium,
    Aluminium,
    Silicon,
    Phosphorus,
    Sulfur,
    Chlorine,
    Argon,
    Potassium,
    Calcium,
    Scandium,
    Titanium,
    Vanadium,
    Chromium,
    Manganese,
    Iron,
    Cobalt,
    Nickel,
    Copper,
    Zinc,
    Gallium,
    Germanium,
    Arsenic,
    Selenium,
    Bromine,
    Krypton,
    Rubidium,
    Strontium,
    Yttrium,
    Zirconium,
    Niobium,
    Molybdenum,
    Technetium,
    Ruthenium,
    Rhodium,
    Palladium,
    Silver,
    Cadmium,
    Indium,
    Tin,
    Antimony,
    Tellurium,
    Iodine,
    Xenon,
    Cesium,
    Barium,
    Lanthanum,
    Cerium,
    Praseodymium,
    Neodymium,
    Promethium,
    Samarium,
    Europium,
    Gadolinium,
    Terbium,
    Dysprosium,
    Holmium,
    Erbium,
    Thulium,
    Ytterbium,
    Lutetium,
    Hafnium,
    Tantalum,
    Tungsten,
    Rhenium,
    Osmium,
    Iridium,
    Platinum,
    Gold,
    Mercury,
    Thallium,
    Lead,
    Bismuth,
    Polonium,
    Astatine,
    Radon,
    Francium,
    Radium,
    Actinium,
    Thorium,
    Protactinium,
    Uranium,
    Neptunium,
    Plutonium,
    Americium,
    Curium,
    Berkelium,
    Californium,
    Einsteinium,
    Fermium,
    Mendelevium,
    Nobelium,
    Lawrencium,
    Rutherfordium,
    Dubnium,
    Seaborgium,
    Bohrium,
    Hassium,
    Meitnerium,
    Darmstadtium,
    Roentgenium,
    Copernicium,
    Nihonium,
    Flerovium,
    Moscovium,
    Livermorium,
    Tennessine,
    Oganesson,
}
const_assert_eq!(Element::Oganesson as usize, 118);

impl Element {
    pub const EMPTY: Self = Element::Empty; // 0

    pub const MIN: Self = Element::Hydrogen; // 1
    pub const MAX: Self = Element::Oganesson; // 118

    pub fn from_symbol(symbol: &str) -> Option<Self> {
        match symbol.to_ascii_uppercase().as_str() {
            "H" => Some(Element::Hydrogen),
            "HE" => Some(Element::Helium),
            "LI" => Some(Element::Lithium),
            "BE" => Some(Element::Beryllium),
            "B" => Some(Element::Boron),
            "C" => Some(Element::Carbon),
            "N" => Some(Element::Nitrogen),
            "O" => Some(Element::Oxygen),
            "F" => Some(Element::Fluorine),
            "NE" => Some(Element::Neon),
            "NA" => Some(Element::Sodium),
            "MG" => Some(Element::Magnesium),
            "AL" => Some(Element::Aluminium),
            "SI" => Some(Element::Silicon),
            "P" => Some(Element::Phosphorus),
            "S" => Some(Element::Sulfur),
            "CL" => Some(Element::Chlorine),
            "AR" => Some(Element::Argon),
            "K" => Some(Element::Potassium),
            "CA" => Some(Element::Calcium),
            "SC" => Some(Element::Scandium),
            "TI" => Some(Element::Titanium),
            "V" => Some(Element::Vanadium),
            "CR" => Some(Element::Chromium),
            "MN" => Some(Element::Manganese),
            "FE" => Some(Element::Iron),
            "CO" => Some(Element::Cobalt),
            "NI" => Some(Element::Nickel),
            "CU" => Some(Element::Copper),
            "ZN" => Some(Element::Zinc),
            "GA" => Some(Element::Gallium),
            "GE" => Some(Element::Germanium),
            "AS" => Some(Element::Arsenic),
            "SE" => Some(Element::Selenium),
            "BR" => Some(Element::Bromine),
            "KR" => Some(Element::Krypton),
            "RB" => Some(Element::Rubidium),
            "SR" => Some(Element::Strontium),
            "Y" => Some(Element::Yttrium),
            "ZR" => Some(Element::Zirconium),
            "NB" => Some(Element::Niobium),
            "MO" => Some(Element::Molybdenum),
            "TC" => Some(Element::Technetium),
            "RU" => Some(Element::Ruthenium),
            "RH" => Some(Element::Rhodium),
            "PD" => Some(Element::Palladium),
            "AG" => Some(Element::Silver),
            "CD" => Some(Element::Cadmium),
            "IN" => Some(Element::Indium),
            "SN" => Some(Element::Tin),
            "SB" => Some(Element::Antimony),
            "TE" => Some(Element::Tellurium),
            "I" => Some(Element::Iodine),
            "XE" => Some(Element::Xenon),
            "CS" => Some(Element::Cesium),
            "BA" => Some(Element::Barium),
            "LA" => Some(Element::Lanthanum),
            "CE" => Some(Element::Cerium),
            "PR" => Some(Element::Praseodymium),
            "ND" => Some(Element::Neodymium),
            "PM" => Some(Element::Promethium),
            "SM" => Some(Element::Samarium),
            "EU" => Some(Element::Europium),
            "GD" => Some(Element::Gadolinium),
            "TB" => Some(Element::Terbium),
            "DY" => Some(Element::Dysprosium),
            "HO" => Some(Element::Holmium),
            "ER" => Some(Element::Erbium),
            "TM" => Some(Element::Thulium),
            "YB" => Some(Element::Ytterbium),
            "LU" => Some(Element::Lutetium),
            "HF" => Some(Element::Hafnium),
            "TA" => Some(Element::Tantalum),
            "W" => Some(Element::Tungsten),
            "RE" => Some(Element::Rhenium),
            "OS" => Some(Element::Osmium),
            "IR" => Some(Element::Iridium),
            "PT" => Some(Element::Platinum),
            "AU" => Some(Element::Gold),
            "HG" => Some(Element::Mercury),
            "TL" => Some(Element::Thallium),
            "PB" => Some(Element::Lead),
            "BI" => Some(Element::Bismuth),
            "PO" => Some(Element::Polonium),
            "AT" => Some(Element::Astatine),
            "RN" => Some(Element::Radon),
            "FR" => Some(Element::Francium),
            "RA" => Some(Element::Radium),
            "AC" => Some(Element::Actinium),
            "TH" => Some(Element::Thorium),
            "PA" => Some(Element::Protactinium),
            "U" => Some(Element::Uranium),
            "NP" => Some(Element::Neptunium),
            "PU" => Some(Element::Plutonium),
            "AM" => Some(Element::Americium),
            "CM" => Some(Element::Curium),
            "BK" => Some(Element::Berkelium),
            "CF" => Some(Element::Californium),
            "ES" => Some(Element::Einsteinium),
            "FM" => Some(Element::Fermium),
            "MD" => Some(Element::Mendelevium),
            "NO" => Some(Element::Nobelium),
            "LR" => Some(Element::Lawrencium),
            "RF" => Some(Element::Rutherfordium),
            "DB" => Some(Element::Dubnium),
            "SG" => Some(Element::Seaborgium),
            "BH" => Some(Element::Bohrium),
            "HS" => Some(Element::Hassium),
            "MT" => Some(Element::Meitnerium),
            "DS" => Some(Element::Darmstadtium),
            "RG" => Some(Element::Roentgenium),
            "CN" => Some(Element::Copernicium),
            "NH" => Some(Element::Nihonium),
            "FL" => Some(Element::Flerovium),
            "MC" => Some(Element::Moscovium),
            "LV" => Some(Element::Livermorium),
            "TS" => Some(Element::Tennessine),
            "OG" => Some(Element::Oganesson),
            _ => None,
        }
    }

    pub fn from_atomic_number(n: u8) -> Option<Self> {
        if Self::MIN as u8 <= n && n <= Self::MAX as u8 {
            Some(unsafe { mem::transmute::<u8, Element>(n) })
        } else {
            None
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct ElementProperties {
    pub color: Vec3, // RGB color space
    pub radius: f32, // in angstroms
}

const_assert_eq!(mem::size_of::<ElementProperties>(), 16);

#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct PeriodicTable {
    // +1 to include “element 0” (empty/available binding site)
    pub element_reprs: [ElementProperties; Element::MAX as usize + 1],
}

impl Default for PeriodicTable {
    fn default() -> Self {
        Self::new()
    }
}

impl PeriodicTable {
    pub fn new() -> Self {
        #[inline]
        fn rgb(r: u8, g: u8, b: u8) -> Vec3 {
            Vec3 {
                x: r as f32 / 255.0,
                y: g as f32 / 255.0,
                z: b as f32 / 255.0,
            }
        }

        let mut element_reprs = [ElementProperties {
            color: rgb(0, 0, 0), // midnight black
            radius: 1.0,
        }; Element::MAX as usize + 1];

        // Empty / available binding site
        element_reprs[0] = ElementProperties {
            color: rgb(206, 32, 41), // fire engine red
            radius: 0.4,
        };

        // Main group elements
        // Colors: https://jmol.sourceforge.net/jscolors/
        // Radii: https://www.ncbi.nlm.nih.gov/pmc/articles/PMC3658832/
        element_reprs[Element::Hydrogen as usize] = ElementProperties {
            color: rgb(255, 255, 255), // snow white
            radius: 1.10,
        };
        element_reprs[Element::Helium as usize] = ElementProperties {
            color: rgb(217, 255, 255), // ice blue
            radius: 1.40,
        };
        element_reprs[Element::Lithium as usize] = ElementProperties {
            color: rgb(204, 128, 255), // lavender
            radius: 1.81,
        };
        element_reprs[Element::Beryllium as usize] = ElementProperties {
            color: rgb(194, 255, 0), // electric lime
            radius: 1.53,
        };
        element_reprs[Element::Boron as usize] = ElementProperties {
            color: rgb(255, 181, 181), // salmon pink
            radius: 1.92,
        };
        element_reprs[Element::Carbon as usize] = ElementProperties {
            color: rgb(144, 144, 144), // gray goo
            radius: 1.70,
        };
        element_reprs[Element::Nitrogen as usize] = ElementProperties {
            color: rgb(48, 80, 248), // royal blue
            radius: 1.55,
        };
        element_reprs[Element::Oxygen as usize] = ElementProperties {
            color: rgb(255, 13, 13), // cherry red
            radius: 1.52,
        };
        element_reprs[Element::Fluorine as usize] = ElementProperties {
            color: rgb(144, 224, 80), // spring green
            radius: 1.47,
        };
        element_reprs[Element::Neon as usize] = ElementProperties {
            color: rgb(179, 227, 245), // baby blue
            radius: 1.54,
        };
        element_reprs[Element::Sodium as usize] = ElementProperties {
            color: rgb(171, 92, 242), // wisteria purple
            radius: 2.27,
        };
        element_reprs[Element::Magnesium as usize] = ElementProperties {
            color: rgb(138, 255, 0), // lime green
            radius: 1.73,
        };
        element_reprs[Element::Aluminium as usize] = ElementProperties {
            color: rgb(191, 166, 166), // dusty rose
            radius: 1.84,
        };
        element_reprs[Element::Silicon as usize] = ElementProperties {
            color: rgb(240, 200, 160), // desert sand
            radius: 2.10,
        };
        element_reprs[Element::Phosphorus as usize] = ElementProperties {
            color: rgb(255, 128, 0), // tangerine
            radius: 1.80,
        };
        element_reprs[Element::Sulfur as usize] = ElementProperties {
            color: rgb(255, 255, 48), // sunshine yellow
            radius: 1.80,
        };
        element_reprs[Element::Chlorine as usize] = ElementProperties {
            color: rgb(31, 240, 31), // neon green
            radius: 1.75,
        };
        element_reprs[Element::Argon as usize] = ElementProperties {
            color: rgb(128, 209, 227), // sky blue
            radius: 1.88,
        };
        element_reprs[Element::Potassium as usize] = ElementProperties {
            color: rgb(143, 64, 212), // amethyst
            radius: 2.75,
        };
        element_reprs[Element::Calcium as usize] = ElementProperties {
            color: rgb(61, 255, 0), // fluorescent green
            radius: 2.31,
        };
        element_reprs[Element::Gallium as usize] = ElementProperties {
            color: rgb(194, 143, 143), // rosy brown
            radius: 1.87,
        };
        element_reprs[Element::Germanium as usize] = ElementProperties {
            color: rgb(102, 143, 143), // cadet blue
            radius: 2.11,
        };
        element_reprs[Element::Arsenic as usize] = ElementProperties {
            color: rgb(189, 128, 227), // orchid
            radius: 1.85,
        };
        element_reprs[Element::Selenium as usize] = ElementProperties {
            color: rgb(255, 161, 0), // marigold
            radius: 1.90,
        };
        element_reprs[Element::Bromine as usize] = ElementProperties {
            color: rgb(166, 41, 41), // mahogany
            radius: 1.83,
        };
        element_reprs[Element::Krypton as usize] = ElementProperties {
            color: rgb(92, 184, 209), // cornflower blue
            radius: 2.02,
        };
        element_reprs[Element::Rubidium as usize] = ElementProperties {
            color: rgb(112, 46, 176), // grape
            radius: 3.03,
        };
        element_reprs[Element::Strontium as usize] = ElementProperties {
            color: rgb(0, 255, 0), // shamrock green
            radius: 2.49,
        };
        element_reprs[Element::Indium as usize] = ElementProperties {
            color: rgb(166, 117, 115), // clay
            radius: 1.93,
        };
        element_reprs[Element::Tin as usize] = ElementProperties {
            color: rgb(102, 128, 128), // slate gray
            radius: 2.17,
        };
        element_reprs[Element::Antimony as usize] = ElementProperties {
            color: rgb(158, 99, 181), // lilac
            radius: 2.06,
        };
        element_reprs[Element::Tellurium as usize] = ElementProperties {
            color: rgb(212, 122, 0), // burnt orange
            radius: 2.06,
        };
        element_reprs[Element::Iodine as usize] = ElementProperties {
            color: rgb(148, 0, 148), // plum purple
            radius: 1.98,
        };
        element_reprs[Element::Xenon as usize] = ElementProperties {
            color: rgb(66, 158, 176), // teal blue
            radius: 2.16,
        };
        element_reprs[Element::Cesium as usize] = ElementProperties {
            color: rgb(87, 23, 143), // royal purple
            radius: 3.43,
        };
        element_reprs[Element::Barium as usize] = ElementProperties {
            color: rgb(0, 201, 0), // emerald green
            radius: 2.68,
        };
        element_reprs[Element::Thallium as usize] = ElementProperties {
            color: rgb(166, 84, 77), // terracotta
            radius: 1.96,
        };
        element_reprs[Element::Lead as usize] = ElementProperties {
            color: rgb(87, 89, 97), // charcoal
            radius: 2.02,
        };
        element_reprs[Element::Bismuth as usize] = ElementProperties {
            color: rgb(158, 79, 181), // violet
            radius: 2.07,
        };
        element_reprs[Element::Polonium as usize] = ElementProperties {
            color: rgb(171, 92, 0), // amber
            radius: 1.97,
        };
        element_reprs[Element::Astatine as usize] = ElementProperties {
            color: rgb(117, 79, 69), // cinnamon
            radius: 2.02,
        };
        element_reprs[Element::Radon as usize] = ElementProperties {
            color: rgb(66, 130, 150), // steel blue
            radius: 2.20,
        };
        element_reprs[Element::Francium as usize] = ElementProperties {
            color: rgb(66, 0, 102), // indigo
            radius: 3.48,
        };
        element_reprs[Element::Radium as usize] = ElementProperties {
            color: rgb(0, 125, 0), // forest green
            radius: 2.83,
        };

        Self { element_reprs }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32;

    /// Test that atomic numbers correctly convert to elements
    #[test]
    fn test_element_from_atomic_number() {
        // Test some valid atomic numbers
        assert_eq!(Element::from_atomic_number(1), Some(Element::Hydrogen));
        assert_eq!(Element::from_atomic_number(6), Some(Element::Carbon));
        assert_eq!(Element::from_atomic_number(47), Some(Element::Silver));
        assert_eq!(Element::from_atomic_number(79), Some(Element::Gold));
        assert_eq!(Element::from_atomic_number(118), Some(Element::Oganesson));

        // Test boundary cases
        assert_eq!(Element::from_atomic_number(0), None); // Below minimum
        assert_eq!(Element::from_atomic_number(119), None); // Above maximum

        // Test MIN and MAX constants
        assert_eq!(Element::MIN, Element::Hydrogen);
        assert_eq!(Element::MIN as u8, 1);
        assert_eq!(Element::MAX, Element::Oganesson);
        assert_eq!(Element::MAX as u8, 118);
    }

    /// Test element ordering and comparisons
    #[test]
    fn test_element_ordering() {
        // Test ordering using PartialOrd
        assert!(Element::Hydrogen < Element::Helium);
        assert!(Element::Carbon > Element::Beryllium);
        assert!(Element::Gold > Element::Silver);

        // Test complex ordering
        let mut elements = vec![
            Element::Oxygen,
            Element::Carbon,
            Element::Hydrogen,
            Element::Nitrogen,
        ];
        elements.sort();

        assert_eq!(
            elements,
            vec![
                Element::Hydrogen,
                Element::Carbon,
                Element::Nitrogen,
                Element::Oxygen,
            ]
        );
    }

    /// Test that elements can be used as array indices with proper offset
    #[test]
    fn test_element_array_indexing() {
        let table = PeriodicTable::new();

        // Hydrogen (1) should be at index 1
        assert_eq!(table.element_reprs[Element::Hydrogen as usize].radius, 1.10);

        // Carbon (6) should be at index 6
        assert_eq!(table.element_reprs[Element::Carbon as usize].radius, 1.70);

        // Check that Oxygen has the expected properties
        let oxygen = table.element_reprs[Element::Oxygen as usize];
        assert_eq!(oxygen.radius, 1.52);

        // The RGB values for Oxygen should be cherry red
        let oxygen_color = oxygen.color;
        assert!((oxygen_color.x - 1.0).abs() < f32::EPSILON); // R = 255/255 = 1.0
        assert!((oxygen_color.y - 13.0 / 255.0).abs() < f32::EPSILON); // G = 13/255
        assert!((oxygen_color.z - 13.0 / 255.0).abs() < f32::EPSILON); // B = 13/255
    }

    /// Test that the PeriodicTable initializes with the correct number of elements
    #[test]
    fn test_periodic_table_initialization() {
        let table = PeriodicTable::new();

        // Should have entries for all elements from 1 to 118, plus one for the “empty” site
        assert_eq!(table.element_reprs.len(), 1 + 118);

        // Check if the first element (Hydrogen) has the expected radius
        let first_index = Element::Hydrogen as usize;
        assert!(first_index < table.element_reprs.len());
        assert!((table.element_reprs[first_index].radius - 1.10).abs() < f32::EPSILON);

        // Check if the last element (Oganesson) is properly initialized
        let last_index = Element::Oganesson as usize;
        assert!(last_index < table.element_reprs.len());

        // Test that Default implementation matches new()
        let default_table = PeriodicTable::default();
        assert_eq!(
            default_table.element_reprs[Element::Carbon as usize].radius,
            table.element_reprs[Element::Carbon as usize].radius
        );
    }

    /// Test ElementProperties memory layout
    #[test]
    fn test_element_repr_memory_layout() {
        // Verify the size of ElementProperties is what we expect
        assert_eq!(std::mem::size_of::<ElementProperties>(), 16);

        // Verify the alignment
        assert_eq!(std::mem::align_of::<ElementProperties>(), 4);
    }
}

// End of File
