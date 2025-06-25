use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec3_serializer;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::GeometrySummary;
use crate::util::transform::Transform;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DQuat;
use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::node_network::Node;
use glam::f64::DVec3;
use crate::common::csg_types::CSG;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;

#[derive(Debug, Serialize, Deserialize)]
pub struct SphereData {
  #[serde(with = "ivec3_serializer")]
  pub center: IVec3,
  pub radius: i32,
}

impl NodeData for SphereData {
    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
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

  let geometry = if context.explicit_geo_eval_needed { CSG::sphere(
    sphere_data.radius as f64,
    32,
    16,
    None
  )
    .translate(center.x, center.y, center.z) } else { CSG::new() };

  return NetworkResult::Geometry(GeometrySummary { 
    frame_transform: Transform::new(
    center,
    DQuat::IDENTITY,
    ),
    csg: geometry,
  });
}

pub fn implicit_eval_sphere<'a>(
  _evaluator: &ImplicitEvaluator,
  _registry: &NodeTypeRegistry,
  _network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &DVec3) -> f64 {
  let sphere_data = &node.data.as_any_ref().downcast_ref::<SphereData>().unwrap();

  return (sample_point - DVec3::new(sphere_data.center.x as f64, sphere_data.center.y as f64, sphere_data.center.z as f64)).length() 
    - (sphere_data.radius as f64);
}
