use crate::crystolecule::crystolecule_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::geo_tree::GeoNode;
use serde::{Serialize, Deserialize};
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::geo_tree::implicit_geometry::ImplicitGeometry3D;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
  
    let molecule_input_val = network_evaluator.evaluate_arg_required(
      network_stack,
      node_id,
      registry,
      context,
      0,
    );
  
    if let NetworkResult::Error(_) = molecule_input_val {
      return molecule_input_val;
    }

    if let NetworkResult::Atomic(mut atomic_structure) = molecule_input_val {
    
      let shapes_val = network_evaluator.evaluate_arg(
        network_stack,
        node_id,
        registry,
        context,
        1,
      );

      if let NetworkResult::None = shapes_val {
        return NetworkResult::Atomic(atomic_structure); // no cutters plugged in, just return the input atomic structure unmodified.
      }

      if let NetworkResult::Error(_) = shapes_val {
        return shapes_val;
      }

      let mut shapes: Vec<GeoNode> = Vec::new();

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
    
      let cutter_geo_tree_root = GeoNode::intersection_3d(shapes);
  
      cut_atomic_structure(&mut atomic_structure, &cutter_geo_tree_root, self.cut_sdf_value, self.unit_cell_size);
  
      return NetworkResult::Atomic(atomic_structure);
    }
    return NetworkResult::Atomic(AtomicStructure::new());
  }

  fn clone_box(&self) -> Box<dyn NodeData> {
      Box::new(self.clone())
  }

  fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
      None
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
  for (atom_id, atom) in atomic_structure.iter_atoms() {
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
    atomic_structure.delete_atom(atom_id);
  }
}

pub fn get_node_type() -> NodeType {
  NodeType {
      name: "atom_cut".to_string(),
      description: "Cuts an atomic structure using cutter geometries.".to_string(),
      category: NodeTypeCategory::AtomicStructure,
      parameters: vec![
          Parameter {
              name: "molecule".to_string(),
              data_type: DataType::Atomic,
          },
          Parameter {
            name: "cutters".to_string(),
            data_type: DataType::Array(Box::new(DataType::Geometry)),
        },
      ],
      output_type: DataType::Atomic,
      public: true,
      node_data_creator: || Box::new(AtomCutData::new()),
      node_data_saver: generic_node_data_saver::<AtomCutData>,
      node_data_loader: generic_node_data_loader::<AtomCutData>,
    }
}