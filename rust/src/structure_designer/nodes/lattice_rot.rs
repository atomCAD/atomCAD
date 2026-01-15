use crate::display::gadget::Gadget;
use crate::structure_designer::evaluator::network_evaluator::{
  NetworkEvaluationContext, NetworkEvaluator
};
use crate::structure_designer::evaluator::network_result::{
  runtime_type_error_in_input, GeometrySummary, NetworkResult
};
use crate::geo_tree::GeoNode;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use glam::f64::DVec3;
use crate::structure_designer::text_format::TextValue;
use glam::i32::IVec3;
use crate::util::serialization_utils::ivec3_serializer;
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
pub struct LatticeRotEvalCache {
  pub unit_cell: UnitCellStruct,
  pub pivot_point: IVec3,  // The actual evaluated pivot point (may be overridden by input pin)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeRotData {
  pub axis_index: Option<i32>,  // Index into available symmetry axes, None means no rotation
  pub step: i32,  // Integer multiple of the smallest rotation angle for the selected axis
  #[serde(with = "ivec3_serializer")]
  pub pivot_point: IVec3,  // Pivot point around which rotation occurs
}

impl NodeData for LatticeRotData {
    fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
        let eval_cache = structure_designer.get_selected_node_eval_cache()?;
        let lattice_rot_cache = eval_cache.downcast_ref::<LatticeRotEvalCache>()?;
        
        let gadget = LatticeRotGadget::new(
            self.axis_index,
            self.step,
            lattice_rot_cache.pivot_point,  // Use the evaluated pivot point from cache
            &lattice_rot_cache.unit_cell,
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
        return shape_val;
      }
      else if let NetworkResult::Geometry(shape) = shape_val {

        let axis_index = match network_evaluator.evaluate_or_default(
          network_stack, node_id, registry, context, 1, 
          self.axis_index, 
          NetworkResult::extract_optional_int
        ) {
          Ok(value) => value,
          Err(error) => return error,
        };
    
        let step = match network_evaluator.evaluate_or_default(
          network_stack, node_id, registry, context, 2, 
          self.step, 
          NetworkResult::extract_int
        ) {
          Ok(value) => value,
          Err(error) => return error,
        };

        let pivot_point = match network_evaluator.evaluate_or_default(
          network_stack, node_id, registry, context, 3, 
          self.pivot_point, 
          NetworkResult::extract_ivec3
        ) {
          Ok(value) => value,
          Err(error) => return error,
        };

        // Get all available symmetry axes for this unit cell
        let symmetry_axes = analyze_unit_cell_symmetries(&shape.unit_cell);
        
        // Calculate rotation quaternion using discrete symmetry approach
        let real_rotation_quat = if axis_index.is_none() || step == 0 {
          // No rotation case
          DQuat::IDENTITY
        } else if symmetry_axes.is_empty() {
          // No symmetries available (triclinic system) - only allow identity rotation
          DQuat::IDENTITY
        } else {
          let axis_idx = axis_index.unwrap();
          
          // Apply modulo to axis_index to wrap around available axes
          let safe_axis_index = ((axis_idx % symmetry_axes.len() as i32) + symmetry_axes.len() as i32) % symmetry_axes.len() as i32;
          let selected_symmetry = &symmetry_axes[safe_axis_index as usize];
          
          // Apply modulo to step to wrap around n_fold rotations
          let safe_step = ((step % selected_symmetry.n_fold as i32) + selected_symmetry.n_fold as i32) % selected_symmetry.n_fold as i32;
          
          if safe_step == 0 {
            DQuat::IDENTITY
          } else {
            // Calculate rotation angle
            let angle_per_step = selected_symmetry.smallest_angle_radians();
            let total_angle = angle_per_step * safe_step as f64;
            
            // Create rotation quaternion
            DQuat::from_axis_angle(selected_symmetry.axis, total_angle)
          }
        };
    
        // Store evaluation cache for root-level evaluations (used for gadget creation when this node is selected)
        // Only store for direct evaluations of visible nodes, not for upstream dependency calculations
        if network_stack.len() == 1 {
          let eval_cache = LatticeRotEvalCache {
            unit_cell: shape.unit_cell.clone(),
            pivot_point: pivot_point,  // Store the actual evaluated pivot point
          };
          context.selected_node_eval_cache = Some(Box::new(eval_cache));
        }

        // Transform geometry - rotation around pivot point
        // Convert pivot point from lattice coordinates to real space coordinates
        let pivot_real = shape.unit_cell.ivec3_lattice_to_real(&pivot_point);
        
        // Create transformation that rotates around the pivot point
        // This is equivalent to: translate to origin, rotate, translate back
        let tr = Transform::new_rotation_around_point(pivot_real, real_rotation_quat);

        return NetworkResult::Geometry(GeometrySummary {
          unit_cell: shape.unit_cell.clone(),
          frame_transform: Transform::default(),
          geo_tree_root: GeoNode::transform(tr, Box::new(shape.geo_tree_root)),
        });
      } else {
        return runtime_type_error_in_input(0);
      }
    }

    fn get_subtitle(&self, connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        let show_axis_index = !connected_input_pins.contains("axis_index");
        let show_step = !connected_input_pins.contains("step");
        
        let mut parts = Vec::new();
        
        // Only show rotation info if there's actually a rotation
        let has_rotation = self.axis_index.is_some() && self.step != 0;
        
        if has_rotation && show_axis_index {
            if let Some(axis_idx) = self.axis_index {
                parts.push(format!("axis: {}", axis_idx));
            }
        }
        
        if has_rotation && show_step {
            parts.push(format!("step: {}", self.step));
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
            ("step".to_string(), TextValue::Int(self.step)),
            ("pivot_point".to_string(), TextValue::IVec3(self.pivot_point)),
        ];
        if let Some(axis_idx) = self.axis_index {
            props.insert(0, ("axis_index".to_string(), TextValue::Int(axis_idx)));
        }
        props
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("axis_index") {
            self.axis_index = Some(v.as_int().ok_or_else(|| "axis_index must be an integer".to_string())?);
        }
        if let Some(v) = props.get("step") {
            self.step = v.as_int().ok_or_else(|| "step must be an integer".to_string())?;
        }
        if let Some(v) = props.get("pivot_point") {
            self.pivot_point = v.as_ivec3().ok_or_else(|| "pivot_point must be an IVec3".to_string())?;
        }
        Ok(())
    }
}


#[derive(Clone)]
pub struct LatticeRotGadget {
    pub axis_index: Option<i32>,
    pub step: i32,
    pub pivot_point: IVec3,
    pub unit_cell: UnitCellStruct,
}

impl Tessellatable for LatticeRotGadget {
  fn tessellate(&self, output: &mut TessellationOutput) {
    let output_mesh: &mut Mesh = &mut output.mesh;
    // Visualize rotation axis if present
    if let Some(axis_idx) = self.axis_index {
      let symmetry_axes = analyze_unit_cell_symmetries(&self.unit_cell);
      
      if !symmetry_axes.is_empty() {
        // Apply modulo to axis_index to get valid axis
        let safe_axis_index = ((axis_idx % symmetry_axes.len() as i32) + symmetry_axes.len() as i32) % symmetry_axes.len() as i32;
        let selected_symmetry = &symmetry_axes[safe_axis_index as usize];
        
        let normalized_axis = selected_symmetry.axis;
        let cylinder_length = 30.0;
        let cylinder_radius = 0.1;
        
        // Convert pivot point to real space coordinates
        let pivot_real = self.unit_cell.ivec3_lattice_to_real(&self.pivot_point);
        
        // Create cylinder endpoints along the rotation axis centered at pivot point
        let half_length = cylinder_length * 0.5;
        let top_center = pivot_real + normalized_axis * half_length;
        let bottom_center = pivot_real - normalized_axis * half_length;
        
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
        
        // Tessellate a small red sphere at the pivot point
        let red_material = crate::renderer::mesh::Material::new(
          &glam::f32::Vec3::new(1.0, 0.0, 0.0), // Red color
          0.4,
          0.0
        );
        
        crate::renderer::tessellator::tessellator::tessellate_sphere(
          output_mesh,
          &pivot_real,
          0.4, // radius (same as CENTER_SPHERE_RADIUS)
          12,  // divisions (same as CENTER_SPHERE_DIVISIONS)
          12,  // divisions
          &red_material
        );
      }
    }
  }

  fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
      Box::new(self.clone())
  }
}

impl Gadget for LatticeRotGadget {
  fn hit_test(&self, _ray_origin: DVec3, _ray_direction: DVec3) -> Option<i32> {
      // No interactive handles for now - just display the rotation axis
      None
  }

  fn start_drag(&mut self, _handle_index: i32, _ray_origin: DVec3, _ray_direction: DVec3) {
    // No drag functionality for now
  }

  fn drag(&mut self, _handle_index: i32, _ray_origin: DVec3, _ray_direction: DVec3) {
    // No drag functionality for now
  }

  fn end_drag(&mut self) {
    // No drag functionality for now
  }
}

impl NodeNetworkGadget for LatticeRotGadget {
  fn sync_data(&self, _data: &mut dyn NodeData) {
      // No data to sync for now since we don't have interactive handles
  }

  fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
      Box::new(self.clone())
  }
}

impl LatticeRotGadget {
  pub fn new(axis_index: Option<i32>, step: i32, pivot_point: IVec3, unit_cell: &UnitCellStruct) -> Self {
      Self {
          axis_index,
          step,
          pivot_point,
          unit_cell: unit_cell.clone(),
      }
  }
}

pub fn get_node_type() -> NodeType {
  NodeType {
      name: "lattice_rot".to_string(),
      description: "Rotates geometry in lattice space.
Only rotations that are symmetries of the currently selected unit cell are allowed — the node exposes only those valid lattice-symmetry rotations.
You may provide a pivot point for the rotation; by default the pivot is the origin `(0,0,0)`.".to_string(),
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
          Parameter {
              name: "shape".to_string(),
              data_type: DataType::Geometry,
          },
          Parameter {
            name: "axis_index".to_string(),
            data_type: DataType::Int,
          },
          Parameter {
            name: "step".to_string(),
            data_type: DataType::Int,
          },
          Parameter {
            name: "pivot_point".to_string(),
            data_type: DataType::IVec3,
          },
      ],
      output_type: DataType::Geometry,
      public: true,
      node_data_creator: || Box::new(LatticeRotData {
        axis_index: None,
        step: 0,
        pivot_point: IVec3::new(0, 0, 0),
      }),
      node_data_saver: generic_node_data_saver::<LatticeRotData>,
      node_data_loader: generic_node_data_loader::<LatticeRotData>,
  }
}
