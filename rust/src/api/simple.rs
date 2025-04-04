use std::ffi::{c_int, c_void};
use dlopen::{symbor::{Library, Symbol}, Error as LibError};
use std::time::Instant;
use crate::renderer::renderer::Renderer;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::scene_composer::scene_composer::SceneComposer;
use glam::f64::DVec2;
use glam::f64::DVec3;
use glam::f64::DQuat;
use glam::i32::IVec3;
use std::collections::HashMap;
use super::api_types::{
  APICuboidData,
  APIVec2,
  APISphereData,
  APIHalfSpaceData,
  APIGeoTransData,
  APIAtomTransData,
  SelectModifier,
  APITransform,
  APISceneComposerTool
};
use super::api_types::APIVec3;
use super::api_types::APIIVec3;
use super::api_types::APICamera;
use super::api_types::InputPinView;
use super::api_types::NodeView;
use super::api_types::WireView;
use super::api_types::NodeNetworkView;
use super::api_types::Editor;
use super::api_types::SceneComposerView;
use super::api_types::ClusterView;
use crate::structure_designer::node_type::data_type_to_str;
use crate::structure_designer::node_data::sphere_data::SphereData;
use crate::structure_designer::node_data::cuboid_data::CuboidData;
use crate::structure_designer::node_data::half_space_data::HalfSpaceData;
use crate::structure_designer::node_data::geo_trans_data::GeoTransData;
use crate::structure_designer::node_data::atom_trans_data::AtomTransData;
use crate::util::transform::Transform;

fn to_api_vec3(v: &DVec3) -> APIVec3 {
  return APIVec3{
    x: v.x,
    y: v.y,
    z: v.z
  }
}

fn from_api_vec3(v: &APIVec3) -> DVec3 {
  return DVec3{
    x: v.x,
    y: v.y,
    z: v.z
  }
}

fn to_api_ivec3(v: &IVec3) -> APIIVec3 {
  return APIIVec3{
    x: v.x,
    y: v.y,
    z: v.z
  }
}

fn from_api_ivec3(v: &APIIVec3) -> IVec3 {
  return IVec3{
    x: v.x,
    y: v.y,
    z: v.z
  }
}

fn to_api_vec2(v: &DVec2) -> APIVec2 {
  return APIVec2{
    x: v.x,
    y: v.y,
  }
}

fn from_api_vec2(v: &APIVec2) -> DVec2 {
  return DVec2 {
    x: v.x,
    y: v.y,
  }
}

fn to_api_transform(transform: &Transform) -> APITransform {
  // Convert quaternion to Euler angles (intrinsic rotation in radians)
  let rotation_euler = transform.rotation.to_euler(glam::EulerRot::XYZ);
  
  return APITransform {
    translation: to_api_vec3(&transform.translation),
    rotation: APIVec3 {
      x: rotation_euler.0.to_degrees(), // roll (x-axis rotation)
      y: rotation_euler.1.to_degrees(), // pitch (y-axis rotation)
      z: rotation_euler.2.to_degrees(), // yaw (z-axis rotation)
    },
  }
}

fn from_api_transform(api_transform: &APITransform) -> Transform {
  // Convert Euler angles (intrinsic XYZ in degrees) to quaternion
  let rotation = DQuat::from_euler(
    glam::EulerRot::XYZ, 
    api_transform.rotation.x.to_radians(), 
    api_transform.rotation.y.to_radians(), 
    api_transform.rotation.z.to_radians()
  );
  
  return Transform {
    translation: from_api_vec3(&api_transform.translation),
    rotation,
  }
}

const INITIAL_VIEWPORT_WIDTH : u32 = 1280;
const INITIAL_VIEWPORT_HEIGHT : u32 = 544;

/// Set the viewport size for rendering
#[no_mangle]
pub fn set_viewport_size(width: u32, height: u32) {
  let start_time = Instant::now();
  println!("API: Setting viewport size to {}x{}", width, height);

  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.renderer.set_viewport_size(width, height);
    }
  }

  println!("set_viewport_size took: {:?}", start_time.elapsed());
}

pub struct CADInstance {
  structure_designer: StructureDesigner,
  scene_composer: SceneComposer,
  renderer: Renderer,
  active_editor: Editor, // This one refreshes itself into the Renderer when the refresh_renderer function is called.
}

pub type FlutterRgbaRendererPluginOnRgba = unsafe extern "C" fn(
  texture_rgba: *mut c_void,
  buffer: *const u8,
  len: c_int,
  width: c_int,
  height: c_int,
  dst_rgba_stride: c_int,
);

#[cfg(all(target_os = "windows"))]
lazy_static::lazy_static! {
    pub static ref TEXTURE_RGBA_RENDERER_PLUGIN: Result<Library, LibError> = Library::open("texture_rgba_renderer_plugin.dll");
}

#[cfg(all(target_os = "linux"))]
lazy_static::lazy_static! {
    pub static ref TEXTURE_RGBA_RENDERER_PLUGIN: Result<Library, LibError> = Library::open("libtexture_rgba_renderer_plugin.so");
}

#[cfg(all(target_os = "macos"))]
lazy_static::lazy_static! {
    pub static ref TEXTURE_RGBA_RENDERER_PLUGIN: Result<Library, LibError> = Library::open_self();
}

lazy_static::lazy_static! {
  pub static ref RGBA_FUNCTION: Result<Symbol<'static, FlutterRgbaRendererPluginOnRgba>, String> = {
      match &*TEXTURE_RGBA_RENDERER_PLUGIN {
          Ok(library) => {
              // Attempt to load the symbol and return it as a Result
              unsafe { library.symbol::<FlutterRgbaRendererPluginOnRgba>("FlutterRgbaRendererPluginOnRgba") }
                  .map_err(|e| format!("Failed to load symbol: {:?}", e))
          }
          Err(e) => Err(format!("Library not loaded: {:?}", e)),
      }
  };
}

static mut CAD_INSTANCE: Option<CADInstance> = None;

async fn initialize_cad_instance_async() {
  unsafe {
    CAD_INSTANCE = Some(
      CADInstance {
        structure_designer: StructureDesigner::new(),
        scene_composer: SceneComposer::new(),
        renderer: Renderer::new(INITIAL_VIEWPORT_WIDTH, INITIAL_VIEWPORT_HEIGHT).await,
        active_editor: Editor::None,
      }
    );

    if let Some(ref mut cad_instance) = CAD_INSTANCE {
      cad_instance.renderer.refresh_background();
      add_sample_network(&mut cad_instance.structure_designer);
      let scene = cad_instance.structure_designer.generate_scene("sample", false);
      cad_instance.renderer.refresh(&scene, false);
    }
  }
}

fn refresh_renderer(cad_instance: &mut CADInstance, node_network_name: &str, lightweight: bool) {
  match cad_instance.active_editor {
    Editor::StructureDesigner => {
      let scene = cad_instance.structure_designer.generate_scene(node_network_name, lightweight);
      cad_instance.renderer.refresh(&scene, lightweight);
    },
    Editor::SceneComposer => {
      cad_instance.renderer.refresh(&cad_instance.scene_composer, lightweight);
    },
    Editor::None => {}
  }
}

fn add_sample_model(kernel: &mut StructureDesigner) {
  let atom_id1 = kernel.add_atom(6, DVec3::new(-1.3, 0.0, 0.0));
  let atom_id2 = kernel.add_atom(6, DVec3::new(1.3, 0.0, 0.0));
  kernel.add_atom(6, DVec3::new(1.3, 3.0, 0.0));
  kernel.add_bond(atom_id1, atom_id2, 1);
}

fn add_sample_network(kernel: &mut StructureDesigner) {
  kernel.add_node_network("sample");
  let cuboid_id = kernel.add_node("sample", "cuboid", DVec2::new(30.0, 30.0));
  let sphere_id = kernel.add_node("sample", "sphere", DVec2::new(100.0, 100.0));
  let diff_id_1 = kernel.add_node("sample", "diff", DVec2::new(300.0, 80.0));
  let diff_id_2 = kernel.add_node("sample", "diff", DVec2::new(500.0, 80.0));

  kernel.connect_nodes("sample", cuboid_id, diff_id_1, 0);
  kernel.connect_nodes("sample", sphere_id, diff_id_1, 1);
  kernel.connect_nodes("sample", diff_id_1, diff_id_2, 1);
}

fn generate_mock_image(width: u32, height: u32) -> Vec<u8> {
  let mut v : Vec<u8> = vec![0; (width as usize)*(height as usize)*4];
  for i in 0..((width as usize)*(height as usize)) {
    let index = i * 4;
    v[index] = 0;
    v[index + 1] = 255;
    v[index + 2] = 0;
    v[index + 3] = 255;
  }
  return v;
}

// Send the given texture in memory to Flutter.
fn send_texture(texture_ptr: u64, width: u32, height: u32, v : Vec<u8>) {
  match &*RGBA_FUNCTION {
    Ok(on_rgba) => {
      unsafe {
        on_rgba(texture_ptr as *mut c_void, v.as_ptr(), (width * height * 4) as i32, width as i32, height as i32, 0);
      }
    }
    Err(err) => {
      println!("Failed to load render function: {}", err);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_active_editor(editor: Editor) {
  unsafe {
    if let Some(cad_instance) = &mut CAD_INSTANCE {
      cad_instance.active_editor = editor;
    }
  }
}

#[flutter_rust_bridge::frb(sync, type_64bit_int)]
pub fn provide_texture(texture_ptr: u64) -> f64 {

  let start = Instant::now(); // Record the start time

  match unsafe { &mut CAD_INSTANCE } {
    Some(cad_instance) => {
      let v = cad_instance.renderer.render();
      send_texture(texture_ptr, cad_instance.renderer.texture_size.width, cad_instance.renderer.texture_size.height, v);
    }
    None => {
      let v: Vec<u8> = generate_mock_image(INITIAL_VIEWPORT_WIDTH, INITIAL_VIEWPORT_HEIGHT);
      send_texture(texture_ptr, INITIAL_VIEWPORT_WIDTH, INITIAL_VIEWPORT_HEIGHT, v);
    }
  };

  let duration = start.elapsed(); // Calculate elapsed time
  //println!("Provide texture time: {:?}", duration);

  return duration.as_secs_f64();
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_camera() -> Option<APICamera> {
  unsafe {
    if let Some(cad_instance) = &CAD_INSTANCE {
      let camera = &cad_instance.renderer.camera;
      return Some(APICamera {
        eye: to_api_vec3(&camera.eye),
        target: to_api_vec3(&camera.target),
        up: to_api_vec3(&camera.up),
        aspect: camera.aspect,
        fovy: camera.fovy,
        znear: camera.znear,
        zfar: camera.zfar,      
      });
    } else {
      return None;
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn move_camera(eye: APIVec3, target: APIVec3, up: APIVec3) {
  unsafe {
    if let Some(cad_instance) = &mut CAD_INSTANCE {
      cad_instance.renderer.move_camera(&from_api_vec3(&eye), &from_api_vec3(&target), &from_api_vec3(&up));
    }
  }
}

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
pub fn find_pivot_point(ray_start: APIVec3, ray_dir: APIVec3) -> APIVec3 {
  unsafe {
    if let Some(cad_instance) = &CAD_INSTANCE {
      let model = &cad_instance.scene_composer.model;
      return to_api_vec3(&model.find_pivot_point(&from_api_vec3(&ray_start), &from_api_vec3(&ray_dir)));
    } else {
      return APIVec3{
        x: 0.0,
        y: 0.0,
        z: 0.0
      }
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

#[flutter_rust_bridge::frb(sync)]
pub fn gadget_hit_test(ray_origin: APIVec3, ray_direction: APIVec3) -> Option<i32> {
  unsafe {
    let instance = CAD_INSTANCE.as_ref()?;

    match instance.active_editor {
      Editor::StructureDesigner => {
        return instance.structure_designer.gadget_hit_test(from_api_vec3(&ray_origin), from_api_vec3(&ray_direction));
      },
      Editor::SceneComposer => {
        return instance.scene_composer.gadget_hit_test(from_api_vec3(&ray_origin), from_api_vec3(&ray_direction));
      },
      Editor::None => { None }
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn gadget_start_drag(node_network_name: String, handle_index: i32, ray_origin: APIVec3, ray_direction: APIVec3) {
  unsafe {
    let Some(mut instance) = CAD_INSTANCE.as_mut() else { return };

    match instance.active_editor {
      Editor::StructureDesigner => {
        instance.structure_designer.gadget_start_drag(handle_index, from_api_vec3(&ray_origin), from_api_vec3(&ray_direction));
      },
      Editor::SceneComposer => {
        instance.scene_composer.gadget_start_drag(handle_index, from_api_vec3(&ray_origin), from_api_vec3(&ray_direction));
      },
      Editor::None => {}
    }

    refresh_renderer(instance, &node_network_name, false);
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn gadget_drag(node_network_name: String, handle_index: i32, ray_origin: APIVec3, ray_direction: APIVec3) {
  unsafe {
    let Some(mut instance) = CAD_INSTANCE.as_mut() else { return };

    match instance.active_editor {
      Editor::StructureDesigner => {
        instance.structure_designer.gadget_drag(handle_index, from_api_vec3(&ray_origin), from_api_vec3(&ray_direction));
        refresh_renderer(instance, &node_network_name, true);
      },
      Editor::SceneComposer => {
        instance.scene_composer.gadget_drag(handle_index, from_api_vec3(&ray_origin), from_api_vec3(&ray_direction));
        refresh_renderer(instance, &node_network_name, false);
      },
      Editor::None => {}
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn gadget_end_drag(node_network_name: String) {
  unsafe {
    let Some(mut instance) = CAD_INSTANCE.as_mut() else { return };

    match instance.active_editor {
      Editor::StructureDesigner => {
        instance.structure_designer.gadget_end_drag();
      },
      Editor::SceneComposer => {
        instance.scene_composer.gadget_end_drag();
      },
      Editor::None => {}
    }
    refresh_renderer(instance, &node_network_name, false);
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn sync_gadget_data(node_network_name: String) -> bool {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      match instance.active_editor {
        Editor::StructureDesigner => {
          return instance.structure_designer.sync_gadget_data(&node_network_name);
        },
        Editor::SceneComposer => {
          instance.scene_composer.sync_gadget_to_model();
          return true;
        },
        Editor::None => { false }
      }
    } else {
      false
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn import_xyz(file_path: &str) {
  unsafe {
    if let Some(cad_instance) = &mut CAD_INSTANCE {
      cad_instance.scene_composer.import_xyz(file_path).unwrap();
      refresh_renderer(cad_instance, "", false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn export_xyz(file_path: &str) -> bool {
  unsafe {
    if let Some(cad_instance) = &mut CAD_INSTANCE {
      cad_instance.scene_composer.export_xyz(file_path).is_ok()
    } else {
      false
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_cluster_by_ray(ray_start: APIVec3, ray_dir: APIVec3, select_modifier: SelectModifier) -> Option<u64> {
  unsafe {
    let instance = CAD_INSTANCE.as_mut()?;
    let selected_cluster = instance.scene_composer.select_cluster_by_ray(
      &from_api_vec3(&ray_start),
      &from_api_vec3(&ray_dir),
      select_modifier);
    refresh_renderer(instance, "", false);
    selected_cluster
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_cluster_by_id(cluster_id: u64, select_modifier: SelectModifier) {
  unsafe {
    let instance = CAD_INSTANCE.as_mut().unwrap();
    instance.scene_composer.select_cluster_by_id(cluster_id, select_modifier);
    refresh_renderer(instance, "", false);
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_scene_composer_view() -> Option<SceneComposerView> {
  unsafe {
    let cad_instance = CAD_INSTANCE.as_ref()?;

    let mut scene_composer_view = SceneComposerView {
      clusters: Vec::new(),
      active_tool: cad_instance.scene_composer.get_active_tool(),
      available_tools: cad_instance.scene_composer.get_available_tools(),
    };

    for cluster in cad_instance.scene_composer.model.clusters.values() {
      scene_composer_view.clusters.push(ClusterView {
        id: cluster.id,
        name: cluster.name.clone(),
        selected: cluster.selected,
      });
    }

    Some(scene_composer_view)
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_selected_frame_transform() -> Option<APITransform> {
  unsafe {
    let instance = CAD_INSTANCE.as_ref()?;
    let transform = instance.scene_composer.get_selected_frame_transform()?;
    Some(to_api_transform(&transform))
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_selected_frame_transform(transform: APITransform) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.scene_composer.set_selected_frame_transform(from_api_transform(&transform));
      refresh_renderer(instance, "", false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_camera_transform() -> APITransform {
  unsafe {
    if let Some(instance) = &CAD_INSTANCE {
      let transform = instance.renderer.get_camera_transform();
      return to_api_transform(&transform);
    }
    // Return identity transform as fallback
    to_api_transform(&Transform::default())
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_camera_transform(transform: APITransform) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      let transform = from_api_transform(&transform);
      instance.renderer.set_camera_transform(&transform);
      refresh_renderer(instance, "", false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn translate_along_local_axis(axis_index: u32, translation: f64) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.scene_composer.translate_along_local_axis(axis_index, translation);
      refresh_renderer(instance, "", false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn rotate_around_local_axis(axis_index: u32, angle_degrees: f64) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.scene_composer.rotate_around_local_axis(axis_index, angle_degrees);
      refresh_renderer(instance, "", false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn is_frame_locked_to_atoms() -> bool {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      return instance.scene_composer.is_frame_locked_to_atoms();
    }
  }
  
  false
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_frame_locked_to_atoms(locked: bool) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.scene_composer.set_frame_locked_to_atoms(locked);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_align_atom_by_ray(ray_start: APIVec3, ray_dir: APIVec3) -> Option<u64> {
  unsafe {
    if let Some(ref mut cad_instance) = CAD_INSTANCE {
      let ray_start_dvec3 = from_api_vec3(&ray_start);
      let ray_dir_dvec3 = from_api_vec3(&ray_dir);
      let ret = cad_instance.scene_composer.select_align_atom_by_ray(&ray_start_dvec3, &ray_dir_dvec3);
      refresh_renderer(cad_instance, "", false);
      return ret;
    }
  }
  None
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_active_scene_composer_tool(tool: APISceneComposerTool) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.scene_composer.set_active_tool(tool);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_align_tool_state_text() -> String {
  let start_time = Instant::now();
  
  let result = unsafe {
    if let Some(ref mut cad_instance) = CAD_INSTANCE {
      cad_instance.scene_composer.get_align_tool_state_text()
    } else {
      String::new()
    }
  };
  
  result
}

#[flutter_rust_bridge::frb(sync)]
pub fn scene_composer_new_model() {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.scene_composer.new_model();
      refresh_renderer(instance, "", false);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn greet(name: String) -> String {
    format!("Hello, {name}!")
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    // Default utilities - feel free to customize
    flutter_rust_bridge::setup_default_user_utils();

    tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .unwrap()
    .block_on(initialize_cad_instance_async());
}
