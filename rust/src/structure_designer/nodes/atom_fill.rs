// Node wrapper for lattice filling algorithm
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use serde::de::Deserializer;
use crate::structure_designer::text_format::TextValue;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::crystolecule::atomic_structure::AtomicStructure;
use std::collections::HashMap;
use glam::f64::DVec3;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::common_constants::{REAL_IMPLICIT_VOLUME_MIN, REAL_IMPLICIT_VOLUME_MAX};
use crate::crystolecule::crystolecule_constants::DEFAULT_ZINCBLENDE_MOTIF;
use crate::crystolecule::motif_parser::parse_parameter_element_values;
use crate::structure_designer::node_network::ValidationError;
use crate::util::serialization_utils::dvec3_serializer;
use crate::util::daabox::DAABox;

// Import the lattice fill algorithm
use crate::crystolecule::lattice_fill::{fill_lattice, LatticeFillConfig, LatticeFillOptions};



#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Deserialize)]
struct AtomFillDataDeserialized {
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
}

impl<'de> Deserialize<'de> for AtomFillData {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let de = AtomFillDataDeserialized::deserialize(deserializer)?;

    let mut data = AtomFillData {
      parameter_element_value_definition: de.parameter_element_value_definition,
      motif_offset: de.motif_offset,
      hydrogen_passivation: de.hydrogen_passivation,
      remove_single_bond_atoms_before_passivation: de.remove_single_bond_atoms_before_passivation,
      surface_reconstruction: de.surface_reconstruction,
      invert_phase: de.invert_phase,
      error: None,
      parameter_element_values: HashMap::new(),
    };

    if !data.parameter_element_value_definition.trim().is_empty() {
      match parse_parameter_element_values(&data.parameter_element_value_definition) {
        Ok(values) => {
          data.parameter_element_values = values;
        }
        Err(parse_error) => {
          data.error = Some(format!("Parameter element parse error: {}", parse_error));
        }
      }
    }

    Ok(data)
  }
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

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            // Note: parameter_element_value_definition has no matching parameter (stored-only field)
            ("parameter_element_value_definition".to_string(), TextValue::String(self.parameter_element_value_definition.clone())),
            // Property names match parameter names for connection shadowing
            ("m_offset".to_string(), TextValue::Vec3(self.motif_offset)),
            ("passivate".to_string(), TextValue::Bool(self.hydrogen_passivation)),
            ("rm_single".to_string(), TextValue::Bool(self.remove_single_bond_atoms_before_passivation)),
            ("surf_recon".to_string(), TextValue::Bool(self.surface_reconstruction)),
            ("invert_phase".to_string(), TextValue::Bool(self.invert_phase)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("parameter_element_value_definition") {
            self.parameter_element_value_definition = v.as_string().ok_or_else(|| "parameter_element_value_definition must be a string".to_string())?.to_string();
        }
        if let Some(v) = props.get("m_offset") {
            self.motif_offset = v.as_vec3().ok_or_else(|| "m_offset must be a Vec3".to_string())?;
        }
        if let Some(v) = props.get("passivate") {
            self.hydrogen_passivation = v.as_bool().ok_or_else(|| "passivate must be a boolean".to_string())?;
        }
        if let Some(v) = props.get("rm_single") {
            self.remove_single_bond_atoms_before_passivation = v.as_bool().ok_or_else(|| "rm_single must be a boolean".to_string())?;
        }
        if let Some(v) = props.get("surf_recon") {
            self.surface_reconstruction = v.as_bool().ok_or_else(|| "surf_recon must be a boolean".to_string())?;
        }
        if let Some(v) = props.get("invert_phase") {
            self.invert_phase = v.as_bool().ok_or_else(|| "invert_phase must be a boolean".to_string())?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("shape".to_string(), (true, None)); // required
        m.insert("motif".to_string(), (false, Some("cubic zincblende".to_string())));
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
      name: "atom_fill".to_string(),
      description: "Converts a 3D geometry into an atomic structure by carving out a crystal from an infinite crystal lattice using the geometry on its `shape` input.".to_string(),
      summary: None,
      category: NodeTypeCategory::AtomicStructure,
      parameters: vec![
          Parameter {
              id: None,
              name: "shape".to_string(),
              data_type: DataType::Geometry,
          },
          Parameter {
              id: None,
              name: "motif".to_string(),
              data_type: DataType::Motif,
          },
          Parameter {
              id: None,
              name: "m_offset".to_string(),
              data_type: DataType::Vec3,
          },
          Parameter {
              id: None,
              name: "passivate".to_string(),
              data_type: DataType::Bool,
          },
          Parameter {
              id: None,
              name: "rm_single".to_string(),
              data_type: DataType::Bool,
          },
          Parameter {
              id: None,
              name: "surf_recon".to_string(),
              data_type: DataType::Bool,
          },
          Parameter {
              id: None,
              name: "invert_phase".to_string(),
              data_type: DataType::Bool,
          },
      ],
      output_type: DataType::Atomic,
      public: true,
      node_data_creator: || Box::new(AtomFillData {
        parameter_element_value_definition: String::new(),
        motif_offset: DVec3::ZERO,
        hydrogen_passivation: true,
        remove_single_bond_atoms_before_passivation: false,
        surface_reconstruction: false,
        invert_phase: false,
        error: None,
        parameter_element_values: HashMap::new(),
      }),
      node_data_saver: generic_node_data_saver::<AtomFillData>,
      node_data_loader: generic_node_data_loader::<AtomFillData>,
    }
}
