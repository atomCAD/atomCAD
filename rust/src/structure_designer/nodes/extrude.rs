use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::f64::DVec3;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use crate::util::serialization_utils::ivec3_serializer;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::DQuat;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_result::runtime_type_error_in_input;
use crate::structure_designer::evaluator::network_result::GeometrySummary;
use crate::util::transform::Transform;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::geo_tree::GeoNode;
use crate::structure_designer::node_type::NodeType;

fn default_extrude_direction() -> IVec3 {
  IVec3::new(0, 0, 1)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtrudeData {
  pub height: i32,
  #[serde(with = "ivec3_serializer")]
  #[serde(default = "default_extrude_direction")]
  pub extrude_direction: IVec3,
}

impl NodeData for ExtrudeData {
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
      context: &mut NetworkEvaluationContext,
    ) -> NetworkResult {
      //let _timer = Timer::new("eval_extrude");
      let shape_val = network_evaluator.evaluate_arg_required(
        network_stack,
        node_id,
        registry,
        context,
        0,
      );
    
      // NOTE: Input pin 1 (extrude pin) is deprecated but kept for backward compatibility with existing networks.
      // We ignore it and use the unit cell from the shape instead.

      if let NetworkResult::Error(_) = shape_val {
        return shape_val;
      }

      let height = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 2, 
        self.height, 
        NetworkResult::extract_int
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      if let NetworkResult::Geometry2D(shape) = shape_val {
        // Extract unit cell from the drawing plane
        let unit_cell = shape.drawing_plane.unit_cell.clone();
        
        // Validate extrusion direction for this plane (in world space)
        let (world_direction, dir_length) = match shape.drawing_plane.validate_extrude_direction(&self.extrude_direction) {
            Ok(result) => result,
            Err(error_msg) => return NetworkResult::Error(error_msg),
        };
        
        // Calculate actual extrusion height based on direction length and height multiplier
        let height_real = dir_length * (height as f64);
        
        // Compute plane_to_world transform from DrawingPlane
        let plane_to_world_transform = shape.drawing_plane.to_world_transform();
        
        // Transform world extrusion direction to plane-local coordinates
        let world_to_plane_rotation = plane_to_world_transform.rotation.inverse();
        let local_direction = world_to_plane_rotation * world_direction;
        
        let frame_translation_2d = shape.frame_transform.translation;
    
        let frame_transform = Transform::new(
          DVec3::new(frame_translation_2d.x, frame_translation_2d.y, 0.0),
          DQuat::from_rotation_z(shape.frame_transform.rotation),
        );
    
        let s = shape.geo_tree_root;
        return NetworkResult::Geometry(GeometrySummary { 
          unit_cell,
          frame_transform,
          geo_tree_root: GeoNode::extrude(height_real, local_direction, Box::new(s), plane_to_world_transform)
        });
      } else {
        return runtime_type_error_in_input(0);
      }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        Some(format!("h: {} dir: [{},{},{}]", 
            self.height,
            self.extrude_direction.x,
            self.extrude_direction.y,
            self.extrude_direction.z
        ))
    }
}



















