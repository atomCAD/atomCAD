use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::util::transform::Transform2D;
use glam::f64::DVec2;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_result::GeometrySummary2D;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::geo_tree::GeoNode;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::NodeType;
use serde::{Serialize, Deserialize};
use crate::structure_designer::evaluator::network_result::unit_cell_mismatch_error;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intersect2DData {
}

impl NodeData for Intersect2DData {
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
    //let _timer = Timer::new("eval_intersect");
    let mut shapes: Vec<GeoNode> = Vec::new();
    let mut frame_translation = DVec2::ZERO;
  
    let shapes_val = network_evaluator.evaluate_arg_required(
      network_stack,
      node_id,
      registry,
      context,
      0,
    );
  
    if let NetworkResult::Error(_) = shapes_val {
      return shapes_val;
    }
  
    // Extract the array elements from shapes_val
    let shape_results = if let NetworkResult::Array(array_elements) = shapes_val {
      array_elements
    } else {
      return NetworkResult::Error("Expected array of geometry shapes".to_string());
    };
  
    let shape_count = shape_results.len();
    
    if shape_count == 0 {
      return NetworkResult::Error("Intersect requires at least one input geometry".to_string());
    }
  
    // Extract geometries and check unit cell compatibility
    let mut geometries: Vec<GeometrySummary2D> = Vec::new();
    for shape_val in shape_results {
      if let NetworkResult::Geometry2D(shape) = shape_val {
        geometries.push(shape);
      } else {
        return NetworkResult::Error("All inputs must be geometry objects".to_string());
      }
    }
    
    // Check unit cell compatibility - compare all to the first geometry
    if !GeometrySummary2D::all_have_compatible_unit_cells(&geometries) {
      return unit_cell_mismatch_error();
    }
    
    // All unit cells are compatible, proceed with intersection
    let first_unit_cell = &geometries[0].unit_cell;
    for geometry in &geometries {
      shapes.push(geometry.geo_tree_root.clone()); 
      frame_translation += geometry.frame_transform.translation;
    }
  
    frame_translation /= shape_count as f64;
  
    return NetworkResult::Geometry2D(GeometrySummary2D { 
      unit_cell: first_unit_cell.clone(),
      frame_transform: Transform2D::new(
        frame_translation,
        0.0,
      ),
      geo_tree_root: GeoNode::Intersection2D { shapes },
    });
  }

  fn clone_box(&self) -> Box<dyn NodeData> {
      Box::new(self.clone())
  }

  fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
      None
  }
}
