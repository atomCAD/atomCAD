use crate::data_type::DataType;
use crate::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::evaluator::network_evaluator::NetworkEvaluator;
use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::evaluator::network_result::NetworkResult;
use crate::node_data::{EvalOutput, NodeData, NodeDataError};
use crate::node_network::ValidationError;
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{NodeType, OutputPinDefinition, generic_node_data_saver};
use crate::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::StructureDesigner;
use crate::text_format::TextValue;
use atomcad_crystolecule::crystolecule_constants::DEFAULT_ZINCBLENDE_MOTIF_TEXT;
use atomcad_crystolecule::motif::Motif;
use atomcad_crystolecule::motif_parser::parse_motif;
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
    ) -> EvalOutput {
        // Return the parsed motif if available
        if let Some(ref motif) = self.motif {
            EvalOutput::single(NetworkResult::Motif(motif.clone()))
        } else if let Some(ref error) = self.error {
            EvalOutput::single(NetworkResult::Error(error.clone()))
        } else {
            EvalOutput::single(NetworkResult::Error("Motif not parsed".to_string()))
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

    /// Blocking: with an unparsed definition there is no motif to emit — `eval`
    /// returns this very text as its `NetworkResult::Error`. Reporting it here
    /// puts the failure in the unified error list (`doc/design_error_management.md`
    /// D9) and cone-poisons the node, which makes D8's dedupe drop the
    /// now-redundant eval entry so the badge shows the sentence exactly once.
    fn get_data_error(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<NodeDataError> {
        self.error.clone().map(NodeDataError::blocking)
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        let mut props = vec![(
            "definition".to_string(),
            TextValue::String(self.definition.clone()),
        )];
        // Always written, `""` for `None`: a fresh node defaults to a *named*
        // motif, so an omitted `name` would read back as "cubic zincblende"
        // and a `--replace` of a nameless motif would not round-trip.
        props.push((
            "name".to_string(),
            TextValue::String(self.name.clone().unwrap_or_default()),
        ));
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
            let name = v
                .as_string()
                .ok_or_else(|| "name must be a string".to_string())?;
            self.name = (!name.is_empty()).then(|| name.to_string());
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
      description: "Produces a `Motif` value: the atom sites and bonds repeated in every unit cell. Wire it into a `structure` node, whose `Structure` is then carved into atoms by `materialize`.

## Motif Definition Language

Three commands define a motif (commands are case-insensitive):

**PARAM** - Define a parameter element, with an optional default element (C if omitted). Parameter elements can be overridden downstream by `materialize` or `motif_sub`:
```
PARAM <name> [<default_element>]
PARAM PRIMARY C
PARAM SECONDARY C
```

**SITE** - Define an atomic site with fractional coordinates. The element is a chemical element or a declared parameter:
```
SITE <id> <element> <frac_x> <frac_y> <frac_z>
SITE CORNER PRIMARY 0 0 0
SITE FACE_X PRIMARY 0 0.5 0.5
SITE INTERIOR1 SECONDARY 0.25 0.25 0.25
SITE INTERIOR2 SECONDARY 0.25 0.75 0.75
```

**BOND** - Define a bond between two sites, with an optional positive multiplicity (default 1):
```
BOND <site1> [<relative_cell_prefix>]<site2> [<multiplicity>]
```
The prefix is 3 characters for (x,y,z) directions: `.` = same cell, `+` = next cell, `-` = previous cell; omitting it means `...`.
The first site must be in the current cell (prefix `...` or omitted).

Examples:
```
# same cell
BOND INTERIOR1 ...CORNER
# second site in the y+1, z+1 cell
BOND INTERIOR2 .++CORNER
# second site in the x+1 cell
BOND INTERIOR1 +..FACE_X
```

Lines starting with `#` are comments. A comment must be on its own line: text after a command on the same line is read as extra arguments.".to_string(),
      summary: None,
      category: NodeTypeCategory::OtherBuiltin,
      parameters: vec![],
      output_pins: OutputPinDefinition::single(DataType::Motif),
      zone_input_pins: vec![],
      zone_output_pins: vec![],
      public: true,
      node_data_creator: || {
        let definition = DEFAULT_ZINCBLENDE_MOTIF_TEXT.trim().to_string();
        let motif = parse_motif(&definition).ok();
        Box::new(MotifData {
          definition,
          name: Some("cubic zincblende".to_string()),
          motif,
          error: None,
        })
      },
      node_data_saver: generic_node_data_saver::<MotifData>,
      node_data_loader: motif_data_loader,
    }
}
