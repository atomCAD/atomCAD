use crate::api::api_common::from_api_ivec2;
use crate::api::api_common::from_api_vec3;
use crate::api::api_common::refresh_structure_designer_auto;
use crate::api::api_common::to_api_ivec2;
use crate::api::api_common::to_api_vec3;
use crate::api::api_common::with_mut_cad_instance;
use crate::api::api_common::with_cad_instance;
use crate::api::api_common::with_mut_cad_instance_or;
use crate::api::api_common::with_cad_instance_or;
use crate::api::common_api_types::APIResult;
use crate::api::structure_designer::structure_designer_api_types::{NodeNetworkView, APINetworkWithValidationErrors, APINodeCategoryView, APIDataTypeBase};
use crate::structure_designer::nodes::string::StringData;
use crate::structure_designer::nodes::bool::BoolData;
use crate::structure_designer::nodes::int::IntData;
use crate::structure_designer::nodes::float::FloatData;
use crate::structure_designer::nodes::vec2::Vec2Data;
use crate::structure_designer::nodes::vec3::Vec3Data;
use crate::structure_designer::nodes::ivec2::IVec2Data;
use crate::structure_designer::nodes::ivec3::IVec3Data;
use crate::structure_designer::nodes::range::RangeData;
use crate::structure_designer::nodes::circle::CircleData;
use crate::structure_designer::nodes::extrude::ExtrudeData;
use crate::structure_designer::nodes::extrude::ExtrudeEvalCache;
use crate::structure_designer::nodes::half_plane::HalfPlaneData;
use crate::structure_designer::nodes::reg_poly::RegPolyData;
use crate::structure_designer::nodes::rect::RectData;
use std::collections::HashMap;
use crate::api::structure_designer::structure_designer_api_types::InputPinView;
use crate::api::structure_designer::structure_designer_api_types::NodeView;
use crate::api::structure_designer::structure_designer_api_types::WireView;
use crate::api::common_api_types::APIVec2;
use crate::api::structure_designer::structure_designer_api_types::APIStringData;
use crate::api::structure_designer::structure_designer_api_types::APIBoolData;
use crate::api::structure_designer::structure_designer_api_types::APIIntData;
use crate::api::structure_designer::structure_designer_api_types::APIFloatData;
use crate::api::structure_designer::structure_designer_api_types::APIVec2Data;
use crate::api::structure_designer::structure_designer_api_types::APIVec3Data;
use crate::api::structure_designer::structure_designer_api_types::APIIVec2Data;
use crate::api::structure_designer::structure_designer_api_types::APIIVec3Data;
use crate::api::structure_designer::structure_designer_api_types::APIRangeData;
use crate::api::structure_designer::structure_designer_api_types::APICuboidData;
use crate::api::structure_designer::structure_designer_api_types::APISphereData;
use crate::api::structure_designer::structure_designer_api_types::APIHalfSpaceData;
use crate::api::structure_designer::structure_designer_api_types::APIDrawingPlaneData;
use crate::api::structure_designer::structure_designer_api_types::APIGeoTransData;
use crate::api::structure_designer::structure_designer_api_types::APIAtomMoveData;
use crate::api::structure_designer::structure_designer_api_types::APIAtomRotData;
use crate::api::structure_designer::structure_designer_api_types::APIAtomTransData;
use crate::api::structure_designer::structure_designer_api_types::APIEditAtomData;
use crate::api::structure_designer::structure_designer_api_types::APIAtomCutData;
use crate::api::structure_designer::structure_designer_api_types::APIUnitCellData;
use crate::api::structure_designer::structure_designer_api_types::{APILatticeSymopData, APILatticeMoveData, APILatticeRotData, APIRotationalSymmetry};
use crate::structure_designer::nodes::cuboid::CuboidData;
use crate::structure_designer::nodes::unit_cell::UnitCellData;
use crate::structure_designer::nodes::sphere::SphereData;
use crate::structure_designer::nodes::half_space::HalfSpaceData;
use crate::structure_designer::nodes::drawing_plane::DrawingPlaneData;
use crate::structure_designer::nodes::geo_trans::GeoTransData;
use crate::structure_designer::nodes::lattice_symop::{LatticeSymopData, LatticeSymopEvalCache};
use crate::structure_designer::nodes::lattice_move::LatticeMoveData;
use crate::structure_designer::nodes::lattice_rot::{LatticeRotData, LatticeRotEvalCache};
use crate::crystolecule::unit_cell_symmetries::{analyze_unit_cell_complete, CrystalSystem, classify_crystal_system};
use crate::structure_designer::nodes::edit_atom::edit_atom::EditAtomData;
use crate::structure_designer::nodes::edit_atom::edit_atom::EditAtomTool;
use crate::structure_designer::nodes::atom_move::AtomMoveData;
use crate::structure_designer::nodes::atom_rot::AtomRotData;
use crate::structure_designer::nodes::atom_trans::AtomTransData;
use crate::structure_designer::nodes::atom_cut::AtomCutData;
use crate::structure_designer::nodes::import_xyz::ImportXYZData;
use crate::structure_designer::nodes::export_xyz::ExportXYZData;
use crate::api::api_common::to_api_vec2;
use crate::api::api_common::from_api_vec2;
use crate::api::api_common::to_api_ivec3;
use crate::api::api_common::from_api_ivec3;
use super::structure_designer_api_types::APICircleData;
use super::structure_designer_api_types::APIExtrudeData;
use super::structure_designer_api_types::APIHalfPlaneData;
use super::structure_designer_api_types::APIRegPolyData;
use super::structure_designer_api_types::APIRectData;
use super::structure_designer_api_types::APIParameterData;
use super::structure_designer_api_types::APIDataType;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::nodes::parameter::ParameterData;
use crate::structure_designer::nodes::expr::ExprData;
use crate::structure_designer::nodes::map::MapData;
use crate::structure_designer::nodes::motif::MotifData;
use crate::structure_designer::nodes::atom_fill::AtomFillData;
use crate::structure_designer::nodes::comment::CommentData;
use super::structure_designer_api_types::APIExprData;
use super::structure_designer_api_types::APIMapData;
use super::structure_designer_api_types::APIMotifData;
use super::structure_designer_api_types::APIAtomFillData;
use super::structure_designer_api_types::APIImportXYZData;
use super::structure_designer_api_types::APIExportXYZData;
use super::structure_designer_api_types::APICommentData;
use super::structure_designer_api_types::APINodeEvaluationResult;
use super::structure_designer_api_types::APIExprParameter;
use super::structure_designer_preferences::StructureDesignerPreferences;
use crate::structure_designer::cli_runner;
use crate::structure_designer::layout;

fn api_data_type_to_data_type(api_data_type: &APIDataType) -> Result<DataType, String> {
    let base_type = match api_data_type.data_type_base {
        APIDataTypeBase::None => DataType::None,
        APIDataTypeBase::Bool => DataType::Bool,
        APIDataTypeBase::String => DataType::String,
        APIDataTypeBase::Int => DataType::Int,
        APIDataTypeBase::Float => DataType::Float,
        APIDataTypeBase::Vec2 => DataType::Vec2,
        APIDataTypeBase::Vec3 => DataType::Vec3,
        APIDataTypeBase::IVec2 => DataType::IVec2,
        APIDataTypeBase::IVec3 => DataType::IVec3,
        APIDataTypeBase::UnitCell => DataType::UnitCell,
        APIDataTypeBase::DrawingPlane => DataType::DrawingPlane,
        APIDataTypeBase::Geometry2D => DataType::Geometry2D,
        APIDataTypeBase::Geometry => DataType::Geometry,
        APIDataTypeBase::Atomic => DataType::Atomic,
        APIDataTypeBase::Motif => DataType::Motif,
        APIDataTypeBase::Custom => {
            if let Some(custom_str) = &api_data_type.custom_data_type {
                return DataType::from_string(custom_str);
            } else {
                return Err("Custom data type string is missing".to_string());
            }
        }
    };

    if api_data_type.array {
        Ok(DataType::Array(Box::new(base_type)))
    } else {
        Ok(base_type)
    }
}

fn data_type_to_api_data_type(data_type: &DataType) -> APIDataType {
    let (base_data_type, is_array) = if let DataType::Array(element_type) = data_type {
        (element_type.as_ref(), true)
    } else {
        (data_type, false)
    };

    let data_type_base = match base_data_type {
        DataType::None => APIDataTypeBase::None,
        DataType::Bool => APIDataTypeBase::Bool,
        DataType::String => APIDataTypeBase::String,
        DataType::Int => APIDataTypeBase::Int,
        DataType::Float => APIDataTypeBase::Float,
        DataType::Vec2 => APIDataTypeBase::Vec2,
        DataType::Vec3 => APIDataTypeBase::Vec3,
        DataType::IVec2 => APIDataTypeBase::IVec2,
        DataType::IVec3 => APIDataTypeBase::IVec3,
        DataType::UnitCell => APIDataTypeBase::UnitCell,
        DataType::DrawingPlane => APIDataTypeBase::DrawingPlane,
        DataType::Geometry2D => APIDataTypeBase::Geometry2D,
        DataType::Geometry => APIDataTypeBase::Geometry,
        DataType::Atomic => APIDataTypeBase::Atomic,
        DataType::Motif => APIDataTypeBase::Motif,
        _ => APIDataTypeBase::Custom, // All other types are considered custom
    };

    let custom_data_type = if let APIDataTypeBase::Custom = data_type_base {
        Some(data_type.to_string())
    } else {
        None
    };

    APIDataType {
        data_type_base,
        custom_data_type,
        array: is_array,
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_node_network_view() -> Option<NodeNetworkView> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_network_name = match &cad_instance.structure_designer.active_node_network_name {
          Some(name) => name,
          None => return None,
        };

        let node_network = match cad_instance.structure_designer.node_type_registry.node_networks.get(node_network_name) {
          Some(network) => network,
          None => return None,
        };

        let mut node_network_view = NodeNetworkView {
          name: node_network.node_type.name.clone(),
          nodes: HashMap::new(),
          wires: Vec::new(),
        };

        for (_id, node) in node_network.nodes.iter() {
          let mut input_pins: Vec<InputPinView> = Vec::new();
          let node_type = match cad_instance.structure_designer.node_type_registry.get_node_type_for_node(node) {
            Some(nt) => nt,
            None => return None
          };
          let num_of_params = node_type.parameters.len();
          for i in 0..num_of_params {
            let param = &node_type.parameters[i];
            let data_type = &cad_instance.structure_designer.node_type_registry.get_node_param_data_type(node, i);
            input_pins.push(InputPinView {
              name: param.name.clone(),
              data_type: data_type.to_string(),
              multi: data_type.is_array(),
            });
          }

          // Collect validation errors for this node
          let mut error_messages = Vec::new();
          
          // Add validation errors from the node network
          for validation_error in &node_network.validation_errors {
            if validation_error.node_id == Some(node.id) {
              error_messages.push(validation_error.error_text.clone());
            }
          }
          
          // Only add evaluation errors if there are no validation errors in the entire network
          if node_network.validation_errors.is_empty() {
            if let Some(eval_error) = cad_instance.structure_designer.last_generated_structure_designer_scene.get_all_node_errors().get(&node.id) {
              error_messages.push(eval_error.clone());
            }
          }
          
          // Combine all errors with newline separator
          let error = if error_messages.is_empty() {
            None
          } else {
            Some(error_messages.join("\n"))
          };

          let output_string = cad_instance.structure_designer.last_generated_structure_designer_scene.get_all_node_output_strings().get(&node.id).cloned();

          // Collect connected input pin names for subtitle generation
          let mut connected_input_pins = std::collections::HashSet::new();
          for (param_index, argument) in node.arguments.iter().enumerate() {
            if !argument.is_empty() {
              if param_index < node_type.parameters.len() {
                connected_input_pins.insert(node_type.parameters[param_index].name.clone());
              }
            }
          }

          // Generate subtitle using the node's get_subtitle method
          let subtitle = node.data.get_subtitle(&connected_input_pins);

          // Extract comment-specific data if this is a Comment node
          let (comment_label, comment_text, comment_width, comment_height) = 
            if let Some(comment_data) = node.data.as_any_ref().downcast_ref::<CommentData>() {
              (
                Some(comment_data.label.clone()),
                Some(comment_data.text.clone()),
                Some(comment_data.width),
                Some(comment_data.height),
              )
            } else {
              (None, None, None, None)
            };

          let output_type = node_type.output_type.clone();
          let function_type = node_type.get_function_type();
          node_network_view.nodes.insert(node.id, NodeView {
            id: node.id,
            node_type_name: node.node_type_name.clone(),
            position: to_api_vec2(&node.position),
            input_pins,
            output_type: output_type.to_string(),
            function_type: function_type.to_string(),
            selected: node_network.is_node_selected(node.id),
            active: node_network.is_node_active(node.id),
            displayed: node_network.is_node_displayed(node.id),
            return_node: node_network.return_node_id == Some(node.id),
            error,
            output_string,
            subtitle,
            comment_label,
            comment_text,
            comment_width,
            comment_height,
          });
        }

        for (_id, node) in node_network.nodes.iter() {
          for (index, argument) in node.arguments.iter().enumerate() {
            for (argument_node_id, output_pin_index) in argument.argument_output_pins.iter() {
              node_network_view.wires.push(WireView {
                source_node_id: *argument_node_id,
                source_output_pin_index: *output_pin_index,
                dest_node_id: node.id,
                dest_param_index: index,
                selected: node_network.is_wire_selected(*argument_node_id, *output_pin_index, node.id, index),
              });
            }
          }
        }

        Some(node_network_view)
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn move_node(node_id: u64, position: APIVec2) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      cad_instance.structure_designer.move_node(node_id, from_api_vec2(&position));
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn add_node(node_type_name: &str, position: APIVec2) -> u64 {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        let ret = cad_instance.structure_designer.add_node(node_type_name, from_api_vec2(&position));
        refresh_structure_designer_auto(cad_instance);
        ret
      },
      0 // Default value if CAD_INSTANCE is None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn duplicate_node(node_id: u64) -> u64 {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        let ret = cad_instance.structure_designer.duplicate_node(node_id);
        refresh_structure_designer_auto(cad_instance);
        ret
      },
      0 // Default value if CAD_INSTANCE is None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn can_connect_nodes(source_node_id: u64, source_output_pin_index: i32, dest_node_id: u64, dest_param_index: usize) -> bool {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        cad_instance.structure_designer.can_connect_nodes(source_node_id, source_output_pin_index, dest_node_id, dest_param_index)
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn connect_nodes(source_node_id: u64, source_output_pin_index: i32, dest_node_id: u64, dest_param_index: usize) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      cad_instance.structure_designer.connect_nodes(source_node_id, source_output_pin_index, dest_node_id, dest_param_index);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

/// Auto-connects a source pin to the first compatible pin on a target node.
/// 
/// - `source_node_id`: The node where the wire was dragged from
/// - `source_pin_index`: The pin index on the source node
/// - `source_is_output`: true if dragging from output pin, false if from input pin
/// - `target_node_id`: The newly created node to connect to
/// 
/// Returns true if a connection was made, false otherwise.
#[flutter_rust_bridge::frb(sync)]
pub fn auto_connect_to_node(
  source_node_id: u64,
  source_pin_index: i32,
  source_is_output: bool,
  target_node_id: u64,
) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        let result = cad_instance.structure_designer.auto_connect_to_node(
          source_node_id,
          source_pin_index,
          source_is_output,
          target_node_id,
        );
        refresh_structure_designer_auto(cad_instance);
        result
      },
      false
    )
  }
}

/// Returns all compatible pins on the target node for auto-connection.
/// Each element contains (pin_index, pin_name, data_type_string).
/// When source_is_output is true, returns compatible INPUT pins on target.
/// When source_is_output is false, returns the OUTPUT pin if compatible.
#[flutter_rust_bridge::frb(sync)]
pub fn get_compatible_pins_for_auto_connect(
  source_node_id: u64,
  source_pin_index: i32,
  source_is_output: bool,
  target_node_id: u64,
) -> Vec<(i32, String, String)> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        cad_instance.structure_designer.get_compatible_pins_for_auto_connect(
          source_node_id,
          source_pin_index,
          source_is_output,
          target_node_id,
        )
      },
      Vec::new()
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_node_type_views() -> Option<Vec<APINodeCategoryView>> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        Some(cad_instance.structure_designer.node_type_registry.get_node_type_views())
      },
      None
    )
  }
}

/// Returns node types that have at least one pin compatible with the given type.
/// 
/// - `source_type_str`: The data type being dragged (serialized string, e.g., "Geometry", "Float")
/// - `dragging_from_output`: true if dragging from output pin, false if from input pin
/// 
/// When dragging from OUTPUT: find nodes with compatible INPUT pins
/// When dragging from INPUT: find nodes with compatible OUTPUT pins
#[flutter_rust_bridge::frb(sync)]
pub fn get_compatible_node_types(
  source_type_str: String,
  dragging_from_output: bool,
) -> Option<Vec<APINodeCategoryView>> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let source_type = DataType::from_string(&source_type_str).ok()?;
        Some(cad_instance.structure_designer.node_type_registry.get_compatible_node_types(
          &source_type,
          dragging_from_output,
        ))
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_node_network_names() -> Option<Vec<String>> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        Some(cad_instance.structure_designer.node_type_registry.get_node_network_names())
      },
      None
    )
  }
}

/// Checks if a node type name corresponds to a custom node (i.e., a user-defined node network).
/// Returns false if the CAD instance is not available.
#[flutter_rust_bridge::frb(sync)]
pub fn is_custom_node_type(node_type_name: String) -> bool {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        cad_instance.structure_designer.node_type_registry.is_custom_node_type(&node_type_name)
      },
      false
    )
  }
}

/// Gets the description of the active node network
#[flutter_rust_bridge::frb(sync)]
pub fn get_active_network_description() -> Option<String> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        cad_instance.structure_designer.get_active_network_description()
      },
      None
    )
  }
}

/// Sets the description of the active node network
#[flutter_rust_bridge::frb(sync)]
pub fn set_active_network_description(description: String) -> Result<(), String> {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        cad_instance.structure_designer.set_active_network_description(description)
      },
      Err("CAD instance not available".to_string())
    )
  }
}

/// Gets the summary of the active node network
#[flutter_rust_bridge::frb(sync)]
pub fn get_active_network_summary() -> Option<String> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        cad_instance.structure_designer.get_active_network_summary()
      },
      None
    )
  }
}

/// Sets the summary of the active node network
/// Pass None or empty string to clear the summary
#[flutter_rust_bridge::frb(sync)]
pub fn set_active_network_summary(summary: Option<String>) -> Result<(), String> {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        cad_instance.structure_designer.set_active_network_summary(summary)
      },
      Err("CAD instance not available".to_string())
    )
  }
}

/// Gets the description of a specific node network
#[flutter_rust_bridge::frb(sync)]
pub fn get_network_description(network_name: String) -> Option<String> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        cad_instance.structure_designer
          .get_network_description(&network_name)
          .map(|(_name, description)| description)
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_node_networks_with_validation() -> Option<Vec<APINetworkWithValidationErrors>> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        Some(cad_instance.structure_designer.node_type_registry.get_node_networks_with_validation())
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn add_new_node_network() {
  unsafe {
    with_mut_cad_instance(|instance| {
      instance.structure_designer.add_new_node_network();
    });
  }
}

/// Add a node network with a specific name.
/// Returns success/error. Auto-activates the new network.
#[flutter_rust_bridge::frb(sync)]
pub fn add_node_network_with_name(name: String) -> APIResult {
  unsafe {
    with_mut_cad_instance_or(
      |instance| {
        // Check if name already exists
        if instance.structure_designer.node_type_registry
            .node_networks.contains_key(&name) {
          return APIResult {
            success: false,
            error_message: format!("Network '{}' already exists", name),
          };
        }
        instance.structure_designer.add_node_network(&name);
        instance.structure_designer.set_active_node_network_name(Some(name));
        instance.structure_designer.set_dirty(true);
        refresh_structure_designer_auto(instance);
        APIResult { success: true, error_message: String::new() }
      },
      APIResult {
        success: false,
        error_message: "CAD instance not available".to_string(),
      }
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_active_node_network(node_network_name: &str) {
  unsafe {
    with_mut_cad_instance(|instance| {
      instance.structure_designer.set_active_node_network_name(Some(node_network_name.to_string()));
      refresh_structure_designer_auto(instance);
    });
  }
}

/// Navigates back in node network history
#[flutter_rust_bridge::frb(sync)]
pub fn navigate_back() -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |instance| {
        let result = instance.structure_designer.navigate_back();
        if result {
          refresh_structure_designer_auto(instance);
        }
        result
      },
      false
    )
  }
}

/// Navigates forward in node network history
#[flutter_rust_bridge::frb(sync)]
pub fn navigate_forward() -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |instance| {
        let result = instance.structure_designer.navigate_forward();
        if result {
          refresh_structure_designer_auto(instance);
        }
        result
      },
      false
    )
  }
}

/// Checks if we can navigate backward in node network history
#[flutter_rust_bridge::frb(sync)]
pub fn can_navigate_back() -> bool {
  unsafe {
    with_cad_instance_or(
      |instance| {
        instance.structure_designer.can_navigate_back()
      },
      false
    )
  }
}

/// Checks if we can navigate forward in node network history
#[flutter_rust_bridge::frb(sync)]
pub fn can_navigate_forward() -> bool {
  unsafe {
    with_cad_instance_or(
      |instance| {
        instance.structure_designer.can_navigate_forward()
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn rename_node_network(old_name: &str, new_name: &str) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |instance| {
        let result = instance.structure_designer.rename_node_network(old_name, new_name);
        refresh_structure_designer_auto(instance);
        result
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn delete_node_network(network_name: &str) -> APIResult {
  unsafe {
    with_mut_cad_instance_or(
      |instance| {
        let result = instance.structure_designer.delete_node_network(network_name);
        refresh_structure_designer_auto(instance);
        
        match result {
          Ok(_) => APIResult {
            success: true,
            error_message: String::new(),
          },
          Err(e) => APIResult {
            success: false,
            error_message: e,
          }
        }
      },
      APIResult {
        success: false,
        error_message: "CAD instance not available".to_string(),
      }
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_node_display(node_id: u64, is_displayed: bool) {
  unsafe {
    with_mut_cad_instance(|instance| {
      instance.structure_designer.set_node_display(node_id, is_displayed);
      refresh_structure_designer_auto(instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_node(node_id: u64) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |instance| {
        let result = instance.structure_designer.select_node(node_id);
        refresh_structure_designer_auto(instance);
        result
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_wire(source_node_id: u64, source_output_pin_index: i32, destination_node_id: u64, destination_argument_index: usize) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |instance| {
        let result = instance.structure_designer.select_wire(source_node_id, source_output_pin_index, destination_node_id, destination_argument_index);
        refresh_structure_designer_auto(instance);
        result
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn clear_selection() {
  unsafe {
    with_mut_cad_instance(|instance| {
      instance.structure_designer.clear_selection();
      refresh_structure_designer_auto(instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn toggle_node_selection(node_id: u64) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |instance| {
        let result = instance.structure_designer.toggle_node_selection(node_id);
        refresh_structure_designer_auto(instance);
        result
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn add_node_to_selection(node_id: u64) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |instance| {
        let result = instance.structure_designer.add_node_to_selection(node_id);
        refresh_structure_designer_auto(instance);
        result
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_nodes(node_ids: Vec<u64>) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |instance| {
        let result = instance.structure_designer.select_nodes(node_ids);
        refresh_structure_designer_auto(instance);
        result
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn toggle_nodes_selection(node_ids: Vec<u64>) {
  unsafe {
    with_mut_cad_instance(|instance| {
      instance.structure_designer.toggle_nodes_selection(node_ids);
      refresh_structure_designer_auto(instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_selected_node_ids() -> Vec<u64> {
  unsafe {
    with_cad_instance_or(
      |instance| {
        instance.structure_designer.get_selected_node_ids()
      },
      Vec::new()
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn move_selected_nodes(delta_x: f64, delta_y: f64) {
  unsafe {
    with_mut_cad_instance(|instance| {
      instance.structure_designer.move_selected_nodes(glam::f64::DVec2::new(delta_x, delta_y));
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn toggle_wire_selection(source_node_id: u64, source_output_pin_index: i32, destination_node_id: u64, destination_argument_index: usize) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |instance| {
        let result = instance.structure_designer.toggle_wire_selection(source_node_id, source_output_pin_index, destination_node_id, destination_argument_index);
        refresh_structure_designer_auto(instance);
        result
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn add_wire_to_selection(source_node_id: u64, source_output_pin_index: i32, destination_node_id: u64, destination_argument_index: usize) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |instance| {
        let result = instance.structure_designer.add_wire_to_selection(source_node_id, source_output_pin_index, destination_node_id, destination_argument_index);
        refresh_structure_designer_auto(instance);
        result
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_selected_wires() -> Vec<WireView> {
  unsafe {
    with_cad_instance_or(
      |instance| {
        instance.structure_designer.get_selected_wires()
          .into_iter()
          .map(|wire| WireView {
            source_node_id: wire.source_node_id,
            source_output_pin_index: wire.source_output_pin_index,
            dest_node_id: wire.destination_node_id,
            dest_param_index: wire.destination_argument_index,
            selected: true,
          })
          .collect()
      },
      Vec::new()
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn add_nodes_to_selection(node_ids: Vec<u64>) {
  unsafe {
    with_mut_cad_instance(|instance| {
      instance.structure_designer.add_nodes_to_selection(node_ids);
      refresh_structure_designer_auto(instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_wires(wires: Vec<super::structure_designer_api_types::WireIdentifier>) {
  unsafe {
    with_mut_cad_instance(|instance| {
      let wire_structs: Vec<crate::structure_designer::node_network::Wire> = wires
        .into_iter()
        .map(|w| crate::structure_designer::node_network::Wire {
          source_node_id: w.source_node_id,
          source_output_pin_index: w.source_output_pin_index,
          destination_node_id: w.destination_node_id,
          destination_argument_index: w.destination_argument_index,
        })
        .collect();
      instance.structure_designer.select_wires(wire_structs);
      refresh_structure_designer_auto(instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn add_wires_to_selection(wires: Vec<super::structure_designer_api_types::WireIdentifier>) {
  unsafe {
    with_mut_cad_instance(|instance| {
      let wire_structs: Vec<crate::structure_designer::node_network::Wire> = wires
        .into_iter()
        .map(|w| crate::structure_designer::node_network::Wire {
          source_node_id: w.source_node_id,
          source_output_pin_index: w.source_output_pin_index,
          destination_node_id: w.destination_node_id,
          destination_argument_index: w.destination_argument_index,
        })
        .collect();
      instance.structure_designer.add_wires_to_selection(wire_structs);
      refresh_structure_designer_auto(instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn toggle_wires_selection(wires: Vec<super::structure_designer_api_types::WireIdentifier>) {
  unsafe {
    with_mut_cad_instance(|instance| {
      let wire_structs: Vec<crate::structure_designer::node_network::Wire> = wires
        .into_iter()
        .map(|w| crate::structure_designer::node_network::Wire {
          source_node_id: w.source_node_id,
          source_output_pin_index: w.source_output_pin_index,
          destination_node_id: w.destination_node_id,
          destination_argument_index: w.destination_argument_index,
        })
        .collect();
      instance.structure_designer.toggle_wires_selection(wire_structs);
      refresh_structure_designer_auto(instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_nodes_and_wires(node_ids: Vec<u64>, wires: Vec<super::structure_designer_api_types::WireIdentifier>) {
  unsafe {
    with_mut_cad_instance(|instance| {
      let wire_structs: Vec<crate::structure_designer::node_network::Wire> = wires
        .into_iter()
        .map(|w| crate::structure_designer::node_network::Wire {
          source_node_id: w.source_node_id,
          source_output_pin_index: w.source_output_pin_index,
          destination_node_id: w.destination_node_id,
          destination_argument_index: w.destination_argument_index,
        })
        .collect();
      instance.structure_designer.select_nodes_and_wires(node_ids, wire_structs);
      refresh_structure_designer_auto(instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn add_nodes_and_wires_to_selection(node_ids: Vec<u64>, wires: Vec<super::structure_designer_api_types::WireIdentifier>) {
  unsafe {
    with_mut_cad_instance(|instance| {
      let wire_structs: Vec<crate::structure_designer::node_network::Wire> = wires
        .into_iter()
        .map(|w| crate::structure_designer::node_network::Wire {
          source_node_id: w.source_node_id,
          source_output_pin_index: w.source_output_pin_index,
          destination_node_id: w.destination_node_id,
          destination_argument_index: w.destination_argument_index,
        })
        .collect();
      instance.structure_designer.add_nodes_and_wires_to_selection(node_ids, wire_structs);
      refresh_structure_designer_auto(instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn toggle_nodes_and_wires_selection(node_ids: Vec<u64>, wires: Vec<super::structure_designer_api_types::WireIdentifier>) {
  unsafe {
    with_mut_cad_instance(|instance| {
      let wire_structs: Vec<crate::structure_designer::node_network::Wire> = wires
        .into_iter()
        .map(|w| crate::structure_designer::node_network::Wire {
          source_node_id: w.source_node_id,
          source_output_pin_index: w.source_output_pin_index,
          destination_node_id: w.destination_node_id,
          destination_argument_index: w.destination_argument_index,
        })
        .collect();
      instance.structure_designer.toggle_nodes_and_wires_selection(node_ids, wire_structs);
      refresh_structure_designer_auto(instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_extrude_data(node_id: u64) -> Option<APIExtrudeData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let extrude_data = match node_data.as_any_ref().downcast_ref::<ExtrudeData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIExtrudeData {
          height: extrude_data.height,
          extrude_direction: to_api_ivec3(&extrude_data.extrude_direction),
          infinite: extrude_data.infinite,
          subdivision: extrude_data.subdivision,
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_extrude_drawing_plane_miller_direction(node_id: u64) -> Option<crate::api::common_api_types::APIIVec3> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let selected_node_id = match cad_instance.structure_designer.get_selected_node_id_with_type("extrude") {
          Some(id) => id,
          None => return None,
        };

        if selected_node_id != node_id {
          return None;
        }

        let eval_cache = cad_instance.structure_designer.get_selected_node_eval_cache()?;
        let extrude_cache = eval_cache.downcast_ref::<ExtrudeEvalCache>()?;
        Some(to_api_ivec3(&extrude_cache.drawing_plane_miller_direction))
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_int_data(node_id: u64) -> Option<APIIntData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let int_data = match node_data.as_any_ref().downcast_ref::<IntData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIIntData {
          value: int_data.value
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_string_data(node_id: u64) -> Option<APIStringData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let string_data = match node_data.as_any_ref().downcast_ref::<StringData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIStringData {
          value: string_data.value.clone(),
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_bool_data(node_id: u64) -> Option<APIBoolData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let bool_data = match node_data.as_any_ref().downcast_ref::<BoolData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIBoolData {
          value: bool_data.value
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_float_data(node_id: u64) -> Option<APIFloatData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let float_data = match node_data.as_any_ref().downcast_ref::<FloatData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIFloatData {
          value: float_data.value
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_ivec2_data(node_id: u64) -> Option<APIIVec2Data> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let ivec2_data = match node_data.as_any_ref().downcast_ref::<IVec2Data>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIIVec2Data {
          value: to_api_ivec2(&ivec2_data.value)
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_ivec3_data(node_id: u64) -> Option<APIIVec3Data> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let ivec3_data = match node_data.as_any_ref().downcast_ref::<IVec3Data>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIIVec3Data {
          value: to_api_ivec3(&ivec3_data.value)
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_range_data(node_id: u64) -> Option<APIRangeData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let range_data = match node_data.as_any_ref().downcast_ref::<RangeData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIRangeData {
          start: range_data.start,
          step: range_data.step,
          count: range_data.count,
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_vec2_data(node_id: u64) -> Option<APIVec2Data> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let vec2_data = match node_data.as_any_ref().downcast_ref::<Vec2Data>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIVec2Data {
          value: to_api_vec2(&vec2_data.value)
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_vec3_data(node_id: u64) -> Option<APIVec3Data> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let vec3_data = match node_data.as_any_ref().downcast_ref::<Vec3Data>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIVec3Data {
          value: to_api_vec3(&vec3_data.value)
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_rect_data(node_id: u64) -> Option<APIRectData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let rect_data = match node_data.as_any_ref().downcast_ref::<RectData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIRectData {
          min_corner: to_api_ivec2(&rect_data.min_corner),
          extent: to_api_ivec2(&rect_data.extent),
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_reg_poly_data(node_id: u64) -> Option<APIRegPolyData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let reg_poly_data = match node_data.as_any_ref().downcast_ref::<RegPolyData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIRegPolyData {
          num_sides: reg_poly_data.num_sides,
          radius: reg_poly_data.radius,
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_circle_data(node_id: u64) -> Option<APICircleData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let circle_data = match node_data.as_any_ref().downcast_ref::<CircleData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APICircleData {
          center: to_api_ivec2(&circle_data.center),
          radius: circle_data.radius,
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_half_plane_data(node_id: u64) -> Option<APIHalfPlaneData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let half_plane_data = match node_data.as_any_ref().downcast_ref::<HalfPlaneData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIHalfPlaneData {
          point1: to_api_ivec2(&half_plane_data.point1),
          point2: to_api_ivec2(&half_plane_data.point2),
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_cuboid_data(node_id: u64) -> Option<APICuboidData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let cuboid_data = match node_data.as_any_ref().downcast_ref::<CuboidData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APICuboidData {
          min_corner: to_api_ivec3(&cuboid_data.min_corner),
          extent: to_api_ivec3(&cuboid_data.extent),
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_atom_cut_data(node_id: u64) -> Option<APIAtomCutData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let atom_cut_data = match node_data.as_any_ref().downcast_ref::<AtomCutData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIAtomCutData {
          cut_sdf_value: atom_cut_data.cut_sdf_value,
          unit_cell_size: atom_cut_data.unit_cell_size,
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_import_xyz_data(node_id: u64) -> Option<APIImportXYZData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let import_xyz_data = match node_data.as_any_ref().downcast_ref::<ImportXYZData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIImportXYZData {
          file_name: import_xyz_data.file_name.clone(),
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_export_xyz_data(node_id: u64) -> Option<APIExportXYZData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let export_xyz_data = match node_data.as_any_ref().downcast_ref::<ExportXYZData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIExportXYZData {
          file_name: export_xyz_data.file_name.clone(),
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_sphere_data(node_id: u64) -> Option<APISphereData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let sphere_data = match node_data.as_any_ref().downcast_ref::<SphereData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APISphereData {
          center: to_api_ivec3(&sphere_data.center),
          radius: sphere_data.radius,
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_half_space_data(node_id: u64) -> Option<APIHalfSpaceData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let half_space_data = match node_data.as_any_ref().downcast_ref::<HalfSpaceData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIHalfSpaceData {
          max_miller_index: half_space_data.max_miller_index,
          miller_index: to_api_ivec3(&half_space_data.miller_index),
          center: to_api_ivec3(&half_space_data.center),
          shift: half_space_data.shift,
          subdivision: half_space_data.subdivision,
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_drawing_plane_data(node_id: u64) -> Option<APIDrawingPlaneData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let drawing_plane_data = match node_data.as_any_ref().downcast_ref::<DrawingPlaneData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIDrawingPlaneData {
          max_miller_index: drawing_plane_data.max_miller_index,
          miller_index: to_api_ivec3(&drawing_plane_data.miller_index),
          center: to_api_ivec3(&drawing_plane_data.center),
          shift: drawing_plane_data.shift,
          subdivision: drawing_plane_data.subdivision,
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_geo_trans_data(node_id: u64) -> Option<APIGeoTransData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let geo_trans_data = match node_data.as_any_ref().downcast_ref::<GeoTransData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIGeoTransData {
          translation: to_api_ivec3(&geo_trans_data.translation),
          rotation: to_api_ivec3(&geo_trans_data.rotation),
          transform_only_frame: geo_trans_data.transform_only_frame,
        })
      },
      None
    )
  }
}

/// Helper function to convert CrystalSystem enum to string
fn crystal_system_to_string(crystal_system: CrystalSystem) -> String {
  match crystal_system {
    CrystalSystem::Cubic => "Cubic".to_string(),
    CrystalSystem::Tetragonal(_) => "Tetragonal".to_string(),
    CrystalSystem::Orthorhombic => "Orthorhombic".to_string(),
    CrystalSystem::Hexagonal(_) => "Hexagonal".to_string(),
    CrystalSystem::Trigonal => "Trigonal".to_string(),
    CrystalSystem::Monoclinic(_) => "Monoclinic".to_string(),
    CrystalSystem::Triclinic => "Triclinic".to_string(),
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_lattice_symop_data(node_id: u64) -> Option<APILatticeSymopData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let lattice_symop_data = match node_data.as_any_ref().downcast_ref::<LatticeSymopData>() {
          Some(data) => data,
          None => return None,
        };
        
        // Try to get the evaluation cache to access unit cell and compute symmetries and crystal system
        let (api_symmetries, crystal_system_str) = if let Some(eval_cache) = cad_instance.structure_designer.get_selected_node_eval_cache() {
          if let Some(lattice_symop_cache) = eval_cache.downcast_ref::<LatticeSymopEvalCache>() {
            // Analyze unit cell symmetries and crystal system
            let (crystal_system, symmetries) = analyze_unit_cell_complete(&lattice_symop_cache.unit_cell);
            
            // Convert symmetries to API format
            let api_symmetries = symmetries.into_iter().map(|sym| APIRotationalSymmetry {
              axis: to_api_vec3(&sym.axis),
              n_fold: sym.n_fold,
            }).collect();
            
            (api_symmetries, crystal_system_to_string(crystal_system))
          } else {
            // No lattice symop cache available - return empty symmetries and unknown crystal system
            (Vec::new(), "Unknown".to_string())
          }
        } else {
          // No evaluation cache available - return empty symmetries and unknown crystal system
          (Vec::new(), "Unknown".to_string())
        };
        
        Some(APILatticeSymopData {
          translation: to_api_ivec3(&lattice_symop_data.translation),
          rotation_axis: lattice_symop_data.rotation_axis.map(|axis| to_api_vec3(&axis)),
          rotation_angle_degrees: lattice_symop_data.rotation_angle_degrees,
          transform_only_frame: lattice_symop_data.transform_only_frame,
          rotational_symmetries: api_symmetries,
          crystal_system: crystal_system_str,
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_lattice_move_data(node_id: u64) -> Option<APILatticeMoveData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let lattice_move_data = match node_data.as_any_ref().downcast_ref::<LatticeMoveData>() {
          Some(data) => data,
          None => return None,
        };
        
        Some(APILatticeMoveData {
          translation: to_api_ivec3(&lattice_move_data.translation),
          lattice_subdivision: lattice_move_data.lattice_subdivision,
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_lattice_rot_data(node_id: u64) -> Option<APILatticeRotData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let lattice_rot_data = match node_data.as_any_ref().downcast_ref::<LatticeRotData>() {
          Some(data) => data,
          None => return None,
        };
        
        // Try to get the evaluation cache to access unit cell and compute symmetries and crystal system
        let (api_symmetries, crystal_system_str) = if let Some(eval_cache) = cad_instance.structure_designer.get_selected_node_eval_cache() {
          if let Some(lattice_rot_cache) = eval_cache.downcast_ref::<LatticeRotEvalCache>() {
            // Analyze unit cell symmetries and crystal system
            let (crystal_system, symmetries) = analyze_unit_cell_complete(&lattice_rot_cache.unit_cell);
            
            // Convert symmetries to API format
            let api_symmetries = symmetries.into_iter().map(|sym| APIRotationalSymmetry {
              axis: to_api_vec3(&sym.axis),
              n_fold: sym.n_fold,
            }).collect();
            
            (api_symmetries, crystal_system_to_string(crystal_system))
          } else {
            // No lattice rot cache available - return empty symmetries and unknown crystal system
            (Vec::new(), "Unknown".to_string())
          }
        } else {
          // No evaluation cache available - return empty symmetries and unknown crystal system
          (Vec::new(), "Unknown".to_string())
        };
        
        Some(APILatticeRotData {
          axis_index: lattice_rot_data.axis_index,
          step: lattice_rot_data.step,
          pivot_point: to_api_ivec3(&lattice_rot_data.pivot_point),
          rotational_symmetries: api_symmetries,
          crystal_system: crystal_system_str,
        })
      },
      None
    )
  }
}


#[flutter_rust_bridge::frb(sync)]
pub fn get_atom_move_data(node_id: u64) -> Option<APIAtomMoveData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let atom_move_data = match node_data.as_any_ref().downcast_ref::<AtomMoveData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIAtomMoveData {
          translation: to_api_vec3(&atom_move_data.translation),
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_atom_rot_data(node_id: u64) -> Option<APIAtomRotData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let atom_rot_data = match node_data.as_any_ref().downcast_ref::<AtomRotData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIAtomRotData {
          angle: atom_rot_data.angle,
          rot_axis: to_api_vec3(&atom_rot_data.rot_axis),
          pivot_point: to_api_vec3(&atom_rot_data.pivot_point),
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_atom_trans_data(node_id: u64) -> Option<APIAtomTransData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let atom_trans_data = match node_data.as_any_ref().downcast_ref::<AtomTransData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIAtomTransData {
          translation: to_api_vec3(&atom_trans_data.translation),
          rotation: to_api_vec3(&atom_trans_data.rotation),
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_edit_atom_data(node_id: u64) -> Option<APIEditAtomData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let edit_atom_data = match node_data.as_any_ref().downcast_ref::<EditAtomData>() {
          Some(data) => data,
          None => return None,
        };
        
        // Get the appropriate values based on the active tool
        let (replacement_atomic_number, add_atom_tool_atomic_number, bond_tool_last_atom_id) = match &edit_atom_data.active_tool {
          EditAtomTool::Default(state) => (
            Some(state.replacement_atomic_number),
            None,
            None
          ),
          EditAtomTool::AddAtom(state) => (
            None,
            Some(state.atomic_number),
            None
          ),
          EditAtomTool::AddBond(state) => (
            None,
            None,
            state.last_atom_id
          ),
        };
        
        // Get the atomic structure from the selected node to check for selections
        let atomic_structure = cad_instance.structure_designer.get_atomic_structure_from_selected_node();
        
        // Default values if no atomic structure is found
        let has_selected_atoms = atomic_structure.map_or(false, |structure| structure.has_selected_atoms());
        let has_selection = atomic_structure.map_or(false, |structure| structure.has_selection());
        
        Some(APIEditAtomData {
          active_tool: edit_atom_data.get_active_tool(),
          can_undo: edit_atom_data.can_undo(),
          can_redo: edit_atom_data.can_redo(),
          bond_tool_last_atom_id,
          replacement_atomic_number,
          add_atom_tool_atomic_number,
          has_selected_atoms,
          has_selection,
          selection_transform: edit_atom_data.selection_transform.as_ref().map(|transform| crate::api::api_common::to_api_transform(transform))
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_parameter_data(node_id: u64) -> Option<APIParameterData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance.structure_designer.get_node_network_data(node_id)?;
                let parameter_data = node_data.as_any_ref().downcast_ref::<ParameterData>()?;

                let api_data_type = if parameter_data.data_type == DataType::None {
                    if let Some(dt_str) = &parameter_data.data_type_str {
                        // If parsing failed, reconstruct the APIDataType from the stored string
                        APIDataType {
                            data_type_base: APIDataTypeBase::Custom,
                            custom_data_type: Some(dt_str.clone()),
                            array: false, // This is inferred from the custom string itself
                        }
                    } else {
                        // Fallback for safety
                        data_type_to_api_data_type(&parameter_data.data_type)
                    }
                } else {
                    // If parsing succeeded, convert as usual
                    data_type_to_api_data_type(&parameter_data.data_type)
                };

                Some(APIParameterData {
                    param_index: parameter_data.param_index,
                    param_name: parameter_data.param_name.clone(),
                    data_type: api_data_type,
                    sort_order: parameter_data.sort_order,
                    error: parameter_data.error.clone(),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_expr_data(node_id: u64) -> Option<APIExprData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let expr_data = match node_data.as_any_ref().downcast_ref::<ExprData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIExprData {
            parameters: expr_data.parameters.iter().map(|param| {
                let api_data_type = if param.data_type == DataType::None {
                    if let Some(dt_str) = &param.data_type_str {
                        // If parsing failed, reconstruct the APIDataType from the stored string
                        APIDataType {
                            data_type_base: APIDataTypeBase::Custom,
                            custom_data_type: Some(dt_str.clone()),
                            array: false, // This is inferred from the custom string itself
                        }
                    } else {
                        // Fallback for safety, though this case should ideally not happen
                        data_type_to_api_data_type(&param.data_type)
                    }
                } else {
                    // If parsing succeeded, convert as usual
                    data_type_to_api_data_type(&param.data_type)
                };

                APIExprParameter {
                    name: param.name.clone(),
                    data_type: api_data_type,
                }
            }).collect(),
            expression: expr_data.expression.clone(),
            error: expr_data.error.clone(),
            output_type: expr_data.output_type.as_ref().map(data_type_to_api_data_type),
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_map_data(node_id: u64) -> Option<APIMapData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = cad_instance.structure_designer.get_node_network_data(node_id)?;
        let map_data = node_data.as_any_ref().downcast_ref::<MapData>()?;

        Some(APIMapData {
          input_type: data_type_to_api_data_type(&map_data.input_type),
          output_type: data_type_to_api_data_type(&map_data.output_type),
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_motif_data(node_id: u64) -> Option<APIMotifData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = cad_instance.structure_designer.get_node_network_data(node_id)?;
        let motif_data = node_data.as_any_ref().downcast_ref::<MotifData>()?;

        Some(APIMotifData {
          definition: motif_data.definition.clone(),
          name: motif_data.name.clone(),
          error: motif_data.error.clone(),
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_int_data(node_id: u64, data: APIIntData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let int_data = Box::new(IntData {
        value: data.value,
      });
      cad_instance.structure_designer.set_node_network_data(node_id, int_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_string_data(node_id: u64, data: APIStringData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let string_data = Box::new(StringData {
        value: data.value,
      });
      cad_instance.structure_designer.set_node_network_data(node_id, string_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_bool_data(node_id: u64, data: APIBoolData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let bool_data = Box::new(BoolData {
        value: data.value,
      });
      cad_instance.structure_designer.set_node_network_data(node_id, bool_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_float_data(node_id: u64, data: APIFloatData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let float_data = Box::new(FloatData {
        value: data.value,
      });
      cad_instance.structure_designer.set_node_network_data(node_id, float_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_vec2_data(node_id: u64, data: APIVec2Data) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let vec2_data = Box::new(Vec2Data {
        value: from_api_vec2(&data.value),
      });
      cad_instance.structure_designer.set_node_network_data(node_id, vec2_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_vec3_data(node_id: u64, data: APIVec3Data) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let vec3_data = Box::new(Vec3Data {
        value: from_api_vec3(&data.value),
      });
      cad_instance.structure_designer.set_node_network_data(node_id, vec3_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_ivec2_data(node_id: u64, data: APIIVec2Data) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let ivec2_data = Box::new(IVec2Data {
        value: from_api_ivec2(&data.value),
      });
      cad_instance.structure_designer.set_node_network_data(node_id, ivec2_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_ivec3_data(node_id: u64, data: APIIVec3Data) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let ivec3_data = Box::new(IVec3Data {
        value: from_api_ivec3(&data.value),
      });
      cad_instance.structure_designer.set_node_network_data(node_id, ivec3_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_range_data(node_id: u64, data: APIRangeData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let range_data = Box::new(RangeData {
        start: data.start,
        step: data.step,
        count: data.count,
      });
      cad_instance.structure_designer.set_node_network_data(node_id, range_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_rect_data(node_id: u64, data: APIRectData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let rect_data = Box::new(RectData {
        min_corner: from_api_ivec2(&data.min_corner),
        extent: from_api_ivec2(&data.extent),
      });
      cad_instance.structure_designer.set_node_network_data(node_id, rect_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_reg_poly_data(node_id: u64, data: APIRegPolyData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      if let Some(node_data) = cad_instance.structure_designer.get_node_network_data_mut(node_id) {
        if let Some(reg_poly_data) = node_data.as_any_mut().downcast_mut::<RegPolyData>() {
          reg_poly_data.num_sides = data.num_sides;
          reg_poly_data.radius = data.radius;
          refresh_structure_designer_auto(cad_instance);
        }
      }
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_circle_data(node_id: u64, data: APICircleData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let circle_data = Box::new(CircleData {
        center: from_api_ivec2(&data.center),
        radius: data.radius,
      });
      cad_instance.structure_designer.set_node_network_data(node_id, circle_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}



#[flutter_rust_bridge::frb(sync)]
pub fn set_half_plane_data(node_id: u64, data: APIHalfPlaneData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let half_plane_data = Box::new(HalfPlaneData {
        point1: from_api_ivec2(&data.point1),
        point2: from_api_ivec2(&data.point2),
      });
      cad_instance.structure_designer.set_node_network_data(node_id, half_plane_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_extrude_data(node_id: u64, data: APIExtrudeData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let extrude_data = Box::new(ExtrudeData {
        height: data.height,
        extrude_direction: from_api_ivec3(&data.extrude_direction),
        infinite: data.infinite,
        subdivision: data.subdivision,
      });
      cad_instance.structure_designer.set_node_network_data(node_id, extrude_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_cuboid_data(node_id: u64, data: APICuboidData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let cuboid_data = Box::new(CuboidData {
        min_corner: from_api_ivec3(&data.min_corner),
        extent: from_api_ivec3(&data.extent),
      });
      cad_instance.structure_designer.set_node_network_data(node_id, cuboid_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_sphere_data(node_id: u64, data: APISphereData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let sphere_data = Box::new(SphereData {
        center: from_api_ivec3(&data.center),
        radius: data.radius,
      });
      cad_instance.structure_designer.set_node_network_data(node_id, sphere_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_half_space_data(node_id: u64, data: APIHalfSpaceData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let half_space_data = Box::new(HalfSpaceData {
        max_miller_index: data.max_miller_index,
        miller_index: from_api_ivec3(&data.miller_index),
        center: from_api_ivec3(&data.center),
        shift: data.shift,
        subdivision: data.subdivision,
      });
      cad_instance.structure_designer.set_node_network_data(node_id, half_space_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_drawing_plane_data(node_id: u64, data: APIDrawingPlaneData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let drawing_plane_data = Box::new(DrawingPlaneData {
        max_miller_index: data.max_miller_index,
        miller_index: from_api_ivec3(&data.miller_index),
        center: from_api_ivec3(&data.center),
        shift: data.shift,
        subdivision: data.subdivision,
      });
      cad_instance.structure_designer.set_node_network_data(node_id, drawing_plane_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_geo_trans_data(node_id: u64, data: APIGeoTransData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let geo_trans_data = Box::new(GeoTransData {
        transform_only_frame: data.transform_only_frame,
        translation: from_api_ivec3(&data.translation),
        rotation: from_api_ivec3(&data.rotation),
      });
      cad_instance.structure_designer.set_node_network_data(node_id, geo_trans_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_lattice_symop_data(node_id: u64, data: APILatticeSymopData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let lattice_symop_data = Box::new(LatticeSymopData {
        translation: from_api_ivec3(&data.translation),
        rotation_axis: data.rotation_axis.map(|axis| from_api_vec3(&axis)),
        rotation_angle_degrees: data.rotation_angle_degrees,
        transform_only_frame: data.transform_only_frame,
      });
      cad_instance.structure_designer.set_node_network_data(node_id, lattice_symop_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_lattice_move_data(node_id: u64, data: APILatticeMoveData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let lattice_move_data = Box::new(LatticeMoveData {
        translation: from_api_ivec3(&data.translation),
        lattice_subdivision: data.lattice_subdivision,
      });
      cad_instance.structure_designer.set_node_network_data(node_id, lattice_move_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_lattice_rot_data(node_id: u64, data: APILatticeRotData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let lattice_rot_data = Box::new(LatticeRotData {
        axis_index: data.axis_index,
        step: data.step,
        pivot_point: from_api_ivec3(&data.pivot_point),
      });
      cad_instance.structure_designer.set_node_network_data(node_id, lattice_rot_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_atom_move_data(node_id: u64, data: APIAtomMoveData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let atom_move_data = Box::new(AtomMoveData {
        translation: from_api_vec3(&data.translation),
      });
      cad_instance.structure_designer.set_node_network_data(node_id, atom_move_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_atom_rot_data(node_id: u64, data: APIAtomRotData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let atom_rot_data = Box::new(AtomRotData {
        angle: data.angle,
        rot_axis: from_api_vec3(&data.rot_axis),
        pivot_point: from_api_vec3(&data.pivot_point),
      });
      cad_instance.structure_designer.set_node_network_data(node_id, atom_rot_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_atom_trans_data(node_id: u64, data: APIAtomTransData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let atom_trans_data = Box::new(AtomTransData {
        translation: from_api_vec3(&data.translation),
        rotation: from_api_vec3(&data.rotation),
      });
      cad_instance.structure_designer.set_node_network_data(node_id, atom_trans_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_atom_cut_data(node_id: u64, data: APIAtomCutData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let atom_cut_data = Box::new(AtomCutData {
        cut_sdf_value: data.cut_sdf_value,
        unit_cell_size: data.unit_cell_size,
      });
      cad_instance.structure_designer.set_node_network_data(node_id, atom_cut_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_import_xyz_data(node_id: u64, data: APIImportXYZData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let import_xyz_data = Box::new(ImportXYZData {
        file_name: data.file_name.clone(),
        atomic_structure: None,
      });
      cad_instance.structure_designer.set_node_network_data(node_id, import_xyz_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_export_xyz_data(node_id: u64, data: APIExportXYZData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let export_xyz_data = Box::new(ExportXYZData {
        file_name: data.file_name.clone(),
      });
      cad_instance.structure_designer.set_node_network_data(node_id, export_xyz_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}


#[flutter_rust_bridge::frb(sync)]
pub fn set_parameter_data(node_id: u64, data: APIParameterData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let (data_type, data_type_str, error) = match api_data_type_to_data_type(&data.data_type) {
                Ok(parsed_data_type) => (parsed_data_type, None, None),
                Err(e) => (
                    DataType::None, // Set to None on error
                    data.data_type.custom_data_type, // Preserve the original string
                    Some(e),
                ),
            };

            // Preserve existing param_id from the current node data (for wire preservation across renames)
            let existing_param_id = cad_instance.structure_designer.get_active_node_network()
                .and_then(|network| network.nodes.get(&node_id))
                .and_then(|node| node.data.as_any_ref().downcast_ref::<ParameterData>())
                .and_then(|param_data| param_data.param_id);

            let parameter_data = Box::new(ParameterData {
                param_id: existing_param_id,  // Preserve existing ID for wire preservation
                param_index: data.param_index,
                param_name: data.param_name,
                data_type,
                sort_order: data.sort_order,
                data_type_str,
                error,
            });

            cad_instance.structure_designer.set_node_network_data(node_id, parameter_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_map_data(node_id: u64, data: APIMapData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let input_type = match api_data_type_to_data_type(&data.input_type) {
                Ok(parsed_data_type) => parsed_data_type,
                Err(_) => DataType::None, // Fallback to None on error
            };

            let output_type = match api_data_type_to_data_type(&data.output_type) {
                Ok(parsed_data_type) => parsed_data_type,
                Err(_) => DataType::None, // Fallback to None on error
            };

            let map_data = Box::new(MapData {
                input_type,
                output_type,
            });

            cad_instance.structure_designer.set_node_network_data(node_id, map_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_expr_data(node_id: u64, data: APIExprData) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                // Get existing expr data to preserve parameter IDs for wire preservation
                let existing_params: Vec<crate::structure_designer::nodes::expr::ExprParameter> = cad_instance.structure_designer.get_active_node_network()
                    .and_then(|network| network.nodes.get(&node_id))
                    .and_then(|node| node.data.as_any_ref().downcast_ref::<ExprData>())
                    .map(|expr_data| expr_data.parameters.clone())
                    .unwrap_or_default();

                // Find the next available ID for new parameters
                let mut next_id = existing_params.iter()
                    .filter_map(|p| p.id)
                    .max()
                    .unwrap_or(0) + 1;

                // Track which IDs have been assigned to avoid duplicates
                let mut used_ids = std::collections::HashSet::new();

                let mut parameters = Vec::new();
                let mut first_error = None;

                for (new_index, api_param) in data.parameters.into_iter().enumerate() {
                    // Preserve ID: first match by name (handles reordering), then by position (handles renames)
                    // Also check that the ID hasn't already been used (prevents duplicates when reordering + adding)
                    let id = if let Some(existing) = existing_params.iter().find(|p| p.name == api_param.name) {
                        // Match by name first (handles reordering)
                        if let Some(existing_id) = existing.id {
                            if !used_ids.contains(&existing_id) {
                                existing.id
                            } else {
                                // ID already used, generate new one
                                let id = next_id;
                                next_id += 1;
                                Some(id)
                            }
                        } else {
                            existing.id
                        }
                    } else if new_index < existing_params.len() && existing_params[new_index].id.is_some() {
                        // Fall back to position (handles renames - name changed but position same)
                        let pos_id = existing_params[new_index].id.unwrap();
                        if !used_ids.contains(&pos_id) {
                            existing_params[new_index].id
                        } else {
                            // ID already used, generate new one
                            let id = next_id;
                            next_id += 1;
                            Some(id)
                        }
                    } else {
                        // New parameter - generate new ID
                        let id = next_id;
                        next_id += 1;
                        Some(id)
                    };

                    // Track the assigned ID
                    if let Some(assigned_id) = id {
                        used_ids.insert(assigned_id);
                    }

                    match api_data_type_to_data_type(&api_param.data_type) {
                        Ok(dt) => {
                            parameters.push(crate::structure_designer::nodes::expr::ExprParameter {
                                id,
                                name: api_param.name,
                                data_type: dt,
                                data_type_str: None, // Successfully parsed, no need to store the string
                            });
                        }
                        Err(e) => {
                            if first_error.is_none() {
                                first_error = Some(e.clone());
                            }
                            parameters.push(crate::structure_designer::nodes::expr::ExprParameter {
                                id,
                                name: api_param.name,
                                data_type: DataType::None, // Set to None on error
                                data_type_str: if api_param.data_type.data_type_base == APIDataTypeBase::Custom {
                                    api_param.data_type.custom_data_type
                                } else {
                                    None
                                },
                            });
                        }
                    }
                }

                let expr_data = Box::new(ExprData {
                    parameters,
                    expression: data.expression,
                    expr: None,
                    error: first_error,
                    output_type: None,
                });

                cad_instance.structure_designer.set_node_network_data(node_id, expr_data);
                refresh_structure_designer_auto(cad_instance);

                APIResult { success: true, error_message: String::new() }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_motif_data(node_id: u64, data: APIMotifData) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let mut motif_data = Box::new(MotifData {
                    definition: data.definition,
                    name: data.name,
                    motif: None,
                    error: None,
                });
                motif_data.parse_and_validate(node_id);
                cad_instance.structure_designer.set_node_network_data(node_id, motif_data);
                refresh_structure_designer_auto(cad_instance);

                APIResult { success: true, error_message: String::new() }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_atom_fill_data(node_id: u64) -> Option<APIAtomFillData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = cad_instance.structure_designer.get_node_network_data(node_id)?;
        let atom_fill_data = node_data.as_any_ref().downcast_ref::<AtomFillData>()?;

        Some(APIAtomFillData {
          parameter_element_value_definition: atom_fill_data.parameter_element_value_definition.clone(),
          motif_offset: to_api_vec3(&atom_fill_data.motif_offset),
          hydrogen_passivation: atom_fill_data.hydrogen_passivation,
          remove_single_bond_atoms_before_passivation: atom_fill_data.remove_single_bond_atoms_before_passivation,
          surface_reconstruction: atom_fill_data.surface_reconstruction,
          invert_phase: atom_fill_data.invert_phase,
          error: atom_fill_data.error.clone(),
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_atom_fill_data(node_id: u64, data: APIAtomFillData) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let mut atom_fill_data = Box::new(AtomFillData {
                    parameter_element_value_definition: data.parameter_element_value_definition,
                    motif_offset: from_api_vec3(&data.motif_offset),
                    hydrogen_passivation: data.hydrogen_passivation,
                    remove_single_bond_atoms_before_passivation: data.remove_single_bond_atoms_before_passivation,
                    surface_reconstruction: data.surface_reconstruction,
                    invert_phase: data.invert_phase,
                    error: None,
                    parameter_element_values: HashMap::new(),
                });
                atom_fill_data.parse_and_validate(node_id);
                cad_instance.structure_designer.set_node_network_data(node_id, atom_fill_data);
                refresh_structure_designer_auto(cad_instance);

                APIResult { success: true, error_message: String::new() }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn delete_selected() {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      cad_instance.structure_designer.delete_selected();
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_return_node_id(node_id: Option<u64>) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        let result = cad_instance.structure_designer.set_return_node_id(node_id);
        refresh_structure_designer_auto(cad_instance);
        result
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn save_node_networks_as(file_path: String) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance | {
        // Call the method in StructureDesigner
        match cad_instance.structure_designer.save_node_networks_as(&file_path) {
          Ok(_) => true,
          Err(_) => false
        }
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn save_node_networks() -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance | {
        // Call the method in StructureDesigner
        match cad_instance.structure_designer.save_node_networks() {
          Some(Ok(_)) => true,
          Some(Err(_)) => false,
          None => false, // No file path available
        }
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn is_design_dirty() -> bool {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        cad_instance.structure_designer.is_dirty()
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_design_file_path() -> Option<String> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        cad_instance.structure_designer.get_file_path().cloned()
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn load_node_networks(file_path: String) -> APIResult {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        // Call the method in StructureDesigner
        let result = cad_instance.structure_designer.load_node_networks(&file_path);
        
        print!("Result: {:?}", result);

        // Refresh the renderer to reflect any loaded structures (even if there was an error)
        refresh_structure_designer_auto(cad_instance);
        
        match result {
          Ok(_) => APIResult {
            success: true,
            error_message: String::new(),
          },
          Err(e) => APIResult {
            success: false,
            error_message: e.to_string(),
          }
        }
      },
      APIResult {
        success: false,
        error_message: "CAD instance not available".to_string(),
      }
    )
  }
}

/// Creates a new empty project, clearing all networks and resetting state.
///
/// This is equivalent to File > New:
/// - Clears all networks
/// - Creates a fresh "Main" network
/// - Clears the file path
/// - Clears the dirty flag
#[flutter_rust_bridge::frb(sync)]
pub fn new_project() {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      cad_instance.structure_designer.new_project();
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

/// Returns the number of node networks in the current project.
#[flutter_rust_bridge::frb(sync)]
pub fn get_network_count() -> i32 {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        cad_instance.structure_designer.node_type_registry.node_networks.len() as i32
      },
      0
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn is_node_type_active(node_type: String) -> bool {
  unsafe {
    with_cad_instance_or(
      |cad_instance| cad_instance.structure_designer.is_node_type_active(&node_type),
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_api_data_type_display_name(api_data_type: APIDataType) -> String {
    match api_data_type_to_data_type(&api_data_type) {
        Ok(data_type) => data_type.to_string(),
        Err(_) => api_data_type.custom_data_type.unwrap_or_else(|| "Invalid Type".to_string()),
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_structure_designer_preferences() -> Option<StructureDesignerPreferences> {
  unsafe {
    with_cad_instance(|cad_instance| {
      cad_instance.structure_designer.preferences.clone()
    })
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_structure_designer_preferences(preferences: StructureDesignerPreferences) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      cad_instance.structure_designer.set_preferences(preferences);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn export_visible_atomic_structures(file_path: String) -> APIResult {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        // Call the method in StructureDesigner
        match cad_instance.structure_designer.export_visible_atomic_structures(&file_path) {
          Ok(_) => APIResult {
            success: true,
            error_message: String::new(),
          },
          Err(e) => APIResult {
            success: false,
            error_message: e,
          }
        }
      },
      APIResult {
        success: false,
        error_message: "CAD instance not available".to_string(),
      }
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_unit_cell_data(node_id: u64) -> Option<APIUnitCellData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let unit_cell_data = match node_data.as_any_ref().downcast_ref::<UnitCellData>() {
          Some(data) => data,
          None => return None,
        };
        // Convert to UnitCellStruct and detect crystal system
        let unit_cell_struct = unit_cell_data.to_unit_cell_struct();
        let crystal_system = classify_crystal_system(&unit_cell_struct);
        let crystal_system_str = crystal_system_to_string(crystal_system);
        
        Some(APIUnitCellData {
          cell_length_a: unit_cell_data.cell_length_a,
          cell_length_b: unit_cell_data.cell_length_b,
          cell_length_c: unit_cell_data.cell_length_c,
          cell_angle_alpha: unit_cell_data.cell_angle_alpha,
          cell_angle_beta: unit_cell_data.cell_angle_beta,
          cell_angle_gamma: unit_cell_data.cell_angle_gamma,
          crystal_system: crystal_system_str,
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_unit_cell_data(node_id: u64, data: APIUnitCellData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let unit_cell_data = Box::new(UnitCellData {
        cell_length_a: data.cell_length_a,
        cell_length_b: data.cell_length_b,
        cell_length_c: data.cell_length_c,
        cell_angle_alpha: data.cell_angle_alpha,
        cell_angle_beta: data.cell_angle_beta,
        cell_angle_gamma: data.cell_angle_gamma,
      });
      cad_instance.structure_designer.set_node_network_data(node_id, unit_cell_data);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn validate_active_network() {
  unsafe {
    with_mut_cad_instance(|instance| {
      instance.structure_designer.validate_active_network();
      refresh_structure_designer_auto(instance);
    });
  }
}

/// Run atomCAD in headless CLI mode with a single configuration
#[flutter_rust_bridge::frb(sync)]
pub fn run_cli_single(config: super::structure_designer_api_types::CliConfig) -> APIResult {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        match cli_runner::run_cli_single_mode(&mut cad_instance.structure_designer, config) {
          Ok(_) => APIResult {
            success: true,
            error_message: String::new(),
          },
          Err(e) => APIResult {
            success: false,
            error_message: e,
          }
        }
      },
      APIResult {
        success: false,
        error_message: "CAD instance not available".to_string(),
      }
    )
  }
}

/// Run atomCAD in headless CLI batch mode
#[flutter_rust_bridge::frb(sync)]
pub fn run_cli_batch(config: super::structure_designer_api_types::BatchCliConfig) -> APIResult {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        match cli_runner::run_cli_batch_mode(&mut cad_instance.structure_designer, config) {
          Ok(_) => APIResult {
            success: true,
            error_message: String::new(),
          },
          Err(e) => APIResult {
            success: false,
            error_message: e,
          }
        }
      },
      APIResult {
        success: false,
        error_message: "CAD instance not available".to_string(),
      }
    )
  }
}

/// Resize a comment node
#[flutter_rust_bridge::frb(sync)]
pub fn resize_comment_node(node_id: u64, width: f64, height: f64) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      if let Some(network_name) = &cad_instance.structure_designer.active_node_network_name.clone() {
        if let Some(network) = cad_instance.structure_designer.node_type_registry.node_networks.get_mut(network_name) {
          if let Some(node) = network.nodes.get_mut(&node_id) {
            if let Some(comment_data) = node.data.as_any_mut().downcast_mut::<CommentData>() {
              comment_data.width = width.max(100.0);
              comment_data.height = height.max(60.0);
            }
          }
        }
      }
    });
  }
}

/// Update a comment node's label and text
#[flutter_rust_bridge::frb(sync)]
pub fn update_comment_node(node_id: u64, label: String, text: String) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      if let Some(network_name) = &cad_instance.structure_designer.active_node_network_name.clone() {
        if let Some(network) = cad_instance.structure_designer.node_type_registry.node_networks.get_mut(network_name) {
          if let Some(node) = network.nodes.get_mut(&node_id) {
            if let Some(comment_data) = node.data.as_any_mut().downcast_mut::<CommentData>() {
              comment_data.label = label;
              comment_data.text = text;
            }
          }
        }
      }
    });
  }
}

/// Get comment node data for property panel editing
#[flutter_rust_bridge::frb(sync)]
pub fn get_comment_data(node_id: u64) -> Option<APICommentData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = cad_instance.structure_designer.get_node_network_data(node_id)?;
        let comment_data = node_data.as_any_ref().downcast_ref::<CommentData>()?;

        Some(APICommentData {
          label: comment_data.label.clone(),
          text: comment_data.text.clone(),
          width: comment_data.width,
          height: comment_data.height,
        })
      },
      None
    )
  }
}

/// Evaluate a node and return its result string.
///
/// # Arguments
/// * `node_identifier` - Either a numeric node ID or the node's custom name
/// * `verbose` - If true, return detailed output for complex types
///
/// # Returns
/// * `Ok(APINodeEvaluationResult)` - The evaluation result
/// * `Err(String)` - If node not found or evaluation fails
#[flutter_rust_bridge::frb(sync)]
pub fn evaluate_node(
  node_identifier: String,
  verbose: bool,
) -> Result<APINodeEvaluationResult, String> {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        let designer = &mut cad_instance.structure_designer;

        // Try parsing as numeric ID first, then fall back to name lookup
        let node_id = node_identifier.parse::<u64>()
          .ok()
          .or_else(|| designer.find_node_id_by_name(&node_identifier))
          .ok_or_else(|| format!("Node not found: {}", node_identifier))?;

        designer.evaluate_node_for_cli(node_id, verbose)
      },
      Err("CAD instance not available".to_string())
    )
  }
}

/// Apply auto-layout to the active node network.
///
/// This function recomputes positions for all nodes in the active network
/// using the user's preferred layout algorithm from preferences.
///
/// # Behavior
/// - Uses the layout algorithm specified in StructureDesignerPreferences
/// - Reorganizes all nodes in the active network for improved readability
/// - Automatically refreshes the UI after layout
#[flutter_rust_bridge::frb(sync)]
pub fn layout_active_network() {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let structure_designer = &mut cad_instance.structure_designer;

      // Get the active network name
      let network_name = match &structure_designer.active_node_network_name {
        Some(name) => name.clone(),
        None => return,
      };

      // Get the layout algorithm from preferences
      let algorithm = structure_designer.preferences.layout_preferences.layout_algorithm.into();

      // Get a const pointer to the registry for layout computation
      let registry_ptr = &structure_designer.node_type_registry as *const crate::structure_designer::node_type_registry::NodeTypeRegistry;

      // Apply layout to the network
      if let Some(network) = structure_designer.node_type_registry.node_networks.get_mut(&network_name) {
        layout::layout_network(network, &*registry_ptr, algorithm);
      }

      // Refresh the UI
      refresh_structure_designer_auto(cad_instance);
    });
  }
}






