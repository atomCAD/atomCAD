// Node wrapper for lattice filling algorithm
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::crystolecule::atomic_structure::AtomicStructure;
use std::collections::HashMap;
use glam::f64::DVec3;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::common_constants::{REAL_IMPLICIT_VOLUME_MIN, REAL_IMPLICIT_VOLUME_MAX};
use crate::crystolecule::crystolecule_constants::DEFAULT_ZINCBLENDE_MOTIF;
use crate::crystolecule::motif_parser::parse_parameter_element_values;
use crate::structure_designer::node_network::ValidationError;
use crate::util::serialization_utils::dvec3_serializer;
use crate::util::daabox::DAABox;

// Import the lattice fill algorithm
use crate::crystolecule::lattice_fill::{fill_lattice, LatticeFillConfig, LatticeFillOptions};



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomFillData {
  pub parameter_element_value_definition: String,
  #[serde(with = "dvec3_serializer")]
  pub motif_offset: DVec3,
  pub hydrogen_passivation: bool,
  #[serde(default)]
  pub remove_single_bond_atoms_before_passivation: bool,
  #[serde(default)]
  pub surface_reconstruction: bool,
  #[serde(default)]
  pub invert_phase: bool,
  #[serde(skip)]
  pub error: Option<String>,
  #[serde(skip)]
  pub parameter_element_values: HashMap<String, i16>,
}

impl AtomFillData {
  /// Parses and validates the parameter element definition and returns any validation errors
  pub fn parse_and_validate(&mut self, node_id: u64) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    
    // Clear previous state
    self.parameter_element_values.clear();
    self.error = None;
    
    // Skip validation if definition is empty
    if self.parameter_element_value_definition.trim().is_empty() {
      return errors;
    }
    
    // Parse the parameter element value definition
    match parse_parameter_element_values(&self.parameter_element_value_definition) {
      Ok(values) => {
        self.parameter_element_values = values;
      },
      Err(parse_error) => {
        let error_msg = format!("Parameter element parse error: {}", parse_error);
        self.error = Some(error_msg.clone());
        errors.push(ValidationError::new(error_msg, Some(node_id)));
      }
    }
    
    errors
  }
}

impl NodeData for AtomFillData {
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
      context: &mut NetworkEvaluationContext
    ) -> NetworkResult {
      // Evaluate geometry input
      let shape_val = network_evaluator.evaluate_arg_required(&network_stack, node_id, registry, context, 0);
      if let NetworkResult::Error(_) = shape_val {
        return shape_val;
      }

      let mesh = match shape_val {
        NetworkResult::Geometry(mesh) => mesh,
        _ => return NetworkResult::Atomic(AtomicStructure::new()),
      };
      
      // Evaluate motif input (with default)
      let motif = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 1,
        DEFAULT_ZINCBLENDE_MOTIF.clone(),
        NetworkResult::extract_motif
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      // Evaluate m_offset input (with default)
      let motif_offset = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 2,
        self.motif_offset,
        NetworkResult::extract_vec3
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      // Evaluate passivate input (with default)
      let hydrogen_passivation = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 3,
        self.hydrogen_passivation,
        NetworkResult::extract_bool
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      // Evaluate rm_single input (with default)
      let remove_single_bond_atoms = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 4,
        self.remove_single_bond_atoms_before_passivation,
        NetworkResult::extract_bool
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      // Evaluate surf_recon input (with default)
      let surface_reconstruction = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 5,
        self.surface_reconstruction,
        NetworkResult::extract_bool
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      let invert_phase = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 6,
        self.invert_phase,
        NetworkResult::extract_bool
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      // Calculate effective parameter element values (fill in defaults for missing values)
      let effective_parameter_values = motif.get_effective_parameter_element_values(&self.parameter_element_values);

      // Build configuration
      let config = LatticeFillConfig {
        unit_cell: mesh.unit_cell,
        motif,
        parameter_element_values: effective_parameter_values,
        geometry: mesh.geo_tree_root,
        motif_offset,
      };

      let options = LatticeFillOptions {
        hydrogen_passivation,
        remove_single_bond_atoms,
        reconstruct_surface: surface_reconstruction,
        invert_phase,
      };

      // Define fill region
      let fill_region = DAABox::from_start_and_size(
        REAL_IMPLICIT_VOLUME_MIN,
        REAL_IMPLICIT_VOLUME_MAX - REAL_IMPLICIT_VOLUME_MIN
      );

      // Call the lattice fill algorithm
      let result = fill_lattice(&config, &options, &fill_region);

      NetworkResult::Atomic(result.atomic_structure)
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        None
    }
}

// All implementation logic has moved to crystolecule::lattice_fill::fill_algorithm
