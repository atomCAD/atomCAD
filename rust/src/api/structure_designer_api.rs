use crate::api::api_common::refresh_renderer;
use crate::api::api_common::CAD_INSTANCE;
use crate::api::structure_designer_api_types::NodeNetworkView;
use std::collections::HashMap;
use crate::api::structure_designer_api_types::InputPinView;
use crate::api::structure_designer_api_types::NodeView;
use crate::api::structure_designer_api_types::WireView;
use crate::api::common_api_types::APIVec2;
use crate::api::common_api_types::APIVec3;
use crate::api::structure_designer_api_types::APICuboidData;
use crate::api::structure_designer_api_types::APISphereData;
use crate::api::structure_designer_api_types::APIHalfSpaceData;
use crate::api::structure_designer_api_types::APIGeoTransData;
use crate::api::structure_designer_api_types::APIAtomTransData;
use crate::structure_designer::node_type::data_type_to_str;
use crate::structure_designer::node_data::cuboid_data::CuboidData;
use crate::structure_designer::node_data::sphere_data::SphereData;
use crate::structure_designer::node_data::half_space_data::HalfSpaceData;
use crate::structure_designer::node_data::geo_trans_data::GeoTransData;
use crate::structure_designer::node_data::atom_trans_data::AtomTransData;
use crate::api::api_common::to_api_vec2;
use crate::api::api_common::from_api_vec2;
use crate::api::api_common::to_api_ivec3;
use crate::api::api_common::from_api_ivec3;
use crate::api::api_common::to_api_vec3;
use crate::api::api_common::from_api_vec3;

#[flutter_rust_bridge::frb(sync)]
pub fn add_atom(atomic_number: i32, position: APIVec3) {
  unsafe {
    if let Some(cad_instance) = &mut CAD_INSTANCE {
      cad_instance.structure_designer.add_atom(atomic_number, from_api_vec3(&position));
      //cad_instance.renderer.refresh(cad_instance.kernel.get_atomic_structure());
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_node_network_view(node_network_name: String) -> Option<NodeNetworkView> {
  unsafe {
    let cad_instance = CAD_INSTANCE.as_ref()?;
    let node_network = cad_instance.structure_designer.node_type_registry.node_networks.get(&node_network_name)?;

    let mut node_network_view = NodeNetworkView {
      name: node_network.node_type.name.clone(),
      nodes: HashMap::new(),
      wires: Vec::new(),
    };

    for (_id, node) in node_network.nodes.iter() {
      let mut input_pins: Vec<InputPinView> = Vec::new();
      let node_type = cad_instance.structure_designer.node_type_registry.get_node_type(&node.node_type_name)?;
      let num_of_params = node_type.parameters.len();
      for i in 0..num_of_params {
        let param = &node_type.parameters[i];
        input_pins.push(InputPinView {
          name: param.name.clone(),
          data_type: data_type_to_str(&param.data_type),
          multi: param.multi,
        });
      }

      node_network_view.nodes.insert(node.id, NodeView {
        id: node.id,
        node_type_name: node.node_type_name.clone(),
        position: to_api_vec2(&node.position),
        input_pins,
        output_type: data_type_to_str(&node_type.output_type),
        selected: node_network.selected_node_id == Some(node.id),
        displayed: node_network.displayed_node_ids.contains(&node.id),
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

    return Some(node_network_view);
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn move_node(node_network_name: &str, node_id: u64, position: APIVec2) {
  unsafe {
    if let Some(cad_instance) = &mut CAD_INSTANCE {
      cad_instance.structure_designer.move_node(node_network_name, node_id, from_api_vec2(&position));
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn add_node(node_network_name: &str, node_type_name: &str, position: APIVec2) -> u64 {
    unsafe {
        if let Some(cad_instance) = &mut CAD_INSTANCE {
            return cad_instance.structure_designer.add_node(node_network_name, node_type_name, from_api_vec2(&position));
        }
    }
    0
}

#[flutter_rust_bridge::frb(sync)]
pub fn connect_nodes(node_network_name: &str, source_node_id: u64, dest_node_id: u64, dest_param_index: usize) {
  unsafe {
    if let Some(cad_instance) = &mut CAD_INSTANCE {
      cad_instance.structure_designer.connect_nodes(node_network_name, source_node_id, dest_node_id, dest_param_index);
      refresh_renderer(cad_instance, &node_network_name, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_node_type_names() -> Option<Vec<String>> {
  unsafe {
    let cad_instance = CAD_INSTANCE.as_ref()?;
    return Some(cad_instance.structure_designer.node_type_registry.get_node_type_names());
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_node_display(node_network_name: String, node_id: u64, is_displayed: bool) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.structure_designer.set_node_display(&node_network_name, node_id, is_displayed);
      refresh_renderer(instance, &node_network_name, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_node(node_network_name: String, node_id: u64) -> bool {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      let ret = instance.structure_designer.select_node(&node_network_name, node_id);
      refresh_renderer(instance, &node_network_name, false);
      ret
    } else {
      false
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_wire(node_network_name: String, source_node_id: u64, destination_node_id: u64, destination_argument_index: usize) -> bool {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      let ret = instance.structure_designer.select_wire(&node_network_name, source_node_id, destination_node_id, destination_argument_index);
      refresh_renderer(instance, &node_network_name, false);
      ret
    } else {
      false
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn clear_selection(node_network_name: String) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.structure_designer.clear_selection(&node_network_name);
      refresh_renderer(instance, &node_network_name, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_cuboid_data(node_network_name: String, node_id: u64) -> Option<APICuboidData> {
  unsafe {
    let cad_instance = CAD_INSTANCE.as_ref()?;
    let node_data = cad_instance.structure_designer.get_node_network_data(&node_network_name, node_id)?;
    let cuboid_data = node_data.as_any_ref().downcast_ref::<CuboidData>()?;
    return Some(APICuboidData {
      min_corner: to_api_ivec3(&cuboid_data.min_corner),
      extent: to_api_ivec3(&cuboid_data.extent),
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_sphere_data(node_network_name: String, node_id: u64) -> Option<APISphereData> {
  unsafe {
    let cad_instance = CAD_INSTANCE.as_ref()?;
    let node_data = cad_instance.structure_designer.get_node_network_data(&node_network_name, node_id)?;
    let sphere_data = node_data.as_any_ref().downcast_ref::<SphereData>()?;
    return Some(APISphereData {
      center: to_api_ivec3(&sphere_data.center),
      radius: sphere_data.radius,
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_half_space_data(node_network_name: String, node_id: u64) -> Option<APIHalfSpaceData> {
  unsafe {
    let cad_instance = CAD_INSTANCE.as_ref()?;
    let node_data = cad_instance.structure_designer.get_node_network_data(&node_network_name, node_id)?;
    let half_space_data = node_data.as_any_ref().downcast_ref::<HalfSpaceData>()?;
    return Some(APIHalfSpaceData {
      miller_index: to_api_ivec3(&half_space_data.miller_index),
      shift: half_space_data.shift,
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_geo_trans_data(node_network_name: String, node_id: u64) -> Option<APIGeoTransData> {
  unsafe {
    let cad_instance = CAD_INSTANCE.as_ref()?;
    let node_data = cad_instance.structure_designer.get_node_network_data(&node_network_name, node_id)?;
    let geo_trans_data = node_data.as_any_ref().downcast_ref::<GeoTransData>()?;
    return Some(APIGeoTransData {
      transform_only_frame: geo_trans_data.transform_only_frame,
      translation: to_api_ivec3(&geo_trans_data.translation),
      rotation: to_api_ivec3(&geo_trans_data.rotation),
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_atom_trans_data(node_network_name: String, node_id: u64) -> Option<APIAtomTransData> {
  unsafe {
    let cad_instance = CAD_INSTANCE.as_ref()?;
    let node_data = cad_instance.structure_designer.get_node_network_data(&node_network_name, node_id)?;
    let atom_trans_data = node_data.as_any_ref().downcast_ref::<AtomTransData>()?;
    return Some(APIAtomTransData {
      translation: to_api_vec3(&atom_trans_data.translation),
      rotation: to_api_vec3(&atom_trans_data.rotation),
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_cuboid_data(node_network_name: String, node_id: u64, data: APICuboidData) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      let cuboid_data = Box::new(CuboidData {
        min_corner: from_api_ivec3(&data.min_corner),
        extent: from_api_ivec3(&data.extent),
      });
      instance.structure_designer.set_node_network_data(&node_network_name, node_id, cuboid_data);
      refresh_renderer(instance, &node_network_name, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_sphere_data(node_network_name: String, node_id: u64, data: APISphereData) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      let sphere_data = Box::new(SphereData {
        center: from_api_ivec3(&data.center),
        radius: data.radius,
      });
      instance.structure_designer.set_node_network_data(&node_network_name, node_id, sphere_data);
      refresh_renderer(instance, &node_network_name, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_half_space_data(node_network_name: String, node_id: u64, data: APIHalfSpaceData) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      let half_space_data = Box::new(HalfSpaceData {
        miller_index: from_api_ivec3(&data.miller_index),
        shift: data.shift,
      });
      instance.structure_designer.set_node_network_data(&node_network_name, node_id, half_space_data);
      refresh_renderer(instance, &node_network_name, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_geo_trans_data(node_network_name: String, node_id: u64, data: APIGeoTransData) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      let geo_trans_data = Box::new(GeoTransData {
        transform_only_frame: data.transform_only_frame,
        translation: from_api_ivec3(&data.translation),
        rotation: from_api_ivec3(&data.rotation),
      });
      instance.structure_designer.set_node_network_data(&node_network_name, node_id, geo_trans_data);
      refresh_renderer(instance, &node_network_name, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_atom_trans_data(node_network_name: String, node_id: u64, data: APIAtomTransData) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      let atom_trans_data = Box::new(AtomTransData {
        translation: from_api_vec3(&data.translation),
        rotation: from_api_vec3(&data.rotation),
      });
      instance.structure_designer.set_node_network_data(&node_network_name, node_id, atom_trans_data);
      refresh_renderer(instance, &node_network_name, false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn delete_selected(node_network_name: String) {
  unsafe {
    if let Some(ref mut cad_instance) = CAD_INSTANCE {
      cad_instance.structure_designer.delete_selected(&node_network_name);
      refresh_renderer(cad_instance, &node_network_name, false);
    }
  }
}