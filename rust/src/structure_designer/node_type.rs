use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::data_type::FunctionType;
use crate::structure_designer::node_data::NodeData;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::io;

#[derive(Clone, PartialEq)]
pub struct Parameter {
    pub id: Option<u64>, // Persistent identifier for wire preservation across renames
    pub name: String,
    pub data_type: DataType,
}

/// Specification for how an output pin's concrete data type is determined.
///
/// `Fixed` declares a static type. `SameAsInput` and `SameAsArrayElements` describe
/// polymorphic pins whose concrete type is derived from an input pin at validation
/// time (see `NodeTypeRegistry::resolve_output_type`).
///
/// `SameAsInput` may declare a `fallback_if_disconnected` concrete type that the
/// resolver returns when the named input pin has zero connections. This is used
/// by nodes whose evaluation produces meaningful intrinsic content with no input
/// (e.g. `atom_edit` whose diff is itself a `Molecule`); without a fallback the
/// pin remains unresolved when the input is disconnected, which is the right
/// behavior for pure transformations (`atom_union`, `structure_move`, etc.).
#[derive(Clone, Debug, PartialEq)]
pub enum PinOutputType {
    /// Fixed, statically declared output type.
    Fixed(DataType),
    /// Output type mirrors the resolved concrete type of the named input pin.
    /// When the input is disconnected, the optional `fallback_if_disconnected`
    /// is used; otherwise the pin remains unresolved.
    SameAsInput {
        input_pin_name: String,
        fallback_if_disconnected: Option<DataType>,
    },
    /// Output type mirrors the element type of the named `Array[..]` input pin.
    SameAsArrayElements(String),
}

impl PinOutputType {
    /// Returns the declared `DataType` when this pin is `Fixed`; `None` otherwise.
    pub fn fixed_type(&self) -> Option<&DataType> {
        match self {
            PinOutputType::Fixed(t) => Some(t),
            _ => None,
        }
    }
}

impl fmt::Display for PinOutputType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PinOutputType::Fixed(t) => write!(f, "{}", t),
            PinOutputType::SameAsInput {
                input_pin_name,
                fallback_if_disconnected,
            } => {
                if let Some(fallback) = fallback_if_disconnected {
                    write!(f, "SameAsInput({}, default={})", input_pin_name, fallback)
                } else {
                    write!(f, "SameAsInput({})", input_pin_name)
                }
            }
            PinOutputType::SameAsArrayElements(name) => {
                write!(f, "SameAsArrayElements({})", name)
            }
        }
    }
}

/// Stable `&'static DataType::None` used as a fallback return from `output_type()`
/// when a pin is polymorphic (not yet resolved against a concrete input). Callers
/// that need the resolved concrete type must use `NodeTypeRegistry::resolve_output_type`.
fn none_data_type_ref() -> &'static DataType {
    static NONE_TYPE: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
    NONE_TYPE.get_or_init(|| DataType::None)
}

/// Definition of an output pin on a node type.
#[derive(Clone, Debug, PartialEq)]
pub struct OutputPinDefinition {
    pub name: String,
    pub data_type: PinOutputType,
    /// Optional stable editing identity for the field this pin represents.
    /// Set only on `record_destructure` per-field output pins (the field's
    /// `FieldId`); `None` for every other output pin. Used by
    /// `repair_node_network` to remap output wires across a field reorder /
    /// rename / delete by identity rather than slot index. Invisible to the
    /// type system. See `doc/design_record_field_identity.md` §4.4 (R3).
    pub id: Option<u64>,
}

impl OutputPinDefinition {
    /// Output pin with a statically declared type.
    pub fn fixed(name: &str, data_type: DataType) -> Self {
        Self {
            name: name.to_string(),
            data_type: PinOutputType::Fixed(data_type),
            id: None,
        }
    }

    /// Output pin that mirrors the resolved concrete type of the named input pin.
    pub fn same_as_input(name: &str, input_pin_name: &str) -> Self {
        Self {
            name: name.to_string(),
            data_type: PinOutputType::SameAsInput {
                input_pin_name: input_pin_name.to_string(),
                fallback_if_disconnected: None,
            },
            id: None,
        }
    }

    /// Output pin that mirrors the named input pin's resolved concrete type
    /// when connected, and falls back to `fallback` when the input is
    /// disconnected. Use this for nodes whose evaluation produces meaningful
    /// intrinsic content with no input (e.g. `atom_edit` whose diff is itself
    /// a `Molecule`).
    pub fn same_as_input_or_default(name: &str, input_pin_name: &str, fallback: DataType) -> Self {
        Self {
            name: name.to_string(),
            data_type: PinOutputType::SameAsInput {
                input_pin_name: input_pin_name.to_string(),
                fallback_if_disconnected: Some(fallback),
            },
            id: None,
        }
    }

    /// Output pin that mirrors the element type of the named `Array[..]` input pin.
    pub fn same_as_array_elements(name: &str, input_pin_name: &str) -> Self {
        Self {
            name: name.to_string(),
            data_type: PinOutputType::SameAsArrayElements(input_pin_name.to_string()),
            id: None,
        }
    }

    /// Attach a stable field identity (`FieldId.0`) to this output pin. Used by
    /// the `record_destructure` pin builder so output wires can be remapped by
    /// identity across a field reorder/rename/delete.
    pub fn with_id(mut self, id: u64) -> Self {
        self.id = Some(id);
        self
    }

    /// Backward-compatible convenience for single-output nodes with a fixed type.
    pub fn single(data_type: DataType) -> Vec<OutputPinDefinition> {
        vec![OutputPinDefinition::fixed("result", data_type)]
    }

    /// Single-output node with a fixed type.
    pub fn single_fixed(data_type: DataType) -> Vec<OutputPinDefinition> {
        vec![OutputPinDefinition::fixed("result", data_type)]
    }

    /// Single-output node whose type mirrors the named input pin.
    pub fn single_same_as(input_pin_name: &str) -> Vec<OutputPinDefinition> {
        vec![OutputPinDefinition::same_as_input("result", input_pin_name)]
    }

    /// Single-output node whose type mirrors the named input pin, with a
    /// disconnected-input fallback. See `same_as_input_or_default`.
    pub fn single_same_as_or_default(
        input_pin_name: &str,
        fallback: DataType,
    ) -> Vec<OutputPinDefinition> {
        vec![OutputPinDefinition::same_as_input_or_default(
            "result",
            input_pin_name,
            fallback,
        )]
    }

    /// Single-output node whose type mirrors the element type of an `Array[..]` input pin.
    pub fn single_same_as_array_elements(input_pin_name: &str) -> Vec<OutputPinDefinition> {
        vec![OutputPinDefinition::same_as_array_elements(
            "result",
            input_pin_name,
        )]
    }

    /// Returns the declared fixed `DataType` when this pin is `Fixed`; `None` otherwise.
    pub fn fixed_type(&self) -> Option<&DataType> {
        self.data_type.fixed_type()
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
    /// Inside-facing left-edge pins on a zone-owning (HOF) node. Each entry
    /// declares an iteration value the body sees as a source. Empty for every
    /// non-HOF node type. Reuses `OutputPinDefinition` because zone-input
    /// pins, from the body's perspective, are sources that produce values.
    pub zone_input_pins: Vec<OutputPinDefinition>,
    /// Inside-facing right-edge pins on a zone-owning (HOF) node. Each entry
    /// declares a value the body must produce. Empty for every non-HOF node
    /// type. Reuses `Parameter` because zone-output pins, from the body's
    /// perspective, are destinations that consume values.
    pub zone_output_pins: Vec<Parameter>,
    pub public: bool, // whether this node type is available for users to add
    pub node_data_creator: fn() -> Box<dyn NodeData>,
    #[allow(clippy::type_complexity)]
    pub node_data_saver: fn(&mut dyn NodeData, Option<&str>) -> io::Result<Value>,
    #[allow(clippy::type_complexity)]
    pub node_data_loader: fn(&Value, Option<&str>) -> io::Result<Box<dyn NodeData>>,
}

impl NodeType {
    /// The primary output type (pin 0). Panics if no output pins.
    ///
    /// Returns the declared `Fixed` type when pin 0 is `Fixed`; otherwise returns
    /// a reference to a static `DataType::None` sentinel. Callers that require the
    /// resolved concrete type of a polymorphic pin must use
    /// `NodeTypeRegistry::resolve_output_type` with a node/network context.
    pub fn output_type(&self) -> &DataType {
        self.output_pins[0]
            .fixed_type()
            .unwrap_or_else(|| none_data_type_ref())
    }

    pub fn get_function_type(&self) -> DataType {
        DataType::Function(FunctionType::new(
            self.parameters
                .iter()
                .map(|p| p.data_type.clone())
                .collect(),
            self.output_type().clone(),
        ))
    }

    pub fn get_output_pin_type(&self, output_pin_index: i32) -> DataType {
        if output_pin_index == -1 {
            self.get_function_type()
        } else {
            self.output_pins
                .get(output_pin_index as usize)
                .map(|p| p.fixed_type().cloned().unwrap_or(DataType::None))
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

    /// True if this node type declares any zone pins — i.e. it's a higher-order
    /// function (HOF) node that owns an inline body. Phase 2 lands the fields
    /// but no built-in type populates them yet, so this returns `false` for
    /// every existing node type. The marker is used by validation invariants
    /// to keep `Node.zone` / `Node.zone_output_arguments` consistent with the
    /// node's declared type.
    pub fn has_zone(&self) -> bool {
        !self.zone_input_pins.is_empty() || !self.zone_output_pins.is_empty()
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
