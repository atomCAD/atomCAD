
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::util::imat3::IMat3;
use glam::i32::IVec3;
use glam::{DMat3, DVec3};
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec3_serializer;
use crate::common::atomic_structure::{Atom, AtomicStructure};
use crate::common::crystal_utils::crystal_rot_to_mat;
use crate::common::crystal_utils::id_to_in_crystal_pos;
use crate::common::crystal_utils::in_crystal_pos_to_id;

#[derive(Debug, Serialize, Deserialize)]
pub struct StampPlacement {
  #[serde(with = "ivec3_serializer")]
  pub position: IVec3,
  pub x_dir: i32, // +x, -x, +y, -y, +z, -z: 6 possibilities
  pub y_dir: i32, // 4 possibilities
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StampData {
  pub stamp_placements: Vec<StampPlacement>,
}

impl NodeData for StampData {
  fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
    None
  }
}

impl StampData {
  pub fn new() -> Self {
      Self {
          stamp_placements: Vec::new(),
      }
  }
}

pub fn eval_stamp<'a>(network_evaluator: &NetworkEvaluator, network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, registry: &NodeTypeRegistry) -> NetworkResult {  
  let node = NetworkStackElement::get_top_node(network_stack, node_id);

  let crystal_val = if node.arguments[0].argument_node_ids.is_empty() {
    NetworkResult::Atomic(AtomicStructure::new())
  } else {
    let input_node_id = node.arguments[0].get_node_id().unwrap();
    network_evaluator.evaluate(network_stack, input_node_id, registry)[0].clone()
  };

  let stamp_val = if node.arguments[1].argument_node_ids.is_empty() {
    return crystal_val;
  } else {
    let input_node_id = node.arguments[1].get_node_id().unwrap();
    network_evaluator.evaluate(network_stack, input_node_id, registry)[0].clone()
  };

  if let NetworkResult::Atomic(stamp_structure) = stamp_val {
      
    let anchor_position = match stamp_structure.anchor_position {
      Some(anchor_position) => {
        anchor_position
      },
      None => {return crystal_val; },
    };

    let stamp_data = &node.data.as_any_ref().downcast_ref::<StampData>().unwrap();
    
    if let NetworkResult::Atomic(mut crystal_structure) = crystal_val {
      for stamp_placement in stamp_data.stamp_placements.iter() {
        place_stamp(&mut crystal_structure, &stamp_structure, stamp_placement);
      }
      return NetworkResult::Atomic(crystal_structure);
    }
    return crystal_val;
  }
  return NetworkResult::Atomic(AtomicStructure::new());
}

fn place_stamp(
  crystal_structure: &mut AtomicStructure,
  stamp_structure: &AtomicStructure,
  stamp_placement: &StampPlacement) {
    let stamping_rotation = crystal_rot_to_mat(stamp_placement.x_dir, stamp_placement.y_dir);
    let stamping_translation = stamp_placement.position - stamp_structure.anchor_position.unwrap();
    let double_stamping_rotation = stamping_rotation.as_dmat3();
    let double_stamping_translation = stamping_translation.as_dvec3();
    for atom in stamp_structure.atoms.values() {
      let dest_atom_id = calc_dest_atom_id(atom.id, &stamping_rotation, &stamping_translation);
      let dest_pos = calc_dest_pos(atom, &double_stamping_rotation, &double_stamping_translation);
      crystal_structure.replace_atom(dest_atom_id, atom.atomic_number);
      crystal_structure.set_atom_position(dest_atom_id, dest_pos);
    }
}

fn calc_dest_atom_id(
  source_atom_id: u64,
  rotation: &IMat3,
  translation: &IVec3) -> u64 {
    let source_pos = id_to_in_crystal_pos(source_atom_id);
    let dest_pos = rotation.mul(&source_pos) + translation;
    in_crystal_pos_to_id(&dest_pos)
}

fn calc_dest_pos(
  source_atom: &Atom,
  rotation: &DMat3,
  translation: &DVec3) -> DVec3 {
    rotation.mul_vec3(source_atom.position) + translation
}
