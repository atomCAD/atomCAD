use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::data_type::FunctionType;
use crate::structure_designer::node_data::NodeData;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io;

#[derive(Clone, PartialEq)]
pub struct Parameter {
    pub id: Option<u64>, // Persistent identifier for wire preservation across renames
    pub name: String,
    pub data_type: DataType,
}

/// Definition of an output pin on a node type.
#[derive(Clone, Debug, PartialEq)]
pub struct OutputPinDefinition {
    pub name: String,
    pub data_type: DataType,
}

impl OutputPinDefinition {
    /// Convenience constructor for single-output nodes.
    pub fn single(data_type: DataType) -> Vec<OutputPinDefinition> {
        vec![OutputPinDefinition {
            name: "result".to_string(),
            data_type,
        }]
    }
}

// A built-in or user defined node type.
#[derive(Clone)]
pub struct NodeType {
    pub name: String,
    pub description: String,
    /// Optional short summary for CLI verbose listings. If provided, this is
    /// displayed instead of truncating the description.
    pub summary: Option<String>,
    pub category: NodeTypeCategory,
    pub parameters: Vec<Parameter>,
    pub output_pins: Vec<OutputPinDefinition>,
    pub public: bool, // whether this node type is available for users to add
    pub node_data_creator: fn() -> Box<dyn NodeData>,
    #[allow(clippy::type_complexity)]
    pub node_data_saver: fn(&mut dyn NodeData, Option<&str>) -> io::Result<Value>,
    #[allow(clippy::type_complexity)]
    pub node_data_loader: fn(&Value, Option<&str>) -> io::Result<Box<dyn NodeData>>,
}

impl NodeType {
    /// The primary output type (pin 0). Panics if no output pins.
    pub fn output_type(&self) -> &DataType {
        &self.output_pins[0].data_type
    }

    pub fn get_function_type(&self) -> DataType {
        DataType::Function(FunctionType {
            parameter_types: self
                .parameters
                .iter()
                .map(|p| p.data_type.clone())
                .collect(),
            output_type: Box::new(self.output_type().clone()),
        })
    }

    pub fn get_output_pin_type(&self, output_pin_index: i32) -> DataType {
        if output_pin_index == -1 {
            self.get_function_type()
        } else {
            self.output_pins
                .get(output_pin_index as usize)
                .map(|p| p.data_type.clone())
                .unwrap_or(DataType::None)
        }
    }

    /// Number of result output pins (excludes function pin).
    pub fn output_pin_count(&self) -> usize {
        self.output_pins.len()
    }

    /// Whether this node type has multiple output pins.
    pub fn has_multi_output(&self) -> bool {
        self.output_pins.len() > 1
    }
}

/// Generic saver function for node data types that implement Serialize
pub fn generic_node_data_saver<T: NodeData + Serialize + 'static>(
    node_data: &mut dyn NodeData,
    _design_dir: Option<&str>,
) -> io::Result<Value> {
    if let Some(typed_data) = node_data.as_any_mut().downcast_ref::<T>() {
        serde_json::to_value(typed_data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Data type mismatch",
        ))
    }
}

/// Generic loader function for node data types that implement Deserialize
pub fn generic_node_data_loader<T: NodeData + for<'de> Deserialize<'de> + 'static>(
    value: &Value,
    _design_dir: Option<&str>,
) -> io::Result<Box<dyn NodeData>> {
    let data: T = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Box::new(data))
}

/// Saver function for NoData types (returns empty JSON object)
pub fn no_data_saver(
    _node_data: &mut dyn NodeData,
    _design_dir: Option<&str>,
) -> io::Result<Value> {
    Ok(serde_json::json!({}))
}

/// Loader function for NoData types (returns NoData instance)
pub fn no_data_loader(_value: &Value, _design_dir: Option<&str>) -> io::Result<Box<dyn NodeData>> {
    Ok(Box::new(crate::structure_designer::node_data::NoData {}))
}
