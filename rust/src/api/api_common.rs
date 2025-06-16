use super::common_api_types::{APIVec3, APIVec2, APIIVec3, APIIVec2, APITransform};
use glam::f64::DVec3;
use glam::i32::IVec3;
use glam::i32::IVec2;
use glam::f64::DVec2;
use glam::DQuat;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::scene_composer::scene_composer::SceneComposer;
use crate::renderer::renderer::Renderer;
use crate::api::common_api_types::Editor;
use crate::util::transform::Transform;

pub fn to_api_vec3(v: &DVec3) -> APIVec3 {
    return APIVec3{
      x: v.x,
      y: v.y,
      z: v.z
    }
  }
  
  pub fn from_api_vec3(v: &APIVec3) -> DVec3 {
    return DVec3{
      x: v.x,
      y: v.y,
      z: v.z
    }
  }
  
  pub fn to_api_ivec2(v: &IVec2) -> APIIVec2 {
    return APIIVec2{
      x: v.x,
      y: v.y,
    }
  }
  
  pub fn from_api_ivec2(v: &APIIVec2) -> IVec2 {
    return IVec2{
      x: v.x,
      y: v.y,
    }
  }

  pub fn to_api_ivec3(v: &IVec3) -> APIIVec3 {
    return APIIVec3{
      x: v.x,
      y: v.y,
      z: v.z
    }
  }
  
  pub fn from_api_ivec3(v: &APIIVec3) -> IVec3 {
    return IVec3{
      x: v.x,
      y: v.y,
      z: v.z
    }
  }
  
  pub fn to_api_vec2(v: &DVec2) -> APIVec2 {
    return APIVec2{
      x: v.x,
      y: v.y,
    }
  }
  
  pub fn from_api_vec2(v: &APIVec2) -> DVec2 {
    return DVec2 {
      x: v.x,
      y: v.y,
    }
  }
  
  pub fn to_api_transform(transform: &Transform) -> APITransform {
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
  
  pub fn from_api_transform(api_transform: &APITransform) -> Transform {
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

  pub struct CADInstance {
    pub structure_designer: StructureDesigner,
    pub scene_composer: SceneComposer,
    pub renderer: Renderer,
    pub active_editor: Editor, // This one refreshes itself into the Renderer when the refresh_renderer function is called.
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
  /// ```
  /// use crate::api::api_common::with_mut_cad_instance;
  /// 
  /// unsafe {
  ///     with_mut_cad_instance(|instance| {
  ///         instance.renderer.set_viewport_size(width, height);
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
  /// ```
  /// use crate::api::api_common::with_mut_cad_instance_or;
  /// 
  /// unsafe {
  ///     let result = with_mut_cad_instance_or(
  ///         |instance| {
  ///             instance.renderer.update_something();
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
  /// ```
  /// use crate::api::api_common::with_cad_instance;
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
  /// ```
  /// use crate::api::api_common::with_cad_instance_or;
  /// 
  /// unsafe {
  ///     let result = with_cad_instance_or(
  ///         |instance| instance.renderer.get_viewport_size(),
  ///         (0, 0)
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
    kernel.set_active_node_network_name(Some("sample".to_string()));
    let cuboid_id = kernel.add_node("cuboid", DVec2::new(30.0, 30.0));
    let sphere_id = kernel.add_node("sphere", DVec2::new(100.0, 100.0));
    let diff_id_1 = kernel.add_node("diff", DVec2::new(300.0, 80.0));
    let diff_id_2 = kernel.add_node("diff", DVec2::new(500.0, 80.0));
  
    kernel.connect_nodes(cuboid_id, diff_id_1, 0);
    kernel.connect_nodes(sphere_id, diff_id_1, 1);
    kernel.connect_nodes(diff_id_1, diff_id_2, 1);
}

  pub fn refresh_structure_designer(cad_instance: &mut CADInstance, lightweight: bool) {
    let scene = cad_instance.structure_designer.generate_scene(lightweight);
    cad_instance.renderer.refresh(
      &scene,
      lightweight,
      &cad_instance.structure_designer.preferences.geometry_visualization_preferences
    );
    cad_instance.structure_designer.set_last_generated_structure_designer_scene(scene);
  }

  pub fn refresh_renderer(cad_instance: &mut CADInstance, lightweight: bool) {
    match cad_instance.active_editor {
      Editor::StructureDesigner => {
        refresh_structure_designer(cad_instance, lightweight);
      },
      Editor::SceneComposer => {
        cad_instance.renderer.refresh(
          &cad_instance.scene_composer,
          lightweight,
          &cad_instance.structure_designer.preferences.geometry_visualization_preferences
        );
      },
      Editor::None => {}
    }
  }
