// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::HashMap;

use common::ids::*;
use periodic_table::Element;
use serde::{Deserialize, Serialize};

use crate::{molecule::AtomNode, BondOrder};

#[derive(Debug)]
pub enum ReferenceType {
    Atom,
    Edit,
    // Molecule,
    // File,
    // etc.
}

#[derive(Debug)]
pub enum EditError {
    BrokenReference(ReferenceType),
    AtomOverwrite,
}

/// A proxy trait that allows a molecule to be manipulated without exposing its implementation.
/// Features can only manipulate a molecule using MoleculeCommands.
pub trait EditContext {
    fn find_atom(&self, spec: &AtomSpecifier) -> Option<&AtomNode>;
    fn pos(&self, spec: &AtomSpecifier) -> Option<&ultraviolet::Vec3>;
    fn add_atom(
        &mut self,
        element: Element,
        pos: ultraviolet::Vec3,
        spec: AtomSpecifier,
        head: Option<AtomSpecifier>,
    ) -> Result<(), EditError>;
    fn remove_atom(&mut self, target: &AtomSpecifier) -> Result<(), EditError>;
    fn create_bond(
        &mut self,
        a1: &AtomSpecifier,
        a2: &AtomSpecifier,
        order: BondOrder,
    ) -> Result<(), EditError>;
    fn add_bonded_atom(
        &mut self,
        element: Element,
        pos: ultraviolet::Vec3,
        spec: AtomSpecifier,
        bond_target: AtomSpecifier,
        bond_order: BondOrder,
    ) -> Result<(), EditError>;
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BondedAtom {
    pub target: AtomSpecifier,
    pub element: Element,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateBond {
    pub start: AtomSpecifier,
    pub stop: AtomSpecifier,
    pub order: u8,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PdbData {
    pub name: String,
    pub contents: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Edit {
    RootAtom(Element),
    BondedAtom(BondedAtom),
    DeleteAtom(AtomSpecifier),
    CreateBond(CreateBond),
    PdbImport(PdbData),
}

impl Edit {
    pub fn apply(&self, edit_id: &EditId, ctx: &mut dyn EditContext) -> Result<(), EditError> {
        match self {
            Edit::RootAtom(element) => {
                ctx.add_atom(
                    *element,
                    Default::default(),
                    AtomSpecifier::new(*edit_id),
                    None,
                )?;
            }
            Edit::BondedAtom(BondedAtom { target, element }) => {
                let spec = AtomSpecifier::new(*edit_id);

                let pos = *ctx
                    .pos(target)
                    .ok_or(EditError::BrokenReference(ReferenceType::Atom))?;
                let pos = pos + ultraviolet::Vec3::new(5.0, 0.0, 0.0);

                ctx.add_bonded_atom(*element, pos, spec, target.clone(), 1)?;
            }
            Edit::DeleteAtom(spec) => {
                ctx.remove_atom(&spec)?;
            }
            Edit::CreateBond(CreateBond { start, stop, order }) => {
                ctx.create_bond(&start, &stop, *order)?;
            }
            Edit::PdbImport(PdbData { name, contents }) => {
                crate::pdb::spawn_pdb(name, contents, edit_id, ctx)?;
            }
        }

        Ok(())
    }
}

/// A container that stores a list of features. It allows the list to be manipulated without
/// changing the indexes of existing features.
#[derive(Default, Clone, Deserialize, Serialize)]
pub struct EditList {
    counter: usize,
    order: Vec<EditId>,
    edits: HashMap<EditId, Edit>,
}

impl EditList {
    // Inserts a feature at position `location` within the feature list, shifting all features after it to the right.
    pub fn insert(&mut self, edit: Edit, location: usize) -> usize {
        let id = self.counter;

        self.order.insert(location, id);
        self.edits.insert(id, edit);

        self.counter += 1;
        self.counter
    }

    // Removes the feature with the given `id` from the feature list, shifting all features after it to the left.
    pub fn remove(&mut self, id: EditId) {
        self.edits.remove(&id);
        self.order.remove(id);
    }

    pub fn get(&self, id: &EditId) -> Option<&Edit> {
        self.edits.get(id)
    }

    // Adds a new feature to the end of the feature list.
    pub fn push_back(&mut self, edit: Edit) -> usize {
        let id = self.counter;

        self.order.push(id);
        self.edits.insert(id, edit);

        self.counter += 1;
        self.counter
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn order(&self) -> &[EditId] {
        &self.order
    }
}

/// Allows a FeatureList to be iterated over.
pub struct EditListIter<'a> {
    list: &'a EditList,
    // This stores the index of iteration - but not the current feature ID.
    // The current feature ID is given by `list.order.get(self.current_index)`
    current_index: usize,
}

impl<'a> Iterator for EditListIter<'a> {
    type Item = &'a Edit;

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.list.order.get(self.current_index)?;
        self.current_index += 1;
        self.list.get(index)
    }
}

impl<'a> IntoIterator for &'a EditList {
    type Item = &'a Edit;
    type IntoIter = EditListIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        EditListIter {
            list: self,
            current_index: 0,
        }
    }
}
