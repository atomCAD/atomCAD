use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::structure_designer::nodes::atom_edit::atom_edit::{AtomEditData, DEFAULT_TOLERANCE};
use crate::util::serialization_utils::dvec3_serializer;
use glam::f64::DVec3;
use serde::{Deserialize, Serialize};
use std::io;

/// Serializable representation of an atom in a diff structure.
#[derive(Serialize, Deserialize)]
pub struct SerializableAtom {
    pub id: u32,
    pub atomic_number: i16,
    #[serde(with = "dvec3_serializer")]
    pub position: DVec3,
}

/// Serializable representation of a bond in a diff structure.
#[derive(Serialize, Deserialize)]
pub struct SerializableBond {
    pub atom_id1: u32,
    pub atom_id2: u32,
    pub bond_order: u8,
}

/// Serializable representation of an anchor position (for moved atoms).
#[derive(Serialize, Deserialize)]
pub struct SerializableAnchor {
    pub atom_id: u32,
    #[serde(with = "dvec3_serializer")]
    pub position: DVec3,
}

/// Serializable representation of the diff AtomicStructure.
#[derive(Serialize, Deserialize)]
pub struct SerializableDiff {
    pub atoms: Vec<SerializableAtom>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bonds: Vec<SerializableBond>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchor_positions: Vec<SerializableAnchor>,
}

/// Serializable version of AtomEditData for JSON persistence.
#[derive(Serialize, Deserialize)]
pub struct SerializableAtomEditData {
    pub diff: SerializableDiff,
    #[serde(default)]
    pub output_diff: bool,
    #[serde(default)]
    pub show_anchor_arrows: bool,
    #[serde(default = "default_include_base_bonds_in_diff")]
    pub include_base_bonds_in_diff: bool,
    #[serde(default = "default_tolerance")]
    pub tolerance: f64,
}

fn default_include_base_bonds_in_diff() -> bool {
    true
}

fn default_tolerance() -> f64 {
    DEFAULT_TOLERANCE
}

/// Converts an AtomEditData to its serializable representation.
pub fn atom_edit_data_to_serializable(data: &AtomEditData) -> io::Result<SerializableAtomEditData> {
    let diff = &data.diff;

    // Serialize atoms
    let mut atoms: Vec<SerializableAtom> = Vec::new();
    for (_, atom) in diff.iter_atoms() {
        atoms.push(SerializableAtom {
            id: atom.id,
            atomic_number: atom.atomic_number,
            position: atom.position,
        });
    }
    // Sort by ID for deterministic output
    atoms.sort_by_key(|a| a.id);

    // Serialize bonds (each bond once: atom_id1 < atom_id2)
    let mut bonds: Vec<SerializableBond> = Vec::new();
    for (_, atom) in diff.iter_atoms() {
        for bond in &atom.bonds {
            let other_id = bond.other_atom_id();
            if atom.id < other_id {
                bonds.push(SerializableBond {
                    atom_id1: atom.id,
                    atom_id2: other_id,
                    bond_order: bond.bond_order(),
                });
            }
        }
    }
    bonds.sort_by(|a, b| (a.atom_id1, a.atom_id2).cmp(&(b.atom_id1, b.atom_id2)));

    // Serialize anchor positions
    let mut anchor_positions: Vec<SerializableAnchor> = diff
        .anchor_positions()
        .iter()
        .map(|(&atom_id, &position)| SerializableAnchor { atom_id, position })
        .collect();
    anchor_positions.sort_by_key(|a| a.atom_id);

    Ok(SerializableAtomEditData {
        diff: SerializableDiff {
            atoms,
            bonds,
            anchor_positions,
        },
        output_diff: data.output_diff,
        show_anchor_arrows: data.show_anchor_arrows,
        include_base_bonds_in_diff: data.include_base_bonds_in_diff,
        tolerance: data.tolerance,
    })
}

/// Converts a SerializableAtomEditData back to AtomEditData.
pub fn serializable_to_atom_edit_data(
    serializable: &SerializableAtomEditData,
) -> io::Result<AtomEditData> {
    let mut diff = AtomicStructure::new_diff();

    // We need to add atoms with the exact IDs from the serialized data.
    // AtomicStructure assigns sequential IDs (1, 2, 3, ...) on add_atom().
    // To restore exact IDs, we add atoms in order and verify IDs match.
    // If they don't (due to gaps from deleted atoms), we use padding slots.

    // Sort atoms by ID to add in order
    let mut sorted_indices: Vec<usize> = (0..serializable.diff.atoms.len()).collect();
    sorted_indices.sort_by_key(|&i| serializable.diff.atoms[i].id);

    for &idx in &sorted_indices {
        let atom = &serializable.diff.atoms[idx];
        // The next ID that add_atom will assign
        let mut expected_next_id = (diff.get_num_of_atoms_including_deleted() + 1) as u32;

        // Pad with None slots if there are gaps
        while expected_next_id < atom.id {
            diff.add_padding_slot();
            expected_next_id = (diff.get_num_of_atoms_including_deleted() + 1) as u32;
        }

        let actual_id = diff.add_atom(atom.atomic_number, atom.position);
        if actual_id != atom.id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Atom ID mismatch during deserialization: expected {}, got {}",
                    atom.id, actual_id
                ),
            ));
        }
    }

    // Restore bonds (use add_bond_checked to silently deduplicate any legacy duplicates)
    for bond in &serializable.diff.bonds {
        diff.add_bond_checked(bond.atom_id1, bond.atom_id2, bond.bond_order);
    }

    // Restore anchor positions
    for anchor in &serializable.diff.anchor_positions {
        diff.set_anchor_position(anchor.atom_id, anchor.position);
    }

    Ok(AtomEditData::from_deserialized(
        diff,
        serializable.output_diff,
        serializable.show_anchor_arrows,
        serializable.include_base_bonds_in_diff,
        serializable.tolerance,
    ))
}
