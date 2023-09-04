// use petgraph::visit::{IntoNodeReferences, VisitMap, Visitable, Walker};
use scene::{feature::MoleculeCommands, AtomIndex, MoleculeGraph};
use std::collections::HashMap;
use ultraviolet::Vec3;

// considerations:
// - we need to start the relaxation from the current corrections
// - because of this, it would be nice if we can just return a new correction buffer rather than mutating
//   anything
// - additionally, this lets us store corrections buffers as checkpoints and only apply them when needed! good!
// - so don't store corrections in the AtomNode. this does make iteration harder, we can't just iterate
//   over the graph, so we'll need to find another way of handling this. And we need `commands` so that we
//   can figure out the forward direction of the atom we're moving to put the corrections in bond-angle terms
//   (not doing that now but probably will want to)

pub fn relax(graph: &MoleculeGraph, corrections: &mut dyn MoleculeCommands) {
    let mut forces = HashMap::<AtomIndex, Vec3>::new();

    for node_index in graph.node_indices() {
        let node = graph.node_weight(node_index).unwrap();
        if true {
            let mut force = Vec3::default();

            for other_index in graph.node_indices() {
                if other_index == node_index {
                    continue;
                }

                let other = graph.node_weight(other_index).unwrap();
                if true {
                    let displacement = other.pos - node.pos;
                    if graph.contains_edge(node_index, other_index) {
                        let force_str = 2.0 * (displacement.mag() - 1.0);
                        force += displacement.normalized() * force_str;
                    } else {
                        let force_str = 1.0 / displacement.mag_sq();
                        force += -displacement.normalized() * force_str;
                    }
                }
            }

            forces.insert(node_index, force);
        }
    }

    for (node_index, force) in forces {
        let spec = &graph.node_weight(node_index).as_ref().unwrap().spec;

        graph.node_weight_mut(node_index).unwrap().pos += force * 0.1;
    }
}
