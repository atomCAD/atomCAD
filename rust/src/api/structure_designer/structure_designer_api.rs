use crate::api::api_common::from_api_ivec2;
use crate::api::api_common::refresh_renderer;
use crate::api::api_common::to_api_ivec2;
use crate::api::api_common::with_mut_cad_instance;
use crate::api::api_common::with_cad_instance;
use crate::api::api_common::with_mut_cad_instance_or;
use crate::api::api_common::with_cad_instance_or;
use crate::api::common_api_types::APIResult;
use crate::api::structure_designer::structure_designer_api_types::{NodeNetworkView, APINetworkWithValidationErrors};
use crate::structure_designer::nodes::circle::CircleData;
use crate::structure_designer::nodes::extrude::ExtrudeData;
use crate::structure_designer::nodes::geo_to_atom::GeoToAtomData;
use crate::structure_designer::nodes::half_plane::HalfPlaneData;
use crate::structure_designer::nodes::reg_poly::RegPolyData;
use crate::structure_designer::nodes::rect::RectData;
use std::collections::HashMap;
use crate::api::structure_designer::structure_designer_api_types::InputPinView;
use crate::api::structure_designer::structure_designer_api_types::NodeView;
use crate::api::structure_designer::structure_designer_api_types::WireView;
use crate::api::common_api_types::APIVec2;
use crate::api::structure_designer::structure_designer_api_types::APICuboidData;
use crate::api::structure_designer::structure_designer_api_types::APISphereData;
use crate::api::structure_designer::structure_designer_api_types::APIHalfSpaceData;
use crate::api::structure_designer::structure_designer_api_types::APIGeoTransData;
use crate::api::structure_designer::structure_designer_api_types::APIAtomTransData;
use crate::api::structure_designer::structure_designer_api_types::APIEditAtomData;
use crate::api::structure_designer::structure_designer_api_types::APIGeoToAtomData;
use crate::api::structure_designer::structure_designer_api_types::APIAnchorData;
use crate::structure_designer::node_type::data_type_to_str;
use crate::structure_designer::nodes::cuboid::CuboidData;
use crate::structure_designer::nodes::sphere::SphereData;
use crate::structure_designer::nodes::half_space::HalfSpaceData;
use crate::structure_designer::nodes::geo_trans::GeoTransData;
use crate::structure_designer::nodes::edit_atom::edit_atom::EditAtomData;
use crate::structure_designer::nodes::edit_atom::edit_atom::EditAtomTool;
use crate::structure_designer::nodes::atom_trans::AtomTransData;
use crate::structure_designer::nodes::anchor::AnchorData;
use crate::api::api_common::to_api_vec2;
use crate::api::api_common::from_api_vec2;
use crate::api::api_common::to_api_ivec3;
use crate::api::api_common::from_api_ivec3;
use crate::api::api_common::to_api_vec3;
use crate::api::api_common::from_api_vec3;
use super::structure_designer_api_types::APICircleData;
use super::structure_designer_api_types::APIExtrudeData;
use super::structure_designer_api_types::APIHalfPlaneData;
use super::structure_designer_api_types::APIRegPolyData;
use super::structure_designer_api_types::APIRectData;
use super::structure_designer_api_types::APIParameterData;
use super::structure_designer_api_types::APIDataType;
use crate::structure_designer::nodes::parameter::ParameterData;
use super::structure_designer_preferences::StructureDesignerPreferences;

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
          let node_type = match cad_instance.structure_designer.node_type_registry.get_node_type(&node.node_type_name) {
            Some(nt) => nt,
            None => return None
          };
          let num_of_params = node_type.parameters.len();
          for i in 0..num_of_params {
            let param = &node_type.parameters[i];
            input_pins.push(InputPinView {
              name: param.name.clone(),
              data_type: data_type_to_str(&cad_instance.structure_designer.node_type_registry.get_node_param_data_type(node, i)),
              multi: param.multi,
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
            if let Some(eval_error) = cad_instance.structure_designer.last_generated_structure_designer_scene.node_errors.get(&node.id) {
              error_messages.push(eval_error.clone());
            }
          }
          
          // Combine all errors with newline separator
          let error = if error_messages.is_empty() {
            None
          } else {
            Some(error_messages.join("\n"))
          };

          let output_type = cad_instance.structure_designer.node_type_registry.get_node_output_type(node);

          node_network_view.nodes.insert(node.id, NodeView {
            id: node.id,
            node_type_name: node.node_type_name.clone(),
            position: to_api_vec2(&node.position),
            input_pins,
            output_type: data_type_to_str(&output_type),
            selected: node_network.selected_node_id == Some(node.id),
            displayed: node_network.is_node_displayed(node.id),
            return_node: node_network.return_node_id == Some(node.id),
            error,
          });
        }

        for (_id, node) in node_network.nodes.iter() {
          for (index, argument) in node.arguments.iter().enumerate() {
            for argument_node_id in argument.argument_node_ids.iter() {
              node_network_view.wires.push(WireView {
                source_node_id: *argument_node_id,
                dest_node_id: node.id,
                dest_param_index: index,
                selected: node_network.selected_wire.as_ref().map_or(false, |wire| 
                  wire.source_node_id == *argument_node_id && 
                  wire.destination_node_id == node.id && 
                  wire.destination_argument_index == index
                ),
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
        refresh_renderer(cad_instance, false);
        ret
      },
      0 // Default value if CAD_INSTANCE is None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn connect_nodes(source_node_id: u64, dest_node_id: u64, dest_param_index: usize) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      cad_instance.structure_designer.connect_nodes(source_node_id, dest_node_id, dest_param_index);
      refresh_renderer(cad_instance, false);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_node_type_names() -> Option<Vec<String>> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        Some(cad_instance.structure_designer.node_type_registry.get_node_type_names())
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

#[flutter_rust_bridge::frb(sync)]
pub fn set_active_node_network(node_network_name: &str) {
  unsafe {
    with_mut_cad_instance(|instance| {
      instance.structure_designer.set_active_node_network_name(Some(node_network_name.to_string()));
      refresh_renderer(instance, false);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn rename_node_network(old_name: &str, new_name: &str) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |instance| {
        let result = instance.structure_designer.rename_node_network(old_name, new_name);
        refresh_renderer(instance, false);
        result
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_node_display(node_id: u64, is_displayed: bool) {
  unsafe {
    with_mut_cad_instance(|instance| {
      instance.structure_designer.set_node_display(node_id, is_displayed);
      refresh_renderer(instance, false);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_node(node_id: u64) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |instance| {
        let result = instance.structure_designer.select_node(node_id);
        refresh_renderer(instance, false);
        result
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_wire(source_node_id: u64, destination_node_id: u64, destination_argument_index: usize) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |instance| {
        let result = instance.structure_designer.select_wire(source_node_id, destination_node_id, destination_argument_index);
        refresh_renderer(instance, false);
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
      refresh_renderer(instance, false);
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
pub fn get_anchor_data(node_id: u64) -> Option<APIAnchorData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let anchor_data = match node_data.as_any_ref().downcast_ref::<AnchorData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIAnchorData {
          position: anchor_data.position.map(|pos| to_api_ivec3(&pos)),
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

#[flutter_rust_bridge::frb(sync)]
pub fn get_geo_to_atom_data(node_id: u64) -> Option<APIGeoToAtomData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let geo_to_atom_data = match node_data.as_any_ref().downcast_ref::<GeoToAtomData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIGeoToAtomData {
          primary_atomic_number: geo_to_atom_data.primary_atomic_number,
          secondary_atomic_number: geo_to_atom_data.secondary_atomic_number,
          hydrogen_passivation: geo_to_atom_data.hydrogen_passivation,
        })
      },
      None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_geo_to_atom_data(node_id: u64, data: APIGeoToAtomData) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data_mut(node_id) {
          Some(data) => data,
          None => return false,
        };
        
        let geo_to_atom_data = match node_data.as_any_mut().downcast_mut::<GeoToAtomData>() {
          Some(data) => data,
          None => return false,
        };
        
        geo_to_atom_data.primary_atomic_number = data.primary_atomic_number;
        geo_to_atom_data.secondary_atomic_number = data.secondary_atomic_number;
        geo_to_atom_data.hydrogen_passivation = data.hydrogen_passivation;
        refresh_renderer(cad_instance, false);
        true
      },
      false
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
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let parameter_data = match node_data.as_any_ref().downcast_ref::<ParameterData>() {
          Some(data) => data,
          None => return None,
        };
        Some(APIParameterData {
          param_index: parameter_data.param_index,
          param_name: parameter_data.param_name.clone(),
          data_type: parameter_data.data_type,
          multi: parameter_data.multi,
          sort_order: parameter_data.sort_order,
        })
      },
      None
    )
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
      refresh_renderer(cad_instance, false);
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
          refresh_renderer(cad_instance, false);
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
      refresh_renderer(cad_instance, false);
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
      refresh_renderer(cad_instance, false);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_extrude_data(node_id: u64, data: APIExtrudeData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let extrude_data = Box::new(ExtrudeData {
        height: data.height,
      });
      cad_instance.structure_designer.set_node_network_data(node_id, extrude_data);
      refresh_renderer(cad_instance, false);
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
      refresh_renderer(cad_instance, false);
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
      refresh_renderer(cad_instance, false);
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
      });
      cad_instance.structure_designer.set_node_network_data(node_id, half_space_data);
      refresh_renderer(cad_instance, false);
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
      refresh_renderer(cad_instance, false);
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
      refresh_renderer(cad_instance, false);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_parameter_data(node_id: u64, data: APIParameterData) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let parameter_data = Box::new(ParameterData {
        param_index: data.param_index,
        param_name: data.param_name,
        data_type: data.data_type,
        multi: data.multi,
        sort_order: data.sort_order,
      });
      cad_instance.structure_designer.set_node_network_data(node_id, parameter_data);
      refresh_renderer(cad_instance, false);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn delete_selected() {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      cad_instance.structure_designer.delete_selected();
      refresh_renderer(cad_instance, false);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_return_node_id(node_id: Option<u64>) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        let result = cad_instance.structure_designer.set_return_node_id(node_id);
        refresh_renderer(cad_instance, false);
        result
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn save_node_networks(file_path: String) -> bool {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        // Call the method in StructureDesigner
        match cad_instance.structure_designer.save_node_networks(&file_path) {
          Ok(_) => true,
          Err(_) => false
        }
      },
      false
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
        refresh_renderer(cad_instance, false);
        
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
pub fn get_api_data_type_display_name(data_type: APIDataType) -> String {
  data_type_to_str(&data_type)
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
      refresh_renderer(cad_instance, false);
    });
  }
}
