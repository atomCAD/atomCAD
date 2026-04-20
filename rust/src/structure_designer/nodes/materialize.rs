// Materialize: carves atoms out of a Blueprint's structure using the blueprint's geometry.
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::motif_parser::parse_parameter_element_values;
use crate::structure_designer::common_constants::{
    REAL_IMPLICIT_VOLUME_MAX, REAL_IMPLICIT_VOLUME_MIN,
};
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::{CrystalData, NetworkResult};
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network::ValidationError;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::util::daabox::DAABox;
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;

// Import the lattice fill algorithm
use crate::crystolecule::lattice_fill::{LatticeFillConfig, LatticeFillOptions, fill_lattice};

#[derive(Debug, Clone, Serialize)]
pub struct MaterializeData {
    pub parameter_element_value_definition: String,
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
    /// Cached motif parameter list from last evaluation (name, default_atomic_number)
    #[serde(skip)]
    pub available_parameters: RefCell<Vec<(String, i16)>>,
}

#[derive(Debug, Clone, Deserialize)]
struct MaterializeDataDeserialized {
    pub parameter_element_value_definition: String,
    pub hydrogen_passivation: bool,
    #[serde(default)]
    pub remove_single_bond_atoms_before_passivation: bool,
    #[serde(default)]
    pub surface_reconstruction: bool,
    #[serde(default)]
    pub invert_phase: bool,
}

impl<'de> Deserialize<'de> for MaterializeData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let de = MaterializeDataDeserialized::deserialize(deserializer)?;

        let mut data = MaterializeData {
            parameter_element_value_definition: de.parameter_element_value_definition,
            hydrogen_passivation: de.hydrogen_passivation,
            remove_single_bond_atoms_before_passivation: de
                .remove_single_bond_atoms_before_passivation,
            surface_reconstruction: de.surface_reconstruction,
            invert_phase: de.invert_phase,
            error: None,
            parameter_element_values: HashMap::new(),
            available_parameters: RefCell::new(Vec::new()),
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

impl MaterializeData {
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
            }
            Err(parse_error) => {
                let error_msg = format!("Parameter element parse error: {}", parse_error);
                self.error = Some(error_msg.clone());
                errors.push(ValidationError::new(error_msg, Some(node_id)));
            }
        }

        errors
    }
}

impl NodeData for MaterializeData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        network_evaluator: &NetworkEvaluator,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        _decorate: bool,
        context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        // Evaluate geometry input
        let shape_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        if let NetworkResult::Error(_) = shape_val {
            return EvalOutput::single(shape_val);
        }

        let mesh = match shape_val {
            NetworkResult::Blueprint(mesh) => mesh,
            _ => {
                return EvalOutput::single(NetworkResult::Error(
                    "materialize: shape input must be a Blueprint".to_string(),
                ));
            }
        };
        let structure = mesh.structure.clone();
        let geo_tree_root_for_crystal = mesh.geo_tree_root.clone();
        let alignment = mesh.alignment;

        // Motif and motif offset now come from the Blueprint's structure.
        let motif = mesh.structure.motif.clone();
        let motif_offset = mesh.structure.motif_offset;

        // Cache motif parameters for UI display
        *self.available_parameters.borrow_mut() = motif
            .parameters
            .iter()
            .map(|p| (p.name.clone(), p.default_atomic_number))
            .collect();

        // Evaluate passivate input (with default)
        let hydrogen_passivation = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            1,
            self.hydrogen_passivation,
            NetworkResult::extract_bool,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        // Evaluate rm_single input (with default)
        let remove_single_bond_atoms = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            2,
            self.remove_single_bond_atoms_before_passivation,
            NetworkResult::extract_bool,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        // Evaluate surf_recon input (with default)
        let surface_reconstruction = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            3,
            self.surface_reconstruction,
            NetworkResult::extract_bool,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        let invert_phase = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            4,
            self.invert_phase,
            NetworkResult::extract_bool,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        // Calculate effective parameter element values (fill in defaults for missing values)
        let effective_parameter_values =
            motif.get_effective_parameter_element_values(&self.parameter_element_values);

        // Build configuration
        let config = LatticeFillConfig {
            unit_cell: mesh.structure.lattice_vecs,
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
            REAL_IMPLICIT_VOLUME_MAX - REAL_IMPLICIT_VOLUME_MIN,
        );

        // Call the lattice fill algorithm
        let result = fill_lattice(&config, &options, &fill_region);

        EvalOutput::single(NetworkResult::Crystal(CrystalData {
            structure,
            atoms: result.atomic_structure,
            geo_tree_root: Some(geo_tree_root_for_crystal),
            alignment,
        }))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        None
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            // Note: parameter_element_value_definition has no matching parameter (stored-only field)
            (
                "parameter_element_value_definition".to_string(),
                TextValue::String(self.parameter_element_value_definition.clone()),
            ),
            (
                "passivate".to_string(),
                TextValue::Bool(self.hydrogen_passivation),
            ),
            (
                "rm_single".to_string(),
                TextValue::Bool(self.remove_single_bond_atoms_before_passivation),
            ),
            (
                "surf_recon".to_string(),
                TextValue::Bool(self.surface_reconstruction),
            ),
            (
                "invert_phase".to_string(),
                TextValue::Bool(self.invert_phase),
            ),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("parameter_element_value_definition") {
            self.parameter_element_value_definition = v
                .as_string()
                .ok_or_else(|| "parameter_element_value_definition must be a string".to_string())?
                .to_string();
            // Parse the definition into the HashMap used by eval()
            self.parameter_element_values.clear();
            if !self.parameter_element_value_definition.trim().is_empty() {
                match parse_parameter_element_values(&self.parameter_element_value_definition) {
                    Ok(values) => {
                        self.parameter_element_values = values;
                    }
                    Err(parse_error) => {
                        self.error =
                            Some(format!("Parameter element parse error: {}", parse_error));
                    }
                }
            }
        }
        if let Some(v) = props.get("passivate") {
            self.hydrogen_passivation = v
                .as_bool()
                .ok_or_else(|| "passivate must be a boolean".to_string())?;
        }
        if let Some(v) = props.get("rm_single") {
            self.remove_single_bond_atoms_before_passivation = v
                .as_bool()
                .ok_or_else(|| "rm_single must be a boolean".to_string())?;
        }
        if let Some(v) = props.get("surf_recon") {
            self.surface_reconstruction = v
                .as_bool()
                .ok_or_else(|| "surf_recon must be a boolean".to_string())?;
        }
        if let Some(v) = props.get("invert_phase") {
            self.invert_phase = v
                .as_bool()
                .ok_or_else(|| "invert_phase must be a boolean".to_string())?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("shape".to_string(), (true, None)); // required
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
      name: "materialize".to_string(),
      description: "Converts a Blueprint into a Crystal by carving atoms out of the Blueprint's structure using its geometry.".to_string(),
      summary: None,
      category: NodeTypeCategory::AtomicStructure,
      parameters: vec![
          Parameter {
              id: None,
              name: "shape".to_string(),
              data_type: DataType::Blueprint,
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
      output_pins: OutputPinDefinition::single_fixed(DataType::Crystal),
      public: true,
      node_data_creator: || Box::new(MaterializeData {
        parameter_element_value_definition: String::new(),
        hydrogen_passivation: true,
        remove_single_bond_atoms_before_passivation: false,
        surface_reconstruction: false,
        invert_phase: false,
        error: None,
        parameter_element_values: HashMap::new(),
        available_parameters: RefCell::new(Vec::new()),
      }),
      node_data_saver: generic_node_data_saver::<MaterializeData>,
      node_data_loader: generic_node_data_loader::<MaterializeData>,
    }
}
