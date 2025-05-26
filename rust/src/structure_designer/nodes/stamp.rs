
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
use crate::common::atomic_structure::{Atom, AtomDisplayState, AtomicStructure};
use crate::common::crystal_utils::CRYSTAL_ROTATION_MATRICES;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::common::atomic_structure::HitTestResult;
use crate::common::crystal_utils::{
  ZincBlendeAtomType,
  is_crystal_atom_id,
  id_to_in_crystal_pos,
  in_crystal_pos_to_id,
  get_zinc_blende_atom_type_for_pos,
};
use crate::structure_designer::evaluator::network_evaluator::input_missing_error;
use crate::structure_designer::evaluator::network_evaluator::error_in_input;

#[derive(Debug, Serialize, Deserialize)]
pub struct StampPlacement {
  #[serde(with = "ivec3_serializer")]
  pub position: IVec3,
  pub rotation: i32, // Index into CRYSTAL_ROTATION_MATRICES (0-11)
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

pub fn eval_stamp<'a>(network_evaluator: &NetworkEvaluator, network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, registry: &NodeTypeRegistry, decorate: bool, context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext) -> NetworkResult {  
  let node = NetworkStackElement::get_top_node(network_stack, node_id);

  if node.arguments[0].argument_node_ids.is_empty() {
    return input_missing_error("crystal");
  }

  let input_node_id = node.arguments[0].get_node_id().unwrap();
  let crystal_val = network_evaluator.evaluate(network_stack, input_node_id, registry, false, context)[0].clone();

  if let NetworkResult::Error(_error) = crystal_val {
    return error_in_input("crystal");
  }

  if node.arguments[1].argument_node_ids.is_empty() {
    return input_missing_error("stamp");
  }

  let input_node_id = node.arguments[1].get_node_id().unwrap();
  let stamp_val = network_evaluator.evaluate(network_stack, input_node_id, registry, false, context)[0].clone();

  if let NetworkResult::Error(_error) = stamp_val {
    return error_in_input("stamp");
  }

  if let NetworkResult::Atomic(stamp_structure) = stamp_val {

    let anchor_position = match stamp_structure.anchor_position {
      Some(anchor_position) => {
        anchor_position
      },
      None => {return NetworkResult::Error("stamp has no anchor position".to_string()); },
    };

    let stamp_data = &node.data.as_any_ref().downcast_ref::<StampData>().unwrap();
    
    if let NetworkResult::Atomic(mut crystal_structure) = crystal_val {
      for (index, stamp_placement) in stamp_data.stamp_placements.iter().enumerate() {
        let is_selected = stamp_data.selected_stamp_placement.map_or(false, |selected_index| selected_index == index);
        place_stamp(&mut crystal_structure, &stamp_structure, stamp_placement, decorate, is_selected);
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
  stamp_placement: &StampPlacement,
  decorate: bool,
  selected: bool) {
    let quarter_unit_cell_size = stamp_structure.crystal_meta_data.unit_cell_size * 0.25;
    let anchor_position = stamp_structure.anchor_position.unwrap();
    let anchor_position_double = anchor_position.as_dvec3() * quarter_unit_cell_size;

    let anchor_site_type = get_zinc_blende_atom_type_for_pos(&anchor_position);
    let placement_site_type = get_zinc_blende_atom_type_for_pos(&stamp_placement.position);
    let rotation_index = stamp_placement.rotation as usize % 12;
    let mut stamping_rotation = CRYSTAL_ROTATION_MATRICES[rotation_index];

    if anchor_site_type != placement_site_type {
      stamping_rotation = stamping_rotation.mul_imat3(
        &IMat3::new(
          &IVec3::new(0, -1, 0), 
          &IVec3::new(-1, 0, 0), 
          &IVec3::new(0, 0, -1))
      );
    }

    let double_stamping_rotation = stamping_rotation.as_dmat3();

    let stamp_placement_position_double = stamp_placement.position.as_dvec3() * quarter_unit_cell_size;

    for atom in stamp_structure.atoms.values() {
      let dest_atom_id = calc_dest_atom_id(atom.id, &stamping_rotation, &anchor_position, &stamp_placement.position);
      let dest_pos = calc_dest_pos(atom, &double_stamping_rotation, &anchor_position_double, &stamp_placement_position_double);
      crystal_structure.replace_atom(dest_atom_id, atom.atomic_number);
      crystal_structure.set_atom_position(dest_atom_id, dest_pos);
    }

    if decorate {
      let atom_id = in_crystal_pos_to_id(&stamp_placement.position);
      if crystal_structure.atoms.contains_key(&atom_id) {
        crystal_structure.decorator.set_atom_display_state(
          atom_id, 
          if selected {AtomDisplayState::Marked} else {AtomDisplayState::SecondaryMarked}
        );
      }
    }
}

fn calc_dest_atom_id(
  source_atom_id: u64,
  rotation: &IMat3,
  anchor_position: &IVec3,
  stamp_placement_position: &IVec3) -> u64 {
    let source_pos = id_to_in_crystal_pos(source_atom_id);
    let dest_pos = rotation.mul(&(source_pos - anchor_position)) + stamp_placement_position;
    in_crystal_pos_to_id(&dest_pos)
}

fn calc_dest_pos(
  source_atom: &Atom,
  rotation: &DMat3,
  anchor_position: &DVec3,
  stamp_placement_position: &DVec3,
) -> DVec3 {
    rotation.mul_vec3(source_atom.position - anchor_position) + stamp_placement_position
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
    rotation: 0,
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

/// Sets the rotation of the selected stamp placement
/// 
/// The rotation parameter specifies the index into CRYSTAL_ROTATION_MATRICES (0-11).
pub fn set_rotation(structure_designer: &mut StructureDesigner, node_id: u64, rotation: i32) {
  let Some(node_data) = structure_designer.get_node_network_data_mut(node_id) else { return };
  
  let Some(stamp_data) = node_data.as_any_mut().downcast_mut::<StampData>() else { return };
  
  let Some(index) = stamp_data.selected_stamp_placement else { return };
  
  if index < stamp_data.stamp_placements.len() {
    stamp_data.stamp_placements[index].rotation = rotation % 12;
  }
}

/// Deletes the currently selected stamp placement
pub fn delete_selected_stamp_placement(structure_designer: &mut StructureDesigner, node_id: u64) {
  let Some(node_data) = structure_designer.get_node_network_data_mut(node_id) else { return };
  
  let Some(stamp_data) = node_data.as_any_mut().downcast_mut::<StampData>() else { return };
  
  let Some(index) = stamp_data.selected_stamp_placement else { return };
  
  if index < stamp_data.stamp_placements.len() {
    stamp_data.stamp_placements.remove(index);
    stamp_data.selected_stamp_placement = None;
  }
}
