use std::ffi::{c_int, c_void};
use dlopen::{symbor::{Library, Symbol}, Error as LibError};
use std::time::Instant;
use crate::renderer::renderer::Renderer;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::scene_composer::scene_composer::SceneComposer;
use crate::api::common_api_types::APIVec3;
use crate::api::common_api_types::APICamera;
use crate::api::common_api_types::Editor;
use crate::util::transform::Transform;
use crate::api::api_common::CAD_INSTANCE;
use crate::api::api_common::CADInstance;
use crate::api::api_common::to_api_vec3;
use crate::api::api_common::from_api_vec3;
use crate::api::api_common::add_sample_network;
use crate::api::api_common::refresh_renderer;
use crate::api::api_common::to_api_transform;
use crate::api::api_common::from_api_transform;
use crate::api::common_api_types::APITransform;
use crate::api::common_api_types::ElementSummary;
use crate::api::api_common::refresh_structure_designer;
use crate::common::common_constants::ATOM_INFO;

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
      refresh_structure_designer(cad_instance, false);
    }
  }
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
        orthographic: camera.orthographic,
        ortho_half_height: camera.ortho_half_height,
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
pub fn find_pivot_point(ray_start: APIVec3, ray_dir: APIVec3) -> APIVec3 {
  unsafe {
    if let Some(cad_instance) = &CAD_INSTANCE {
      let model = &cad_instance.scene_composer.model.model;
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
pub fn gadget_start_drag(handle_index: i32, ray_origin: APIVec3, ray_direction: APIVec3) {
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

    refresh_renderer(instance, false);
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn gadget_drag(handle_index: i32, ray_origin: APIVec3, ray_direction: APIVec3) {
  unsafe {
    let Some(mut instance) = CAD_INSTANCE.as_mut() else { return };

    match instance.active_editor {
      Editor::StructureDesigner => {
        instance.structure_designer.gadget_drag(handle_index, from_api_vec3(&ray_origin), from_api_vec3(&ray_direction));
        refresh_renderer(instance, true);
      },
      Editor::SceneComposer => {
        instance.scene_composer.gadget_drag(handle_index, from_api_vec3(&ray_origin), from_api_vec3(&ray_direction));

        if instance.scene_composer.model.selected_frame_gadget.as_ref().unwrap().frame_locked_to_atoms {
          let selected_clusters_transform = instance.scene_composer.model.selected_frame_gadget.as_ref().unwrap().get_selected_clusters_transform();
          instance.renderer.set_selected_clusters_transform(&selected_clusters_transform);
        }
        refresh_renderer(instance, true);
      },
      Editor::None => {}
    }
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn gadget_end_drag() {
  unsafe {
    let Some(mut instance) = CAD_INSTANCE.as_mut() else { return };

    match instance.active_editor {
      Editor::StructureDesigner => {
        instance.structure_designer.gadget_end_drag();
      },
      Editor::SceneComposer => {
        instance.renderer.set_selected_clusters_transform(&Transform::default());
        instance.scene_composer.gadget_end_drag();
      },
      Editor::None => {}
    }
    refresh_renderer(instance, false);
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn sync_gadget_data() -> bool {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      match instance.active_editor {
        Editor::StructureDesigner => {
          return instance.structure_designer.sync_gadget_data();
        },
        Editor::SceneComposer => {
          instance.scene_composer.model.sync_gadget_to_model();
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

/// Adjusts the camera target based on a raycast into the scene
/// 
/// This function performs the following steps:
/// 1. Traces a ray into the scene based on the active editor
/// 2. If the ray hits something, calculates the camera target depth
/// 3. Adjusts the camera target point while maintaining its direction from the eye
/// 
/// # Arguments
/// 
/// * `ray_origin` - The origin point of the ray in world space
/// * `ray_direction` - The direction vector of the ray (does not need to be normalized)
/// 
/// # Returns
/// 
/// `true` if the camera target was adjusted, `false` otherwise
#[no_mangle]
pub fn adjust_camera_target(ray_origin: APIVec3, ray_direction: APIVec3) -> bool {
  let ray_origin = from_api_vec3(&ray_origin);
  let ray_direction = from_api_vec3(&ray_direction);
  
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      // Get the current camera eye position
      let eye = instance.renderer.camera.eye;
      
      // Perform raytracing based on the active editor
      let hit_distance = match instance.active_editor {
        Editor::StructureDesigner => {
          instance.structure_designer.raytrace(&ray_origin, &ray_direction)
        },
        Editor::SceneComposer => {
          instance.scene_composer.raytrace(&ray_origin, &ray_direction)
        },
        Editor::None => None,
      };
      
      println!("Hit distance: {:?}", hit_distance);

      // Get the camera forward vector (normalized)
      let camera_forward = (instance.renderer.camera.target - eye).normalize();

      // If we hit something, adjust the camera target
      if let Some(distance) = hit_distance {
        // Calculate the hit point
        let hit_point = ray_origin + ray_direction * distance;
        
        // Calculate the vector from eye to hit point
        let eye_to_hit = hit_point - eye;
        
        // Calculate the target depth (projection of eye_to_hit onto camera_forward)
        let target_depth = eye_to_hit.dot(camera_forward);
        
        // Adjust the camera target by setting it at the new depth along the same direction
        instance.renderer.camera.target = eye + camera_forward * target_depth;
        
        // Update the camera buffer
        instance.renderer.update_camera_buffer();
        
        return true;
      } else {
        // Fallback: Check if both the camera forward vector and input ray hit the XZ plane
        
        // Calculate where camera forward vector intersects XZ plane
        let camera_can_hit_xz = camera_forward.y.abs() > 1e-6; // Avoid division by zero
        let xz_dist_from_camera = if camera_can_hit_xz { -eye.y / camera_forward.y } else { 0.0 };
        let camera_hits_xz = camera_can_hit_xz && 
                           // Check that the intersection is in front of the camera
                           xz_dist_from_camera > 0.0;

        // Calculate where input ray intersects XZ plane
        let ray_can_hit_xz = ray_direction.y.abs() > 1e-6; // Avoid division by zero
        let xz_dist_from_ray = if ray_can_hit_xz { -ray_origin.y / ray_direction.y } else { 0.0 };
        let ray_hits_xz = ray_can_hit_xz && 
                       // Check that the intersection is in front of the ray origin
                       xz_dist_from_ray > 0.0;
        
        // If both hit the XZ plane, use the camera forward intersection as new target
        if camera_hits_xz && ray_hits_xz {
          // Calculate the point where camera forward vector intersects XZ plane
          let xz_intersection = eye + camera_forward * xz_dist_from_camera;
          
          // Set the new target
          instance.renderer.camera.target = xz_intersection;
          
          // Update the camera buffer
          instance.renderer.update_camera_buffer();
          
          return true;
        }
      }
    }
  }
  
  false
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_camera_transform(transform: APITransform) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      let transform = from_api_transform(&transform);
      instance.renderer.set_camera_transform(&transform);
      refresh_renderer(instance, false);
    }
  }
}

/// Set the camera to use orthographic or perspective projection
#[flutter_rust_bridge::frb(sync)]
pub fn set_orthographic_mode(orthographic: bool) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.renderer.set_orthographic_mode(orthographic);
      refresh_renderer(instance, false);
    }
  }
}

/// Get whether the camera is using orthographic projection
#[flutter_rust_bridge::frb(sync)]
pub fn is_orthographic() -> bool {
  unsafe {
    if let Some(instance) = &CAD_INSTANCE {
      return instance.renderer.is_orthographic();
    }
    return false;
  }
}

/// Set the orthographic half height (controls zoom level in orthographic mode)
#[flutter_rust_bridge::frb(sync)]
pub fn set_ortho_half_height(half_height: f64) {
  unsafe {
    if let Some(instance) = &mut CAD_INSTANCE {
      instance.renderer.set_ortho_half_height(half_height);
      refresh_renderer(instance, false);
    }
  }
}

/// Get the current orthographic half height
#[flutter_rust_bridge::frb(sync)]
pub fn get_ortho_half_height() -> f64 {
  unsafe {
    if let Some(instance) = &CAD_INSTANCE {
      return instance.renderer.get_ortho_half_height();
    }
    return 10.0; // Default value
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn greet(name: String) -> String {
  name + " from Rust! 安 😊"
}

/// Returns a list of all chemical elements with their atomic numbers and names,
/// ordered by atomic number
#[flutter_rust_bridge::frb(sync)]
pub fn get_all_elements() -> Vec<ElementSummary> {
  // Get all chemical elements from the ATOM_INFO map
  // Convert to Vec, sort by atomic_number, and map to ElementSummary
  let mut elements: Vec<ElementSummary> = ATOM_INFO.values()
    .map(|atom_info| ElementSummary {
      atomic_number: atom_info.atomic_number,
      element_name: atom_info.element_name.clone(),
    })
    .collect();
  
  // Sort by atomic number to ensure correct order
  elements.sort_by_key(|element| element.atomic_number);
  
  elements
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
