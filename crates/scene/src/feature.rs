use std::{borrow::Borrow, collections::HashMap};

use periodic_table::Element;

use crate::{ids::*, molecule::AtomNode, BondOrder};

#[derive(Debug)]
pub enum ReferenceType {
    Atom,
    Feature,
    // Molecule,
    // File,
    // etc.
}

#[derive(Debug)]
pub enum FeatureError {
    BrokenReference(ReferenceType),
    AtomOverwrite,
}

/// A proxy trait that allows a molecule to be manipulated without exposing its implementation.
/// Features can only manipulate a molecule using MoleculeCommands.
pub trait MoleculeCommands {
    fn find_atom(&self, spec: &AtomSpecifier) -> Option<&AtomNode>;
    fn add_atom(
        &mut self,
        element: Element,
        pos: ultraviolet::Vec3,
        spec: AtomSpecifier,
    ) -> Result<(), FeatureError>;
    fn create_bond(
        &mut self,
        a1: &AtomSpecifier,
        a2: &AtomSpecifier,
        order: BondOrder,
    ) -> Result<(), FeatureError>;
}

pub trait Feature {
    fn apply(
        &self,
        feature_id: &FeatureId,
        commands: &mut dyn MoleculeCommands,
    ) -> Result<(), FeatureError>;
}

pub struct RootAtom {
    pub element: Element,
}

impl Feature for RootAtom {
    fn apply(
        &self,
        feature_id: &FeatureId,
        commands: &mut dyn MoleculeCommands,
    ) -> Result<(), FeatureError> {
        commands.add_atom(
            self.element,
            Default::default(),
            AtomSpecifier::new(*feature_id),
        )?;

        Ok(())
    }
}

pub struct AtomFeature {
    pub target: AtomSpecifier,
    pub element: Element,
}

impl Feature for AtomFeature {
    fn apply(
        &self,
        feature_id: &FeatureId,
        commands: &mut dyn MoleculeCommands,
    ) -> Result<(), FeatureError> {
        let spec = AtomSpecifier::new(*feature_id);

        let x = {
            let atom = commands.find_atom(&self.target);
            let atom = atom.ok_or(FeatureError::BrokenReference(ReferenceType::Atom))?;
            atom.pos.x + 5.0
        };

        commands.add_atom(
            self.element,
            ultraviolet::Vec3::new(x, 0.0, 0.0),
            spec.clone(),
        )?;

        commands.create_bond(&self.target, &spec, 1)?;

        Ok(())
    }
}

/// A container that stores a list of features. It allows the list to be manipulated without
/// changing the indexes of existing features.
#[derive(Default)]
pub struct FeatureList {
    counter: usize,
    order: Vec<FeatureId>,
    features: HashMap<FeatureId, Box<dyn Feature>>,
}

impl FeatureList {
    // Inserts a feature at position `location` within the feature list, shifting all features after it to the right.
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

    pub fn get(&self, id: &FeatureId) -> Option<&dyn Feature> {
        self.features.get(id).map(|feature| feature.borrow())
    }

    // Adds a new feature to the end of the feature list.
    pub fn push_back(&mut self, feature: impl Feature + 'static) -> usize {
        let id = self.counter;

        self.order.push(id);
        self.features.insert(id, Box::new(feature));

        self.counter += 1;
        self.counter
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn order(&self) -> &[FeatureId] {
        &self.order
    }
}

/// Allows a FeatureList to be iterated over.
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
        self.list.get(index)
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
