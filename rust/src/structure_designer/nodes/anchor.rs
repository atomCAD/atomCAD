use crate::{common::atomic_structure::AtomDisplayState, structure_designer::evaluator::network_evaluator::NetworkEvaluator};
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::common::atomic_structure::AtomicStructure;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::option_ivec3_serializer;
use crate::common::crystal_utils::in_crystal_pos_to_id;

#[derive(Debug, Serialize, Deserialize)]
pub struct AnchorData {
  #[serde(with = "option_ivec3_serializer")]
  pub position: Option<IVec3>,
}

impl NodeData for AnchorData {
  fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
    None
  }
}

impl AnchorData {
  pub fn new() -> Self {
      Self {
          position: None,
      }
  }
}

pub fn eval_anchor<'a>(network_evaluator: &NetworkEvaluator, network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, registry: &NodeTypeRegistry) -> NetworkResult {  
  let node = NetworkStackElement::get_top_node(network_stack, node_id);

  let input_val = if node.arguments[0].argument_node_ids.is_empty() {
    return NetworkResult::Atomic(AtomicStructure::new());
  } else {
    let input_node_id = node.arguments[0].get_node_id().unwrap();
    network_evaluator.evaluate(network_stack, input_node_id, registry, false)[0].clone()
  };

  if let NetworkResult::Atomic(mut atomic_structure) = input_val {
    let anchor_data = &node.data.as_any_ref().downcast_ref::<AnchorData>().unwrap();

    atomic_structure.anchor_position = anchor_data.position;

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
