use std::iter::Empty;

use periodic_table::Element;
use petgraph::{
    data::Build,
    graph::Node,
    stable_graph::{self, NodeIndex},
};
use render::{AtomKind, AtomRepr, Atoms, GlobalRenderResources};
use ultraviolet::Vec3;

type BondOrder = u8;
type Graph = stable_graph::StableUnGraph<AtomNode, BondOrder>;

struct AtomNode {
    element: Element,
    pos: Vec3,
}

pub struct Molecule {
    gpu_atoms: Atoms,
    graph: Graph,
    rotation: ultraviolet::Rotor3,
    offset: ultraviolet::Vec3,
}

impl Molecule {
    // Creates a `Molecule` containing no data.
    pub fn from_first_atom(
        gpu_resources: &GlobalRenderResources,
        first_atom: Element,
    ) -> (Self, NodeIndex) {
        let mut graph = Graph::default();

        let first_index = graph.add_node(AtomNode {
            element: first_atom,
            pos: Vec3::default(),
        });

        let gpu_atoms = Atoms::new(
            gpu_resources,
            [AtomRepr {
                kind: AtomKind::new(first_atom),
                pos: Vec3::default(),
            }],
        );

        (
            Molecule {
                gpu_atoms,
                graph,
                rotation: ultraviolet::Rotor3::default(),
                offset: ultraviolet::Vec3::default(),
            },
            first_index,
        )
    }

    //
    pub fn add_atom(
        &mut self,
        element: Element,
        bond_to: NodeIndex,
        bond_order: BondOrder,
        gpu_resources: &GlobalRenderResources,
    ) -> NodeIndex {
        // Create the node on the graph
        let new_node = self.graph.add_node(AtomNode {
            element,
            // TODO: compute pos from the atom being bonded to
            pos: Vec3::new(5., 0., 0.),
        });
        self.graph.add_edge(new_node, bond_to, bond_order);

        // Synchronize with the GPU
        self.reupload_atoms(gpu_resources);
        new_node
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
    }

    pub fn atoms(&self) -> &Atoms {
        &self.gpu_atoms
    }
}
