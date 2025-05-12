use super::common_api_types::{APIVec3, APIVec2, APIIVec3, APITransform};
use glam::f64::DVec3;
use glam::i32::IVec3;
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
    cad_instance.renderer.refresh(&scene, lightweight);
    cad_instance.structure_designer.set_last_generated_structure_designer_scene(scene);
  }

  pub fn refresh_renderer(cad_instance: &mut CADInstance, lightweight: bool) {
    match cad_instance.active_editor {
      Editor::StructureDesigner => {
        refresh_structure_designer(cad_instance, lightweight);
      },
      Editor::SceneComposer => {
        cad_instance.renderer.refresh(&cad_instance.scene_composer, lightweight);
      },
      Editor::None => {}
    }
  }
