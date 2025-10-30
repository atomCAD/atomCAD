use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::f64::DVec3;
use serde::{Serialize, Deserialize};
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
use crate::structure_designer::geo_tree::GeoNode;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtrudeData {
  pub height: i32,
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
    
      let unit_cell = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 1, 
        UnitCellStruct::cubic_diamond(), 
        NetworkResult::extract_unit_cell,
        ) {
        Ok(value) => value,
        Err(error) => return error,
      };

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
        let frame_translation_2d = shape.frame_transform.translation;
    
        let frame_transform = Transform::new(
          DVec3::new(frame_translation_2d.x, frame_translation_2d.y, 0.0),
          DQuat::from_rotation_z(shape.frame_transform.rotation),
        );
    
        let direction = unit_cell.c.normalize();
        let height = unit_cell.c.length() * (height as f64);
        let s = shape.geo_tree_root;
        return NetworkResult::Geometry(GeometrySummary { 
          unit_cell,
          frame_transform,
          geo_tree_root: GeoNode::Extrude { 
            height: height,
            direction: direction,
            shape: Box::new(s),
          },
        });
      } else {
        return runtime_type_error_in_input(0);
      }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        Some(format!("h: {}", self.height))
    }
}



