use std::ffi::{c_int, c_void};
use dlopen::{symbor::{Library, Symbol}, Error as LibError};
use std::time::Instant;
use crate::renderer::renderer::Renderer;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::api::common_api_types::APIVec3;
use crate::api::common_api_types::APICamera;
use crate::api::common_api_types::APICameraCanonicalView;
use crate::util::transform::Transform;
use crate::api::api_common::CAD_INSTANCE;
use crate::api::api_common::CADInstance;
use crate::api::api_common::to_api_vec3;
use crate::api::api_common::from_api_vec3;
use crate::api::api_common::add_sample_network;
use crate::api::api_common::refresh_structure_designer_auto;
use crate::api::structure_designer::structure_designer_preferences::AtomicStructureVisualization;
use crate::api::api_common::to_api_transform;
use crate::api::api_common::from_api_transform;
use crate::api::api_common::with_mut_cad_instance;
use crate::api::common_api_types::APITransform;
use crate::api::common_api_types::ElementSummary;
use crate::crystolecule::atomic_constants::ATOM_INFO;
use crate::api::api_common::with_cad_instance;
use crate::api::api_common::with_cad_instance_or;
use crate::api::api_common::with_mut_cad_instance_or;

fn to_renderer_camera_canonical_view(view: APICameraCanonicalView) -> crate::renderer::camera::CameraCanonicalView {
  match view {
    APICameraCanonicalView::Custom => crate::renderer::camera::CameraCanonicalView::Custom,
    APICameraCanonicalView::Top => crate::renderer::camera::CameraCanonicalView::Top,
    APICameraCanonicalView::Bottom => crate::renderer::camera::CameraCanonicalView::Bottom,
    APICameraCanonicalView::Front => crate::renderer::camera::CameraCanonicalView::Front,
    APICameraCanonicalView::Back => crate::renderer::camera::CameraCanonicalView::Back,
    APICameraCanonicalView::Left => crate::renderer::camera::CameraCanonicalView::Left,
    APICameraCanonicalView::Right => crate::renderer::camera::CameraCanonicalView::Right,
  }
}

fn to_api_camera_canonical_view(view: crate::renderer::camera::CameraCanonicalView) -> APICameraCanonicalView {
  match view {
    crate::renderer::camera::CameraCanonicalView::Custom => APICameraCanonicalView::Custom,
    crate::renderer::camera::CameraCanonicalView::Top => APICameraCanonicalView::Top,
    crate::renderer::camera::CameraCanonicalView::Bottom => APICameraCanonicalView::Bottom,
    crate::renderer::camera::CameraCanonicalView::Front => APICameraCanonicalView::Front,
    crate::renderer::camera::CameraCanonicalView::Back => APICameraCanonicalView::Back,
    crate::renderer::camera::CameraCanonicalView::Left => APICameraCanonicalView::Left,
    crate::renderer::camera::CameraCanonicalView::Right => APICameraCanonicalView::Right,
  }
}

const INITIAL_VIEWPORT_WIDTH : u32 = 1280;
const INITIAL_VIEWPORT_HEIGHT : u32 = 544;

/// Set the viewport size for rendering
#[flutter_rust_bridge::frb(sync)]
pub fn set_viewport_size(width: u32, height: u32) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      cad_instance.renderer.set_viewport_size(width, height);
    });
  }
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
        renderer: Renderer::new(INITIAL_VIEWPORT_WIDTH, INITIAL_VIEWPORT_HEIGHT).await,
      }
    );

    if let Some(ref mut cad_instance) = CAD_INSTANCE {
      let display_preferences = crate::api::api_common::to_display_preferences(&cad_instance.structure_designer.preferences);
      let background_line_mesh = crate::display::coordinate_system_tessellator::tessellate_background_coordinate_system(
        None,
        &display_preferences.background
      );
      cad_instance.renderer.update_background_mesh(&background_line_mesh);
      add_sample_network(&mut cad_instance.structure_designer);
      cad_instance.structure_designer.apply_node_display_policy(None);
      cad_instance.structure_designer.mark_full_refresh();
      refresh_structure_designer_auto(cad_instance);
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
  v
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

  unsafe {
    // Use regular with_mut_cad_instance which returns Option<()>
    if with_mut_cad_instance(|cad_instance| {
      let background_color = &cad_instance.structure_designer.preferences.background_preferences.background_color;
      let v = cad_instance.renderer.render([
        background_color.x as u8,
        background_color.y as u8,
        background_color.z as u8,
      ]);
      send_texture(texture_ptr, cad_instance.renderer.texture_size.width, cad_instance.renderer.texture_size.height, v);
    }).is_none() {
      // Handle the None case
      let v: Vec<u8> = generate_mock_image(INITIAL_VIEWPORT_WIDTH, INITIAL_VIEWPORT_HEIGHT);
      send_texture(texture_ptr, INITIAL_VIEWPORT_WIDTH, INITIAL_VIEWPORT_HEIGHT, v);
    }
  }

  let duration = start.elapsed(); // Calculate elapsed time
  //println!("Provide texture time: {:?}", duration);

  return duration.as_secs_f64();
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_camera() -> Option<APICamera> {
  unsafe {
    with_cad_instance(|cad_instance| {
      let camera = &cad_instance.renderer.camera;
      APICamera {
        eye: to_api_vec3(&camera.eye),
        target: to_api_vec3(&camera.target),
        up: to_api_vec3(&camera.up),
        aspect: camera.aspect,
        fovy: camera.fovy,
        znear: camera.znear,
        zfar: camera.zfar,
        orthographic: camera.orthographic,
        ortho_half_height: camera.ortho_half_height,
        pivot_point: to_api_vec3(&camera.pivot_point),
      }
    })
    }
  }


#[flutter_rust_bridge::frb(sync)]
pub fn move_camera(eye: APIVec3, target: APIVec3, up: APIVec3) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      cad_instance.renderer.move_camera(&from_api_vec3(&eye), &from_api_vec3(&target), &from_api_vec3(&up));
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn gadget_hit_test(ray_origin: APIVec3, ray_direction: APIVec3) -> Option<i32> {
  unsafe {
    with_cad_instance(|instance| {
      let origin_vec = from_api_vec3(&ray_origin);
      let direction_vec = from_api_vec3(&ray_direction);
      
      instance.structure_designer.gadget_hit_test(origin_vec, direction_vec)
    })?  // Use ? operator here to extract the inner Option
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn gadget_start_drag(handle_index: i32, ray_origin: APIVec3, ray_direction: APIVec3) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let origin_vec = from_api_vec3(&ray_origin);
      let direction_vec = from_api_vec3(&ray_direction);
      
      cad_instance.structure_designer.gadget_start_drag(handle_index, origin_vec, direction_vec);
      
      // Call refresh_renderer inside the closure to access cad_instance
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn gadget_drag(handle_index: i32, ray_origin: APIVec3, ray_direction: APIVec3) {
  unsafe {
    with_mut_cad_instance(|instance| {
      let origin_vec = from_api_vec3(&ray_origin);
      let direction_vec = from_api_vec3(&ray_direction);      
      instance.structure_designer.gadget_drag(handle_index, origin_vec, direction_vec);
      refresh_structure_designer_auto(instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn gadget_end_drag() {
  unsafe {
    with_mut_cad_instance(|instance| {
      instance.structure_designer.gadget_end_drag();
      refresh_structure_designer_auto(instance);
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn sync_gadget_data() -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |instance| {
          instance.structure_designer.sync_gadget_data()
      },
      false // Default return value if CAD_INSTANCE is None
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_camera_transform() -> APITransform {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let transform = cad_instance.renderer.get_camera_transform();
        to_api_transform(&transform)
      },
      // Return identity transform as fallback
      to_api_transform(&Transform::default())
    )
  }
}

/// Adjusts the pivot point of the camera based on a raycast into the scene
/// 
/// This function performs the following steps:
/// 1. Traces a ray into the scene based on the active editor
/// 2. If the ray hits something that point will be the new pivot point
/// 
/// # Arguments
/// 
/// * `ray_origin` - The origin point of the ray in world space
/// * `ray_direction` - The direction vector of the ray (does not need to be normalized)
#[flutter_rust_bridge::frb(sync)]
pub fn adjust_camera_target(ray_origin: APIVec3, ray_direction: APIVec3) {
  let ray_origin = from_api_vec3(&ray_origin);
  let ray_direction = from_api_vec3(&ray_direction);
  
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      // Get the current camera eye position
      let _eye = cad_instance.renderer.camera.eye;
      
      // Perform raytracing based on the active editor using space filling visualization for camera targeting
      let mut hit_distance = cad_instance.structure_designer.raytrace(&ray_origin, &ray_direction, &AtomicStructureVisualization::SpaceFilling);

      // Fallback: Calculate where input ray intersects XY plane
      if hit_distance.is_none() {
        let ray_can_hit_xy = ray_direction.z.abs() > 1e-6; // Avoid division by zero
        let xy_dist_from_ray = if ray_can_hit_xy { -ray_origin.z / ray_direction.z } else { 0.0 };
        // Check that the intersection is in front of the ray origin
        if ray_can_hit_xy && xy_dist_from_ray > 0.0 {
          hit_distance = Some(xy_dist_from_ray);
        }        
      }

      // If we hit something, adjust the pivot point
      if let Some(distance) = hit_distance {
        // Calculate the hit point
        let hit_point = ray_origin + ray_direction * distance;        
        
        cad_instance.renderer.camera.pivot_point = hit_point;
        
        // Update the camera buffer
        cad_instance.renderer.update_camera_buffer();

        cad_instance.structure_designer.mark_lightweight_refresh();
        refresh_structure_designer_auto(cad_instance);    
        return;
      }
    });
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_camera_transform(transform: APITransform) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      let transform = from_api_transform(&transform);
      cad_instance.renderer.set_camera_transform(&transform);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

/// Set the camera to use orthographic or perspective projection
#[flutter_rust_bridge::frb(sync)]
pub fn set_orthographic_mode(orthographic: bool) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      cad_instance.renderer.set_orthographic_mode(orthographic);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

/// Get whether the camera is using orthographic projection
#[flutter_rust_bridge::frb(sync)]
pub fn is_orthographic() -> bool {
  unsafe {
    with_cad_instance_or(
      |cad_instance| cad_instance.renderer.is_orthographic(),
      false // Default to false if CAD_INSTANCE is None
    )
  }
}

/// Set the orthographic half height (controls zoom level in orthographic mode)
#[flutter_rust_bridge::frb(sync)]
pub fn set_ortho_half_height(half_height: f64) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      cad_instance.renderer.set_ortho_half_height(half_height);
      refresh_structure_designer_auto(cad_instance);
    });
  }
}

/// Get the current orthographic half height
#[flutter_rust_bridge::frb(sync)]
pub fn get_ortho_half_height() -> f64 {
  unsafe {
    with_cad_instance_or(
      |cad_instance| cad_instance.renderer.get_ortho_half_height(),
      10.0 // Default value
    )
  }
}

/// Get the canonical view of the current camera orientation
/// Returns one of: Custom, Top, Bottom, Front, Back, Left, Right
#[flutter_rust_bridge::frb(sync)]
pub fn get_camera_canonical_view() -> APICameraCanonicalView {
  unsafe {
    with_cad_instance_or(
      |cad_instance| to_api_camera_canonical_view(cad_instance.renderer.camera.get_canonical_view()),
      // Default to Custom if no CAD instance exists
      APICameraCanonicalView::Custom
    )
  }
}

/// Set the camera to a canonical view orientation
/// Accepts one of: Custom, Top, Bottom, Front, Back, Left, Right
/// If Custom is provided, no changes will be made to the camera orientation
#[flutter_rust_bridge::frb(sync)]
pub fn set_camera_canonical_view(view: APICameraCanonicalView) {
  unsafe {
    with_mut_cad_instance(|cad_instance| {
      cad_instance.renderer.set_camera_canonical_view(to_renderer_camera_canonical_view(view));
      refresh_structure_designer_auto(cad_instance);
    });
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
      atomic_number: atom_info.atomic_number as i16,
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
    
    /*
    Simulation is not fully functional yet, it is disabled right now.
    // Initialize simulation environment to pre-load Python runtime and force field
    match crate::common::simulation::initialize_simulation() {
        Ok(message) => println!("Simulation initialization: {}", message),
        Err(error) => println!("Warning: Simulation initialization failed: {}", error),
    }
    */
    
    // Initialize expression function registries for better performance
    crate::expr::validation::init_function_registries();
}
