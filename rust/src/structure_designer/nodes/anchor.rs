use crate::{common::atomic_structure::AtomDisplayState, structure_designer::evaluator::network_evaluator::NetworkEvaluator};
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::common::atomic_structure::AtomicStructure;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::option_ivec3_serializer;
use crate::common::crystal_utils::{in_crystal_pos_to_id, is_crystal_atom_id, id_to_in_crystal_pos};
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::common::atomic_structure::HitTestResult;
use glam::f64::DVec3;
use crate::structure_designer::evaluator::network_result::input_missing_error;
use crate::structure_designer::evaluator::network_result::error_in_input;
use crate::structure_designer::node_type::NodeType;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorData {
  #[serde(with = "option_ivec3_serializer")]
  pub position: Option<IVec3>,
}

impl NodeData for AnchorData {
  fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
    None
  }

  fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
    None
  }

  fn eval<'a>(
    &self,
    network_evaluator: &NetworkEvaluator,
    network_stack: &Vec<NetworkStackElement<'a>>,
    node_id: u64,
    registry: &NodeTypeRegistry,
    _decorate: bool,
    context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext) -> NetworkResult {

    let input_val = network_evaluator.evaluate_arg_required(
      network_stack,
      node_id,
      registry,
      context,
      0,
    );  

    if let NetworkResult::Error(_) = input_val {
      return input_val;
    }

    if let NetworkResult::Atomic(mut atomic_structure) = input_val {
      atomic_structure.anchor_position = self.position;
  
      if let Some(pos) = atomic_structure.anchor_position {
        let anchor_atom_id = in_crystal_pos_to_id(&pos);
        if atomic_structure.atoms.contains_key(&anchor_atom_id) {
          atomic_structure.decorator.set_atom_display_state(anchor_atom_id, AtomDisplayState::Marked);
        }
      }
  
      return NetworkResult::Atomic(atomic_structure);
    }
    return NetworkResult::Atomic(AtomicStructure::new());
  }

  fn clone_box(&self) -> Box<dyn NodeData> {
      Box::new(self.clone())
  }

  fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
      self.position.map(|pos| format!("({},{},{})", pos.x, pos.y, pos.z))
  }
}

impl AnchorData {
  pub fn new() -> Self {
      Self {
          position: None,
      }
  }
}


pub fn select_anchor_atom_by_ray(structure_designer: &mut StructureDesigner, ray_start: &DVec3, ray_dir: &DVec3) {
  let atomic_structure = match structure_designer.get_atomic_structure_from_selected_node() {
    Some(structure) => structure,
    None => return,
  };

  // Find the atom along the ray, ignoring bond hits
  let atom_id = match atomic_structure.hit_test(ray_start, ray_dir) {
    HitTestResult::Atom(id, _) => id,
    _ => return,
  };

  let anchor_data = match get_selected_anchor_data_mut(structure_designer) {
    Some(data) => data,
    None => return,
  };

  if !is_crystal_atom_id(atom_id) {
    return;
  }

  let position = id_to_in_crystal_pos(atom_id);

  if let Some(pos) = anchor_data.position {
    if pos == position {
      anchor_data.position = None;
      return;
    }
  }

  anchor_data.position = Some(position);
}  

/// Gets the AnchorData for the currently selected anchor node (mutable)
/// 
/// Returns None if:
/// - There is no active node network
/// - No node is selected in the active network
/// - The selected node is not an anchor node
/// - The AnchorData cannot be retrieved or cast
pub fn get_selected_anchor_data_mut(structure_designer: &mut StructureDesigner) -> Option<&mut AnchorData> {
  let selected_node_id = structure_designer.get_selected_node_id_with_type("anchor")?;

  // Get the node data and cast it to EditAtomData
  let node_data = structure_designer.get_node_network_data_mut(selected_node_id)?;
    
  // Try to downcast to EditAtomData
  node_data.as_any_mut().downcast_mut::<AnchorData>()
}
