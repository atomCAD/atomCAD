use std::ffi::{c_int, c_void};
use dlopen::{symbor::{Library, Symbol}, Error as LibError};
use std::time::Instant;
use crate::renderer::dummy_renderer;
use crate::renderer::renderer::Renderer;
use crate::renderer::camera::Camera;
use crate::kernel::kernel::Kernel;
use glam::f32::Vec3;

pub struct APIVec3 {
  pub x: f32,
  pub y: f32,
  pub z: f32,
}

fn to_api_vec3(v: &Vec3) -> APIVec3 {
  return APIVec3{
    x: v.x,
    y: v.y,
    z: v.z
  }
}

fn vec3_to_api(v: &APIVec3) -> Vec3 {
  return Vec3{
    x: v.x,
    y: v.y,
    z: v.z
  }
}

pub struct APICamera {
  pub eye: APIVec3,
  pub target: APIVec3,
  pub up: APIVec3,
  pub aspect: f32,
  pub fovy: f32, // in radians
  pub znear: f32,
  pub zfar: f32,
}

const IMAGE_WIDTH : u32 = 1280;
const IMAGE_HEIGHT : u32 = 704;

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
      add_sample_model(&mut cad_instance.kernel);
      cad_instance.renderer.refresh(cad_instance.kernel.get_model())
    }
  }
}

fn add_sample_model(kernel: &mut Kernel) {
  let atom_id1 = kernel.add_atom(6, Vec3::new(-1.3, 0.0, 0.0));
  let atom_id2 = kernel.add_atom(6, Vec3::new(1.3, 0.0, 0.0));
  kernel.add_atom(6, Vec3::new(1.3, 3.0, 0.0));
  kernel.add_bond(atom_id1, atom_id2, 1);
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
      cad_instance.renderer.move_camera(&vec3_to_api(&eye), &vec3_to_api(&target), &vec3_to_api(&up));
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn add_atom(atomic_number: i32, position: APIVec3) {
  unsafe {
    if let Some(cad_instance) = &mut CAD_INSTANCE {
      cad_instance.kernel.add_atom(atomic_number, vec3_to_api(&position));
      cad_instance.renderer.refresh(cad_instance.kernel.get_model());
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
