use std::{borrow::Borrow, collections::HashMap};

use periodic_table::Element;

use crate::{ids::*, molecule::AtomIndex, BondOrder, Molecule};

pub trait MoleculeCommands {
    // fn find_atom(&self, spec: &AtomSpecifier) -> Option<AtomIndex>;
    fn add_atom(&mut self, element: Element, pos: ultraviolet::Vec3, spec: AtomSpecifier);
    fn create_bond(&mut self, a1: &AtomSpecifier, a2: &AtomSpecifier, order: BondOrder);
}

pub trait Feature {
    fn apply(&self, commands: &mut dyn MoleculeCommands);
}

// A container that stores a list of features. It allows the list to be manipulated without
// changing the indexes of existing features
#[derive(Default)]
pub struct FeatureList {
    counter: usize,
    order: Vec<FeatureId>,
    features: HashMap<FeatureId, Box<dyn Feature>>,
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
    pub fn remove(&mut self, id: FeatureId) {
        self.features.remove(&id);
        self.order.remove(id);
    }

    pub fn get(&self, id: FeatureId) -> Option<&dyn Feature> {
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
