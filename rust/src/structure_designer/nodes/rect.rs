use crate::common::csg_types::CSG;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::util::transform::Transform2D;
use glam::i32::IVec2;
use glam::f64::DVec2;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec2_serializer;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::GeometrySummary2D;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::node_network::Node;

#[derive(Debug, Serialize, Deserialize)]
pub struct RectData {
  #[serde(with = "ivec2_serializer")]
  pub min_corner: IVec2,
  #[serde(with = "ivec2_serializer")]
  pub extent: IVec2,
}

impl NodeData for RectData {
    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }
}

pub fn eval_rect<'a>(
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  _registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let rect_data = &node.data.as_any_ref().downcast_ref::<RectData>().unwrap();

  let min_corner = rect_data.min_corner.as_dvec2();
  let extent = rect_data.extent.as_dvec2();
  let center = min_corner + extent / 2.0;

  let geometry = if context.explicit_geo_eval_needed { 
    CSG::square(extent.x, extent.y, None).translate(min_corner.x, min_corner.y, 0.0)
  } else { 
    CSG::new()
  };

  return NetworkResult::Geometry2D(
    GeometrySummary2D {
      frame_transform: Transform2D::new(
        center,
        0.0,
      ),
      csg: geometry,
    });
}

pub fn implicit_eval_rect<'a>(
  _evaluator: &ImplicitEvaluator,
  _registry: &NodeTypeRegistry,
  _network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &DVec2) -> f64 {
  let rect_data = &node.data.as_any_ref().downcast_ref::<RectData>().unwrap();

  let max_corner = rect_data.min_corner + rect_data.extent;
  let x_val = f64::max((rect_data.min_corner.x as f64) - sample_point.x, sample_point.x - (max_corner.x as f64));
  let y_val = f64::max((rect_data.min_corner.y as f64) - sample_point.y, sample_point.y - (max_corner.y as f64));

  return f64::max(x_val, y_val);
}
