use std::collections::HashMap;

use periodic_table::Element;

use crate::{
    molecule::{self, AtomIndex},
    BondOrder, Molecule,
};
use std::borrow::Borrow;

pub type FeatureIndex = usize;

pub trait Feature {
    fn apply(&self, features: &FeatureList, molecule: &mut Molecule);
    fn depends_on(&self, other_feature: FeatureIndex) -> bool;
    fn get_atoms(&self) -> &[molecule::AtomIndex];
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
}

#[derive(Debug)]
struct FeatureError;

pub struct AtomSpecifier {
    temp: AtomIndex,
}

impl AtomSpecifier {
    pub fn new(temp: AtomIndex) -> Self {
        AtomSpecifier { temp }
    }

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
