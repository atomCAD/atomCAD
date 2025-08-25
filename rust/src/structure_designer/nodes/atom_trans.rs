use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::f64::DVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::dvec3_serializer;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::common::gadget::Gadget;
use glam::f64::DQuat;
use glam::f32::Vec3;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::common::atomic_structure::AtomicStructure;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::utils::xyz_gadget_utils;
use crate::util::transform::Transform;

#[derive(Debug, Clone)]
pub struct AtomTransEvalCache {
  pub input_frame_transform: Transform,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AtomTransData {
  #[serde(with = "dvec3_serializer")]
  pub translation: DVec3,
  #[serde(with = "dvec3_serializer")]
  pub rotation: DVec3, // intrinsic euler angles in radians
}

impl NodeData for AtomTransData {
    fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
        let eval_cache = structure_designer.last_generated_structure_designer_scene.selected_node_eval_cache.as_ref()?;
        let atom_trans_cache = eval_cache.downcast_ref::<AtomTransEvalCache>()?;

        return Some(Box::new(AtomTransGadget::new(
            self.translation,
            self.rotation, 
            atom_trans_cache.input_frame_transform.clone()
        )));
    }
}

pub fn eval_atom_trans<'a>(network_evaluator: &NetworkEvaluator, network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, registry: &NodeTypeRegistry, context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext) -> NetworkResult {  
  let node = NetworkStackElement::get_top_node(network_stack, node_id);

  if node.arguments[0].is_empty() {
    return NetworkResult::Atomic(AtomicStructure::new());
  }
  let input_molecule_node_id = node.arguments[0].get_node_id().unwrap();

  let result = &network_evaluator.evaluate(network_stack, input_molecule_node_id, registry, false, context)[0];
  if let NetworkResult::Atomic(atomic_structure) = result {

    let atom_trans_data = &node.data.as_any_ref().downcast_ref::<AtomTransData>().unwrap();

    let rotation_quat = DQuat::from_euler(
      glam::EulerRot::XYZ,
      atom_trans_data.rotation.x, 
      atom_trans_data.rotation.y, 
      atom_trans_data.rotation.z);

    let frame_transform = atomic_structure.frame_transform.apply_lrot_gtrans_new(&Transform::new(atom_trans_data.translation, rotation_quat));

    // Store evaluation cache for selected node
    if NetworkStackElement::is_node_selected_in_root_network(network_stack, node_id) {
        let eval_cache = AtomTransEvalCache {
          input_frame_transform: atomic_structure.frame_transform.clone(),
        };
        context.selected_node_eval_cache = Some(Box::new(eval_cache));
    }

    // The input is already transformed by the input transform.
    // So we need to do the inverse of the input transform so the structure is first transformed back
    // to its local position.
    // And then we apply the whole frame transform.

    let mut result_atomic_structure = atomic_structure.clone();

    let inverse_input_transform = atomic_structure.frame_transform.inverse();

    result_atomic_structure.transform(&inverse_input_transform.rotation, &inverse_input_transform.translation);
    result_atomic_structure.transform(&frame_transform.rotation, &frame_transform.translation);
    result_atomic_structure.frame_transform = frame_transform;

    return NetworkResult::Atomic(result_atomic_structure);
  }
  return NetworkResult::None;
}

#[derive(Clone)]
pub struct AtomTransGadget {
    pub translation: DVec3,
    pub rotation: DVec3, // intrinsic euler angles in radians
    pub input_frame_transform: Transform,
    pub frame_transform: Transform,
    pub dragged_handle_index: Option<i32>,
    pub start_drag_offset: f64,
}

impl Tessellatable for AtomTransGadget {
    fn tessellate(&self, output_mesh: &mut Mesh) {
        xyz_gadget_utils::tessellate_xyz_gadget(
            output_mesh, 
            self.frame_transform.rotation,
            &self.frame_transform.translation
        );
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for AtomTransGadget {
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        xyz_gadget_utils::xyz_gadget_hit_test(
            self.frame_transform.rotation,
            &self.frame_transform.translation,
            &ray_origin,
            &ray_direction
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

impl NodeNetworkGadget for AtomTransGadget {
    fn sync_data(&self, data: &mut dyn NodeData) {
      if let Some(atom_trans_data) = data.as_any_mut().downcast_mut::<AtomTransData>() {
        atom_trans_data.translation = self.frame_transform.translation - self.input_frame_transform.translation;
        atom_trans_data.rotation = self.rotation;
      }
    }

    fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
        Box::new(self.clone())
    }
}

impl AtomTransGadget {
    pub fn new(translation: DVec3, rotation: DVec3, input_frame_transform: Transform) -> Self {
        let mut ret = Self {
            translation,
            rotation,
            input_frame_transform,
            frame_transform: Transform::new(DVec3::ZERO, DQuat::IDENTITY),
            dragged_handle_index: None,
            start_drag_offset: 0.0,
        };
        ret.refresh_frame_transform();
        return ret;
    }

    // Returns whether the application of the drag offset was successful and the drag start should be reset
    fn apply_drag_offset(&mut self, axis_index: i32, offset_delta: f64) -> bool {
        // Get the local axis direction based on the current rotation
        let local_axis_dir = match xyz_gadget_utils::get_local_axis_direction(self.frame_transform.rotation, axis_index) {
            Some(dir) => dir,
            None => return false, // Invalid axis index
        };    
        let movement_vector = local_axis_dir * offset_delta;
    
        // Apply the movement to the frame transform
        self.frame_transform.translation += movement_vector;
    
        return true;
    }

    fn refresh_frame_transform(&mut self) {
        let rotation_quat = DQuat::from_euler(
          glam::EulerRot::XYZ,
          self.rotation.x, 
          self.rotation.y, 
          self.rotation.z);
    
        self.frame_transform = self.input_frame_transform.apply_lrot_gtrans_new(&Transform::new(self.translation, rotation_quat));
    }
}

