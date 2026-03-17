use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::motif::Motif;
use crate::crystolecule::motif_parser::parse_motif;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network::ValidationError;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{NodeType, generic_node_data_saver};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
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
            }
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
        _network_evaluator: &NetworkEvaluator,
        _network_stack: &[NetworkStackElement<'a>],
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

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        self.name.clone()
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        let mut props = vec![(
            "definition".to_string(),
            TextValue::String(self.definition.clone()),
        )];
        if let Some(ref name) = self.name {
            props.push(("name".to_string(), TextValue::String(name.clone())));
        }
        props
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("definition") {
            self.definition = v
                .as_string()
                .ok_or_else(|| "definition must be a string".to_string())?
                .to_string();
        }
        if let Some(v) = props.get("name") {
            self.name = Some(
                v.as_string()
                    .ok_or_else(|| "name must be a string".to_string())?
                    .to_string(),
            );
        }
        // Parse and validate motif after properties are set
        // (matches what motif_data_loader does after deserializing)
        let _validation_errors = self.parse_and_validate(0);
        Ok(())
    }
}

/// Special loader for MotifData that parses and validates the motif after deserializing
pub fn motif_data_loader(
    value: &Value,
    _design_dir: Option<&str>,
) -> io::Result<Box<dyn NodeData>> {
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
      description: "Produces a `Motif` value for use with `atom_fill` to populate geometry with atoms.

## Motif Definition Language

Three commands define a motif:

**PARAM** - Define parameter elements (can be overridden in atom_fill):
```
PARAM PRIMARY C
PARAM SECONDARY C
```

**SITE** - Define atomic sites with fractional coordinates (0-1):
```
SITE <id> <element> <frac_x> <frac_y> <frac_z>
SITE CORNER PRIMARY 0 0 0
SITE FACE_X PRIMARY 0 0.5 0.5
SITE INTERIOR1 SECONDARY 0.25 0.25 0.25
```

**BOND** - Define bonds between sites:
```
BOND <site1> <relative_cell_prefix><site2>
```
The prefix is 3 characters for (x,y,z) directions: `.` = same cell, `+` = next cell, `-` = previous cell.
First site must be in current cell (prefix `...` or omitted).

Examples:
```
BOND INTERIOR1 ...CORNER      # same cell
BOND INTERIOR2 .++CORNER      # y+1, z+1 cell
BOND INTERIOR3 +..FACE_X      # x+1 cell
```

Lines starting with `#` are comments.".to_string(),
      summary: None,
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
