use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::f64::DVec3;
use serde::{Serialize, Deserialize};
use crate::crystolecule::serialization_utils::dvec3_serializer;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::crystolecule::gadget::Gadget;
use glam::f64::DQuat;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::utils::xyz_gadget_utils;
use crate::util::transform::Transform;
use crate::structure_designer::node_type::NodeType;

#[derive(Debug, Clone)]
pub struct AtomTransEvalCache {
  pub input_frame_transform: Transform,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomTransData {
  #[serde(with = "dvec3_serializer")]
  pub translation: DVec3,
  #[serde(with = "dvec3_serializer")]
  pub rotation: DVec3, // intrinsic euler angles in radians
}

impl NodeData for AtomTransData {
    fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
        let eval_cache = structure_designer.get_selected_node_eval_cache()?;
        let atom_trans_cache = eval_cache.downcast_ref::<AtomTransEvalCache>()?;

        return Some(Box::new(AtomTransGadget::new(
            self.translation,
            self.rotation, 
            atom_trans_cache.input_frame_transform.clone()
        )));
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
      None
    }

    fn eval<'a>(
      &self,
      network_evaluator: &NetworkEvaluator,
      network_stack: &Vec<NetworkStackElement<'a>>,
      node_id: u64,
      registry: &NodeTypeRegistry,
      _decorate: bool,
      context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext) -> NetworkResult {      
      let input_val = network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
      if let NetworkResult::Error(_) = input_val {
        return input_val;
      }
      if let NetworkResult::Atomic(atomic_structure) = input_val {
    
        let translation = match network_evaluator.evaluate_or_default(
          network_stack, node_id, registry, context, 1, 
          self.translation, 
          NetworkResult::extract_vec3
        ) {
          Ok(value) => value,
          Err(error) => return error,
        };
      
        let rotation = match network_evaluator.evaluate_or_default(
          network_stack, node_id, registry, context, 2, 
          self.rotation, 
          NetworkResult::extract_vec3
        ) {
          Ok(value) => value,
          Err(error) => return error,
        };
    
        let rotation_quat = DQuat::from_euler(
          glam::EulerRot::XYZ,
          rotation.x, 
          rotation.y, 
          rotation.z);
    
        let frame_transform = atomic_structure.frame_transform().apply_lrot_gtrans_new(&Transform::new(translation, rotation_quat));
    
        // Store evaluation cache for root-level evaluations (used for gadget creation when this node is selected)
        // Only store for direct evaluations of visible nodes, not for upstream dependency calculations
        if network_stack.len() == 1 {
            let eval_cache = AtomTransEvalCache {
              input_frame_transform: atomic_structure.frame_transform().clone(),
            };
            context.selected_node_eval_cache = Some(Box::new(eval_cache));
        }
    
        // The input is already transformed by the input transform.
        // So we need to do the inverse of the input transform so the structure is first transformed back
        // to its local position.
        // And then we apply the whole frame transform.
    
        let mut result_atomic_structure = atomic_structure.clone();
    
        let inverse_input_transform = atomic_structure.frame_transform().inverse();
    
        result_atomic_structure.transform(&inverse_input_transform.rotation, &inverse_input_transform.translation);
        result_atomic_structure.transform(&frame_transform.rotation, &frame_transform.translation);
        result_atomic_structure.set_frame_transform(frame_transform);
    
        return NetworkResult::Atomic(result_atomic_structure);
      }
      return NetworkResult::None;
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        Some(format!("r: ({:.2},{:.2},{:.2}) t: ({:.2},{:.2},{:.2})", 
            self.rotation.x, self.rotation.y, self.rotation.z,
            self.translation.x, self.translation.y, self.translation.z))
    }
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
            &UnitCellStruct::cubic_diamond(),
            self.frame_transform.rotation,
            &self.frame_transform.translation,
            true,
        );
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for AtomTransGadget {
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        xyz_gadget_utils::xyz_gadget_hit_test(
            &UnitCellStruct::cubic_diamond(),
            self.frame_transform.rotation,
            &self.frame_transform.translation,
            &ray_origin,
            &ray_direction,
            true
        )
    }
  
    fn start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
      self.dragged_handle_index = Some(handle_index);
      self.start_drag_offset = xyz_gadget_utils::get_dragged_axis_offset(
          &UnitCellStruct::cubic_diamond(),
          self.frame_transform.rotation,
          &self.frame_transform.translation,
          handle_index,
          &ray_origin,
          &ray_direction
      );
    }
  
    fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
      let current_offset = xyz_gadget_utils::get_dragged_axis_offset(
          &UnitCellStruct::cubic_diamond(),
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
        
        // Calculate relative rotation from input to current frame transform
        let relative_rotation_quat = self.frame_transform.rotation * self.input_frame_transform.rotation.inverse();
        
        // Convert to intrinsic XYZ euler angles
        let (x, y, z) = relative_rotation_quat.to_euler(glam::EulerRot::XYZ);
        atom_trans_data.rotation = DVec3::new(x, y, z);
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
        match axis_index {
            // Translation handles (0, 1, 2)
            0 | 1 | 2 => {
                // Get the local axis direction based on the current rotation
                let local_axis_dir = match xyz_gadget_utils::get_local_axis_direction(&UnitCellStruct::cubic_diamond(), self.frame_transform.rotation, axis_index) {
                    Some(dir) => dir,
                    None => return false, // Invalid axis index
                };    
                let movement_vector = local_axis_dir * offset_delta;
            
                // Apply the movement to the frame transform
                self.frame_transform.translation += movement_vector;
                
                return true;
            },
            // Rotation handles (3, 4, 5)
            3 | 4 | 5 => {
                // Map rotation handle indices to axis indices (3->0, 4->1, 5->2)
                let rotation_axis_index = axis_index - 3;
                
                // Get the local axis direction for rotation
                let local_axis_dir = match xyz_gadget_utils::get_local_axis_direction(&UnitCellStruct::cubic_diamond(), self.frame_transform.rotation, rotation_axis_index) {
                    Some(dir) => dir,
                    None => return false, // Invalid axis index
                };
                
                let rotation_angle = offset_delta * xyz_gadget_utils::ROTATION_SENSITIVITY;
                let rotation_quat = DQuat::from_axis_angle(local_axis_dir, rotation_angle);

                self.frame_transform.rotation = rotation_quat * self.frame_transform.rotation;
                
                return true;
            },
            _ => return false, // Invalid axis index
        }
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





