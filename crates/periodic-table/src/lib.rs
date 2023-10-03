// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use common::AsBytes;
use serde::{Deserialize, Serialize};
use static_assertions::const_assert_eq;
use std::mem;
use ultraviolet::Vec3;

#[allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[repr(u8)] // Oganesson == 118
pub enum Element {
    Hydrogen = 1,
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
    pub const MIN: Self = Element::Hydrogen; // 1
    pub const MAX: Self = Element::Oganesson; // 118

    pub fn from_atomic_number(n: u8) -> Option<Self> {
        if Self::MIN as u8 <= n && n <= Self::MAX as u8 {
            Some(unsafe { mem::transmute::<u8, Element>(n) })
        } else {
            None
        }
    }
}

pub struct PeriodicTable {
    pub element_reprs: Vec<ElementRepr>,
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

        let mut element_reprs = vec![
            ElementRepr {
                color: rgb(0, 0, 0), // black
                radius: 1.0,
            };
            Element::MAX as usize
        ];

        // Main group elements
        // Colors: https://jmol.sourceforge.net/jscolors/
        // Radii: https://www.ncbi.nlm.nih.gov/pmc/articles/PMC3658832/
        element_reprs[Element::Hydrogen as usize - 1] = ElementRepr {
            color: rgb(255, 255, 255), // bright white
            radius: 1.10,
        };
        element_reprs[Element::Helium as usize - 1] = ElementRepr {
            color: rgb(217, 255, 255), // light cyan
            radius: 1.40,
        };
        element_reprs[Element::Lithium as usize - 1] = ElementRepr {
            color: rgb(204, 128, 255), // light purple
            radius: 1.81,
        };
        element_reprs[Element::Beryllium as usize - 1] = ElementRepr {
            color: rgb(194, 255, 0), // light green
            radius: 1.53,
        };
        element_reprs[Element::Boron as usize - 1] = ElementRepr {
            color: rgb(255, 181, 181), // light red
            radius: 1.92,
        };
        element_reprs[Element::Carbon as usize - 1] = ElementRepr {
            color: rgb(144, 144, 144), // gray
            radius: 1.70,
        };
        element_reprs[Element::Nitrogen as usize - 1] = ElementRepr {
            color: rgb(48, 80, 248), // blue
            radius: 1.55,
        };
        element_reprs[Element::Oxygen as usize - 1] = ElementRepr {
            color: rgb(255, 13, 13), // red
            radius: 1.52,
        };
        element_reprs[Element::Fluorine as usize - 1] = ElementRepr {
            color: rgb(144, 224, 80), // light green
            radius: 1.47,
        };
        element_reprs[Element::Neon as usize - 1] = ElementRepr {
            color: rgb(179, 227, 245), // light blue
            radius: 1.54,
        };
        element_reprs[Element::Sodium as usize - 1] = ElementRepr {
            color: rgb(171, 92, 242), // light purple
            radius: 2.27,
        };
        element_reprs[Element::Magnesium as usize - 1] = ElementRepr {
            color: rgb(138, 255, 0), // light green
            radius: 1.73,
        };
        element_reprs[Element::Aluminium as usize - 1] = ElementRepr {
            color: rgb(191, 166, 166), // light gray
            radius: 1.84,
        };
        element_reprs[Element::Silicon as usize - 1] = ElementRepr {
            color: rgb(240, 200, 160), // light brown
            radius: 2.10,
        };
        element_reprs[Element::Phosphorus as usize - 1] = ElementRepr {
            color: rgb(255, 128, 0), // orange
            radius: 1.80,
        };
        element_reprs[Element::Sulfur as usize - 1] = ElementRepr {
            color: rgb(255, 255, 48), // yellow
            radius: 1.80,
        };
        element_reprs[Element::Chlorine as usize - 1] = ElementRepr {
            color: rgb(31, 240, 31), // green
            radius: 1.75,
        };
        element_reprs[Element::Argon as usize - 1] = ElementRepr {
            color: rgb(128, 209, 227), // light blue
            radius: 1.88,
        };
        element_reprs[Element::Potassium as usize - 1] = ElementRepr {
            color: rgb(143, 64, 212), // purple
            radius: 2.75,
        };
        element_reprs[Element::Calcium as usize - 1] = ElementRepr {
            color: rgb(61, 255, 0), // green
            radius: 2.31,
        };
        element_reprs[Element::Gallium as usize - 1] = ElementRepr {
            color: rgb(194, 143, 143), // light brown
            radius: 1.87,
        };
        element_reprs[Element::Germanium as usize - 1] = ElementRepr {
            color: rgb(102, 143, 143), // light brown
            radius: 2.11,
        };
        element_reprs[Element::Arsenic as usize - 1] = ElementRepr {
            color: rgb(189, 128, 227), // light purple
            radius: 1.85,
        };
        element_reprs[Element::Selenium as usize - 1] = ElementRepr {
            color: rgb(255, 161, 0), // orange
            radius: 1.90,
        };
        element_reprs[Element::Bromine as usize - 1] = ElementRepr {
            color: rgb(166, 41, 41), // brown
            radius: 1.83,
        };
        element_reprs[Element::Krypton as usize - 1] = ElementRepr {
            color: rgb(92, 184, 209), // light blue
            radius: 2.02,
        };
        element_reprs[Element::Rubidium as usize - 1] = ElementRepr {
            color: rgb(112, 46, 176), // purple
            radius: 3.03,
        };
        element_reprs[Element::Strontium as usize - 1] = ElementRepr {
            color: rgb(0, 255, 0), // bright green
            radius: 2.49,
        };
        element_reprs[Element::Indium as usize - 1] = ElementRepr {
            color: rgb(166, 117, 115), // light brown
            radius: 1.93,
        };
        element_reprs[Element::Tin as usize - 1] = ElementRepr {
            color: rgb(102, 128, 128), // light gray
            radius: 2.17,
        };
        element_reprs[Element::Antimony as usize - 1] = ElementRepr {
            color: rgb(158, 99, 181), // light purple
            radius: 2.06,
        };
        element_reprs[Element::Tellurium as usize - 1] = ElementRepr {
            color: rgb(212, 122, 0), // orange
            radius: 2.06,
        };
        element_reprs[Element::Iodine as usize - 1] = ElementRepr {
            color: rgb(148, 0, 148), // purple
            radius: 1.98,
        };
        element_reprs[Element::Xenon as usize - 1] = ElementRepr {
            color: rgb(66, 158, 176), // light blue
            radius: 2.16,
        };
        element_reprs[Element::Cesium as usize - 1] = ElementRepr {
            color: rgb(87, 23, 143), // purple
            radius: 3.43,
        };
        element_reprs[Element::Barium as usize - 1] = ElementRepr {
            color: rgb(0, 201, 0), // green
            radius: 2.68,
        };
        element_reprs[Element::Thallium as usize - 1] = ElementRepr {
            color: rgb(166, 84, 77), // light brown
            radius: 1.96,
        };
        element_reprs[Element::Lead as usize - 1] = ElementRepr {
            color: rgb(87, 89, 97), // dark gray
            radius: 2.02,
        };
        element_reprs[Element::Bismuth as usize - 1] = ElementRepr {
            color: rgb(158, 79, 181), // light purple
            radius: 2.07,
        };
        element_reprs[Element::Polonium as usize - 1] = ElementRepr {
            color: rgb(171, 92, 0), // orange
            radius: 1.97,
        };
        element_reprs[Element::Astatine as usize - 1] = ElementRepr {
            color: rgb(117, 79, 69), // brown
            radius: 2.02,
        };
        element_reprs[Element::Radon as usize - 1] = ElementRepr {
            color: rgb(66, 130, 150), // blue
            radius: 2.20,
        };
        element_reprs[Element::Francium as usize - 1] = ElementRepr {
            color: rgb(66, 0, 102), // dark purple
            radius: 3.48,
        };
        element_reprs[Element::Radium as usize - 1] = ElementRepr {
            color: rgb(0, 125, 0), // dark green
            radius: 2.83,
        };

        Self { element_reprs }
    }
}

impl Default for PeriodicTable {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct ElementRepr {
    pub color: Vec3, // RGB color space
    pub radius: f32, // in angstroms
}

const_assert_eq!(mem::size_of::<ElementRepr>(), 16);
unsafe impl AsBytes for ElementRepr {}

// End of File
