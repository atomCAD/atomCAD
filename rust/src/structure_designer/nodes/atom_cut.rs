use crate::structure_designer::common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
use crate::structure_designer::evaluator::network_result::error_in_input;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_result::input_missing_error;
use crate::structure_designer::geo_tree::GeoNode;
use serde::{Serialize, Deserialize};
use crate::common::atomic_structure::AtomicStructure;
use crate::structure_designer::implicit_eval::implicit_geometry::ImplicitGeometry3D;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::NodeType;

#[derive(Debug, Serialize, Deserialize)]
pub struct AtomCutData {
  pub cut_sdf_value: f64,
  pub unit_cell_size: f64,
}

impl NodeData for AtomCutData {
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
    let node = NetworkStackElement::get_top_node(network_stack, node_id);
  
    let molecule_input_name = registry.get_parameter_name(&node, 0);
    if node.arguments[0].is_empty() {
      return input_missing_error(&molecule_input_name);
    }
    let molecule_input_node_id = node.arguments[0].get_node_id().unwrap();
    let molecule_input_val = network_evaluator.evaluate(network_stack, molecule_input_node_id, 0, registry, false, context);
    if let NetworkResult::Error(_error) = molecule_input_val {
      return error_in_input(&molecule_input_name);
    }
  
    if let NetworkResult::Atomic(mut atomic_structure) = molecule_input_val {
  
      if node.arguments[1].is_empty() {
        return NetworkResult::Atomic(atomic_structure); // no cutters plugged in, just return the input atomic structure unmodified.
      }
  
      let mut shapes: Vec<GeoNode> = Vec::new();
    
      let shapes_val = network_evaluator.evaluate_arg_required(
        network_stack,
        node_id,
        registry,
        context,
        1,
      );
    
      if let NetworkResult::Error(_) = shapes_val {
        return shapes_val;
      }
    
      // Extract the array elements from shapes_val
      let shape_results = if let NetworkResult::Array(array_elements) = shapes_val {
        array_elements
      } else {
        return NetworkResult::Error("Invalid shapes input.".to_string());
      };
    
      for shape_val in shape_results {
        if let NetworkResult::Geometry(shape) = shape_val {
          shapes.push(shape.geo_tree_root); 
        }
      }
    
      let cutter_geo_tree_root = GeoNode::Intersection3D { shapes: shapes };
  
      cut_atomic_structure(&mut atomic_structure, &cutter_geo_tree_root, self.cut_sdf_value, self.unit_cell_size);
  
      return NetworkResult::Atomic(atomic_structure);
    }
    return NetworkResult::Atomic(AtomicStructure::new());
  }
}

impl AtomCutData {
  pub fn new() -> Self {
    Self {
      cut_sdf_value: 0.0,
      unit_cell_size: DIAMOND_UNIT_CELL_SIZE_ANGSTROM,
    }
  }
}

fn cut_atomic_structure(atomic_structure: &mut AtomicStructure, cutter_geo_tree_root: &GeoNode, scaled_cut_sdf_value: f64, unit_cell_size: f64) {
  // Collect atom IDs to delete to avoid borrowing issues during iteration
  let mut atoms_to_delete = Vec::new();
  
  // Iterate over all atoms and check if they are outside the geometry
  for (atom_id, atom) in &atomic_structure.atoms {
    // Evaluate the atom's position against the cutter geometry
    let sdf_value = cutter_geo_tree_root.implicit_eval_3d(&(atom.position / unit_cell_size));
    
    let cut_sdf_value = scaled_cut_sdf_value / unit_cell_size;

    // If the SDF value is greater than the cut threshold, the atom is outside and should be deleted
    if sdf_value > cut_sdf_value {
      atoms_to_delete.push(*atom_id);
    }
  }
  
  // Delete all atoms that are outside the geometry
  // The delete_atom method will also handle removing associated bonds
  for atom_id in atoms_to_delete {
    atomic_structure.delete_atom(atom_id, false);
  }
}
