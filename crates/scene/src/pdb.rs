// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::feature::{FeatureError, MoleculeCommands};
use crate::ids::{AtomSpecifier, FeatureId};

use lib3dmol::{
    parser::read_pdb_txt,
    structures::{atom::AtomType, GetAtom as _},
};
use periodic_table::Element;
use ultraviolet::Vec3;

pub(crate) fn spawn_pdb(
    name: &str,
    contents: &str,
    feature_id: &FeatureId,
    commands: &mut dyn MoleculeCommands,
) -> Result<(), FeatureError> {
    // Currently bonds are ignored because lib3dmol does not support
    // parsing bonding info from PDB files!
    let mut spec = AtomSpecifier::new(*feature_id);
    let structure = read_pdb_txt(contents, name);

    for chain in structure.chains {
        for residue in chain.lst_res {
            for atom in residue.get_atom() {
                let element = atom_type_to_element(&atom.a_type);
                let pos: Vec3 = atom.coord.into();

                commands.add_atom(element, pos, spec.next_spec(), None)?;
            }
        }
    }

    Ok(())
}

fn atom_type_to_element(atom_type: &AtomType) -> Element {
    match atom_type {
        AtomType::Hydrogen => Element::Hydrogen,
        AtomType::Helium => Element::Helium,
        AtomType::Lithium => Element::Lithium,
        AtomType::Beryllium => Element::Beryllium,
        AtomType::Boron => Element::Boron,
        AtomType::Carbon => Element::Carbon,
        AtomType::Nitrogen => Element::Nitrogen,
        AtomType::Oxygen => Element::Oxygen,
        AtomType::Fluorine => Element::Fluorine,
        AtomType::Neon => Element::Neon,
        AtomType::Sodium => Element::Sodium,
        AtomType::Magnesium => Element::Magnesium,
        AtomType::Aluminum => Element::Aluminium,
        AtomType::Silicon => Element::Silicon,
        AtomType::Phosphorus => Element::Phosphorus,
        AtomType::Sulfur => Element::Sulfur,
        AtomType::Chlorine => Element::Chlorine,
        AtomType::Argon => Element::Argon,
        AtomType::Potassium => Element::Potassium,
        AtomType::Calcium => Element::Calcium,
        AtomType::Scandium => Element::Scandium,
        AtomType::Titanium => Element::Titanium,
        AtomType::Vanadium => Element::Vanadium,
        AtomType::Chromium => Element::Chromium,
        AtomType::Manganese => Element::Manganese,
        AtomType::Iron => Element::Iron,
        AtomType::Cobalt => Element::Cobalt,
        AtomType::Nickel => Element::Nickel,
        AtomType::Copper => Element::Copper,
        AtomType::Zinc => Element::Zinc,
        AtomType::Gallium => Element::Gallium,
        AtomType::Germanium => Element::Germanium,
        AtomType::Arsenic => Element::Arsenic,
        AtomType::Selenium => Element::Selenium,
        AtomType::Bromine => Element::Bromine,
        AtomType::Krypton => Element::Krypton,
        AtomType::Rubidium => Element::Rubidium,
        AtomType::Strontium => Element::Strontium,
        AtomType::Yttrium => Element::Yttrium,
        AtomType::Zirconium => Element::Zirconium,
        AtomType::Niobium => Element::Niobium,
        AtomType::Molybdenum => Element::Molybdenum,
        AtomType::Technetium => Element::Technetium,
        AtomType::Ruthenium => Element::Ruthenium,
        AtomType::Rhodium => Element::Rhodium,
        AtomType::Palladium => Element::Palladium,
        AtomType::Silver => Element::Silver,
        AtomType::Cadmium => Element::Cadmium,
        AtomType::Indium => Element::Indium,
        AtomType::Tin => Element::Tin,
        AtomType::Antimony => Element::Antimony,
        AtomType::Tellurium => Element::Tellurium,
        AtomType::Iodine => Element::Iodine,
        AtomType::Xenon => Element::Xenon,
        AtomType::Cesium => Element::Cesium,
        AtomType::Barium => Element::Barium,
        AtomType::Lanthanum => Element::Lanthanum,
        AtomType::Cerium => Element::Cerium,
        AtomType::Praseodymium => Element::Praseodymium,
        AtomType::Neodymium => Element::Neodymium,
        AtomType::Promethium => Element::Promethium,
        AtomType::Samarium => Element::Samarium,
        AtomType::Europium => Element::Europium,
        AtomType::Gadolinium => Element::Gadolinium,
        AtomType::Terbium => Element::Terbium,
        AtomType::Dysprosium => Element::Dysprosium,
        AtomType::Holmium => Element::Holmium,
        AtomType::Erbium => Element::Erbium,
        AtomType::Thulium => Element::Thulium,
        AtomType::Ytterbium => Element::Ytterbium,
        AtomType::Lutetium => Element::Lutetium,
        AtomType::Hafnium => Element::Hafnium,
        AtomType::Tantalum => Element::Tantalum,
        AtomType::Tungsten => Element::Tungsten,
        AtomType::Rhenium => Element::Rhenium,
        AtomType::Osmium => Element::Osmium,
        AtomType::Iridium => Element::Iridium,
        AtomType::Platinum => Element::Platinum,
        AtomType::Gold => Element::Gold,
        AtomType::Mercury => Element::Mercury,
        AtomType::Thallium => Element::Thallium,
        AtomType::Lead => Element::Lead,
        AtomType::Bismuth => Element::Bismuth,
        AtomType::Polonium => Element::Polonium,
        AtomType::Astatine => Element::Astatine,
        AtomType::Radon => Element::Radon,
        AtomType::Francium => Element::Francium,
        AtomType::Radium => Element::Radium,
        AtomType::Actinium => Element::Actinium,
        AtomType::Thorium => Element::Thorium,
        AtomType::Protactinium => Element::Protactinium,
        AtomType::Uranium => Element::Uranium,
        AtomType::Neptunium => Element::Neptunium,
        AtomType::Plutonium => Element::Plutonium,
        AtomType::Americium => Element::Americium,
        AtomType::Curium => Element::Curium,
        AtomType::Berkelium => Element::Berkelium,
        AtomType::Californium => Element::Californium,
        AtomType::Einsteinium => Element::Einsteinium,
        AtomType::Fermium => Element::Fermium,
        AtomType::Mendelevium => Element::Mendelevium,
        AtomType::Nobelium => Element::Nobelium,
        AtomType::Lawrencium => Element::Lawrencium,
        AtomType::Rutherfordium => Element::Rutherfordium,
        AtomType::Dubnium => Element::Dubnium,
        AtomType::Seaborgium => Element::Seaborgium,
        AtomType::Bohrium => Element::Bohrium,
        AtomType::Hassium => Element::Hassium,
        AtomType::Meitnerium => Element::Meitnerium,
        AtomType::Unknown => Element::MAX, // TODO: This could be handled better
    }
}

// End of File
