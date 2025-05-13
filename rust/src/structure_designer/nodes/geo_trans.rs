use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec3_serializer;
use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::node_network::Node;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DVec3;
use glam::DQuat;
use std::f64::consts::PI;

#[derive(Debug, Serialize, Deserialize)]
pub struct GeoTransData {
  #[serde(with = "ivec3_serializer")]
  pub translation: IVec3,
  #[serde(with = "ivec3_serializer")]
  pub rotation: IVec3, // intrinsic euler angles where 1 increment means 90 degrees.
  pub transform_only_frame: bool, // If true, only the reference frame is transformed, the geometry remains in place.
}

impl NodeData for GeoTransData {
    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}

pub fn implicit_eval_geo_trans<'a>(evaluator: &ImplicitEvaluator,
  registry: &NodeTypeRegistry,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &DVec3) -> f64 {

  let mut transformed_point = sample_point.clone(); 

  let geo_trans_data = &node.data.as_any_ref().downcast_ref::<GeoTransData>().unwrap();

  if !geo_trans_data.transform_only_frame {
    let translation = geo_trans_data.translation.as_dvec3();
    let rotation_euler = geo_trans_data.rotation.as_dvec3() * PI * 0.5;

    let rotation_quat = DQuat::from_euler(
        glam::EulerRot::XYX,
        rotation_euler.x, 
        rotation_euler.y, 
        rotation_euler.z);

    transformed_point = rotation_quat.inverse().mul_vec3(sample_point - translation); 
  }

  match node.arguments[0].get_node_id() {
      Some(node_id) => evaluator.implicit_eval(
          network_stack,
          node_id, 
          &transformed_point,
          registry)[0],
      None => f64::MAX
  }
}