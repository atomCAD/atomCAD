
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
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::common::atomic_structure::HitTestResult;
use crate::common::crystal_utils::{is_crystal_atom_id, id_to_in_crystal_pos, in_crystal_pos_to_id};

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
  pub selected_stamp_placement: Option<usize>,
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
          selected_stamp_placement: None,
      }
  }
}

pub fn eval_stamp<'a>(network_evaluator: &NetworkEvaluator, network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, registry: &NodeTypeRegistry) -> NetworkResult {  
  let node = NetworkStackElement::get_top_node(network_stack, node_id);

  let crystal_val = if node.arguments[0].argument_node_ids.is_empty() {
    NetworkResult::Atomic(AtomicStructure::new())
  } else {
    let input_node_id = node.arguments[0].get_node_id().unwrap();
    network_evaluator.evaluate(network_stack, input_node_id, registry, false)[0].clone()
  };

  let stamp_val = if node.arguments[1].argument_node_ids.is_empty() {
    return crystal_val;
  } else {
    let input_node_id = node.arguments[1].get_node_id().unwrap();
    network_evaluator.evaluate(network_stack, input_node_id, registry, false)[0].clone()
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

pub fn add_or_select_stamp_placement_by_ray(structure_designer: &mut StructureDesigner, ray_start: &DVec3, ray_dir: &DVec3) {
  let atomic_structure = match structure_designer.get_atomic_structure_from_selected_node() {
    Some(structure) => structure,
    None => return,
  };

  // Find the atom along the ray, ignoring bond hits
  let atom_id = match atomic_structure.hit_test(ray_start, ray_dir) {
    HitTestResult::Atom(id, _) => id,
    _ => return,
  };

  let stamp_data = match get_selected_stamp_data_mut(structure_designer) {
    Some(data) => data,
    None => return,
  };

  if !is_crystal_atom_id(atom_id) {
    return;
  }

  let position = id_to_in_crystal_pos(atom_id);

  // TODO: not all atom positions can be selected (also depends on whether zinc-blende)
  // but maybe the whole valiadation business should be thought out, as the stamp input can change
  // after setting this data, so invalidation should be done generally on input change too.

  // TODO: maybe select existing placement

  stamp_data.stamp_placements.push(StampPlacement {
    position,
    x_dir: 0,
    y_dir: 0,
  });
  stamp_data.selected_stamp_placement = Some(stamp_data.stamp_placements.len() - 1);
}

/// Gets the StampData for the currently selected stamp node (mutable)
/// 
/// Returns None if:
/// - There is no active node network
/// - No node is selected in the active network
/// - The selected node is not a stamp node
/// - The StampData cannot be retrieved or cast
pub fn get_selected_stamp_data_mut(structure_designer: &mut StructureDesigner) -> Option<&mut StampData> {
  let selected_node_id = structure_designer.get_selected_node_id_with_type("stamp")?;

  let node_data = structure_designer.get_node_network_data_mut(selected_node_id)?;
    
  node_data.as_any_mut().downcast_mut::<StampData>()
}