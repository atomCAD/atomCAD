use crate::structure_designer::evaluator::network_evaluator::{
  NetworkEvaluationContext, NetworkEvaluator
};
use crate::structure_designer::evaluator::network_result::{
  runtime_type_error_in_input, GeometrySummary, NetworkResult, error_in_input
};
use crate::structure_designer::geo_tree::GeoNode;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::{ivec3_serializer, option_dvec3_serializer};
use glam::f64::DVec3;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::DQuat;
use crate::util::transform::Transform;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::evaluator::unit_cell_symmetries::analyze_unit_cell_symmetries;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;

#[derive(Debug, Clone)]
pub struct LatticeSymopEvalCache {
  pub input_frame_transform: Transform,
  pub unit_cell: UnitCellStruct,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeSymopData {
  #[serde(with = "ivec3_serializer")]
  pub translation: IVec3,
  #[serde(with = "option_dvec3_serializer")]
  pub rotation_axis: Option<DVec3>,  // Optional real axis direction unit vector, None means no rotation
  pub rotation_angle_degrees: f64,  // Real angle of rotation in degrees
  pub transform_only_frame: bool, // (a.k.a keep_geo) If true, only the reference frame is transformed, the geometry remains in place.
}

impl NodeData for LatticeSymopData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
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
      context: &mut NetworkEvaluationContext,
    ) -> NetworkResult {
      let shape_val = network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
    
      if let NetworkResult::Error(_) = shape_val {
        return shape_val;
      }
      else if let NetworkResult::Geometry(shape) = shape_val {

        let translation = match network_evaluator.evaluate_or_default(
          network_stack, node_id, registry, context, 1, 
          self.translation, 
          NetworkResult::extract_ivec3
        ) {
          Ok(value) => value,
          Err(error) => return error,
        };
      
        let rotation_axis = match network_evaluator.evaluate_or_default(
          network_stack, node_id, registry, context, 2, 
          self.rotation_axis, 
          NetworkResult::extract_optional_dvec3
        ) {
          Ok(value) => value,
          Err(error) => return error,
        };
    
        let rotation_angle_degrees = match network_evaluator.evaluate_or_default(
          network_stack, node_id, registry, context, 3, 
          self.rotation_angle_degrees, 
          NetworkResult::extract_float
        ) {
          Ok(value) => value,
          Err(error) => return error,
        };

        let transform_only_frame = match network_evaluator.evaluate_or_default(
          network_stack, node_id, registry, context, 4, 
          self.transform_only_frame, 
          NetworkResult::extract_bool
        ) {
          Ok(value) => value,
          Err(error) => return error,
        };

        let real_translation = shape.unit_cell.ivec3_lattice_to_real(&translation);

        // Get all available symmetry axes for this unit cell
        let symmetry_axes = analyze_unit_cell_symmetries(&shape.unit_cell);
        
        // Validate the rotation against available symmetries and calculate quaternion
        let real_rotation_quat = if rotation_axis.is_none() || rotation_angle_degrees.abs() < 1e-6 {
          // No rotation case
          DQuat::IDENTITY
        } else {
          let rotation_axis = rotation_axis.unwrap();
          
          // Check for zero-length axis
          if rotation_axis.length() < 1e-12 {
            DQuat::IDENTITY
          } else {
            // Normalize the rotation axis
            let normalized_axis = rotation_axis.normalize();
          
          // Validate against available symmetries
          let mut is_valid = false;
          
          if !symmetry_axes.is_empty() {
            // Check if the rotation matches any available symmetry
            const AXIS_TOLERANCE: f64 = 1e-6;
            const ANGLE_TOLERANCE: f64 = 1e-4;
            
            for symmetry in &symmetry_axes {
              // Check if axes are parallel (or anti-parallel)
              let dot_product = normalized_axis.dot(symmetry.axis).abs();
              if (1.0 - dot_product).abs() < AXIS_TOLERANCE {
                // Axes match, check if angle is a valid multiple
                let fundamental_angle = symmetry.smallest_angle_degrees();
                let angle_ratio = rotation_angle_degrees.abs() / fundamental_angle;
                let rounded_ratio = angle_ratio.round();
                
                if (angle_ratio - rounded_ratio).abs() < ANGLE_TOLERANCE {
                  is_valid = true;
                  break;
                }
              }
            }
          } else {
            // No symmetries available (triclinic system) - only allow identity rotation
            is_valid = false;
          }
          
          if !is_valid {
            return error_in_input(&format!(
              "Rotation axis {:?} with angle {:.2}° is not allowed for this crystal system", 
              rotation_axis, rotation_angle_degrees
            ));
          }
          
            // Create rotation quaternion
            let angle_radians = rotation_angle_degrees.to_radians();
            DQuat::from_axis_angle(normalized_axis, angle_radians)
          }
        };
    
        // Store evaluation cache for selected node
        if NetworkStackElement::is_node_selected_in_root_network(network_stack, node_id) {
          let eval_cache = LatticeSymopEvalCache {
            input_frame_transform: shape.frame_transform.clone(),
            unit_cell: shape.unit_cell.clone(),
          };
          context.selected_node_eval_cache = Some(Box::new(eval_cache));
        }

        // Calculate the new frame transform
        // The resulting frame transform should only contain translation, no rotation
        let frame_transform = Transform::new(
          shape.frame_transform.translation + real_translation,
          DQuat::IDENTITY  // Frame transform should not contain rotation
        );

        // Handle transform_only_frame flag
        if transform_only_frame {
          // Only transform the reference frame, leave geometry in place
          return NetworkResult::Geometry(GeometrySummary {
            unit_cell: shape.unit_cell.clone(),
            frame_transform,
            geo_tree_root: shape.geo_tree_root.clone(),
          });
        } else {
          // Transform both frame and geometry
          // Since shape.frame_transform.rotation is always identity (deprecated), this simplifies to:
          // 1. Undo the input translation: move geometry back by -shape.frame_transform.translation
          // 2. Apply the new rotation around origin
          // 3. Apply the new translation: frame_transform.translation
          let tr = Transform::new(
            real_rotation_quat.mul_vec3(-shape.frame_transform.translation) + frame_transform.translation, 
            real_rotation_quat
          );

          return NetworkResult::Geometry(GeometrySummary {
            unit_cell: shape.unit_cell.clone(),
            frame_transform,
            geo_tree_root: GeoNode::Transform {
              transform: tr,
              shape: Box::new(shape.geo_tree_root),
            },
          });
        }
      } else {
        return runtime_type_error_in_input(0);
      }
    }

    fn get_subtitle(&self, connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        let show_translation = !connected_input_pins.contains("translation");
        let show_rot_axis = !connected_input_pins.contains("rot_axis");
        let show_rot_angle = !connected_input_pins.contains("rot_angle");
        
        let mut parts = Vec::new();
        
        if show_translation {
            parts.push(format!("t: ({},{},{})", 
                self.translation.x, self.translation.y, self.translation.z));
        }
        
        // Only show rotation info if there's actually a rotation
        let has_rotation = self.rotation_axis.is_some() && self.rotation_angle_degrees.abs() > 1e-6;
        
        if has_rotation && show_rot_axis {
            if let Some(axis) = self.rotation_axis {
                parts.push(format!("ax: ({:.2},{:.2},{:.2})", 
                    axis.x, axis.y, axis.z));
            }
        }
        
        if has_rotation && show_rot_angle {
            parts.push(format!("ang: {:.1}°", self.rotation_angle_degrees));
        }
        
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }
}