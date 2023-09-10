use std::collections::HashMap;

use lazy_static::lazy_static;
use periodic_table::Element;
use petgraph::{stable_graph, visit::IntoNodeReferences};
use render::{AtomBuffer, AtomKind, AtomRepr, GlobalRenderResources};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use ultraviolet::Vec3;

use crate::edit::{EditContext, EditError, ReferenceType};

use common::{ids::AtomSpecifier, BoundingBox};

lazy_static! {
    pub static ref PERIODIC_TABLE: periodic_table::PeriodicTable =
        periodic_table::PeriodicTable::new();
}

pub type MoleculeGraph = stable_graph::StableUnGraph<AtomNode, BondOrder>;
// A map that gives each atom in a molecule a coordinate. Used to cache structure energy minimization
// calculations
pub type AtomPositions = HashMap<AtomSpecifier, Vec3>;
pub type BondOrder = u8;
pub type AtomIndex = stable_graph::NodeIndex;
#[allow(unused)]
pub type BondIndex = stable_graph::EdgeIndex;

#[serde_as]
#[derive(Clone, Serialize, Deserialize)]
pub struct MoleculeCheckpoint {
    graph: MoleculeGraph,
    #[serde_as(as = "Vec<(_, _)>")]
    positions: AtomPositions,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AtomNode {
    pub element: Element,
    pub spec: AtomSpecifier,
    // The atom that this atom was bonded to (and uses as a "forward" direction). If
    // no such atom exists, then this atom is the root atom, and the forward direction
    // should be taken to be the molecule's +z axis. Although this field is not yet
    // used (as of september 3rd 2023), it is needed to describe molecular geometry
    // in terms of bond angles and lengths (which will be useful later on).
    pub head: Option<AtomSpecifier>,
}

impl AtomNode {
    pub fn forward(&self, commands: &dyn EditContext) -> Vec3 {
        match self.head {
            Some(ref head) => {
                let head_pos = commands
                    .pos(head)
                    .expect("The atom specifier an atom that is bonded should exist");
                let pos = commands
                    .pos(&self.spec)
                    .expect("The atom specifier an atom that is bonded should exist");

                (*head_pos - *pos).normalized()
            }
            None => Vec3::unit_z(),
        }
    }
}

/// The concrete representation of the molecule at some time in the feature history.
#[derive(Default)]
pub struct Molecule {
    // TODO: This atom map is a simple but extremely inefficient implementation. This data
    // is highly structued and repetitive: compression, flattening, and a tree could do
    // a lot to optimize this.
    atom_map: HashMap<AtomSpecifier, AtomIndex>,
    pub graph: MoleculeGraph,
    bounding_box: BoundingBox,
    gpu_synced: bool,
    gpu_atoms: Option<AtomBuffer>,
    positions: AtomPositions,
}

impl Molecule {
    pub fn atom_reprs(&self) -> Vec<AtomRepr> {
        self.graph
            .node_weights()
            .map(|node| AtomRepr {
                kind: AtomKind::new(node.element),
                pos: *self
                    .pos(&node.spec)
                    .expect("Every atom in the graph should have a position"),
            })
            .collect()
    }

    pub fn clear(&mut self) {
        self.atom_map.clear();
        self.graph.clear();
        self.bounding_box = Default::default();
        self.gpu_synced = false;
    }

    pub(crate) fn relax(&mut self) {
        self.positions = crate::dynamics::relax(&self.graph, &self.positions, 0.01);
    }

    pub fn reupload_atoms(&mut self, gpu_resources: &GlobalRenderResources) {
        // TODO: not working, see shinzlet/atomCAD #3
        // self.gpu_atoms.reupload_atoms(&atoms, gpu_resources);

        // This is a workaround, but it has bad perf as it always drops and
        // reallocates

        if self.graph.node_count() == 0 {
            self.gpu_atoms = None;
        } else {
            self.gpu_atoms = Some(AtomBuffer::new(gpu_resources, self.atom_reprs()));
        }

        self.gpu_synced = true;
    }

    pub fn atoms(&self) -> Option<&AtomBuffer> {
        self.gpu_atoms.as_ref()
    }

    pub fn set_checkpoint(&mut self, checkpoint: MoleculeCheckpoint) {
        self.graph = checkpoint.graph;
        self.positions = checkpoint.positions;
        self.atom_map.clear();

        for (atom_index, atom) in self.graph.node_references() {
            self.atom_map.insert(atom.spec.clone(), atom_index);
        }
    }

    pub fn make_checkpoint(&self) -> MoleculeCheckpoint {
        MoleculeCheckpoint {
            graph: self.graph.clone(),
            positions: self.positions.clone(),
        }
    }

    pub fn bounding_box(&self) -> &BoundingBox {
        &self.bounding_box
    }

    // TODO: Optimize heavily (use octree, compute entry point of ray analytically)
    pub fn get_ray_hit(&self, origin: Vec3, direction: Vec3) -> Option<AtomSpecifier> {
        // Using `direction` as a velocity vector, determine when the ray will
        // collide with the bounding box. Note the ? - this fn returns early if there
        // isn't a collision.
        let (tmin, tmax) = self.bounding_box.ray_hit_times(origin, direction)?;

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
        let t_span = tmax - f32::max(0.0, tmin);
        // the direction vector is normalized, so 1 unit of time = 1 unit of space
        let num_steps = (t_span / step_size) as usize;

        for _ in 0..num_steps {
            for atom in self.graph.node_weights() {
                let atom_radius_sq = PERIODIC_TABLE.element_reprs[atom.element as usize]
                    .radius
                    .powi(2);

                let atom_pos = *self
                    .positions
                    .get(&atom.spec)
                    .expect("Every atom in the graph should have an associated position");
                if (current_pos - atom_pos).mag_sq() < atom_radius_sq {
                    return Some(atom.spec.clone());
                }
            }

            current_pos += step;
        }

        None
    }
}

impl EditContext for Molecule {
    fn add_bonded_atom(
        &mut self,
        element: Element,
        pos: ultraviolet::Vec3,
        spec: AtomSpecifier,
        bond_target: AtomSpecifier,
        bond_order: BondOrder,
    ) -> Result<(), EditError> {
        self.add_atom(element, pos, spec.clone(), Some(bond_target.clone()))?;
        self.create_bond(&spec, &bond_target, bond_order)
    }

    fn add_atom(
        &mut self,
        element: Element,
        pos: ultraviolet::Vec3,
        spec: AtomSpecifier,
        head: Option<AtomSpecifier>,
    ) -> Result<(), EditError> {
        if self.atom_map.contains_key(&spec) {
            return Err(EditError::AtomOverwrite);
        }

        let index = self.graph.add_node(AtomNode {
            element,
            spec: spec.clone(),
            head,
        });

        self.atom_map.insert(spec.clone(), index);
        self.bounding_box.enclose_sphere(
            pos,
            // TODO: This is
            PERIODIC_TABLE.element_reprs[element as usize].radius,
        );
        self.gpu_synced = false;
        self.positions.insert(spec, pos);

        Ok(())
    }

    fn create_bond(
        &mut self,
        a1: &AtomSpecifier,
        a2: &AtomSpecifier,
        order: BondOrder,
    ) -> Result<(), EditError> {
        match (self.atom_map.get(a1), self.atom_map.get(a2)) {
            (Some(&a1_index), Some(&a2_index)) => {
                self.graph.add_edge(a1_index, a2_index, order);
                Ok(())
            }
            _ => Err(EditError::BrokenReference(ReferenceType::Atom)),
        }
    }

    fn find_atom(&self, spec: &AtomSpecifier) -> Option<&AtomNode> {
        match self.atom_map.get(spec) {
            Some(atom_index) => self.graph.node_weight(*atom_index),
            None => None,
        }
    }

    fn pos(&self, spec: &AtomSpecifier) -> Option<&Vec3> {
        self.positions.get(spec)
    }
}
