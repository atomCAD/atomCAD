use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DVec3;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::GeometrySummary;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::util::transform::Transform;
use glam::f64::DQuat;
use crate::geo_tree::GeoNode;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use serde::{Serialize, Deserialize};
use crate::structure_designer::evaluator::network_result::unit_cell_mismatch_error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntersectData {
}

impl NodeData for IntersectData {
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
    let mut frame_translation = DVec3::ZERO;
  
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
    let mut geometries: Vec<GeometrySummary> = Vec::new();
    for shape_val in shape_results {
      if let NetworkResult::Geometry(shape) = shape_val {
        geometries.push(shape);
      } else {
        return NetworkResult::Error("All inputs must be geometry objects".to_string());
      }
    }
    
    // Check unit cell compatibility - compare all to the first geometry
    if !GeometrySummary::all_have_compatible_unit_cells(&geometries) {
      return unit_cell_mismatch_error();
    }
    
    // All unit cells are compatible, proceed with intersection
    // Take the first unit cell by value before consuming the geometries vector
    let first_unit_cell = geometries[0].unit_cell.clone();
    for geometry in geometries.into_iter() {
      shapes.push(geometry.geo_tree_root);
      frame_translation += geometry.frame_transform.translation;
    }
  
    frame_translation /= shape_count as f64;
  
    return NetworkResult::Geometry(GeometrySummary { 
      unit_cell: first_unit_cell,
      frame_transform: Transform::new(
        frame_translation,
        DQuat::IDENTITY,
      ),
      geo_tree_root: GeoNode::intersection_3d(shapes),
    });
  }

  fn clone_box(&self) -> Box<dyn NodeData> {
      Box::new(self.clone())
  }

  fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
      None
  }
}

pub fn get_node_type() -> NodeType {
  NodeType {
      name: "intersect".to_string(),
      description: "Computes the Boolean intersection of any number of 3D geometries. The `shapes` input accepts an array of `Geometry` values. Use this to cut geometries with a half-space.".to_string(),
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
          Parameter {
              name: "shapes".to_string(),
              data_type: DataType::Array(Box::new(DataType::Geometry)),
          },
      ],
      output_type: DataType::Geometry,
      public: true,
      node_data_creator: || Box::new(IntersectData {}),
      node_data_saver: generic_node_data_saver::<IntersectData>,
      node_data_loader: generic_node_data_loader::<IntersectData>,
    }
}














