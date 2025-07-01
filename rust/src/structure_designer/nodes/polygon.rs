use glam::i32::IVec2;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::vec_ivec2_serializer;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::common::csg_types::CSG;
use crate::structure_designer::evaluator::network_evaluator::GeometrySummary2D;
use crate::util::transform::Transform2D;
use glam::DVec2;

#[derive(Debug, Serialize, Deserialize)]
pub struct PolygonData {
  #[serde(with = "vec_ivec2_serializer")]
  pub vertices: Vec<IVec2>,
}

impl NodeData for PolygonData {
    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }
}

pub fn eval_polygon<'a>(network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, _registry: &NodeTypeRegistry) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let polygon_data = &node.data.as_any_ref().downcast_ref::<PolygonData>().unwrap();

  let mut points: Vec<[f64; 2]> = Vec::new();

  for i in 0..polygon_data.vertices.len() {
      points.push([polygon_data.vertices[i].x as f64, polygon_data.vertices[i].y as f64]);
  }

  let geometry = CSG::polygon(&points, None);

  return NetworkResult::Geometry2D(
    GeometrySummary2D {
      frame_transform: Transform2D::new(
        DVec2::new(0.0, 0.0),
        0.0,
      ),
      csg: geometry,
    }
  );
}
