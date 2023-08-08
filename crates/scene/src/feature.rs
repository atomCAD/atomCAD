use std::collections::HashMap;

use periodic_table::Element;

use crate::{
    molecule::{self, AtomIndex},
    BondOrder, Molecule,
};
use std::borrow::Borrow;

pub type FeatureIndex = usize;

/// Specifies where an atom's data originially came from. For example, although
/// a mirror feature is directly responsible for creating many atoms,
/// it does not own any of them - their original data comes from some other feature
/// or set of features which are being mirrored. Note that if an atom is `DerivedFrom` another
/// feature's data, that feature might not be the owner - for example, if you mirror an atom
/// twice, you will need to trace it back until the AtomSource is `Owned` - the feature which
/// owns an atom will be the original source of its data.
pub enum AtomSource {
    Owned,
    // `DerivedFrom` tells you the feature index in the FeatureList where an atom's data came from
    // most recently. It also tells you what atom index that it is stored under in that feature.
    // I.e. in simple setup like:
    //
    // FeatureList:
    // feature 0: Create atom 0
    // feature 1: mirror feature 0 (creates atom 1)
    // feature 2: mirror feature 1 (creates atom 2)
    //
    // Then feature2.get_atom_source() would return AtomSource::DerivedFrom(1, 1), because atom 2
    // is a mirror of AtomIndex 1, which is owned by feature 1. feature2.get_atom_source_recursive()
    // would return AtomSource::DerivedFrom(0, 0), because the atom source is owned by
    // feature 0 and has AtomIndex 0 in that context.
    DerivedFrom(FeatureIndex, AtomIndex),
}

pub trait Feature {
    fn apply(&self, features: &FeatureList, molecule: &mut Molecule);
    fn depends_on(&self, other_feature: FeatureIndex) -> bool;
    /// Returns a slice containing all of the AtomIndexes that this feature was directly
    /// responsible for creating. i.e. every atom that was created due to running `self.apply`.
    fn get_atoms(&self) -> &[molecule::AtomIndex];
    // fn get_dependency(&self, dependency_id: usize) -> Option<FeatureIndex>;
    // fn each_dependency(&self, f: impl FnMut(FeatureIndex) -> ());

    /// Returns an AtomSource stating where the atom with index `atom_index`
    /// got its data from.
    fn get_atom_source(&self, atom_index: AtomIndex) -> AtomSource;

    fn get_atom_source_recursive(
        &self,
        features: &FeatureList,
        mut atom_index: AtomIndex,
    ) -> AtomSource {
        let mut feature_id;

        match self.get_atom_source(atom_index) {
            AtomSource::DerivedFrom(new_feature_id, new_atom_index) => {
                feature_id = new_feature_id;
                atom_index = new_atom_index
            }
            AtomSource::Owned => return AtomSource::Owned,
        }

        loop {
            // TODO: Error handle in a way the user can see
            let feature = features
                .get(feature_id)
                .expect("A valid feature's dependent atom should belong to another valid feature!");
            match feature.get_atom_source(atom_index) {
                AtomSource::DerivedFrom(new_feature_id, new_atom_index) => {
                    feature_id = new_feature_id;
                    atom_index = new_atom_index;
                }
                AtomSource::Owned => return AtomSource::DerivedFrom(feature_id, atom_index),
            }
        }
    }
}

// Creates a molecule and does not depend on any other features.
pub struct MoleculeFeature {
    root_atom: [AtomIndex; 1],
}

impl MoleculeFeature {
    pub fn new(root_atom: AtomIndex) -> Self {
        MoleculeFeature {
            root_atom: [root_atom],
        }
    }
}

impl Feature for MoleculeFeature {
    fn apply(&self, _features: &FeatureList, _molecule: &mut Molecule) {
        panic!("MoleculeFeature is not meant to be applied - it is just used to give the root atom an owning feature.")
    }

    fn depends_on(&self, other_feature: FeatureIndex) -> bool {
        false
    }

    fn get_atoms(&self) -> &[molecule::AtomIndex] {
        &self.root_atom
    }

    fn get_atom_source(&self, atom_index: AtomIndex) -> AtomSource {
        if atom_index == self.root_atom[0] {
            AtomSource::Owned
        } else {
            // TODO: error handle
            panic!("get_atom_source called for an atom that doesn't belong to this feature!");
        }
    }
}

#[derive(Debug)]
struct FeatureError;

pub struct AtomSpecifier {
    temp: AtomIndex,
    // An atom is specified first by referencing the feature that owns the atom (i.e.,
    // the newest feature in the edit history that, when removed, removes this atom.)
    // For example, if an atom is created by mirroring another feature,
    // then the feature_id in its AtomSpecifier is the mirror feature, not the
    // feature which created the mirror.
    feature_id: FeatureIndex,
    // The feature path specifies how to reach the base feature that directly holds
    // the atom data. Walking the dependencies of the feature with id `feature_id` will
    // bring you to the terminal feature which directly owns a referenced atom.
    feature_path: Vec<usize>,
    // Once the terminal feature is found, this is the atom id within that feature. It is
    // used to look up the actual instantiation of an atom.
    atom_id: usize,
}

impl AtomSpecifier {
    fn depends_on(&self, other_feature: FeatureIndex) -> bool {
        false
    }

    fn resolve(&self, features: &FeatureList) -> Result<AtomIndex, FeatureError> {
        Ok(self.temp)
    }
}

pub struct AtomFeature {
    element: Element,
    target: AtomSpecifier,
    bond_order: BondOrder,
}

impl AtomFeature {
    pub fn new(element: Element, target: AtomSpecifier, bond_order: BondOrder) -> Self {
        AtomFeature {
            element,
            target,
            bond_order,
        }
    }
}

impl Feature for AtomFeature {
    fn apply(&self, features: &FeatureList, molecule: &mut Molecule) {
        molecule.add_atom(
            self.element,
            // TODO: error handle
            self.target.resolve(features).unwrap(),
            self.bond_order,
            None,
        );
    }

    fn depends_on(&self, other_feature: FeatureIndex) -> bool {
        self.target.depends_on(other_feature)
    }

    fn get_atoms(&self) -> &[molecule::AtomIndex] {
        &[]
    }
}

// A container that stores a list of features. It allows the list to be manipulated without
// changing the indexes of existing features,
#[derive(Default)]
pub struct FeatureList {
    counter: usize,
    order: Vec<FeatureIndex>,
    features: HashMap<FeatureIndex, Box<dyn Feature>>,
}

impl FeatureList {
    // Inserts an feature at position `location` within the feature list, shifting all features after it to the right.
    pub fn insert(&mut self, feature: impl Feature + 'static, location: usize) -> usize {
        let id = self.counter;

        self.order.insert(location, id);
        self.features.insert(id, Box::new(feature));

        self.counter += 1;
        self.counter
    }

    // Removes the feature with the given `id` from the feature list, shifting all features after it to the left.
    pub fn remove(&mut self, id: FeatureIndex) {
        self.features.remove(&id);
        self.order.remove(id);
    }

    pub fn get(&self, id: FeatureIndex) -> Option<&dyn Feature> {
        self.features.get(&id).map(|bo| bo.borrow())
    }

    // Adds a new feature to the end of the feature list.
    pub fn push_back(&mut self, feature: impl Feature + 'static) -> usize {
        let id = self.counter;

        self.order.push(id);
        self.features.insert(id, Box::new(feature));

        self.counter += 1;
        self.counter
    }
}

// Allows a feature list to be iterated over.
pub struct FeatureListIter<'a> {
    list: &'a FeatureList,
    // This stores the index of iteration - but not the current feature ID.
    // The current feature ID is given by `list.order.get(self.current_index)`
    current_index: usize,
}

impl<'a> Iterator for FeatureListIter<'a> {
    type Item = &'a dyn Feature;

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.list.order.get(self.current_index)?;
        self.current_index += 1;
        self.list.get(*index)
        // self.list.features.get(index).map(|bo| bo.borrow())
    }
}

impl<'a> IntoIterator for &'a FeatureList {
    type Item = &'a dyn Feature;
    type IntoIter = FeatureListIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        FeatureListIter {
            list: self,
            current_index: 0,
        }
    }
}
