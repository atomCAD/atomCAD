use periodic_table::Element;
use petgraph::stable_graph;
use render::Atoms;

type BondOrder = u8;

struct AtomNode {
    element: Element,
}
pub struct Molecule {
    gpu_buffer: Atoms,
    graph: stable_graph::StableGraph<AtomNode, BondOrder>,
}

impl Molecule {}
