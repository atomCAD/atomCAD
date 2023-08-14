use std::{collections::HashMap, iter::Empty};

use periodic_table::Element;
use petgraph::{
    data::Build,
    graph::Node,
    stable_graph::{self, NodeIndex},
};
use render::{AtomKind, AtomRepr, Atoms, GlobalRenderResources};
use ultraviolet::Vec3;

use crate::{
    feature::{FeatureList, MoleculeCommands},
    ids::{AtomSpecifier, FeatureCopyId},
};

type Graph = stable_graph::StableUnGraph<AtomNode, BondOrder>;
pub type BondOrder = u8;
pub type AtomIndex = stable_graph::NodeIndex;
pub type BondIndex = stable_graph::EdgeIndex;

struct AtomNode {
    element: Element,
    pos: Vec3,
    spec: AtomSpecifier,
}

pub struct Molecule {
    gpu_atoms: Atoms,
    graph: Graph,
    rotation: ultraviolet::Rotor3,
    offset: ultraviolet::Vec3,
    gpu_synced: bool,
    feature_list: FeatureList,
    // TODO: This atom map is a simple but extremely inefficient implementation. This data
    // is highly structued and repetitive: compression, flattening, and a tree could do
    // a lot to optimize this.
    atom_map: HashMap<AtomSpecifier, AtomIndex>,
}

impl Molecule {
    // Creates a `Molecule` containing just one atom. At the moment, it is not possible
    // to construct a `Molecule` with no contents, as wgpu will panic if an empty gpu buffer
    // is created
    pub fn from_first_atom(gpu_resources: &GlobalRenderResources, first_atom: Element) -> Self {
        let mut graph = Graph::default();
        let spec = AtomSpecifier {
            feature_path: vec![FeatureCopyId {
                feature_id: 0,
                copy_index: 0,
            }],
            child_index: 0,
        };

        let first_index = graph.add_node(AtomNode {
            element: first_atom,
            pos: Vec3::default(),
            // TODO: This needs to either come from a feature or be returned
            spec: spec.clone(),
        });

        let gpu_atoms = Atoms::new(
            gpu_resources,
            [AtomRepr {
                kind: AtomKind::new(first_atom),
                pos: Vec3::default(),
            }],
        );

        Molecule {
            gpu_atoms,
            graph,
            rotation: ultraviolet::Rotor3::default(),
            offset: ultraviolet::Vec3::default(),
            gpu_synced: false,
            feature_list: Default::default(),
            atom_map: HashMap::from([(spec, first_index)]),
        }
    }

    pub fn reupload_atoms(&mut self, gpu_resources: &GlobalRenderResources) {
        let atoms: Vec<AtomRepr> = self
            .graph
            .node_weights()
            .map(|node| AtomRepr {
                kind: AtomKind::new(node.element),
                pos: node.pos,
            })
            .collect();

        // TODO: not working, see shinzlet/atomCAD #3
        // self.gpu_atoms.reupload_atoms(&atoms, gpu_resources);

        // This is a workaround, but it has bad perf as it always drops and
        // reallocates
        self.gpu_atoms = Atoms::new(gpu_resources, atoms);
        self.gpu_synced = true;
    }

    pub fn atoms(&self) -> &Atoms {
        &self.gpu_atoms
    }
}

impl MoleculeCommands for Molecule {
    fn add_atom(&mut self, element: Element, pos: ultraviolet::Vec3, spec: AtomSpecifier) {
        let index = self.graph.add_node(AtomNode {
            element,
            pos,
            spec: spec.clone(),
        });
        self.atom_map.insert(spec, index);
        self.gpu_synced = false;
    }

    fn create_bond(&mut self, a1: &AtomSpecifier, a2: &AtomSpecifier, order: BondOrder) {}
}
