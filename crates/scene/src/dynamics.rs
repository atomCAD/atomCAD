use crate::ids::AtomSpecifier;
use crate::molecule::MoleculeGraph;
use std::collections::HashMap;
use ultraviolet::Vec3;

// considerations:
// - we need to start the relaxation from the current corrections
// - because of this, it would be nice if we can just return a new correction buffer rather than mutating
//   anything
// - additionally, this lets us store corrections buffers as checkpoints and only apply them when needed! good!
// - so don't store corrections in the AtomNode. this does make iteration harder, we can't just iterate
//   over the graph, so we'll need to find another way of handling this.
// For now, this is done using cartesian coordinates

pub fn relax(
    graph: &MoleculeGraph,
    positions: &HashMap<AtomSpecifier, Vec3>,
    threshold: f32,
) -> HashMap<AtomSpecifier, Vec3> {
    let mut old_positions = positions.clone();
    let mut positions = HashMap::<AtomSpecifier, Vec3>::with_capacity(positions.len());
    let mut step_count = 0;

    loop {
        let mut largest_adjustment = 0.0;
        for node_index in graph.node_indices() {
            let node = graph.node_weight(node_index).unwrap();
            let pos = old_positions.get(&node.spec).unwrap();

            let mut force = Vec3::default();

            for other_index in graph.node_indices() {
                if other_index == node_index {
                    continue;
                }

                let other = graph.node_weight(other_index).unwrap();
                let displacement = *old_positions.get(&other.spec).unwrap() - *pos;
                if graph.contains_edge(node_index, other_index) {
                    let force_str = 2.0 * (displacement.mag() - 4.0);
                    force += displacement.normalized() * force_str;
                } else {
                    let force_str = 1.0 / displacement.mag_sq();
                    force += -displacement.normalized() * force_str;
                }
            }

            let strength = 0.1;
            let adjustment = force * strength;

            if adjustment.mag() > largest_adjustment {
                largest_adjustment = adjustment.mag();
            }

            let new_pos = *pos + adjustment;
            positions.insert(node.spec.clone(), new_pos);
        }

        std::mem::swap(&mut positions, &mut old_positions);

        if largest_adjustment < threshold {
            break;
        }

        step_count += 1;
    }

    println!("steps taken: {}", step_count);

    positions
}
