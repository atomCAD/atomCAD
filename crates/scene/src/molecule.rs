use std::collections::HashMap;

use lazy_static::lazy_static;
use periodic_table::Element;
use petgraph::stable_graph;
use render::{AtomKind, AtomRepr, Atoms, GlobalRenderResources};
use ultraviolet::Vec3;

use crate::{
    feature::{Feature, FeatureError, FeatureList, MoleculeCommands, ReferenceType},
    ids::AtomSpecifier,
    utils::BoundingBox,
};

pub type Graph = stable_graph::StableUnGraph<AtomNode, BondOrder>;
pub type BondOrder = u8;
pub type AtomIndex = stable_graph::NodeIndex;
#[allow(unused)]
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
    bounding_box: BoundingBox,
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
        self.bounding_box = Default::default();
        self.gpu_synced = false;
    }
}

lazy_static! {
    pub static ref PERIODIC_TABLE: periodic_table::PeriodicTable =
        periodic_table::PeriodicTable::new();
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
        self.bounding_box.enclose_sphere(
            pos,
            // TODO: This is
            PERIODIC_TABLE.element_reprs[element as usize].radius,
        );
        self.gpu_synced = false;

        Ok(())
    }

    fn create_bond(
        &mut self,
        a1: &AtomSpecifier,
        a2: &AtomSpecifier,
        order: BondOrder,
    ) -> Result<(), FeatureError> {
        match (self.atom_map.get(a1), self.atom_map.get(a2)) {
            (Some(&a1_index), Some(&a2_index)) => {
                self.graph.add_edge(a1_index, a2_index, order);
                Ok(())
            }
            _ => Err(FeatureError::BrokenReference(ReferenceType::Atom)),
        }
    }

    fn find_atom(&self, spec: &AtomSpecifier) -> Option<&AtomNode> {
        match self.atom_map.get(spec) {
            Some(atom_index) => self.graph.node_weight(*atom_index),
            None => None,
        }
    }
}

/// Demonstration of how to use the feature system
/// let mut molecule = Molecule::from_feature(
///     &gpu_resources,
///     RootAtom {
///         element: Element::Iodine,
///     },
/// );
///
/// molecule.push_feature(AtomFeature {
///     target: scene::ids::AtomSpecifier::new(0),
///     element: Element::Sulfur,
/// });
/// molecule.apply_all_features();
///
/// molecule.push_feature(AtomFeature {
///     target: scene::ids::AtomSpecifier::new(1),
///     element: Element::Carbon,
/// });
/// molecule.apply_all_features();
///
/// molecule.set_history_step(2);
/// molecule.reupload_atoms(&gpu_resources);
pub struct Molecule {
    pub repr: MoleculeRepr,
    gpu_atoms: Atoms,
    #[allow(unused)]
    rotation: ultraviolet::Rotor3,
    #[allow(unused)]
    offset: ultraviolet::Vec3,
    features: FeatureList,
    // The index one greater than the most recently applied feature's location in the feature list.
    // This is unrelated to feature IDs: it is effectively just a counter of how many features are
    // applied. (i.e. our current location in the edit history timeline)
    history_step: usize,
    // When checkpointing is implemented, this will be needed:
    //
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
            history_step: 1, // This starts at 1 because we applied the primitive feature
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
            println!("Applying feature {}", feature_id);
            let feature = self
                .features
                .get(feature_id)
                .expect("Feature IDs referenced by the FeatureList order should exist!");

            if feature.apply(feature_id, &mut self.repr).is_err() {
                // TODO: Bubble error to the user
                println!("Feature reconstruction error on feature {}", feature_id);
            }

            dbg!(&self.repr.bounding_box);
            println!();
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

    pub fn get_ray_hit(&self, origin: Vec3, direction: Vec3) -> Option<AtomIndex> {
        // Using `direction` as a velocity vector, determine when the ray will
        // collide with the bounding box. Note the ? - this fn returns early if there
        // isn't a collision.
        let (tmin, tmax) = self.repr.bounding_box.ray_hit_times(origin, direction)?;
        println!("got a ray");
        dbg!(tmin, tmax);

        // If the box is fully behind the raycast direction, we will never get a hit.
        if tmax <= 0.0 {
            return None;
        }

        // Knowing that the ray will enter the box, we can now march along it by a fixed step
        // size. At each step, we check for a collision with an atom, and return that atom's index
        // if a collision occurs.

        // We know that the box is first hit at `origin + tmin * direction`. However,
        // tmin can be negative, and we only want to march forwards. So,
        // we constrain tmin to be nonnegative.
        let mut current_pos = origin + f32::max(0.0, tmin) * direction;

        // This is an empirically reasonable value. It is still possible to miss an atom if
        // the user clicks on the very edge of it, but this is rare.
        let step_size = PERIODIC_TABLE.element_reprs[Element::Hydrogen as usize].radius / 10.0;
        let step = direction * step_size;

        while self.repr.bounding_box.contains(current_pos) {
            // Placeholder for additional logic
            dbg!(&current_pos);

            current_pos += step;
        }

        None
    }

    // TODO: Optimize heavily (use octree, compute entry point of ray analytically)
}
