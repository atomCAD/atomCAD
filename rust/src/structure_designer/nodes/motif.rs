use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::crystolecule::motif::Motif;
use crate::crystolecule::motif_parser::parse_motif;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::node_network::ValidationError;
use crate::structure_designer::node_type::{NodeType, generic_node_data_saver};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use serde_json::Value;
use std::io;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotifData {
    pub definition: String,
    pub name: Option<String>,
    #[serde(skip)]
    pub error: Option<String>,
    #[serde(skip)]
    pub motif: Option<Motif>,
}

impl MotifData {
    /// Parses and validates the motif definition and returns any validation errors
    pub fn parse_and_validate(&mut self, node_id: u64) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        
        // Clear previous state
        self.motif = None;
        self.error = None;
        
        // Skip validation if definition is empty
        if self.definition.trim().is_empty() {
            return errors;
        }
        
        // Parse the motif definition
        match parse_motif(&self.definition) {
            Ok(motif) => {
                self.motif = Some(motif);
            },
            Err(parse_error) => {
                let error_msg = format!("Motif parse error: {}", parse_error);
                self.error = Some(error_msg.clone());
                errors.push(ValidationError::new(error_msg, Some(node_id)));
            }
        }
        
        errors
    }
}

impl NodeData for MotifData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        _network_evaluator: &NetworkEvaluator,
        _network_stack: &Vec<NetworkStackElement<'a>>,
        _node_id: u64,
        _registry: &NodeTypeRegistry,
        _decorate: bool,
        _context: &mut NetworkEvaluationContext,
    ) -> NetworkResult {
        // Return the parsed motif if available
        if let Some(ref motif) = self.motif {
            NetworkResult::Motif(motif.clone())
        } else if let Some(ref error) = self.error {
            NetworkResult::Error(error.clone())
        } else {
            NetworkResult::Error("Motif not parsed".to_string())
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        self.name.clone()
    }
}

/// Special loader for MotifData that parses and validates the motif after deserializing
pub fn motif_data_loader(value: &Value, _design_dir: Option<&str>) -> io::Result<Box<dyn NodeData>> {
    // First deserialize the basic data
    let mut data: MotifData = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    
    // Use the existing parse_and_validate method to handle motif parsing and validation
    // We pass a dummy node_id (0) since validation errors aren't used in the loader context
    let _validation_errors = data.parse_and_validate(0);
    
    Ok(Box::new(data))
}

pub fn get_node_type() -> NodeType {
    NodeType {
      name: "motif".to_string(),
      description: "The `motif` node produces a `Motif` value which can be an input to an `atom_fill` node and determines the content which fills the provided geometry.
The motif is defined textually using atomCAD's motif definition language.
The features of the language are basically parameterized fractional atom sites, explicit & periodic bond definitions.
See the atomCAD reference guide for details on the motif definition language.".to_string(),
      category: NodeTypeCategory::OtherBuiltin,
      parameters: vec![],
      output_type: DataType::Motif,
      public: true,
      node_data_creator: || Box::new(MotifData {
        definition: "".to_string(),
        name: None,
        motif: None,
        error: None,
      }),
      node_data_saver: generic_node_data_saver::<MotifData>,
      node_data_loader: motif_data_loader,
    }
}









