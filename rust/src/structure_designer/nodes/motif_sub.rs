use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::motif_parser::parse_parameter_element_values;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network::ValidationError;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_saver,
};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io;

#[derive(Debug, Clone, Serialize)]
pub struct MotifSubData {
    pub parameter_element_value_definition: String,
    #[serde(skip)]
    pub error: Option<String>,
    #[serde(skip)]
    pub parameter_element_values: HashMap<String, i16>,
    /// Cached motif parameter list from last evaluation (name, default_atomic_number)
    #[serde(skip)]
    pub available_parameters: RefCell<Vec<(String, i16)>>,
}

#[derive(Debug, Clone, Deserialize)]
struct MotifSubDataDeserialized {
    pub parameter_element_value_definition: String,
}

impl<'de> Deserialize<'de> for MotifSubData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let de = MotifSubDataDeserialized::deserialize(deserializer)?;

        let mut data = MotifSubData {
            parameter_element_value_definition: de.parameter_element_value_definition,
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

impl MotifSubData {
    pub fn parse_and_validate(&mut self, node_id: u64) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        self.parameter_element_values.clear();
        self.error = None;

        if self.parameter_element_value_definition.trim().is_empty() {
            return errors;
        }

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

impl NodeData for MotifSubData {
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
        // Evaluate motif input (required)
        let motif_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
        if let NetworkResult::Error(_) = motif_val {
            return EvalOutput::single(motif_val);
        }

        let motif = match motif_val {
            NetworkResult::Motif(m) => m,
            _ => {
                return EvalOutput::single(NetworkResult::Error(
                    "Expected Motif input".to_string(),
                ));
            }
        };

        // Cache motif parameters for UI display
        *self.available_parameters.borrow_mut() = motif
            .parameters
            .iter()
            .map(|p| (p.name.clone(), p.default_atomic_number))
            .collect();

        // If no overrides, pass through unchanged
        if self.parameter_element_values.is_empty() {
            return EvalOutput::single(NetworkResult::Motif(motif));
        }

        // Apply real element substitution: for each overridden parameter,
        // replace all site references with the concrete atomic number and
        // remove the parameter. Remaining parameters get re-indexed.
        let mut modified_motif = motif.clone();

        // Build map: old parameter index -> concrete atomic number (for substituted params)
        // Parameters are referenced by sites as -(index+1), so index 0 = -1, index 1 = -2, etc.
        let mut substituted: HashMap<usize, i16> = HashMap::new();
        for (i, param) in motif.parameters.iter().enumerate() {
            if let Some(&override_z) = self.parameter_element_values.get(&param.name) {
                substituted.insert(i, override_z);
            }
        }

        // Build re-index map: old_param_index -> new_param_index for surviving parameters
        let mut reindex: HashMap<usize, usize> = HashMap::new();
        let mut new_idx = 0usize;
        for i in 0..motif.parameters.len() {
            if !substituted.contains_key(&i) {
                reindex.insert(i, new_idx);
                new_idx += 1;
            }
        }

        // Update all sites
        for site in &mut modified_motif.sites {
            if site.atomic_number < 0 {
                let old_param_idx = (-site.atomic_number - 1) as usize;
                if let Some(&concrete_z) = substituted.get(&old_param_idx) {
                    // Replace with concrete element
                    site.atomic_number = concrete_z;
                } else if let Some(&new_param_idx) = reindex.get(&old_param_idx) {
                    // Re-index to new parameter position
                    site.atomic_number = -(new_param_idx as i16 + 1);
                }
            }
        }

        // Remove substituted parameters (keep only non-substituted ones)
        modified_motif.parameters = motif
            .parameters
            .iter()
            .enumerate()
            .filter(|(i, _)| !substituted.contains_key(i))
            .map(|(_, p)| p.clone())
            .collect();

        EvalOutput::single(NetworkResult::Motif(modified_motif))
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
        vec![(
            "parameter_element_value_definition".to_string(),
            TextValue::String(self.parameter_element_value_definition.clone()),
        )]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("parameter_element_value_definition") {
            self.parameter_element_value_definition = v
                .as_string()
                .ok_or_else(|| "parameter_element_value_definition must be a string".to_string())?
                .to_string();
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
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("motif".to_string(), (true, None));
        m
    }
}

pub fn motif_sub_data_loader(
    value: &Value,
    _design_dir: Option<&str>,
) -> io::Result<Box<dyn NodeData>> {
    let mut data: MotifSubData = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let _validation_errors = data.parse_and_validate(0);
    Ok(Box::new(data))
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "motif_sub".to_string(),
        description: "Substitutes parameter element defaults in a motif. Takes a motif input and \
            outputs a new motif with the specified parameter elements replaced."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::OtherBuiltin,
        parameters: vec![Parameter {
            id: None,
            name: "motif".to_string(),
            data_type: DataType::Motif,
        }],
        output_pins: OutputPinDefinition::single(DataType::Motif),
        public: true,
        node_data_creator: || {
            Box::new(MotifSubData {
                parameter_element_value_definition: String::new(),
                error: None,
                parameter_element_values: HashMap::new(),
                available_parameters: RefCell::new(Vec::new()),
            })
        },
        node_data_saver: generic_node_data_saver::<MotifSubData>,
        node_data_loader: motif_sub_data_loader,
    }
}
