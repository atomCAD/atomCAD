use std::collections::HashMap;

use periodic_table::Element;

use crate::{ids::*, molecule::AtomNode, BondOrder};

use serde::{Deserialize, Serialize};

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
    fn pos(&self, spec: &AtomSpecifier) -> Option<ultraviolet::Vec3>;
    fn add_atom(
        &mut self,
        element: Element,
        pos: ultraviolet::Vec3,
        spec: AtomSpecifier,
        head: Option<AtomSpecifier>,
    ) -> Result<(), FeatureError>;
    fn create_bond(
        &mut self,
        a1: &AtomSpecifier,
        a2: &AtomSpecifier,
        order: BondOrder,
    ) -> Result<(), FeatureError>;
    fn add_bonded_atom(
        &mut self,
        element: Element,
        pos: ultraviolet::Vec3,
        spec: AtomSpecifier,
        bond_target: AtomSpecifier,
        bond_order: BondOrder,
    ) -> Result<(), FeatureError>;
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BondedAtom {
    pub target: AtomSpecifier,
    pub element: Element,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PdbFeature {
    pub name: String,
    pub contents: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum Feature {
    RootAtom(Element),
    BondedAtom(BondedAtom),
    PdbFeature(PdbFeature),
}

impl Feature {
    pub fn apply(
        &self,
        feature_id: &FeatureId,
        commands: &mut dyn MoleculeCommands,
    ) -> Result<(), FeatureError> {
        match self {
            Feature::RootAtom(element) => {
                commands.add_atom(
                    *element,
                    Default::default(),
                    AtomSpecifier::new(*feature_id),
                    None,
                )?;
            }
            Feature::BondedAtom(BondedAtom { target, element }) => {
                let spec = AtomSpecifier::new(*feature_id);

                let x = {
                    let atom = commands.find_atom(target);
                    let atom = atom.ok_or(FeatureError::BrokenReference(ReferenceType::Atom))?;
                    atom.raw_pos.x + 5.0
                };

                commands.add_bonded_atom(
                    *element,
                    ultraviolet::Vec3::new(x, 0.0, 0.0),
                    spec.clone(),
                    target.clone(),
                    1,
                )?;
            }
            Feature::PdbFeature(PdbFeature { name, contents }) => {
                crate::pdb::spawn_pdb(name, contents, feature_id, commands)?;
            }
        }

        Ok(())
    }
}

/// A container that stores a list of features. It allows the list to be manipulated without
/// changing the indexes of existing features.
#[derive(Default, Deserialize, Serialize)]
pub struct FeatureList {
    counter: usize,
    order: Vec<FeatureId>,
    features: HashMap<FeatureId, Feature>,
}

impl FeatureList {
    // Inserts a feature at position `location` within the feature list, shifting all features after it to the right.
    pub fn insert(&mut self, feature: Feature, location: usize) -> usize {
        let id = self.counter;

        self.order.insert(location, id);
        self.features.insert(id, feature);

        self.counter += 1;
        self.counter
    }

    // Removes the feature with the given `id` from the feature list, shifting all features after it to the left.
    pub fn remove(&mut self, id: FeatureId) {
        self.features.remove(&id);
        self.order.remove(id);
    }

    pub fn get(&self, id: &FeatureId) -> Option<&Feature> {
        self.features.get(id)
    }

    // Adds a new feature to the end of the feature list.
    pub fn push_back(&mut self, feature: Feature) -> usize {
        let id = self.counter;

        self.order.push(id);
        self.features.insert(id, feature);

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
    type Item = &'a Feature;

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.list.order.get(self.current_index)?;
        self.current_index += 1;
        self.list.get(index)
    }
}

impl<'a> IntoIterator for &'a FeatureList {
    type Item = &'a Feature;
    type IntoIter = FeatureListIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        FeatureListIter {
            list: self,
            current_index: 0,
        }
    }
}
