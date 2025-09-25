use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec3_serializer;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::GeometrySummary;
use crate::util::transform::Transform;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DQuat;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::geo_tree::GeoNode;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::node_type::NodeType;

#[derive(Debug, Serialize, Deserialize)]
pub struct SphereData {
  #[serde(with = "ivec3_serializer")]
  pub center: IVec3,
  pub radius: i32,
}

impl NodeData for SphereData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
      None
    }
}

pub fn eval_sphere<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let sphere_data = &node.data.as_any_ref().downcast_ref::<SphereData>().unwrap();

  let center = match network_evaluator.evaluate_or_default(
    network_stack, node_id, registry, context, 0, 
    sphere_data.center, 
    NetworkResult::extract_ivec3
  ) {
    Ok(value) => value,
    Err(error) => return error,
  };

  let radius = match network_evaluator.evaluate_or_default(
    network_stack, node_id, registry, context, 1, 
    sphere_data.radius, 
    NetworkResult::extract_int
  ) {
    Ok(value) => value,
    Err(error) => return error,
  };

  return NetworkResult::Geometry(GeometrySummary { 
    frame_transform: Transform::new(
    center.as_dvec3(),
    DQuat::IDENTITY,
    ),
    geo_tree_root: GeoNode::Sphere {
      center: center,
      radius: radius,
    },
  });
}
