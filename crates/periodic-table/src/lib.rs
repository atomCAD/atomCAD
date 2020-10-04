use common::AsBytes;
use static_assertions::const_assert_eq;
use std::mem;
use ultraviolet::Vec3;

#[allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
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

impl Element {
    pub const MIN: Self = Element::Hydrogen;
    pub const MAX: Self = Element::Oganesson;

    pub fn from_atomic_number(n: u8) -> Option<Self> {
        if n >= 1 && n <= Self::MAX as u8 {
            Some(unsafe { mem::transmute(n) })
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
        let mut element_reprs = vec![
            ElementRepr {
                color: Vec3::new(0.0, 0.0, 0.0), // Black
                radius: 1.0,
            };
            118
        ];

        element_reprs[Element::Hydrogen as usize - 1] = ElementRepr {
            color: Vec3::new(1.0, 1.0, 1.0), // white
            radius: 1.0,
        };
        element_reprs[Element::Carbon as usize - 1] = ElementRepr {
            color: Vec3::new(0.30196, 0.2902, 0.3098), // dark grey
            radius: 1.4167,                            // van der waals relative to hydrogen
        };
        element_reprs[Element::Oxygen as usize - 1] = ElementRepr {
            color: Vec3::new(0.7490, 0.2118, 0.3176), // red
            radius: 1.267,                            // van der waals relative to hydrogen
        };
        element_reprs[Element::Silicon as usize - 1] = ElementRepr {
            color: Vec3::new(0.7294, 0.5804, 0.1686), // yellow
            radius: 1.75,                             // van der waals relative to hydrogen
        };
        element_reprs[Element::Phosphorus as usize - 1] = ElementRepr {
            color: Vec3::new(0.7019, 0.4314, 0.1451), // orange
            radius: 1.625,                            // van der waals relative to hydrogen
        };
        element_reprs[Element::Nitrogen as usize - 1] = ElementRepr {
            color: Vec3::new(0.2078, 0.4549, 0.6118), // blue
            radius: 1.292,                            // van der waals relative to hydrogen
        };
        element_reprs[Element::Sulfur as usize - 1] = ElementRepr {
            color: Vec3::new(0.7294, 0.5804, 0.1686), // yellow
            radius: 1.5,                              // van der waals relative to hydrogen
        };

        for repr in &mut element_reprs {
            repr.radius *= 0.85;
        }

        Self { element_reprs }
    }
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct ElementRepr {
    color: Vec3,
    radius: f32,
}

const_assert_eq!(mem::size_of::<ElementRepr>(), 16);
unsafe impl AsBytes for ElementRepr {}
