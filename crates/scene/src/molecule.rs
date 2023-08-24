use std::{collections::HashMap, iter::Empty};

use periodic_table::Element;
use petgraph::{
    data::{Build, DataMap},
    graph::Node,
    stable_graph::{self, NodeIndex},
};
use render::{AtomKind, AtomRepr, Atoms, GlobalRenderResources};
use ultraviolet::Vec3;

use crate::{
    feature::{Feature, FeatureError, FeatureList, MoleculeCommands, ReferenceType, RootAtom},
    ids::{AtomSpecifier, FeatureCopyId},
};

pub type Graph = stable_graph::StableUnGraph<AtomNode, BondOrder>;
pub type BondOrder = u8;
pub type AtomIndex = stable_graph::NodeIndex;
pub type BondIndex = stable_graph::EdgeIndex;

pub struct AtomNode {
    pub element: Element,
    pub pos: Vec3,
    pub spec: AtomSpecifier,
}

/// The concrete representation of the molecule at some time in the feature history.
#[derive(Default)]
pub struct MoleculeRepr {
    // TODO: This atom map is a simple but extremely inefficient implementation. This data
    // is highly structued and repetitive: compression, flattening, and a tree could do
    // a lot to optimize this.
    atom_map: HashMap<AtomSpecifier, AtomIndex>,
    graph: Graph,
    gpu_synced: bool,
}

impl MoleculeRepr {
    fn atom_reprs(&self) -> Vec<AtomRepr> {
        self.graph
            .node_weights()
            .map(|node| AtomRepr {
                kind: AtomKind::new(node.element),
                pos: node.pos,
            })
            .collect()
    }

    fn clear(&mut self) {
        self.atom_map.clear();
        self.graph.clear();
        self.gpu_synced = false;
    }
}

impl MoleculeCommands for MoleculeRepr {
    fn add_atom(
        &mut self,
        element: Element,
        pos: ultraviolet::Vec3,
        spec: AtomSpecifier,
    ) -> Result<(), FeatureError> {
        if self.atom_map.contains_key(&spec) {
            return Err(FeatureError::AtomOverwrite);
        }

        let index = self.graph.add_node(AtomNode {
            element,
            pos,
            spec: spec.clone(),
        });
        self.atom_map.insert(spec, index);
        self.gpu_synced = false;
        Ok(())
    }

    fn create_bond(
        &mut self,
        a1: &AtomSpecifier,
        a2: &AtomSpecifier,
        order: BondOrder,
    ) -> Result<(), FeatureError> {
        match (self.atom_map.get(&a1), self.atom_map.get(&a2)) {
            (Some(&a1_index), Some(&a2_index)) => {
                self.graph.add_edge(a1_index, a2_index, order);
                Ok(())
            }
            _ => Err(FeatureError::BrokenReference(ReferenceType::Atom)),
        }
    }

    fn find_atom(&self, spec: &AtomSpecifier) -> Option<&AtomNode> {
        match self.atom_map.get(&spec) {
            Some(atom_index) => self.graph.node_weight(*atom_index),
            None => None,
        }
    }
}

pub struct Molecule {
    pub repr: MoleculeRepr,
    gpu_atoms: Atoms,
    rotation: ultraviolet::Rotor3,
    offset: ultraviolet::Vec3,
    features: FeatureList,
    // The index one greater than the most recently applied feature's location in the feature list.
    // This is unrelated to feature IDs: it is effectively just a counter of how many features are
    // applied. (i.e. our current location in the edit history timeline)
    history_step: usize,
    // When checkpointing is implemented, this will be needed:
    // the history step we cannot equal or exceed without first recomputing. For example, if repr
    // is up to date with the feature list, and then a past feature is changed, dirty_step would change
    // from `features.len()` to the index of the changed feature. This is used to determine if recomputation
    // is needed when moving forwards in the timeline, or if a future checkpoint can be used.
    // dirty_step: usize,
}

impl Molecule {
    pub fn from_feature(
        gpu_resources: &GlobalRenderResources,
        feature: impl Feature + 'static,
    ) -> Self {
        let mut repr = MoleculeRepr::default();
        feature
            .apply(&0, &mut repr)
            .expect("Primitive features should never return a feature error!");
        let gpu_atoms = Atoms::new(gpu_resources, repr.atom_reprs());
        let mut features = FeatureList::default();
        features.push_back(feature);

        Self {
            repr,
            gpu_atoms,
            rotation: ultraviolet::Rotor3::default(),
            offset: ultraviolet::Vec3::default(),
            features,
            history_step: 0,
        }
    }

    pub fn features(&self) -> &FeatureList {
        &self.features
    }

    pub fn push_feature(&mut self, feature: impl Feature + 'static) {
        self.features.insert(feature, self.history_step);
    }

    // Advances the model to a given history step by applying features in the timeline.
    // This will not in general recompute the history, so if a past feature is changed,
    // you must recompute from there.
    pub fn set_history_step(&mut self, history_step: usize) {
        // TODO: Bubble error to user
        assert!(
            history_step <= self.features.len(),
            "history step exceeds feature list size"
        );

        // If we are stepping back, we need to recompute starting at the beginning
        // (we don't currently use checkpoints or feature inversion).
        if history_step < self.history_step {
            self.history_step = 0;
            self.repr.clear();
        }

        for feature_id in &self.features.order()[self.history_step..history_step] {
            let feature = self
                .features
                .get(feature_id)
                .expect("Feature IDs referenced by the FeatureList order should exist!");

            if feature.apply(feature_id, &mut self.repr).is_err() {
                // TODO: Bubble error to the user
                println!("Feature reconstruction error on feature {}", feature_id);
            }
        }

        self.history_step = history_step;
    }

    // equivalent to `set_history_step(features.len()): applies every feature that is in the
    // feature timeline.
    pub fn apply_all_features(&mut self) {
        self.set_history_step(self.features.len())
    }

    pub fn reupload_atoms(&mut self, gpu_resources: &GlobalRenderResources) {
        // TODO: not working, see shinzlet/atomCAD #3
        // self.gpu_atoms.reupload_atoms(&atoms, gpu_resources);

        // This is a workaround, but it has bad perf as it always drops and
        // reallocates
        self.gpu_atoms = Atoms::new(gpu_resources, self.repr.atom_reprs());
        self.repr.gpu_synced = true;
    }

    pub fn atoms(&self) -> &Atoms {
        &self.gpu_atoms
    }
}
