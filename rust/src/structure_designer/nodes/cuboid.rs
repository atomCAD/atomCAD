use crate::structure_designer::geo_tree::GeoNode;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec3_serializer;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::GeometrySummary;
use crate::structure_designer::evaluator::network_result::error_in_input;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::util::transform::Transform;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DQuat;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;

#[derive(Debug, Serialize, Deserialize)]
pub struct CuboidData {
  #[serde(with = "ivec3_serializer")]
  pub min_corner: IVec3,
  #[serde(with = "ivec3_serializer")]
  pub extent: IVec3,
}

impl NodeData for CuboidData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}

pub fn eval_cuboid<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext
) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let cuboid_data = &node.data.as_any_ref().downcast_ref::<CuboidData>().unwrap();

  let min_corner = match network_evaluator.evaluate_or_default(
    network_stack, node_id, registry, context, 0, 
    cuboid_data.min_corner, 
    NetworkResult::extract_ivec3
  ) {
    Ok(value) => value,
    Err(error) => return error,
  };

  let extent = match network_evaluator.evaluate_or_default(
    network_stack, node_id, registry, context, 1, 
    cuboid_data.extent, 
    NetworkResult::extract_ivec3
  ) {
    Ok(value) => value,
    Err(error) => return error,
  };

  let real_min_corner = min_corner.as_dvec3();
  let real_extent = extent.as_dvec3();
  let center = real_min_corner + real_extent / 2.0;

  return NetworkResult::Geometry(GeometrySummary { 
    frame_transform: Transform::new(
      center,
      DQuat::IDENTITY,
    ),
    geo_tree_root: GeoNode::Cuboid {
      min_corner: min_corner,
      extent: extent 
    },
  });
}
