// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::edit::{Edit, EditList};
use crate::molecule::{Molecule, MoleculeCheckpoint};

pub struct MoleculeEditor {
    pub repr: Molecule,
    #[allow(unused)]
    rotation: ultraviolet::Rotor3,
    #[allow(unused)]
    offset: ultraviolet::Vec3,
    edits: EditList,
    // The index one greater than the most recently applied feature's location in the feature list.
    // This is unrelated to feature IDs: it is effectively just a counter of how many features are
    // applied. (i.e. our current location in the edit history timeline)
    history_step: usize,
    // when `history_step` is set to `i`, if `checkpoints` contains the key `i`, then
    // `checkpoints.get(i)` contains the graph and geometry which should be used to render
    // the molecule. This allows feature application and relaxation to be cached until they
    // need to be recomputed. This saves a lot of time, as relaxation is a very expensive operation that does not
    // commute with feature application.
    checkpoints: HashMap<usize, MoleculeCheckpoint>,
    // the history step we cannot equal or exceed without first recomputing. For example, if repr
    // is up to date with the feature list, and then a past feature is changed, dirty_step would change
    // from `features.len()` to the index of the changed feature. This is used to determine if recomputation
    // is needed when moving forwards in the timeline, or if a future checkpoint can be used.
    dirty_step: usize,
}

impl MoleculeEditor {
    pub fn from_feature(edit: Edit) -> Self {
        let mut repr = Molecule::default();
        edit.apply(&0, &mut repr)
            .expect("Primitive features should never return a feature error!");
        // Relaxation is currently causing infinte loops on loaded PDB files.
        // Disabled until the code matures a bit.
        //repr.relax();

        let mut features = EditList::default();
        features.push_back(edit);

        Self {
            repr,
            rotation: ultraviolet::Rotor3::default(),
            offset: ultraviolet::Vec3::default(),
            edits: features,
            history_step: 1, // This starts at 1 because we applied the primitive feature
            checkpoints: Default::default(),
            dirty_step: 1, // Although no checkpoints exist, repr is not dirty, so we advance this to its max
        }
    }

    pub fn edits(&self) -> &EditList {
        &self.edits
    }

    pub fn insert_edit(&mut self, edit: Edit) {
        self.edits.insert(edit, self.history_step);
    }

    // Advances the model to a given history step by applying features in the timeline.
    // This will not in general recompute the history, so if a past feature is changed,
    // you must recompute from there.
    pub fn set_history_step(&mut self, history_step: usize) {
        // TODO: Bubble error to user
        assert!(
            history_step <= self.edits.len(),
            "history step exceeds edit list size"
        );

        // Find the best checkpoint to start reconstructing from:
        let best_checkpoint = self
            .checkpoints
            .keys()
            .filter(|candidate| **candidate <= history_step)
            .max();

        match best_checkpoint {
            None => {
                // If there wasn't a usable checkpoint, we can either keep computing forwards or
                // restart. We only have to restart from scratch if we're moving backwards, otherwise
                // we can just move forwards.

                if self.history_step > history_step {
                    self.history_step = 0;
                    self.repr.clear();
                }
            }
            Some(best_checkpoint) => {
                // If there was, we can go there and resume from that point
                self.repr
                    .set_checkpoint(self.checkpoints.get(best_checkpoint).unwrap().clone());
                self.history_step = *best_checkpoint;
            }
        }

        for edit_id in &self.edits.order()[self.history_step..history_step] {
            println!("Applying edit {}", edit_id);
            let edit = self
                .edits
                .get(edit_id)
                .expect("Feature IDs referenced by the FeatureList order should exist!");

            if edit.apply(edit_id, &mut self.repr).is_err() {
                // TODO: Bubble error to the user
                println!("Failed to apply the edit with id {}", edit_id);
                dbg!(&edit);
            }

            self.repr.relax();
        }

        self.dirty_step = history_step;
        self.history_step = history_step;
    }

    // equivalent to `set_history_step(features.len()): applies every feature that is in the
    // feature timeline.
    pub fn apply_all_edits(&mut self) {
        self.set_history_step(self.edits.len())
    }
}

// This is a stripped down representation of the molecule that removes several
// fields (some are redundant, like repr.atom_map, and some are not serializable,
// like repr.gpu_atoms).
#[derive(Serialize, Deserialize)]
struct ProxyMolecule {
    rotation: ultraviolet::Rotor3,
    offset: ultraviolet::Vec3,
    edits: EditList,
    history_step: usize,
    checkpoints: HashMap<usize, MoleculeCheckpoint>,
    dirty_step: usize,
}

impl Serialize for MoleculeEditor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Custom serialization is used to ensure that the current molecule state is
        // saved as a checkpoint, even if it normally would not be (i.e. if it's already
        // very close to an existing checkpoint). This allows faster loading when the file
        // is reopened.

        let mut checkpoints = self.checkpoints.clone();
        checkpoints.insert(self.history_step, self.repr.make_checkpoint());

        let data = ProxyMolecule {
            rotation: self.rotation,
            offset: self.offset,
            edits: self.edits.clone(),
            history_step: self.history_step,
            checkpoints,
            dirty_step: self.dirty_step,
        };

        data.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MoleculeEditor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // TODO: integrity check of the deserialized struct

        let data = ProxyMolecule::deserialize(deserializer)?;

        let mut molecule = MoleculeEditor {
            repr: Molecule::default(),
            rotation: data.rotation,
            offset: data.offset,
            edits: data.edits,
            history_step: data.history_step, // This starts at 0 because we haven't applied the features, we've just loaded them

            checkpoints: data.checkpoints,
            dirty_step: data.dirty_step,
        };

        // this advances the history step to the correct location
        molecule.set_history_step(data.history_step);

        Ok(molecule)
    }
}
