use petgraph::visit::{IntoNodeReferences, VisitMap, Visitable, Walker};
use scene::{AtomIndex, MoleculeGraph};
use std::collections::HashMap;
use ultraviolet::Vec3;

pub fn relax(graph: &mut MoleculeGraph) {
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
        graph.node_weight_mut(node_index).unwrap().pos += force * 0.1;
    }
}
