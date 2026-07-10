use super::super::camera_settings::CameraSettings;
use super::super::node_data::CustomNodeData;
use super::super::node_data::NoData;
use super::super::node_data::NodeData;
use super::super::node_network::{Argument, CollapseMode, Node, NodeNetwork};
use super::super::node_network::{NodeDisplayState, NodeDisplayType};
use super::super::node_type::{NodeType, OutputPinDefinition, Parameter};
use super::super::node_type::{generic_node_data_loader, generic_node_data_saver};
use super::super::node_type_registry::{
    NodeTypeRegistry, RecordTypeDef, validate_record_type_defs,
};
use super::super::nodes::atom_edit::atom_edit::AtomEditData;
use crate::structure_designer::data_type::DataType;
use crate::util::serialization_utils::{dvec2_serializer, dvec3_serializer};
use glam::f64::{DVec2, DVec3};
use serde::{Deserialize, Serialize};
use serde_json;
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::sync::Arc;

// The current version of the serialization format
const SERIALIZATION_VERSION: u32 = 7;

/// Sentinel `data_type` written for custom (user-network) node instances. Their
/// `node_type_name` is a key in `node_networks`, not `built_in_node_types`, so
/// there is no built-in saver/loader to key off — this marks the persisted blob
/// as a `CustomNodeData` (literal parameter values) on the way back in.
const CUSTOM_NODE_DATA_TYPE: &str = "custom_node";

/// Camera settings that are saved per node network
#[derive(Serialize, Deserialize, Clone)]
pub struct SerializableCameraSettings {
    #[serde(with = "dvec3_serializer")]
    pub eye: DVec3,
    #[serde(with = "dvec3_serializer")]
    pub target: DVec3,
    #[serde(with = "dvec3_serializer")]
    pub up: DVec3,
    pub orthographic: bool,
    pub ortho_half_height: f64,
    #[serde(with = "dvec3_serializer")]
    pub pivot_point: DVec3,
}

impl Default for SerializableCameraSettings {
    fn default() -> Self {
        Self {
            eye: DVec3::new(0.0, -30.0, 10.0),
            target: DVec3::new(0.0, 0.0, 0.0),
            up: DVec3::new(0.0, 0.32, 0.95),
            orthographic: false,
            ortho_half_height: 10.0,
            pivot_point: DVec3::new(0.0, 0.0, 0.0),
        }
    }
}

/// Serializable version of Parameter struct for JSON serialization
#[derive(Serialize, Deserialize)]
pub struct SerializableParameter {
    pub name: String,
    pub data_type: String,
}

/// Serializable output pin definition for JSON serialization.
#[derive(Serialize, Deserialize, Clone)]
pub struct SerializableOutputPin {
    pub name: String,
    pub data_type: String,
}

/// Serializable version of NodeType struct for JSON serialization
#[derive(Serialize, Deserialize)]
pub struct SerializableNodeType {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default = "default_category")]
    pub category: String,
    pub parameters: Vec<SerializableParameter>,
    /// New field: always written on save. Contains output pin definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_pins: Vec<SerializableOutputPin>,
    /// Old field: only read for migration from old .cnnd files. Never written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_type: Option<String>,
    /// Inside-facing left-edge pins on a zone-owning (HOF) node type. Empty
    /// for every node type today — reserved for the later zone phases.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zone_input_pins: Vec<SerializableOutputPin>,
    /// Inside-facing right-edge pins on a zone-owning (HOF) node type. Empty
    /// for every node type today — reserved for the later zone phases.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zone_output_pins: Vec<SerializableParameter>,
}

fn default_category() -> String {
    "Custom".to_string()
}

/// Converts NodeTypeCategory enum to string for serialization
fn category_to_string(
    category: &crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory,
) -> String {
    use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
    match category {
        NodeTypeCategory::Annotation => "Annotation".to_string(),
        NodeTypeCategory::MathAndProgramming => "MathAndProgramming".to_string(),
        NodeTypeCategory::Geometry2D => "Geometry2D".to_string(),
        NodeTypeCategory::Geometry3D => "Geometry3D".to_string(),
        NodeTypeCategory::AtomicStructure => "AtomicStructure".to_string(),
        NodeTypeCategory::OtherBuiltin => "OtherBuiltin".to_string(),
        NodeTypeCategory::Custom => "Custom".to_string(),
    }
}

/// Converts string to NodeTypeCategory enum for deserialization
/// Defaults to Custom if the string is not recognized for backward compatibility
fn category_from_string(
    category_str: &str,
) -> crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory {
    use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
    match category_str {
        "Annotation" => NodeTypeCategory::Annotation,
        "MathAndProgramming" => NodeTypeCategory::MathAndProgramming,
        "Geometry2D" => NodeTypeCategory::Geometry2D,
        "Geometry3D" => NodeTypeCategory::Geometry3D,
        "AtomicStructure" => NodeTypeCategory::AtomicStructure,
        "OtherBuiltin" => NodeTypeCategory::OtherBuiltin,
        "Custom" => NodeTypeCategory::Custom,
        _ => NodeTypeCategory::Custom, // Default for unknown/old files
    }
}

/// Serializable version of Node without trait objects for JSON serialization
#[derive(Serialize, Deserialize)]
pub struct SerializableNode {
    pub id: u64,
    pub node_type_name: String,
    /// User-specified name for this node (e.g., "mybox" from "mybox = cuboid {...}").
    /// If None, the node will be named using auto-generated names like "cuboid1".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_name: Option<String>,
    #[serde(with = "dvec2_serializer")]
    pub position: DVec2,
    pub arguments: Vec<Argument>,
    // Use a string type tag and direct JSON value for the polymorphic data
    pub data_type: String,
    pub data: serde_json::Value, // Store as native JSON value instead of a string for better readability
    /// The owned zone body for HOF nodes. Always `None` for every node
    /// produced by Phase 2 — the field is reserved for later phases that
    /// rewrite `map` / `filter` / `fold` / `foreach` onto zones. `#[serde(default)]`
    /// keeps old fixtures loadable without regeneration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone: Option<SerializableNodeNetwork>,
    /// Wires terminating at zone-output (inside-right) pins. Always empty for
    /// non-HOF nodes. Phase 2 never populates this; reserved for later phases.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zone_output_arguments: Vec<Argument>,
    /// Stored body width for HOF nodes (logical pixels). Defaulted on load so
    /// pre-zone `.cnnd` fixtures continue to deserialize. Meaningful only when
    /// `zone.is_some()`; see `doc/design_zones_ui.md` §"Body sizing".
    #[serde(default = "default_body_width")]
    pub body_width: f64,
    /// Stored body height for HOF nodes (logical pixels). See [`body_width`].
    #[serde(default = "default_body_height")]
    pub body_height: f64,
    /// User's collapse-mode choice for HOF nodes. `Auto` (the default) follows
    /// the `f` pin; the two overrides force it. Defaulted on load so older
    /// `.cnnd` files deserialize. Inert on non-HOF nodes. See
    /// `doc/design_hof_node_collapse.md`.
    #[serde(default)]
    pub collapse_mode: CollapseMode,
}

fn default_body_width() -> f64 {
    crate::structure_designer::node_network::DEFAULT_BODY_WIDTH
}

fn default_body_height() -> f64 {
    crate::structure_designer::node_network::DEFAULT_BODY_HEIGHT
}

/// Serializable version of NodeNetwork for JSON serialization
#[derive(Serialize, Deserialize)]
pub struct SerializableNodeNetwork {
    pub next_node_id: u64,
    pub node_type: SerializableNodeType,
    pub nodes: Vec<SerializableNode>, // Store as vec instead of HashMap
    pub return_node_id: Option<u64>,
    pub displayed_node_ids: Vec<(u64, NodeDisplayType)>, // Always written for backward compat
    /// Per-node pin display state. Omitted from JSON if empty (backward compat).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub displayed_output_pins: Vec<(u64, Vec<i32>)>,
    /// Camera settings for this network's 3D viewport (backward compatible - defaults to None for old files)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera_settings: Option<SerializableCameraSettings>,
}

/// Container for serializing all node networks in the NodeTypeRegistry
#[derive(Serialize, Deserialize)]
pub struct SerializableNodeTypeRegistryNetworks {
    pub node_networks: Vec<(String, SerializableNodeNetwork)>,
    pub version: u32, // For future compatibility
    /// Whether the file was saved in direct editing mode.
    /// Missing field defaults to false (Node Network Mode) for backward compatibility.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub direct_editing_mode: bool,
    /// CLI access rules: sparse map of namespace/network prefixes to allowed (true) / denied (false).
    /// Missing field defaults to empty map (all access allowed) for backward compatibility.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub cli_access_rules: std::collections::HashMap<String, bool>,
    /// Named record type defs. Backward-compat: missing field deserializes to
    /// an empty list, so pre-record `.cnnd` files load unchanged. Emitted
    /// sorted by name on save for deterministic on-disk order. See
    /// `doc/design_record_types.md` Phase 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub record_type_defs: Vec<RecordTypeDef>,
    /// Deliberately-created, currently-empty folder paths. Backward-compat:
    /// missing field deserializes to an empty set, so pre-folder `.cnnd` files
    /// load unchanged; `BTreeSet` keeps the on-disk order deterministic. See
    /// `doc/design_empty_folders.md`.
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub folders: std::collections::BTreeSet<String>,
}

/// Converts a NodeType to its serializable counterpart
pub fn node_type_to_serializable(node_type: &NodeType) -> SerializableNodeType {
    let serializable_parameters = node_type
        .parameters
        .iter()
        .map(|param| SerializableParameter {
            name: param.name.clone(),
            data_type: param.data_type.to_string(),
        })
        .collect();

    let serializable_output_pins = node_type
        .output_pins
        .iter()
        .map(|pin| SerializableOutputPin {
            name: pin.name.clone(),
            data_type: pin.data_type.to_string(),
        })
        .collect();

    let serializable_zone_input_pins = node_type
        .zone_input_pins
        .iter()
        .map(|pin| SerializableOutputPin {
            name: pin.name.clone(),
            data_type: pin.data_type.to_string(),
        })
        .collect();

    let serializable_zone_output_pins = node_type
        .zone_output_pins
        .iter()
        .map(|param| SerializableParameter {
            name: param.name.clone(),
            data_type: param.data_type.to_string(),
        })
        .collect();

    SerializableNodeType {
        name: node_type.name.clone(),
        description: node_type.description.clone(),
        summary: node_type.summary.clone(),
        category: category_to_string(&node_type.category),
        parameters: serializable_parameters,
        output_pins: serializable_output_pins,
        output_type: None, // Old field: never written
        zone_input_pins: serializable_zone_input_pins,
        zone_output_pins: serializable_zone_output_pins,
    }
}

/// Converts a SerializableNodeType back to a NodeType
///
/// # Returns
/// * `io::Result<NodeType>` - The converted NodeType or an error if conversion fails
pub fn serializable_to_node_type(serializable: &SerializableNodeType) -> io::Result<NodeType> {
    // Parse output pins: prefer new format, fall back to old output_type field
    let output_pins = if !serializable.output_pins.is_empty() {
        // New format: use output_pins directly
        serializable
            .output_pins
            .iter()
            .map(|p| {
                let data_type = DataType::from_string(&p.data_type).map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "Invalid output pin type for pin '{}' (type_str={:?}): {}",
                            p.name, p.data_type, e
                        ),
                    )
                })?;
                Ok(OutputPinDefinition::fixed(&p.name, data_type))
            })
            .collect::<io::Result<Vec<_>>>()?
    } else if let Some(ref output_type_str) = serializable.output_type {
        // Old format: migrate single output_type to output_pins[0]
        let output_type = DataType::from_string(output_type_str).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Invalid output type (type_str={:?}): {}",
                    output_type_str, e
                ),
            )
        })?;
        OutputPinDefinition::single(output_type)
    } else {
        // Fallback: no output
        OutputPinDefinition::single(DataType::None)
    };

    // Create parameters from the serializable parameters
    let parameters = serializable
        .parameters
        .iter()
        .map(|serializable_param| {
            // Parse the data type using the helper function
            let data_type = DataType::from_string(&serializable_param.data_type).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Invalid parameter data type for parameter '{}' (type_str={:?}): {}",
                        serializable_param.name, serializable_param.data_type, e
                    ),
                )
            })?;

            Ok(Parameter {
                id: None,
                name: serializable_param.name.clone(),
                data_type,
            })
        })
        .collect::<io::Result<Vec<Parameter>>>()?;

    // Parse category from string
    let category = category_from_string(&serializable.category);

    // Parse zone pin definitions. Empty for every node type today; reserved
    // for later phases when HOFs are rewritten on top of zones.
    let zone_input_pins = serializable
        .zone_input_pins
        .iter()
        .map(|p| {
            let data_type = DataType::from_string(&p.data_type).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Invalid zone-input pin type for pin '{}' (type_str={:?}): {}",
                        p.name, p.data_type, e
                    ),
                )
            })?;
            Ok(OutputPinDefinition::fixed(&p.name, data_type))
        })
        .collect::<io::Result<Vec<_>>>()?;

    let zone_output_pins = serializable
        .zone_output_pins
        .iter()
        .map(|p| {
            let data_type = DataType::from_string(&p.data_type).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Invalid zone-output pin type for pin '{}' (type_str={:?}): {}",
                        p.name, p.data_type, e
                    ),
                )
            })?;
            Ok(Parameter {
                id: None,
                name: p.name.clone(),
                data_type,
            })
        })
        .collect::<io::Result<Vec<_>>>()?;

    // Create the NodeType with CustomNodeData to support literal parameters
    Ok(NodeType {
        name: serializable.name.clone(),
        description: serializable.description.clone(),
        summary: serializable.summary.clone(),
        category,
        parameters,
        output_pins,
        zone_input_pins,
        zone_output_pins,
        node_data_creator: || Box::new(CustomNodeData::default()),
        node_data_saver: generic_node_data_saver::<CustomNodeData>,
        node_data_loader: generic_node_data_loader::<CustomNodeData>,
        public: true, // TODO: we should save this info (with proper backward compatibility), but we do not save it yet
    })
}

//  &node.data.as_any_ref().downcast_ref::<HalfSpaceData>().unwrap();

/// Converts a Node to a SerializableNode, handling the polymorphic NodeData
///
/// # Returns
/// * `io::Result<SerializableNode>` - The serializable node or an error if serialization fails
pub fn node_to_serializable(
    id: u64,
    node: &mut Node,
    built_in_node_types: &std::collections::HashMap<
        String,
        crate::structure_designer::node_type::NodeType,
    >,
    design_dir: Option<&str>,
) -> io::Result<SerializableNode> {
    // Handle the polymorphic node data based on its type
    let node_type_name = node.node_type_name.clone();

    // Convert the node data to a JSON value using the built-in node types
    let (data_type, json_data) = if let Some(node_type) = built_in_node_types.get(&node_type_name) {
        let json_data = (node_type.node_data_saver)(node.data.as_mut(), design_dir)?;
        (node_type_name.clone(), json_data)
    } else if node
        .data
        .as_any_ref()
        .downcast_ref::<CustomNodeData>()
        .is_some()
    {
        // Custom (user-network) node instance: its `node_type_name` is a key in
        // `node_networks`, not `built_in_node_types`, so it has no built-in
        // saver. Persist its `CustomNodeData` (literal parameter values) under a
        // sentinel `data_type` so they survive save/load. Without this, custom
        // nodes were written as `no_data` and reloaded as `NoData`, silently
        // dropping every stored literal and making the property panel
        // non-editable after load.
        let json_data = generic_node_data_saver::<CustomNodeData>(node.data.as_mut(), design_dir)?;
        (CUSTOM_NODE_DATA_TYPE.to_string(), json_data)
    } else {
        // Fallback for unknown types
        ("no_data".to_string(), serde_json::json!({}))
    };

    // Serialize the owned zone body (if any). Phase 2 never populates this on
    // any node, but the path has to exist so later phases drop in transparently.
    // `Arc::make_mut` is the cheap path here too: the saved-side body needs a
    // mutable reference to walk node data savers, and serialization is the only
    // caller, so the refcount at this point is 1.
    let zone = if let Some(arc_body) = node.zone.as_mut() {
        let body_ref = Arc::make_mut(arc_body);
        Some(node_network_to_serializable(
            body_ref,
            built_in_node_types,
            design_dir,
        )?)
    } else {
        None
    };

    // Create the serializable node
    Ok(SerializableNode {
        id,
        node_type_name: node.node_type_name.clone(),
        custom_name: node.custom_name.clone(),
        position: node.position,
        arguments: node.arguments.clone(),
        data_type,
        data: json_data,
        zone,
        zone_output_arguments: node.zone_output_arguments.clone(),
        body_width: node.body_width,
        body_height: node.body_height,
        collapse_mode: node.collapse_mode,
    })
}

/// Creates a Node instance from a SerializableNode
///
/// # Returns
/// * `io::Result<Node>` - The deserialized Node or an error if deserialization fails
pub fn serializable_to_node(
    serializable: &SerializableNode,
    built_in_node_types: &std::collections::HashMap<
        String,
        crate::structure_designer::node_type::NodeType,
    >,
    design_dir: Option<&str>,
) -> io::Result<Node> {
    // Create the node data using the built-in node types
    let data: Box<dyn NodeData> =
        if let Some(node_type) = built_in_node_types.get(&serializable.data_type) {
            (node_type.node_data_loader)(&serializable.data, design_dir)?
        } else if serializable.data_type == CUSTOM_NODE_DATA_TYPE {
            // Custom node saved with persisted literal values (see
            // `node_to_serializable`).
            generic_node_data_loader::<CustomNodeData>(&serializable.data, design_dir)?
        } else if !built_in_node_types.contains_key(&serializable.node_type_name) {
            // The node's *type* is not built-in, so this is a custom
            // (user-network) instance — or a reference to a type that no longer
            // exists. Either way, give it an empty `CustomNodeData` rather than
            // `NoData` so its literal parameters stay editable. This also
            // upgrades legacy files where custom nodes were saved as `no_data`
            // before custom-node literal persistence existed (those carry no
            // literal values to recover, but become editable again).
            Box::new(CustomNodeData::default())
        } else {
            // Default to NoData for unknown data on a known built-in type.
            Box::new(NoData {})
        };

    // Reconstruct the owned zone body (if any) and zone-output arguments.
    // Both are empty for every node serialized today; populated when zone-
    // bearing HOFs land in later phases. `Arc` is per the zones design
    // (cheap-clone semantics, CoW via `Arc::make_mut`); the runtime is
    // single-threaded so the lack of `Send + Sync` on `NodeNetwork` is
    // immaterial — keep the warning suppressed locally.
    #[allow(clippy::arc_with_non_send_sync)]
    let zone = serializable
        .zone
        .as_ref()
        .map(|sn| -> io::Result<Arc<NodeNetwork>> {
            let body = serializable_to_node_network(sn, built_in_node_types, design_dir)?;
            Ok(Arc::new(body))
        })
        .transpose()?;

    // Create the Node instance
    Ok(Node {
        id: serializable.id,
        node_type_name: serializable.node_type_name.clone(),
        custom_name: serializable.custom_name.clone(),
        position: serializable.position,
        arguments: serializable.arguments.clone(),
        data,
        custom_node_type: None,
        zone,
        zone_output_arguments: serializable.zone_output_arguments.clone(),
        body_width: serializable.body_width,
        body_height: serializable.body_height,
        collapse_mode: serializable.collapse_mode,
    })
}

/// Converts a NodeNetwork to a SerializableNodeNetwork
///
/// # Returns
/// * `io::Result<SerializableNodeNetwork>` - The serializable network or an error if serialization fails
pub fn node_network_to_serializable(
    network: &mut NodeNetwork,
    built_in_node_types: &std::collections::HashMap<
        String,
        crate::structure_designer::node_type::NodeType,
    >,
    design_dir: Option<&str>,
) -> io::Result<SerializableNodeNetwork> {
    // Convert each node to a SerializableNode
    let mut serializable_nodes = Vec::new();

    for (id, node) in &mut network.nodes {
        let serializable_node = node_to_serializable(*id, node, built_in_node_types, design_dir)?;
        serializable_nodes.push(serializable_node);
    }

    // Split displayed_nodes into displayed_node_ids + displayed_output_pins for serialization
    let displayed_node_ids: Vec<(u64, NodeDisplayType)> = network
        .displayed_nodes
        .iter()
        .map(|(&id, state)| (id, state.display_type))
        .collect();

    // Only write displayed_output_pins for nodes with non-default pin state
    let default_pins: std::collections::HashSet<i32> = std::collections::HashSet::from([0]);
    let displayed_output_pins: Vec<(u64, Vec<i32>)> = network
        .displayed_nodes
        .iter()
        .filter(|(_, state)| state.displayed_pins != default_pins)
        .map(|(&id, state)| (id, state.displayed_pins.iter().copied().collect()))
        .collect();

    // Create a serializable version of the node type
    let serializable_node_type = node_type_to_serializable(&network.node_type);

    // Convert camera settings if present
    let camera_settings = network
        .camera_settings
        .as_ref()
        .map(|cs| SerializableCameraSettings {
            eye: cs.eye,
            target: cs.target,
            up: cs.up,
            orthographic: cs.orthographic,
            ortho_half_height: cs.ortho_half_height,
            pivot_point: cs.pivot_point,
        });

    // Create the serializable network
    Ok(SerializableNodeNetwork {
        next_node_id: network.next_node_id,
        node_type: serializable_node_type,
        nodes: serializable_nodes,
        return_node_id: network.return_node_id,
        displayed_node_ids,
        displayed_output_pins,
        camera_settings,
    })
}

/// Creates a NodeNetwork from a SerializableNodeNetwork
///
/// # Returns
/// * `io::Result<NodeNetwork>` - The deserialized network or an error if deserialization fails
pub fn serializable_to_node_network(
    serializable: &SerializableNodeNetwork,
    built_in_node_types: &std::collections::HashMap<
        String,
        crate::structure_designer::node_type::NodeType,
    >,
    design_dir: Option<&str>,
) -> io::Result<NodeNetwork> {
    // Create the node type from the serializable node type
    let node_type = serializable_to_node_type(&serializable.node_type)?;

    // Create a new network
    let mut network = NodeNetwork::new(node_type);

    // Set next_node_id and return_node_id
    network.next_node_id = serializable.next_node_id;
    network.return_node_id = serializable.return_node_id;

    // Build displayed_nodes from the two serialized fields (merge)
    let mut displayed_nodes = std::collections::HashMap::new();
    for (node_id, display_type) in &serializable.displayed_node_ids {
        displayed_nodes.insert(*node_id, NodeDisplayState::with_type(*display_type));
    }
    // Overlay explicit pin display state where present
    for (node_id, pins) in &serializable.displayed_output_pins {
        if let Some(state) = displayed_nodes.get_mut(node_id) {
            state.displayed_pins = pins.iter().copied().collect();
        }
    }
    network.displayed_nodes = displayed_nodes;

    // Process each node
    for serializable_node in &serializable.nodes {
        let node = serializable_to_node(serializable_node, built_in_node_types, design_dir)?;
        network.nodes.insert(node.id, node);
    }

    // Restore the parameter-id counter. `next_param_id` is intentionally NOT
    // part of the serialized form, so derive it from the loaded parameter nodes:
    // the next id handed out must be strictly greater than every existing
    // `param_id`. Without this, every deserialize path (`.cnnd` load,
    // `duplicate_node_network`, and the undo/snapshot-restore commands — all of
    // which round-trip through this function) would reset `next_param_id` to 1
    // (`NodeNetwork::new`), so the next parameter added to the network reuses an
    // existing id; `repair_call_sites_for_network` then resolves the collision to
    // the first parameter's index and clones its wire onto the new pin, silently
    // mis-rewiring instances in other networks. See
    // `doc/design_parameter_wire_stability.md` (F1). `.max(..)` keeps the
    // `NodeNetwork::new` floor of 1 and never lowers an already-higher counter.
    let max_param_id = network
        .nodes
        .values()
        .filter_map(|node| {
            node.data
                .as_ref()
                .as_any_ref()
                .downcast_ref::<crate::structure_designer::nodes::parameter::ParameterData>()
                .and_then(|p| p.param_id)
        })
        .max();
    if let Some(m) = max_param_id {
        network.next_param_id = network.next_param_id.max(m + 1);
    }

    // Migration: atom_edit output_diff → displayed_pins
    // For old files where output_diff: true was used to switch to diff view,
    // migrate to displayed_pins: {1} (show diff pin only) if the node wasn't
    // already in displayed_output_pins (which would mean a newer file format).
    {
        let nodes_with_explicit_pins: std::collections::HashSet<u64> = serializable
            .displayed_output_pins
            .iter()
            .map(|(id, _)| *id)
            .collect();

        let mut nodes_to_migrate: Vec<u64> = Vec::new();
        for (&node_id, node) in &network.nodes {
            if node.node_type_name == "atom_edit"
                && !nodes_with_explicit_pins.contains(&node_id)
                && network.displayed_nodes.contains_key(&node_id)
                && let Some(data) = node
                    .data
                    .as_ref()
                    .as_any_ref()
                    .downcast_ref::<AtomEditData>()
                && data.output_diff
            {
                nodes_to_migrate.push(node_id);
            }
        }
        for node_id in nodes_to_migrate {
            if let Some(state) = network.displayed_nodes.get_mut(&node_id) {
                state.displayed_pins = std::collections::HashSet::from([1]);
            }
        }
    }

    // Migration: assign names to nodes without custom_name (old files)
    // This ensures that files created before persistent node names was implemented
    // will get names assigned when loaded.
    // Sort by node ID for deterministic name assignment order.
    let mut nodes_needing_names: Vec<(u64, String)> = network
        .nodes
        .iter()
        .filter(|(_, node)| node.custom_name.is_none())
        .map(|(id, node)| (*id, node.node_type_name.clone()))
        .collect();
    nodes_needing_names.sort_by_key(|(id, _)| *id);

    for (node_id, node_type_name) in nodes_needing_names {
        let name = network.generate_unique_display_name(&node_type_name);
        if let Some(node) = network.nodes.get_mut(&node_id) {
            node.custom_name = Some(name);
        }
    }

    // Convert camera settings if present
    network.camera_settings = serializable
        .camera_settings
        .as_ref()
        .map(|scs| CameraSettings {
            eye: scs.eye,
            target: scs.target,
            up: scs.up,
            orthographic: scs.orthographic,
            ortho_half_height: scs.ortho_half_height,
            pivot_point: scs.pivot_point,
        });

    Ok(network)
}

/// Saves node networks from a NodeTypeRegistry to a JSON file
///
/// # Parameters
/// * `registry` - The NodeTypeRegistry to save
/// * `file_path` - Path to the output JSON file
///
/// # Returns
/// * `io::Result<()>` - Success or an error if saving fails
pub fn save_node_networks_to_file(
    registry: &mut NodeTypeRegistry,
    file_path: &Path,
    direct_editing_mode: bool,
    cli_access_rules: &std::collections::HashMap<String, bool>,
) -> io::Result<()> {
    // Extract design directory early
    let design_dir = file_path.parent().and_then(|p| p.to_str());

    // Convert the node networks to a serializable format. Sort by name so the
    // file's network array order is deterministic across saves (HashMap
    // iteration order is not stable, which would otherwise leak into the
    // serialized file and shuffle which network ends up "first" — affecting
    // any consumer that keys off `LoadResult.first_network_name`, including
    // the snapshot test suite).
    let mut sorted_names: Vec<&String> = registry.node_networks.keys().collect();
    sorted_names.sort();
    let sorted_names: Vec<String> = sorted_names.into_iter().cloned().collect();

    let mut serializable_networks = Vec::new();
    for name in &sorted_names {
        let network = registry
            .node_networks
            .get_mut(name)
            .expect("name came from this map's keys");
        let serializable_network =
            node_network_to_serializable(network, &registry.built_in_node_types, design_dir)?;
        serializable_networks.push((name.clone(), serializable_network));
    }

    // Collect record type defs sorted by name for deterministic output across
    // saves (HashMap iteration order is not stable).
    let mut record_type_defs: Vec<RecordTypeDef> =
        registry.record_type_defs.values().cloned().collect();
    record_type_defs.sort_by(|a, b| a.name.cmp(&b.name));

    // Create the container with version information
    let serializable_registry = SerializableNodeTypeRegistryNetworks {
        node_networks: serializable_networks,
        version: SERIALIZATION_VERSION,
        direct_editing_mode,
        cli_access_rules: cli_access_rules.clone(),
        record_type_defs,
        // `BTreeSet` is already ordered, so the on-disk array is deterministic.
        folders: registry.folders.clone(),
    };

    // Serialize to JSON
    let json_data = serde_json::to_string_pretty(&serializable_registry)?;

    // Create the parent directory if it doesn't exist
    if let Some(parent) = Path::new(file_path).parent() {
        fs::create_dir_all(parent)?;
    }

    // Write to file
    fs::write(file_path, json_data)?;

    registry.design_file_name = Some(file_path.to_string_lossy().to_string());

    Ok(())
}

/// Result of loading a .cnnd file
pub struct LoadResult {
    /// Name of the first network in the file (empty if no networks)
    pub first_network_name: String,
    /// Whether the file was saved in direct editing mode
    pub direct_editing_mode: bool,
    /// CLI access rules loaded from the file
    pub cli_access_rules: std::collections::HashMap<String, bool>,
}

/// Loads node networks from a JSON file into a NodeTypeRegistry
///
/// # Parameters
/// * `registry` - The NodeTypeRegistry to load into
/// * `file_path` - The file path to load from as a string
///
/// # Returns
/// * `io::Result<LoadResult>` - Ok with load metadata if successful, Err otherwise
pub fn load_node_networks_from_file(
    registry: &mut NodeTypeRegistry,
    file_path: &str,
) -> io::Result<LoadResult> {
    // Extract design directory early
    let design_dir = std::path::Path::new(file_path)
        .parent()
        .and_then(|p| p.to_str());

    // Read the file content
    let mut file = fs::File::open(file_path)?;
    let mut json_data = String::new();
    file.read_to_string(&mut json_data)?;

    // Parse to an untyped JSON value first so the version field can be inspected and any
    // pre-serde migration pass can rewrite the shape before strict deserialization.
    let mut root_value: serde_json::Value = serde_json::from_str(&json_data)?;

    // Read the version field. Missing or non-integer is treated as 0 (ancient file).
    let version: u32 = root_value
        .get("version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    if version > SERIALIZATION_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Unsupported version: {}", version),
        ));
    }

    // Chained historical up-converters. Each pass runs only if the loaded
    // file pre-dates the version after that pass. A v2 file chains through
    // all passes; a v3 file runs v3→v4, v5→v6, then v6→v7; a v6 file runs
    // only v6→v7.
    //
    // Note: there is no v4→v5 transform pass — v4 and v5 are structurally
    // identical. The legacy main-branch function-pin idiom (a node's `-1` pin
    // feeding an HOF `f` pin with some inputs wired as captures) loads
    // directly: the custom `Argument` deserializer converts the wire storage
    // shape, and the function-pin synthesizer (`build_node_function_closure`)
    // reproduces the capture/parameter partition at evaluation time. See
    // `doc/design_node_function_pin_captures.md`.
    //
    // v5→v6 (issue #384): `free_rot`'s angle input switches radians → degrees.
    // See `doc/design_degree_angle_inputs.md` and `migrate_v5_to_v6`.
    //
    // v6→v7 (issue #353): the built-in `export_xyz` node is renamed to
    // `export_atoms` (format now derived from the file extension). Mechanical
    // whole-tree rename of the `node_type_name` / `data_type` reference keys.
    // See `doc/design_export_atoms_node.md` and `migrate_v6_to_v7`.
    if version < 3 {
        super::migrate_v2_to_v3::migrate_v2_to_v3(&mut root_value).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("v2→v3 migration failed: {}", e),
            )
        })?;
    }
    if version < 4 {
        super::migrate_v3_to_v4::migrate_v3_to_v4(&mut root_value).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("v3→v4 migration failed: {}", e),
            )
        })?;
    }
    if version < 6 {
        super::migrate_v5_to_v6::migrate_v5_to_v6(&mut root_value).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("v5→v6 migration failed: {}", e),
            )
        })?;
    }
    if version < 7 {
        super::migrate_v6_to_v7::migrate_v6_to_v7(&mut root_value).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("v6→v7 migration failed: {}", e),
            )
        })?;
    }
    if version < SERIALIZATION_VERSION {
        // Reflect the new version in the in-memory value so any downstream
        // reader (deserializer, validator) sees the current version.
        if let Some(obj) = root_value.as_object_mut() {
            obj.insert(
                "version".to_string(),
                serde_json::Value::from(SERIALIZATION_VERSION),
            );
        }
    }

    // Deserialize the (possibly migrated) value into the strict typed form.
    let serializable_registry: SerializableNodeTypeRegistryNetworks =
        serde_json::from_value(root_value)?;

    let direct_editing_mode = serializable_registry.direct_editing_mode;
    let cli_access_rules = serializable_registry.cli_access_rules;

    registry.node_networks.clear();
    registry.record_type_defs.clear();
    // Empty-folder markers (doc/design_empty_folders.md). Reconciled against the
    // loaded entities after networks/records are in, below.
    registry.folders = serializable_registry.folders;

    // Load record type defs first so any networks referencing them can resolve
    // schemas during validation. Defensive: a hand-edited file can carry a
    // duplicate name or a cycle that the in-memory editor would have rejected.
    // We accept the data here (so the user isn't locked out of opening the
    // file) and let the post-load validation pass surface errors.
    for def in serializable_registry.record_type_defs {
        registry.record_type_defs.insert(def.name.clone(), def);
    }

    // Canonicalize every stored `DataType::Function` reachable through record
    // type def field types. Serde routing through `FunctionType::new` already
    // canonicalizes JSON-deserialized values; this is the belt-and-braces pass
    // covering any path that bypassed serde. See `canonicalize.rs`.
    crate::structure_designer::canonicalize::canonicalize_record_type_defs(
        &mut registry.record_type_defs,
    );

    // Re-run cycle + dangling-reference checks on the loaded record_type_defs
    // (defensive against hand-edited `.cnnd` files). Errors are non-fatal at
    // load — they're logged so the user can fix them in the schema editor and
    // surface in network validation as the related wires get checked.
    for err in validate_record_type_defs(registry) {
        eprintln!("Warning: {}", err);
    }

    // Track the first network name
    let mut first_network_name = String::new();

    // Process each network
    for (name, serializable_network) in serializable_registry.node_networks {
        // Capture the first network name
        if first_network_name.is_empty() {
            first_network_name = name.clone();
        }

        let mut network = serializable_to_node_network(
            &serializable_network,
            &registry.built_in_node_types,
            design_dir,
        )
        .map_err(|e| io::Error::new(e.kind(), format!("In network '{}': {}", name, e)))?;
        // Canonicalize every `DataType::Function` reachable from this network
        // before any validator or evaluator touches it. Serde routing through
        // `FunctionType::new` and the data-type-string parser already
        // canonicalize the common load paths; this is the belt-and-braces
        // walker for any path that bypassed them. See `canonicalize.rs` and
        // `doc/design_currying.md` Phase 1.
        crate::structure_designer::canonicalize::canonicalize_network(&mut network);
        registry.initialize_custom_node_types_for_network(&mut network);

        registry.repair_node_network(&mut network);

        registry.node_networks.insert(name, network);
    }

    // Drop any empty-folder marker that is redundant because an entity (or
    // another marker) already lives at or under it — defensive against
    // hand-edited / out-of-order files. See `doc/design_empty_folders.md`.
    registry.prune_redundant_folders();

    // Set the design file name after successful load
    registry.design_file_name = Some(file_path.to_string());

    Ok(LoadResult {
        first_network_name,
        direct_editing_mode,
        cli_access_rules,
    })
}
