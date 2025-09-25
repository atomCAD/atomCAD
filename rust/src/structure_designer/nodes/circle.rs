use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::i32::IVec2;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec2_serializer;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::GeometrySummary2D;
use crate::util::transform::Transform2D;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::geo_tree::GeoNode;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::node_type::NodeType;

#[derive(Debug, Serialize, Deserialize)]
pub struct CircleData {
  #[serde(with = "ivec2_serializer")]
  pub center: IVec2,
  pub radius: i32,
}

impl NodeData for CircleData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
      None
    }
}

pub fn eval_circle<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let circle_data = &node.data.as_any_ref().downcast_ref::<CircleData>().unwrap();

  let center = match network_evaluator.evaluate_or_default(
    network_stack, node_id, registry, context, 0, 
    circle_data.center, 
    NetworkResult::extract_ivec2
  ) {
    Ok(value) => value,
    Err(error) => return error,
  };

  let radius = match network_evaluator.evaluate_or_default(
    network_stack, node_id, registry, context, 1, 
    circle_data.radius, 
    NetworkResult::extract_int
  ) {
    Ok(value) => value,
    Err(error) => return error,
  };

  return NetworkResult::Geometry2D(
    GeometrySummary2D {
      frame_transform: Transform2D::new(
        center.as_dvec2(),
        0.0,
      ),
      geo_tree_root: GeoNode::Circle {
        center: center,
        radius: radius,
      },
  });
}

