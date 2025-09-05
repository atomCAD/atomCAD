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
}

pub fn eval_sphere<'a>(
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  _registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let sphere_data = &node.data.as_any_ref().downcast_ref::<SphereData>().unwrap();

  let center = sphere_data.center.as_dvec3();

  return NetworkResult::Geometry(GeometrySummary { 
    frame_transform: Transform::new(
    center,
    DQuat::IDENTITY,
    ),
    geo_tree_root: GeoNode::Sphere {
      center: sphere_data.center,
      radius: sphere_data.radius,
    },
  });
}
