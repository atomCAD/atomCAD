use crate::structure_designer::evaluator::network_evaluator::{
  NetworkEvaluationContext, NetworkEvaluator
};
use crate::structure_designer::evaluator::network_result::{
  error_in_input, input_missing_error, GeometrySummary, NetworkResult
};
use crate::structure_designer::geo_tree::GeoNode;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec3_serializer;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DVec3;
use glam::DQuat;
use std::f64::consts::PI;
use crate::util::transform::Transform;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::common::gadget::Gadget;
use crate::structure_designer::utils::xyz_gadget_utils;
use crate::renderer::mesh::Mesh;
use crate::structure_designer::common_constants;

#[derive(Debug, Clone)]
pub struct GeoTransEvalCache {
  pub input_frame_transform: Transform,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GeoTransData {
  #[serde(with = "ivec3_serializer")]
  pub translation: IVec3,
  #[serde(with = "ivec3_serializer")]
  pub rotation: IVec3, // intrinsic euler angles where 1 increment means 90 degrees.
  pub transform_only_frame: bool, // If true, only the reference frame is transformed, the geometry remains in place.
}

impl NodeData for GeoTransData {
    fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
        let eval_cache = structure_designer.last_generated_structure_designer_scene.selected_node_eval_cache.as_ref()?;
        let geo_trans_cache = eval_cache.downcast_ref::<GeoTransEvalCache>()?;
        
        let gadget = GeoTransGadget::new(
            self.translation,
            self.rotation,
            geo_trans_cache.input_frame_transform.clone(),
        );
        Some(Box::new(gadget))
    }
}

pub fn eval_geo_trans<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext,
) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let shape_input_name = registry.get_parameter_name(&node.node_type_name, 0);

  if node.arguments[0].is_empty() {
    return input_missing_error(&shape_input_name);
  }

  let input_node_id = node.arguments[0].get_node_id().unwrap();
  let shape_val = network_evaluator.evaluate(network_stack, input_node_id, registry, false, context)[0].clone();

  if let NetworkResult::Error(_error) = shape_val {
    return error_in_input(&shape_input_name);
  } else if let NetworkResult::Geometry(shape) = shape_val {

    let geo_trans_data = &node.data.as_any_ref().downcast_ref::<GeoTransData>().unwrap();
    let translation = geo_trans_data.translation.as_dvec3();
    let rotation_euler = geo_trans_data.rotation.as_dvec3() * PI * 0.5;
    let rotation_quat = DQuat::from_euler(
      glam::EulerRot::XYZ,
      rotation_euler.x, 
      rotation_euler.y, 
      rotation_euler.z);

    let frame_transform = shape.frame_transform.apply_lrot_gtrans_new(&Transform::new(translation, rotation_quat));

    // Store evaluation cache for selected node
    if NetworkStackElement::is_node_selected_in_root_network(network_stack, node_id) {
      let eval_cache = GeoTransEvalCache {
        input_frame_transform: shape.frame_transform.clone(),
      };
      context.selected_node_eval_cache = Some(Box::new(eval_cache));
    }

    // We need to be a bit tricky here.
    // The input geometry (shape) is already transformed by the input transform.
    // So theoretically we need to do the inverse of the input transform (shape transform) so the geometry is first transformed back
    // to its local position.
    // And then we apply the whole frame transform.
    let rot = frame_transform.rotation * shape.frame_transform.rotation.inverse();
    let tr = Transform::new(
      rot.mul_vec3(-shape.frame_transform.translation) + frame_transform.translation, 
      rot
    );

    return NetworkResult::Geometry(GeometrySummary { 
      frame_transform,
      geo_tree_root: GeoNode::Transform {
        transform: tr,
        shape: Box::new(shape.geo_tree_root),
      },
    });
  } else {
    return error_in_input(&shape_input_name);
  }
}

#[derive(Clone)]
pub struct GeoTransGadget {
    pub translation: IVec3,
    pub rotation: IVec3, // intrinsic euler angles where 1 increment means 90 degrees
    pub input_frame_transform: Transform,
    pub frame_transform: Transform,
    pub dragged_handle_index: Option<i32>,
    pub start_drag_offset: f64,
}

impl Tessellatable for GeoTransGadget {
  fn tessellate(&self, output_mesh: &mut Mesh) {
    xyz_gadget_utils::tessellate_xyz_gadget(
      output_mesh, 
      self.frame_transform.rotation,
      &self.frame_transform.translation,
      false // Don't include rotation handles for now
    );
  }

  fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
      Box::new(self.clone())
  }
}

impl Gadget for GeoTransGadget {
  fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
      xyz_gadget_utils::xyz_gadget_hit_test(
          self.frame_transform.rotation,
          &self.frame_transform.translation,
          &ray_origin,
          &ray_direction,
          false // Don't include rotation handles for now
      )
  }

  fn start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
    self.dragged_handle_index = Some(handle_index);
    self.start_drag_offset = xyz_gadget_utils::get_dragged_axis_offset(
        self.frame_transform.rotation,
        &self.frame_transform.translation,
        handle_index,
        &ray_origin,
        &ray_direction
    );
  }

  fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
    let current_offset = xyz_gadget_utils::get_dragged_axis_offset(
        self.frame_transform.rotation,
        &self.frame_transform.translation,
        handle_index,
        &ray_origin,
        &ray_direction
    );
    let offset_delta = current_offset - self.start_drag_offset;
    if self.apply_drag_offset(handle_index, offset_delta) {
      self.start_drag(handle_index, ray_origin, ray_direction);
    }
  }

  fn end_drag(&mut self) {
    self.dragged_handle_index = None;
  }
}

impl NodeNetworkGadget for GeoTransGadget {
  fn sync_data(&self, data: &mut dyn NodeData) {
      if let Some(geo_trans_data) = data.as_any_mut().downcast_mut::<GeoTransData>() {
        let delta_translation = (self.frame_transform.translation - self.input_frame_transform.translation) / (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);
        geo_trans_data.translation = delta_translation.round().as_ivec3();
        geo_trans_data.rotation = self.rotation;
      }
  }

  fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
      Box::new(self.clone())
  }
}

impl GeoTransGadget {
  pub fn new(translation: IVec3, rotation: IVec3, input_frame_transform: Transform) -> Self {
      let mut ret = Self {
          translation,
          rotation,
          input_frame_transform: Transform::new(input_frame_transform.translation * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64), input_frame_transform.rotation),
          frame_transform: Transform::new(DVec3::ZERO, DQuat::IDENTITY),
          dragged_handle_index: None,
          start_drag_offset: 0.0,
      };
      ret.refresh_frame_transform();
      return ret;
  }

  // Returns whether the application of the drag offset was successful and the drag start should be reset
  fn apply_drag_offset(&mut self, axis_index: i32, offset_delta: f64) -> bool {
    let rounded_delta = (offset_delta / (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64)).round();
    
    // Early return if no movement
    if rounded_delta == 0.0 {
      return false;
    }
    
    // Get the local axis direction based on the current rotation
    let local_axis_dir = match xyz_gadget_utils::get_local_axis_direction(self.frame_transform.rotation, axis_index) {
      Some(dir) => dir,
      None => return false, // Invalid axis index
    };
    
    // Calculate the movement vector
    let movement_distance = rounded_delta * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);
    let movement_vector = local_axis_dir * movement_distance;
    
    // Apply the movement to the frame transform
    self.frame_transform.translation += movement_vector;
    
    return true;
  }

  fn refresh_frame_transform(&mut self) {
    let translation = self.translation.as_dvec3() * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);
    let rotation_euler = self.rotation.as_dvec3() * PI * 0.5;
    let rotation_quat = DQuat::from_euler(
      glam::EulerRot::XYZ,
      rotation_euler.x, 
      rotation_euler.y, 
      rotation_euler.z);

    self.frame_transform = self.input_frame_transform.apply_lrot_gtrans_new(&Transform::new(translation, rotation_quat));
  }
}
