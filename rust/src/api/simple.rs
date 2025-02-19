use std::ffi::{c_int, c_void};
use dlopen::{symbor::{Library, Symbol}, Error as LibError};
use std::time::Instant;
use crate::renderer::renderer::Renderer;
use crate::kernel::kernel::Kernel;
use glam::f32::Vec2;
use glam::f32::Vec3;
use std::collections::HashMap;
use super::api_types::APIVec2;
use super::api_types::APIVec3;
use super::api_types::APICamera;
use super::api_types::InputPinView;
use super::api_types::NodeView;
use super::api_types::WireView;
use super::api_types::NodeNetworkView;
use crate::kernel::node_type::data_type_to_str;

fn to_api_vec3(v: &Vec3) -> APIVec3 {
  return APIVec3{
    x: v.x,
    y: v.y,
    z: v.z
  }
}

fn from_api_vec3(v: &APIVec3) -> Vec3 {
  return Vec3{
    x: v.x,
    y: v.y,
    z: v.z
  }
}

fn to_api_vec2(v: &Vec2) -> APIVec2 {
  return APIVec2{
    x: v.x,
    y: v.y,
  }
}

fn from_api_vec2(v: &APIVec2) -> Vec2 {
  return Vec2 {
    x: v.x,
    y: v.y,
  }
}

const IMAGE_WIDTH : u32 = 1280;
const IMAGE_HEIGHT : u32 = 544;

pub struct CADInstance {
  kernel: Kernel,
  renderer: Renderer,
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
        kernel: Kernel::new(),
        renderer: Renderer::new(IMAGE_WIDTH, IMAGE_HEIGHT).await
      }
    );

    if let Some(ref mut cad_instance) = CAD_INSTANCE {
      add_sample_network(&mut cad_instance.kernel);
      let scene = cad_instance.kernel.generate_scene("sample");
      cad_instance.renderer.refresh(&scene);
    }
  }
}

fn refresh_renderer(cad_instance: &mut CADInstance, node_network_name: &str) {
  let scene = cad_instance.kernel.generate_scene(node_network_name);
  cad_instance.renderer.refresh(&scene);
}

fn add_sample_model(kernel: &mut Kernel) {
  let atom_id1 = kernel.add_atom(6, Vec3::new(-1.3, 0.0, 0.0));
  let atom_id2 = kernel.add_atom(6, Vec3::new(1.3, 0.0, 0.0));
  kernel.add_atom(6, Vec3::new(1.3, 3.0, 0.0));
  kernel.add_bond(atom_id1, atom_id2, 1);
}

fn add_sample_network(kernel: &mut Kernel) {
  kernel.add_node_network("sample");
  let cuboid_id = kernel.add_node("sample", "cuboid", Vec2::new(30.0, 30.0));
  let sphere_id = kernel.add_node("sample", "sphere", Vec2::new(100.0, 100.0));
  let diff_id_1 = kernel.add_node("sample", "diff", Vec2::new(300.0, 80.0));
  let diff_id_2 = kernel.add_node("sample", "diff", Vec2::new(500.0, 80.0));

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

#[flutter_rust_bridge::frb(sync, type_64bit_int)]
pub fn provide_texture(texture_ptr: u64) -> f64 {

  let start = Instant::now(); // Record the start time

  match unsafe { &mut CAD_INSTANCE } {
    Some(cad_instance) => {
      let v = cad_instance.renderer.render();
      send_texture(texture_ptr, IMAGE_WIDTH, IMAGE_HEIGHT, v);
    }
    None => {
      let v: Vec<u8> = generate_mock_image(IMAGE_WIDTH, IMAGE_HEIGHT);
      send_texture(texture_ptr, IMAGE_WIDTH, IMAGE_HEIGHT, v);
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
      cad_instance.kernel.add_atom(atomic_number, from_api_vec3(&position));
      //cad_instance.renderer.refresh(cad_instance.kernel.get_atomic_structure());
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn find_pivot_point(ray_start: APIVec3, ray_dir: APIVec3) -> APIVec3 {
  unsafe {
    if let Some(cad_instance) = &CAD_INSTANCE {
      let model = cad_instance.kernel.get_atomic_structure();
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
    let node_network = cad_instance.kernel.node_type_registry.node_networks.get(&node_network_name)?;

    let mut node_network_view = NodeNetworkView {
      name: node_network.node_type.name.clone(),
      nodes: HashMap::new(),
      wires: Vec::new(),
    };

    for (_id, node) in node_network.nodes.iter() {
      let mut input_pins: Vec<InputPinView> = Vec::new();
      let node_type = cad_instance.kernel.node_type_registry.get_node_type(&node.node_type_name)?;
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
      cad_instance.kernel.move_node(node_network_name, node_id, from_api_vec2(&position));
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn connect_nodes(node_network_name: &str, source_node_id: u64, dest_node_id: u64, dest_param_index: usize) {
  unsafe {
    if let Some(cad_instance) = &mut CAD_INSTANCE {
      cad_instance.kernel.connect_nodes(node_network_name, source_node_id, dest_node_id, dest_param_index);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_node_type_names() -> Option<Vec<String>> {
  unsafe {
    let cad_instance = CAD_INSTANCE.as_ref()?;
    return Some(cad_instance.kernel.node_type_registry.get_node_type_names());
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_node_display(node_network_name: String, node_id: u64, is_displayed: bool) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.kernel.set_node_display(&node_network_name, node_id, is_displayed);
      refresh_renderer(instance, &node_network_name);
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_node(node_network_name: String, node_id: u64) -> bool {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.kernel.select_node(&node_network_name, node_id)
    } else {
      false
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_wire(node_network_name: String, source_node_id: u64, destination_node_id: u64, destination_argument_index: usize) -> bool {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.kernel.select_wire(&node_network_name, source_node_id, destination_node_id, destination_argument_index)
    } else {
      false
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn clear_selection(node_network_name: String) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.kernel.clear_selection(&node_network_name);
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
