use crate::display::gadget::Gadget;
use crate::structure_designer::evaluator::network_evaluator::{
  NetworkEvaluationContext, NetworkEvaluator
};
use crate::structure_designer::evaluator::network_result::{
  runtime_type_error_in_input, GeometrySummary, NetworkResult, error_in_input
};
use crate::geo_tree::GeoNode;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::utils::xyz_gadget_utils;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::util::serialization_utils::{ivec3_serializer, option_dvec3_serializer};
use crate::structure_designer::text_format::TextValue;
use glam::f64::DVec3;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::DQuat;
use crate::util::transform::Transform;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::crystolecule::unit_cell_symmetries::analyze_unit_cell_symmetries;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator::{Tessellatable, TessellationOutput};

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
    fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
        let eval_cache = structure_designer.get_selected_node_eval_cache()?;
        let geo_trans_cache = eval_cache.downcast_ref::<LatticeSymopEvalCache>()?;
        
        let gadget = LatticeSymopGadget::new(
            self.translation,
            self.rotation_axis,
            geo_trans_cache.input_frame_transform.clone(),
            &geo_trans_cache.unit_cell,
        );
        Some(Box::new(gadget))
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
        shape_val
      } else if let NetworkResult::Geometry(shape) = shape_val {

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
    
        // Store evaluation cache for root-level evaluations (used for gadget creation when this node is selected)
        // Only store for direct evaluations of visible nodes, not for upstream dependency calculations
        if network_stack.len() == 1 {
          let eval_cache = LatticeSymopEvalCache {
            input_frame_transform: shape.frame_transform.clone(),
            unit_cell: shape.unit_cell.clone(),
          };
          context.selected_node_eval_cache = Some(Box::new(eval_cache));
        }

        // Move fields we need by value out of the shape summary
        let GeometrySummary { unit_cell, frame_transform: input_frame_transform, geo_tree_root } = shape;

        // Calculate the new frame transform
        // The resulting frame transform should only contain translation, no rotation
        let frame_transform = Transform::new(
          input_frame_transform.translation + real_translation,
          DQuat::IDENTITY  // Frame transform should not contain rotation
        );

        // Build the output geometry, moving the geo_tree_root instead of cloning it
        let output_geo_tree_root = if transform_only_frame {
          // Only transform the reference frame, leave geometry in place
          geo_tree_root
        } else {
          // Transform both frame and geometry
          // Since input_frame_transform.rotation is always identity (deprecated), this simplifies to:
          // 1. Undo the input translation: move geometry back by -input_frame_transform.translation
          // 2. Apply the new rotation around origin
          // 3. Apply the new translation: frame_transform.translation
          let tr = Transform::new(
            real_rotation_quat.mul_vec3(-input_frame_transform.translation) + frame_transform.translation, 
            real_rotation_quat
          );

          GeoNode::transform(tr, Box::new(geo_tree_root))
        };

        return NetworkResult::Geometry(GeometrySummary {
          unit_cell,
          frame_transform,
          geo_tree_root: output_geo_tree_root,
        });
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

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        let mut props = vec![
            ("translation".to_string(), TextValue::IVec3(self.translation)),
            ("rotation_angle_degrees".to_string(), TextValue::Float(self.rotation_angle_degrees)),
            ("transform_only_frame".to_string(), TextValue::Bool(self.transform_only_frame)),
        ];
        if let Some(axis) = self.rotation_axis {
            props.push(("rotation_axis".to_string(), TextValue::Vec3(axis)));
        }
        props
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("translation") {
            self.translation = v.as_ivec3().ok_or_else(|| "translation must be an IVec3".to_string())?;
        }
        if let Some(v) = props.get("rotation_axis") {
            self.rotation_axis = Some(v.as_vec3().ok_or_else(|| "rotation_axis must be a Vec3".to_string())?);
        }
        if let Some(v) = props.get("rotation_angle_degrees") {
            self.rotation_angle_degrees = v.as_float().ok_or_else(|| "rotation_angle_degrees must be a float".to_string())?;
        }
        if let Some(v) = props.get("transform_only_frame") {
            self.transform_only_frame = v.as_bool().ok_or_else(|| "transform_only_frame must be a boolean".to_string())?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("shape".to_string(), (true, None)); // required
        m
    }
}


#[derive(Clone)]
pub struct LatticeSymopGadget {
    pub translation: IVec3,
    pub rotation_axis: Option<DVec3>,
    pub input_frame_transform: Transform,
    pub frame_transform: Transform,
    pub dragged_handle_index: Option<i32>,
    pub start_drag_offset: f64,
    pub unit_cell: UnitCellStruct,
}

impl Tessellatable for LatticeSymopGadget {
  fn tessellate(&self, output: &mut TessellationOutput) {
    let output_mesh: &mut Mesh = &mut output.mesh;
    xyz_gadget_utils::tessellate_xyz_gadget(
      output_mesh,
      &self.unit_cell,
      DQuat::IDENTITY,
      &self.frame_transform.translation,
      false // Don't include rotation handles for now
    );
    
    // Visualize rotation axis if present
    if let Some(axis) = self.rotation_axis {
      if axis.length() > 1e-12 {
        let normalized_axis = axis.normalize();
        let cylinder_length = 30.0;
        let cylinder_radius = 0.1;
        
        // Create cylinder endpoints along the rotation axis
        let half_length = cylinder_length * 0.5;
        let top_center = self.frame_transform.translation + normalized_axis * half_length;
        let bottom_center = self.frame_transform.translation - normalized_axis * half_length;
        
        // Use yellow color for the rotation axis
        let yellow_material = crate::renderer::mesh::Material::new(
          &glam::f32::Vec3::new(1.0, 1.0, 0.0), // Yellow color
          0.4,
          0.8
        );
        
        crate::renderer::tessellator::tessellator::tessellate_cylinder(
          output_mesh,
          &top_center,
          &bottom_center,
          cylinder_radius,
          16, // divisions
          &yellow_material,
          true, // include top and bottom caps
          Some(&yellow_material),
          Some(&yellow_material)
        );
      }
    }
  }

  fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
      Box::new(self.clone())
  }
}

impl Gadget for LatticeSymopGadget {
  fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
      xyz_gadget_utils::xyz_gadget_hit_test(
          &self.unit_cell,
          DQuat::IDENTITY,
          &self.frame_transform.translation,
          &ray_origin,
          &ray_direction,
          false // Don't include rotation handles for now
      )
  }

  fn start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
    self.dragged_handle_index = Some(handle_index);
    self.start_drag_offset = xyz_gadget_utils::get_dragged_axis_offset(
        &self.unit_cell,
        DQuat::IDENTITY,
        &self.frame_transform.translation,
        handle_index,
        &ray_origin,
        &ray_direction
    );
  }

  fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
    let current_offset = xyz_gadget_utils::get_dragged_axis_offset(
        &self.unit_cell,
        DQuat::IDENTITY,
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

impl NodeNetworkGadget for LatticeSymopGadget {
  fn sync_data(&self, data: &mut dyn NodeData) {
      if let Some(lattice_symop_data) = data.as_any_mut().downcast_mut::<LatticeSymopData>() {
        let real_delta_translation = self.frame_transform.translation - self.input_frame_transform.translation;
        lattice_symop_data.translation = self.unit_cell.real_to_ivec3_lattice(&real_delta_translation);
      }
  }

  fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
      Box::new(self.clone())
  }
}

impl LatticeSymopGadget {
  pub fn new(translation: IVec3, rotation_axis: Option<DVec3>, input_frame_transform: Transform, unit_cell: &UnitCellStruct) -> Self {
      let mut ret = Self {
          translation,
          rotation_axis,
          input_frame_transform: Transform::new(input_frame_transform.translation, input_frame_transform.rotation),
          frame_transform: Transform::new(DVec3::ZERO, DQuat::IDENTITY),
          dragged_handle_index: None,
          start_drag_offset: 0.0,
          unit_cell: unit_cell.clone(),
      };
      ret.refresh_frame_transform();
      ret
  }

  // Returns whether the application of the drag offset was successful and the drag start should be reset
  fn apply_drag_offset(&mut self, axis_index: i32, offset_delta: f64) -> bool {
    let axis_basis_vector = self.unit_cell.get_basis_vector(axis_index);
    let rounded_delta = (offset_delta / axis_basis_vector.length()).round();

    // Early return if no movement
    if rounded_delta == 0.0 {
      return false;
    }
    
    // Get the local axis direction based on the current rotation
    let local_axis_dir = match xyz_gadget_utils::get_local_axis_direction(&self.unit_cell, DQuat::IDENTITY, axis_index) {
      Some(dir) => dir,
      None => return false, // Invalid axis index
    };

    // Calculate the movement vector
    let movement_distance = rounded_delta * axis_basis_vector.length();
    let movement_vector = local_axis_dir * movement_distance;
    
    // Apply the movement to the frame transform
    self.frame_transform.translation += movement_vector;

    true
  }

  fn refresh_frame_transform(&mut self) {
    let real_translation = self.unit_cell.ivec3_lattice_to_real(&self.translation);
    let rotation_quat = DQuat::IDENTITY;

    self.frame_transform = self.input_frame_transform.apply_lrot_gtrans_new(&Transform::new(real_translation, rotation_quat));
  }
}

pub fn get_node_type() -> NodeType {
  NodeType {
      name: "lattice_symop".to_string(),
      description: "".to_string(),
      summary: None,
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
          Parameter {
              id: None,
              name: "shape".to_string(),
              data_type: DataType::Geometry,
          },
          Parameter {
            id: None,
            name: "translation".to_string(),
            data_type: DataType::IVec3,
          },
          Parameter {
            id: None,
            name: "rot_axis".to_string(),
            data_type: DataType::Vec3,
          },
          Parameter {
            id: None,
            name: "rot_angle".to_string(),
            data_type: DataType::Float,
          },
          Parameter {
            id: None,
            name: "keep_geo".to_string(),
            data_type: DataType::Float,
          },
      ],
      output_type: DataType::Geometry,
      public: false,
      node_data_creator: || Box::new(LatticeSymopData {
        translation: IVec3::new(0, 0, 0),
        rotation_axis: None,
        rotation_angle_degrees: 0.0,
        transform_only_frame: false,
      }),
      node_data_saver: generic_node_data_saver::<LatticeSymopData>,
      node_data_loader: generic_node_data_loader::<LatticeSymopData>,
    }
}
