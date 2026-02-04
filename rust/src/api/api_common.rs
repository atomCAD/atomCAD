use super::common_api_types::{APIVec3, APIVec2, APIIVec3, APIIVec2, APITransform};
use glam::f64::DVec3;
use glam::i32::IVec3;
use glam::i32::IVec2;
use glam::f64::DVec2;
use glam::DQuat;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::structure_designer_changes::StructureDesignerChanges;
use crate::renderer::renderer::Renderer;
use crate::util::transform::Transform;
use crate::api::structure_designer::structure_designer_preferences as api_prefs;
use crate::display::preferences as display_prefs;
use crate::structure_designer::camera_settings::CameraSettings;

pub fn to_api_vec3(v: &DVec3) -> APIVec3 {
    APIVec3{
      x: v.x,
      y: v.y,
      z: v.z
    }
  }

  fn to_display_mesh_smoothing(smoothing: &api_prefs::MeshSmoothing) -> display_prefs::MeshSmoothing {
    match smoothing {
      api_prefs::MeshSmoothing::Smooth => display_prefs::MeshSmoothing::Smooth,
      api_prefs::MeshSmoothing::Sharp => display_prefs::MeshSmoothing::Sharp,
      api_prefs::MeshSmoothing::SmoothingGroupBased => display_prefs::MeshSmoothing::SmoothingGroupBased,
    }
  }

  fn to_display_atomic_structure_visualization(v: &api_prefs::AtomicStructureVisualization) -> display_prefs::AtomicStructureVisualization {
    match v {
      api_prefs::AtomicStructureVisualization::BallAndStick => display_prefs::AtomicStructureVisualization::BallAndStick,
      api_prefs::AtomicStructureVisualization::SpaceFilling => display_prefs::AtomicStructureVisualization::SpaceFilling,
    }
  }

  fn to_display_atomic_rendering_method(m: &api_prefs::AtomicRenderingMethod) -> display_prefs::AtomicRenderingMethod {
    match m {
      api_prefs::AtomicRenderingMethod::TriangleMesh => display_prefs::AtomicRenderingMethod::TriangleMesh,
      api_prefs::AtomicRenderingMethod::Impostors => display_prefs::AtomicRenderingMethod::Impostors,
    }
  }

  pub fn to_display_preferences(preferences: &api_prefs::StructureDesignerPreferences) -> display_prefs::DisplayPreferences {
    display_prefs::DisplayPreferences {
      geometry_visualization: display_prefs::GeometryVisualizationPreferences {
        wireframe_geometry: preferences.geometry_visualization_preferences.wireframe_geometry,
        mesh_smoothing: to_display_mesh_smoothing(&preferences.geometry_visualization_preferences.mesh_smoothing),
        display_camera_target: preferences.geometry_visualization_preferences.display_camera_target,
      },
      atomic_structure_visualization: display_prefs::AtomicStructureVisualizationPreferences {
        visualization: to_display_atomic_structure_visualization(&preferences.atomic_structure_visualization_preferences.visualization),
        rendering_method: to_display_atomic_rendering_method(&preferences.atomic_structure_visualization_preferences.rendering_method),
        ball_and_stick_cull_depth: preferences.atomic_structure_visualization_preferences.ball_and_stick_cull_depth,
        space_filling_cull_depth: preferences.atomic_structure_visualization_preferences.space_filling_cull_depth,
      },
      background: display_prefs::BackgroundPreferences {
        show_grid: preferences.background_preferences.show_grid,
        grid_size: preferences.background_preferences.grid_size,
        grid_color: [
          preferences.background_preferences.grid_color.x as u8,
          preferences.background_preferences.grid_color.y as u8,
          preferences.background_preferences.grid_color.z as u8,
        ],
        grid_strong_color: [
          preferences.background_preferences.grid_strong_color.x as u8,
          preferences.background_preferences.grid_strong_color.y as u8,
          preferences.background_preferences.grid_strong_color.z as u8,
        ],
        show_lattice_axes: preferences.background_preferences.show_lattice_axes,
        show_lattice_grid: preferences.background_preferences.show_lattice_grid,
        lattice_grid_color: [
          preferences.background_preferences.lattice_grid_color.x as u8,
          preferences.background_preferences.lattice_grid_color.y as u8,
          preferences.background_preferences.lattice_grid_color.z as u8,
        ],
        lattice_grid_strong_color: [
          preferences.background_preferences.lattice_grid_strong_color.x as u8,
          preferences.background_preferences.lattice_grid_strong_color.y as u8,
          preferences.background_preferences.lattice_grid_strong_color.z as u8,
        ],
        drawing_plane_grid_color: [
          preferences.background_preferences.drawing_plane_grid_color.x as u8,
          preferences.background_preferences.drawing_plane_grid_color.y as u8,
          preferences.background_preferences.drawing_plane_grid_color.z as u8,
        ],
        drawing_plane_grid_strong_color: [
          preferences.background_preferences.drawing_plane_grid_strong_color.x as u8,
          preferences.background_preferences.drawing_plane_grid_strong_color.y as u8,
          preferences.background_preferences.drawing_plane_grid_strong_color.z as u8,
        ],
      },
    }
  }
  
  pub fn from_api_vec3(v: &APIVec3) -> DVec3 {
    DVec3{
      x: v.x,
      y: v.y,
      z: v.z
    }
  }
  
  pub fn to_api_ivec2(v: &IVec2) -> APIIVec2 {
    APIIVec2{
      x: v.x,
      y: v.y,
    }
  }
  
  pub fn from_api_ivec2(v: &APIIVec2) -> IVec2 {
    IVec2{
      x: v.x,
      y: v.y,
    }
  }

  pub fn to_api_ivec3(v: &IVec3) -> APIIVec3 {
    APIIVec3{
      x: v.x,
      y: v.y,
      z: v.z
    }
  }
  
  pub fn from_api_ivec3(v: &APIIVec3) -> IVec3 {
    IVec3{
      x: v.x,
      y: v.y,
      z: v.z
    }
  }
  
  pub fn to_api_vec2(v: &DVec2) -> APIVec2 {
    APIVec2{
      x: v.x,
      y: v.y,
    }
  }
  
  pub fn from_api_vec2(v: &APIVec2) -> DVec2 {
    DVec2 {
      x: v.x,
      y: v.y,
    }
  }
  
  pub fn to_api_transform(transform: &Transform) -> APITransform {
    // Convert quaternion to Euler angles (intrinsic rotation in radians)
    let rotation_euler = transform.rotation.to_euler(glam::EulerRot::XYZ);
    
    APITransform {
      translation: to_api_vec3(&transform.translation),
      rotation: APIVec3 {
        x: rotation_euler.0.to_degrees(), // roll (x-axis rotation)
        y: rotation_euler.1.to_degrees(), // pitch (y-axis rotation)
        z: rotation_euler.2.to_degrees(), // yaw (z-axis rotation)
      },
    }
  }
  
  pub fn from_api_transform(api_transform: &APITransform) -> Transform {
    // Convert Euler angles (intrinsic XYZ in degrees) to quaternion
    let rotation = DQuat::from_euler(
      glam::EulerRot::XYZ, 
      api_transform.rotation.x.to_radians(), 
      api_transform.rotation.y.to_radians(), 
      api_transform.rotation.z.to_radians()
    );
    
    Transform {
      translation: from_api_vec3(&api_transform.translation),
      rotation,
    }
  }

  pub struct CADInstance {
    pub structure_designer: StructureDesigner,
    pub renderer: Renderer,
  }

  pub static mut CAD_INSTANCE: Option<CADInstance> = None;

  /// Helper function to safely access the CAD_INSTANCE static variable with mutable access
  /// 
  /// This function takes a closure that will be called with a mutable reference to the CAD instance
  /// if it exists. This is a thread-safe way to access the static variable in Rust 2024.
  /// 
  /// # Safety
  /// 
  /// This function uses unsafe code to access a mutable static.
  /// The caller must ensure proper synchronization when accessing CAD_INSTANCE.
  /// 
  /// # Examples
  /// 
  /// ```no_run
  /// use rust_lib_flutter_cad::api::api_common::with_mut_cad_instance;
  /// 
  /// unsafe {
  ///     with_mut_cad_instance(|instance| {
  ///         instance.renderer.set_viewport_size(800, 600);
  ///     });
  /// }
  /// ```
  pub unsafe fn with_mut_cad_instance<F, R>(f: F) -> Option<R>
  where
      F: FnOnce(&mut CADInstance) -> R,
  { unsafe {
      use std::ptr::addr_of_mut;
      
      let cad_instance_ptr = addr_of_mut!(CAD_INSTANCE);
      (*cad_instance_ptr).as_mut().map(f)
  }}
  
  /// Helper function to safely access the CAD_INSTANCE with mutable access and a default value
  /// 
  /// Similar to `with_mut_cad_instance` but returns a default value if CAD_INSTANCE is None
  /// 
  /// # Safety
  /// 
  /// This function uses unsafe code to access a mutable static.
  /// The caller must ensure proper synchronization when accessing CAD_INSTANCE.
  /// 
  /// # Examples
  /// 
  /// ```no_run
  /// use rust_lib_flutter_cad::api::api_common::with_mut_cad_instance_or;
  /// 
  /// unsafe {
  ///     let success = with_mut_cad_instance_or(
  ///         |instance| {
  ///             // perform operation
  ///             true // success
  ///         },
  ///         false // failure
  ///     );
  /// }
  /// ```
  pub unsafe fn with_mut_cad_instance_or<F, R>(f: F, default: R) -> R
  where
      F: FnOnce(&mut CADInstance) -> R,
  { unsafe {
      use std::ptr::addr_of_mut;
      
      let cad_instance_ptr = addr_of_mut!(CAD_INSTANCE);
      (*cad_instance_ptr).as_mut().map(f).unwrap_or(default)
  }}
  
  /// Helper function to safely access the CAD_INSTANCE static variable with immutable access
  /// 
  /// This function takes a closure that will be called with an immutable reference to the CAD instance
  /// if it exists. This is a thread-safe way to access the static variable in Rust 2024.
  /// 
  /// # Safety
  /// 
  /// This function uses unsafe code to access a mutable static.
  /// The caller must ensure proper synchronization when accessing CAD_INSTANCE.
  /// 
  /// # Examples
  /// 
  /// ```no_run
  /// use rust_lib_flutter_cad::api::api_common::with_cad_instance;
  /// 
  /// unsafe {
  ///     with_cad_instance(|instance| {
  ///         let camera = &instance.renderer.camera;
  ///         // use camera data
  ///     });
  /// }
  /// ```
  pub unsafe fn with_cad_instance<F, R>(f: F) -> Option<R>
  where
      F: FnOnce(&CADInstance) -> R,
  { unsafe {
      use std::ptr::addr_of;
      
      let cad_instance_ptr = addr_of!(CAD_INSTANCE);
      (*cad_instance_ptr).as_ref().map(f)
  }}
  
  /// Helper function to safely access the CAD_INSTANCE with immutable access and a default value
  /// 
  /// Similar to `with_cad_instance` but returns a default value if CAD_INSTANCE is None
  /// 
  /// # Safety
  /// 
  /// This function uses unsafe code to access a mutable static.
  /// The caller must ensure proper synchronization when accessing CAD_INSTANCE.
  /// 
  /// # Examples
  /// 
  /// ```no_run
  /// use rust_lib_flutter_cad::api::api_common::with_cad_instance_or;
  /// 
  /// unsafe {
  ///     let aspect_ratio = with_cad_instance_or(
  ///         |instance| instance.renderer.camera.aspect,
  ///         1.0
  ///     );
  /// }
  /// ```
  pub unsafe fn with_cad_instance_or<F, R>(f: F, default: R) -> R
  where
      F: FnOnce(&CADInstance) -> R,
  { unsafe {
      use std::ptr::addr_of;
      
      let cad_instance_ptr = addr_of!(CAD_INSTANCE);
      (*cad_instance_ptr).as_ref().map(f).unwrap_or(default)
  }}


pub fn add_sample_network(kernel: &mut StructureDesigner) {
    kernel.add_node_network("sample");
    // New networks don't have camera settings, ignore return value
    let _ = kernel.set_active_node_network_name(Some("sample".to_string()));
    kernel.add_node("cuboid", DVec2::new(30.0, 30.0));
}

  pub fn refresh_structure_designer(cad_instance: &mut CADInstance, changes: &StructureDesignerChanges) {
    //let _timer = Timer::new(&format!("refresh_structure_designer changes: {:?}", changes.mode));
    
    cad_instance.structure_designer.refresh(changes);
    
    // Get lightweight flag from the changes for renderer
    let renderer_lightweight = changes.is_lightweight();

    let display_preferences = to_display_preferences(&cad_instance.structure_designer.preferences);
    let (lightweight_mesh, gadget_line_mesh, main_mesh, wireframe_mesh, atom_impostor_mesh, bond_impostor_mesh) =
      crate::display::scene_tessellator::tessellate_scene_content(
        &cad_instance.structure_designer.last_generated_structure_designer_scene,
        &cad_instance.renderer.camera,
        renderer_lightweight,
        &display_preferences
      );

    cad_instance.renderer.update_all_gpu_meshes(
      &lightweight_mesh,
      &gadget_line_mesh,
      &main_mesh,
      &wireframe_mesh,
      &atom_impostor_mesh,
      &bond_impostor_mesh,
      !renderer_lightweight
    );

    if !renderer_lightweight {
      let background_line_mesh = crate::display::coordinate_system_tessellator::tessellate_background_coordinate_system(
        cad_instance.structure_designer.last_generated_structure_designer_scene.unit_cell.as_ref(),
        &display_preferences.background
      );
      cad_instance.renderer.update_background_mesh(&background_line_mesh);
    }
  }

  /// Convenience wrapper that gets pending changes and refreshes both StructureDesigner and Renderer
  /// This is the standard way to refresh after any StructureDesigner operation
  pub fn refresh_structure_designer_auto(cad_instance: &mut CADInstance) {
    let changes = cad_instance.structure_designer.get_pending_changes();
    refresh_structure_designer(cad_instance, &changes);
  }

  /// Syncs the current camera state to the active node network's camera_settings.
  /// Call this after any camera modification to keep the network's settings up-to-date.
  /// Also marks the design as dirty since camera settings are saved per network.
  pub fn sync_camera_to_active_network(cad_instance: &mut CADInstance) {
    let camera = &cad_instance.renderer.camera;
    if let Some(network) = cad_instance.structure_designer.get_active_node_network_mut() {
      network.camera_settings = Some(CameraSettings {
        eye: camera.eye,
        target: camera.target,
        up: camera.up,
        orthographic: camera.orthographic,
        ortho_half_height: camera.ortho_half_height,
        pivot_point: camera.pivot_point,
      });
    }
    // Camera settings are saved per network, so mark design as dirty
    cad_instance.structure_designer.set_dirty(true);
  }

  /// Applies camera settings to the renderer (if Some).
  /// Call this after any StructureDesigner method that returns Option<CameraSettings>.
  pub fn apply_camera_settings(renderer: &mut Renderer, settings: Option<&CameraSettings>) {
    if let Some(s) = settings {
      renderer.camera.eye = s.eye;
      renderer.camera.target = s.target;
      renderer.camera.up = s.up;
      renderer.camera.orthographic = s.orthographic;
      renderer.camera.ortho_half_height = s.ortho_half_height;
      renderer.camera.pivot_point = s.pivot_point;
      renderer.update_camera_buffer();
    }
  }

















