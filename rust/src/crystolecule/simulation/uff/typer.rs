// UFF atom type assignment from atomic connectivity.
//
// Maps (atomic_number, bond_list) → UFF atom type label (e.g., "C_3", "N_R").
// Simplified port of RDKit's AtomTyper.cpp, using atomCAD's explicit bond orders
// instead of SMARTS perception.
//
// The atom type label is a string key into the UFF parameter table (params.rs).
// The label format is: element symbol (1-2 chars, padded with '_' if 1 char)
// + hybridization/geometry digit (1/2/3/R/4/5/6) + optional charge suffix (+2, etc.)

use crate::crystolecule::atomic_structure::InlineBond;
use crate::crystolecule::atomic_structure::inline_bond::{
    BOND_AROMATIC, BOND_DELETED, BOND_DOUBLE, BOND_SINGLE, BOND_TRIPLE,
};

use super::params::{UffAtomParams, get_uff_params};

/// Result of assigning UFF atom types to a structure.
#[derive(Debug)]
pub struct AtomTypeAssignment {
    /// UFF atom type label for each atom (indexed by position in the input array).
    pub labels: Vec<&'static str>,
    /// Reference to the UFF parameters for each atom.
    pub params: Vec<&'static UffAtomParams>,
}

/// Assigns UFF atom types to all atoms in a structure.
///
/// Takes parallel slices of atomic numbers and bond lists (one per atom).
/// Returns labels and parameter references, or an error if any atom cannot be typed.
pub fn assign_uff_types(
    atomic_numbers: &[i16],
    bond_lists: &[&[InlineBond]],
) -> Result<AtomTypeAssignment, String> {
    let n = atomic_numbers.len();
    if n != bond_lists.len() {
        return Err(format!(
            "Mismatched lengths: {} atomic numbers, {} bond lists",
            n,
            bond_lists.len()
        ));
    }

    let mut labels = Vec::with_capacity(n);
    let mut params = Vec::with_capacity(n);

    for i in 0..n {
        let label = assign_uff_type(atomic_numbers[i], bond_lists[i])?;
        let p = get_uff_params(label).ok_or_else(|| {
            format!(
                "Atom {}: assigned type '{}' but no UFF parameters found (element Z={})",
                i, label, atomic_numbers[i]
            )
        })?;
        labels.push(label);
        params.push(p);
    }

    Ok(AtomTypeAssignment { labels, params })
}

/// Assigns a UFF atom type label to a single atom.
///
/// The label is a static string slice referencing the same strings used in the
/// parameter table, so it can be used directly with `get_uff_params()`.
///
/// # Arguments
/// * `atomic_number` - Element's atomic number (1=H, 6=C, etc.)
/// * `bonds` - Slice of InlineBond representing all bonds to this atom
///
/// # Returns
/// A static string like "C_3", "N_R", "H_", or an error if the atom cannot be typed.
pub fn assign_uff_type(atomic_number: i16, bonds: &[InlineBond]) -> Result<&'static str, String> {
    // Filter out deleted bonds
    let active_bonds: Vec<&InlineBond> = bonds.iter().filter(|b| !b.is_delete_marker()).collect();
    let total_valence = active_bonds.len();

    // Count bond types
    let has_aromatic = active_bonds.iter().any(|b| b.bond_order() == BOND_AROMATIC);
    let has_double = active_bonds.iter().any(|b| b.bond_order() == BOND_DOUBLE);
    let has_triple = active_bonds.iter().any(|b| b.bond_order() == BOND_TRIPLE);

    // Sum of bond orders (for effective valence)
    let bond_order_sum: u32 = active_bonds
        .iter()
        .map(|b| match b.bond_order() {
            BOND_SINGLE => 1,
            BOND_DOUBLE => 2,
            BOND_TRIPLE => 3,
            BOND_AROMATIC => 1, // aromatic counts as ~1.5 but for valence counting use 1
            _ => 1,
        })
        .sum();

    match atomic_number {
        // ================================================================
        // Hydrogen (Z=1): always H_
        // ================================================================
        1 => Ok("H_"),

        // ================================================================
        // Helium (Z=2): He4+4
        // ================================================================
        2 => Ok("He4+4"),

        // ================================================================
        // Lithium (Z=3): Li
        // ================================================================
        3 => Ok("Li"),

        // ================================================================
        // Beryllium (Z=4): Be3+2
        // ================================================================
        4 => Ok("Be3+2"),

        // ================================================================
        // Boron (Z=5): B_2 or B_3
        // ================================================================
        5 => {
            if total_valence <= 2 || has_double {
                Ok("B_2")
            } else {
                Ok("B_3")
            }
        }

        // ================================================================
        // Carbon (Z=6): C_1, C_2, C_3, C_R
        // ================================================================
        6 => assign_carbon_type(&active_bonds, has_aromatic, has_double, has_triple),

        // ================================================================
        // Nitrogen (Z=7): N_1, N_2, N_3, N_R
        // ================================================================
        7 => assign_nitrogen_type(&active_bonds, has_aromatic, has_double, has_triple),

        // ================================================================
        // Oxygen (Z=8): O_1, O_2, O_3, O_R
        // ================================================================
        8 => assign_oxygen_type(&active_bonds, has_aromatic, has_double, has_triple),

        // ================================================================
        // Fluorine (Z=9): F_
        // ================================================================
        9 => Ok("F_"),

        // ================================================================
        // Neon (Z=10): Ne4+4
        // ================================================================
        10 => Ok("Ne4+4"),

        // ================================================================
        // Sodium (Z=11): Na
        // ================================================================
        11 => Ok("Na"),

        // ================================================================
        // Magnesium (Z=12): Mg3+2 (forced sp3)
        // ================================================================
        12 => Ok("Mg3+2"),

        // ================================================================
        // Aluminium (Z=13): Al3 (forced sp3)
        // ================================================================
        13 => Ok("Al3"),

        // ================================================================
        // Silicon (Z=14): Si3 (forced sp3)
        // ================================================================
        14 => Ok("Si3"),

        // ================================================================
        // Phosphorus (Z=15): P_3+3 or P_3+5 (forced sp3, charge from valence)
        // ================================================================
        15 => {
            if total_valence <= 3 {
                Ok("P_3+3")
            } else {
                Ok("P_3+5")
            }
        }

        // ================================================================
        // Sulfur (Z=16): S_2, S_R, S_3+2, S_3+4, S_3+6
        // ================================================================
        16 => assign_sulfur_type(&active_bonds, total_valence, has_aromatic, has_double),

        // ================================================================
        // Chlorine (Z=17): Cl
        // ================================================================
        17 => Ok("Cl"),

        // ================================================================
        // Argon (Z=18): Ar4+4
        // ================================================================
        18 => Ok("Ar4+4"),

        // ================================================================
        // Potassium (Z=19): K_
        // ================================================================
        19 => Ok("K_"),

        // ================================================================
        // Calcium (Z=20): Ca6+2
        // ================================================================
        20 => Ok("Ca6+2"),

        // ================================================================
        // Scandium (Z=21): Sc3+3
        // ================================================================
        21 => Ok("Sc3+3"),

        // ================================================================
        // Titanium (Z=22): Ti3+4 or Ti6+4
        // ================================================================
        22 => {
            if total_valence <= 4 {
                Ok("Ti3+4")
            } else {
                Ok("Ti6+4")
            }
        }

        // ================================================================
        // Vanadium (Z=23): V_3+5
        // ================================================================
        23 => Ok("V_3+5"),

        // ================================================================
        // Chromium (Z=24): Cr6+3
        // ================================================================
        24 => Ok("Cr6+3"),

        // ================================================================
        // Manganese (Z=25): Mn6+2
        // ================================================================
        25 => Ok("Mn6+2"),

        // ================================================================
        // Iron (Z=26): Fe3+2 or Fe6+2
        // ================================================================
        26 => {
            if total_valence <= 4 {
                Ok("Fe3+2")
            } else {
                Ok("Fe6+2")
            }
        }

        // ================================================================
        // Cobalt (Z=27): Co6+3
        // ================================================================
        27 => Ok("Co6+3"),

        // ================================================================
        // Nickel (Z=28): Ni4+2
        // ================================================================
        28 => Ok("Ni4+2"),

        // ================================================================
        // Copper (Z=29): Cu3+1
        // ================================================================
        29 => Ok("Cu3+1"),

        // ================================================================
        // Zinc (Z=30): Zn3+2
        // ================================================================
        30 => Ok("Zn3+2"),

        // ================================================================
        // Gallium (Z=31): Ga3+3
        // ================================================================
        31 => Ok("Ga3+3"),

        // ================================================================
        // Germanium (Z=32): Ge3 (forced sp3)
        // ================================================================
        32 => Ok("Ge3"),

        // ================================================================
        // Arsenic (Z=33): As3+3
        // ================================================================
        33 => Ok("As3+3"),

        // ================================================================
        // Selenium (Z=34): Se3+2
        // ================================================================
        34 => Ok("Se3+2"),

        // ================================================================
        // Bromine (Z=35): Br
        // ================================================================
        35 => Ok("Br"),

        // ================================================================
        // Krypton (Z=36): Kr4+4
        // ================================================================
        36 => Ok("Kr4+4"),

        // ================================================================
        // Rubidium (Z=37): Rb
        // ================================================================
        37 => Ok("Rb"),

        // ================================================================
        // Strontium (Z=38): Sr6+2
        // ================================================================
        38 => Ok("Sr6+2"),

        // ================================================================
        // Yttrium (Z=39): Y_3+3
        // ================================================================
        39 => Ok("Y_3+3"),

        // ================================================================
        // Zirconium (Z=40): Zr3+4
        // ================================================================
        40 => Ok("Zr3+4"),

        // ================================================================
        // Niobium (Z=41): Nb3+5
        // ================================================================
        41 => Ok("Nb3+5"),

        // ================================================================
        // Molybdenum (Z=42): Mo3+6 or Mo6+6
        // ================================================================
        42 => {
            if total_valence <= 4 {
                Ok("Mo3+6")
            } else {
                Ok("Mo6+6")
            }
        }

        // ================================================================
        // Technetium (Z=43): Tc6+5
        // ================================================================
        43 => Ok("Tc6+5"),

        // ================================================================
        // Ruthenium (Z=44): Ru6+2
        // ================================================================
        44 => Ok("Ru6+2"),

        // ================================================================
        // Rhodium (Z=45): Rh6+3
        // ================================================================
        45 => Ok("Rh6+3"),

        // ================================================================
        // Palladium (Z=46): Pd4+2
        // ================================================================
        46 => Ok("Pd4+2"),

        // ================================================================
        // Silver (Z=47): Ag1+1
        // ================================================================
        47 => Ok("Ag1+1"),

        // ================================================================
        // Cadmium (Z=48): Cd3+2
        // ================================================================
        48 => Ok("Cd3+2"),

        // ================================================================
        // Indium (Z=49): In3+3
        // ================================================================
        49 => Ok("In3+3"),

        // ================================================================
        // Tin (Z=50): Sn3 (forced sp3)
        // ================================================================
        50 => Ok("Sn3"),

        // ================================================================
        // Antimony (Z=51): Sb3+3 (forced sp3)
        // ================================================================
        51 => Ok("Sb3+3"),

        // ================================================================
        // Tellurium (Z=52): Te3+2 (forced sp3)
        // ================================================================
        52 => Ok("Te3+2"),

        // ================================================================
        // Iodine (Z=53): I_
        // ================================================================
        53 => Ok("I_"),

        // ================================================================
        // Xenon (Z=54): Xe4+4
        // ================================================================
        54 => Ok("Xe4+4"),

        // ================================================================
        // Caesium (Z=55): Cs
        // ================================================================
        55 => Ok("Cs"),

        // ================================================================
        // Barium (Z=56): Ba6+2
        // ================================================================
        56 => Ok("Ba6+2"),

        // ================================================================
        // Lanthanides (Z=57-71): all X6+3 (forced sp3d2, +3 charge)
        // ================================================================
        57 => Ok("La3+3"),
        58 => Ok("Ce6+3"),
        59 => Ok("Pr6+3"),
        60 => Ok("Nd6+3"),
        61 => Ok("Pm6+3"),
        62 => Ok("Sm6+3"),
        63 => Ok("Eu6+3"),
        64 => Ok("Gd6+3"),
        65 => Ok("Tb6+3"),
        66 => Ok("Dy6+3"),
        67 => Ok("Ho6+3"),
        68 => Ok("Er6+3"),
        69 => Ok("Tm6+3"),
        70 => Ok("Yb6+3"),
        71 => Ok("Lu6+3"),

        // ================================================================
        // Hafnium (Z=72): Hf3+4
        // ================================================================
        72 => Ok("Hf3+4"),

        // ================================================================
        // Tantalum (Z=73): Ta3+5
        // ================================================================
        73 => Ok("Ta3+5"),

        // ================================================================
        // Tungsten (Z=74): W_3+4, W_3+6, or W_6+6
        // ================================================================
        74 => {
            if total_valence <= 4 {
                Ok("W_3+4")
            } else if total_valence <= 5 {
                Ok("W_3+6")
            } else {
                Ok("W_6+6")
            }
        }

        // ================================================================
        // Rhenium (Z=75): Re3+7 or Re6+5
        // ================================================================
        75 => {
            if total_valence <= 4 {
                Ok("Re3+7")
            } else {
                Ok("Re6+5")
            }
        }

        // ================================================================
        // Osmium (Z=76): Os6+6
        // ================================================================
        76 => Ok("Os6+6"),

        // ================================================================
        // Iridium (Z=77): Ir6+3
        // ================================================================
        77 => Ok("Ir6+3"),

        // ================================================================
        // Platinum (Z=78): Pt4+2
        // ================================================================
        78 => Ok("Pt4+2"),

        // ================================================================
        // Gold (Z=79): Au4+3
        // ================================================================
        79 => Ok("Au4+3"),

        // ================================================================
        // Mercury (Z=80): Hg1+2 (forced sp)
        // ================================================================
        80 => Ok("Hg1+2"),

        // ================================================================
        // Thallium (Z=81): Tl3+3 (forced sp3)
        // ================================================================
        81 => Ok("Tl3+3"),

        // ================================================================
        // Lead (Z=82): Pb3 (forced sp3)
        // ================================================================
        82 => Ok("Pb3"),

        // ================================================================
        // Bismuth (Z=83): Bi3+3 (forced sp3)
        // ================================================================
        83 => Ok("Bi3+3"),

        // ================================================================
        // Polonium (Z=84): Po3+2 (forced sp3)
        // ================================================================
        84 => Ok("Po3+2"),

        // ================================================================
        // Astatine (Z=85): At
        // ================================================================
        85 => Ok("At"),

        // ================================================================
        // Radon (Z=86): Rn4+4
        // ================================================================
        86 => Ok("Rn4+4"),

        // ================================================================
        // Francium (Z=87): Fr
        // ================================================================
        87 => Ok("Fr"),

        // ================================================================
        // Radium (Z=88): Ra6+2
        // ================================================================
        88 => Ok("Ra6+2"),

        // ================================================================
        // Actinium (Z=89): Ac6+3
        // ================================================================
        89 => Ok("Ac6+3"),

        // ================================================================
        // Thorium (Z=90): Th6+4
        // ================================================================
        90 => Ok("Th6+4"),

        // ================================================================
        // Protactinium (Z=91): Pa6+4
        // ================================================================
        91 => Ok("Pa6+4"),

        // ================================================================
        // Uranium (Z=92): U_6+4
        // ================================================================
        92 => Ok("U_6+4"),

        // ================================================================
        // Neptunium (Z=93): Np6+4
        // ================================================================
        93 => Ok("Np6+4"),

        // ================================================================
        // Plutonium (Z=94): Pu6+4
        // ================================================================
        94 => Ok("Pu6+4"),

        // ================================================================
        // Americium (Z=95): Am6+4
        // ================================================================
        95 => Ok("Am6+4"),

        // ================================================================
        // Curium (Z=96): Cm6+3
        // ================================================================
        96 => Ok("Cm6+3"),

        // ================================================================
        // Berkelium (Z=97): Bk6+3
        // ================================================================
        97 => Ok("Bk6+3"),

        // ================================================================
        // Californium (Z=98): Cf6+3
        // ================================================================
        98 => Ok("Cf6+3"),

        // ================================================================
        // Einsteinium (Z=99): Es6+3
        // ================================================================
        99 => Ok("Es6+3"),

        // ================================================================
        // Fermium (Z=100): Fm6+3
        // ================================================================
        100 => Ok("Fm6+3"),

        // ================================================================
        // Mendelevium (Z=101): Md6+3
        // ================================================================
        101 => Ok("Md6+3"),

        // ================================================================
        // Nobelium (Z=102): No6+3
        // ================================================================
        102 => Ok("No6+3"),

        // ================================================================
        // Lawrencium (Z=103): Lw6+3
        // ================================================================
        103 => Ok("Lw6+3"),

        _ => {
            // Bond order sum can give a hint about the valence
            let _ = bond_order_sum;
            Err(format!(
                "No UFF atom type for element with atomic number {}",
                atomic_number
            ))
        }
    }
}

/// Carbon type assignment: C_1 (sp), C_2 (sp2), C_3 (sp3), C_R (resonance/aromatic)
fn assign_carbon_type(
    bonds: &[&InlineBond],
    has_aromatic: bool,
    has_double: bool,
    has_triple: bool,
) -> Result<&'static str, String> {
    if has_aromatic {
        return Ok("C_R");
    }
    if has_triple {
        return Ok("C_1");
    }
    let double_count = bonds
        .iter()
        .filter(|b| b.bond_order() == BOND_DOUBLE)
        .count();
    if double_count >= 2 && bonds.len() == 2 {
        // Allene-like: two double bonds on same carbon → sp
        return Ok("C_1");
    }
    if has_double {
        return Ok("C_2");
    }
    Ok("C_3")
}

/// Nitrogen type assignment: N_1 (sp), N_2 (sp2), N_3 (sp3), N_R (resonance/aromatic)
fn assign_nitrogen_type(
    bonds: &[&InlineBond],
    has_aromatic: bool,
    has_double: bool,
    has_triple: bool,
) -> Result<&'static str, String> {
    if has_aromatic {
        return Ok("N_R");
    }
    if has_triple {
        return Ok("N_1");
    }
    let double_count = bonds
        .iter()
        .filter(|b| b.bond_order() == BOND_DOUBLE)
        .count();
    if double_count >= 2 && bonds.len() == 2 {
        return Ok("N_1");
    }
    if has_double {
        return Ok("N_2");
    }
    Ok("N_3")
}

/// Oxygen type assignment: O_1 (sp), O_2 (sp2), O_3 (sp3), O_R (resonance/aromatic)
fn assign_oxygen_type(
    bonds: &[&InlineBond],
    has_aromatic: bool,
    has_double: bool,
    has_triple: bool,
) -> Result<&'static str, String> {
    if has_aromatic {
        return Ok("O_R");
    }
    if has_triple {
        return Ok("O_1");
    }
    let double_count = bonds
        .iter()
        .filter(|b| b.bond_order() == BOND_DOUBLE)
        .count();
    if double_count >= 2 && bonds.len() == 2 {
        return Ok("O_1");
    }
    if has_double {
        // Carbonyl oxygen (C=O) or similar
        return Ok("O_2");
    }
    // Single bonds only: sp3 (water, alcohol, ether)
    Ok("O_3")
}

/// Sulfur type assignment:
/// - S_2: sp2 (no charge suffix in RDKit for sp2 sulfur)
/// - S_R: aromatic (thiophene-like)
/// - S_3+2: sp3 with 2 bonds (thiol, thioether)
/// - S_3+4: sp3 with 4 bonds (sulfoxide)
/// - S_3+6: sp3 with 6 bonds (sulfone, sulfate)
///
/// RDKit logic: if hybridization is SP2, use S_2 or S_R.
/// Otherwise, use S_3+N where N is the total valence (2, 4, or 6).
fn assign_sulfur_type(
    _bonds: &[&InlineBond],
    total_valence: usize,
    has_aromatic: bool,
    has_double: bool,
) -> Result<&'static str, String> {
    // Check for aromatic first
    if has_aromatic {
        return Ok("S_R");
    }
    // sp2 sulfur (e.g., C=S thione)
    if has_double && total_valence <= 2 {
        return Ok("S_2");
    }
    // sp3 sulfur: charge from total valence
    match total_valence {
        0..=2 => Ok("S_3+2"),
        3..=4 => Ok("S_3+4"),
        _ => Ok("S_3+6"),
    }
}

/// Determines the effective bond order as an f64 for UFF parameter calculations.
///
/// This converts atomCAD's discrete bond order representation to the continuous
/// values used in UFF formulas (matching RDKit's `getBondTypeAsDouble()`).
pub fn bond_order_to_f64(bond_order: u8) -> f64 {
    match bond_order {
        BOND_DELETED => 0.0,
        BOND_SINGLE => 1.0,
        BOND_DOUBLE => 2.0,
        BOND_TRIPLE => 3.0,
        BOND_AROMATIC => 1.5,
        _ => 1.0, // dative, metallic, quadruple → treat as single for UFF
    }
}

/// Returns the hybridization inferred from the UFF atom type label.
///
/// The last character before any charge suffix indicates hybridization:
/// - '1' → sp (linear)
/// - '2' → sp2 (trigonal planar)
/// - '3' → sp3 (tetrahedral)
/// - 'R' → resonance (treated as sp2 for most purposes)
/// - '4' → sp2d (square planar)
/// - '5' → sp3d (trigonal bipyramidal)
/// - '6' → sp3d2 (octahedral)
///
/// Returns 0 for unrecognized labels (halogens like "F_", "Cl", etc. with no
/// hybridization character — these are typically terminal atoms where
/// hybridization doesn't affect angle/torsion terms).
pub fn hybridization_from_label(label: &str) -> u8 {
    // Strip charge suffix ("+N") if present
    let base = if let Some(pos) = label.find('+') {
        &label[..pos]
    } else {
        label
    };

    // The hybridization character is the last character of the base
    match base.as_bytes().last() {
        Some(b'1') => 1,
        Some(b'2') => 2,
        Some(b'3') => 3,
        Some(b'R') => 2, // resonance ≈ sp2 for geometry purposes
        Some(b'4') => 4,
        Some(b'5') => 5,
        Some(b'6') => 6,
        _ => 0, // Halogens, noble gases, alkali metals
    }
}

#[cfg(test)]
mod tests {
    // Tests are in rust/tests/crystolecule/simulation/uff_typer_test.rs
}
